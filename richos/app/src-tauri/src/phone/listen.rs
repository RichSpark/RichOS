//! **THE SOCKET — and everything about it that plan §2.5 promised in exchange for opening one.**
//!
//! > *"Bar (c) is gone, and this is **RichOS's first inbound network listener** … That is the real
//! > security cost of §57, and it is contained rather than hand-waved."*
//!
//! This file is the containment, and each item is at the line that enforces it:
//!
//!  1. **It does not exist until he pairs a phone.** [`Listener::start`] is called from the
//!     pairing surface; [`Listener`]'s `Drop` closes every socket and JOINS the thread, so "the
//!     listener stopped" means the port is free when the call returns rather than soon.
//!  2. **It binds the LAN interfaces only, never `0.0.0.0`** — every address comes from
//!     [`super::names`], which read them off this machine by name, and [`check_addresses`] refuses
//!     a wildcard and an empty list by hand.
//!  3. **TLS with our own leaf** — [`tls_config`], built from what [`super::ca`] issued.
//!  4. **Every route requires the credential, everything else is a flat 404** —
//!     [`super::routes`].
//!  6. **Body-size limits at the edge** — `Limited` fails the read rather than buffering the body
//!     and then measuring it, which is the difference between a limit and a check.
//!
//! # Why the channel owns its own runtime
//!
//! A current-thread tokio runtime on one named thread, created when the listener starts and
//! dropped when it stops. The app's own async runtime outlives the listener by design, so hanging
//! the sockets off it would make "stopped" a flag rather than an absence. This way **stopping the
//! channel destroys the thing that was listening** — item 1 stated as a fact about the process
//! rather than about a boolean. Dropping the runtime also ends every open event stream, which is
//! why the stream task does not have to watch for shutdown itself.
//!
//! # Why there is no `tokio::select!` in here
//!
//! The macro lives behind `tokio/macros`, which would add `tokio-macros` as a new package.
//! `futures_util::future::select` does the same job for the one race that needs it — one
//! `accept()` against one shutdown — and `tokio::time::timeout` does the other. An attribute is
//! not worth a `[[package]]` line.

use super::device::{DeviceDesk, PAIR_WAIT_MAX_SECONDS};
use super::routes::{dispatch, Channel, Incoming, Outcome};
use super::{PhoneError, KEEPALIVE_MS};
#[cfg(test)]
use super::HTTPS_PORT;
use bytes::Bytes;
use futures_util::future::{select, Either};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming as HyperBody;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use std::net::{IpAddr, SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use tokio::sync::watch;

type BoxBody = http_body_util::combinators::BoxBody<Bytes, std::convert::Infallible>;

/// A running channel. Dropping it stops the channel.
pub struct Listener {
    shutdown: watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Every address and port actually bound. Shown on the settings screen, and the thing the
    /// tests assert against — a claim about what was bound must come from the socket rather than
    /// from what we asked for.
    pub bound: Vec<SocketAddr>,
    /// The desk whose held asks [`Listener::stop`] releases before it signals shutdown.
    devices: Arc<DeviceDesk>,
}

/// **HOW LONG A STOPPING CHANNEL WAITS FOR THE ANSWERS IT HAS JUST RELEASED TO BE WRITTEN.**
///
/// Sage's pair-v2 hypotheses review §1 says a held ask is released by `They do not match` on this
/// Mac, by the expiry sweep and by the channel's teardown, and that the Mac "re-runs the request
/// and sends that outcome". **The first two are followed at once by the teardown**, and before
/// this bound existed the teardown dropped the runtime with the answer still unwritten —
/// `they_do_not_match_over_the_wire_stops_the_listener` records exactly that race for an ordinary
/// request. So the serving loops stop accepting first (the port closes at once, as before), and
/// then wait up to this long for the held answers, and only those, to leave. One second: a
/// released answer is one signature check and one small write, measured in milliseconds by
/// `a_held_ask_hears_they_do_not_match_on_the_mac_before_the_channel_closes`.
const HELD_ANSWER_DRAIN_MS: u64 = 1_000;

/// **Read `Prefer: wait=N` (RFC 7240) as the seconds this Mac may hold the answer**, capped at
/// [`PAIR_WAIT_MAX_SECONDS`]. `None` for no header, no `wait`, zero, or a value that is not a
/// number. The header is unsigned on purpose (Sage's review, "Security, plainly"): stripping or
/// forging it changes only how long an answer takes, never what the answer is.
pub fn hold_seconds(prefer: Option<&str>) -> Option<u64> {
    for preference in prefer?.split(',') {
        let token = preference.split(';').next().unwrap_or("").trim();
        let Some((name, value)) = token.split_once('=') else { continue };
        if !name.trim().eq_ignore_ascii_case("wait") {
            continue;
        }
        let seconds: u64 = value.trim().trim_matches('"').parse().ok()?;
        return (seconds > 0).then(|| seconds.min(PAIR_WAIT_MAX_SECONDS));
    }
    None
}

/// The held answers still being written, so a stopping channel can wait for them and nothing
/// else. Counted by a guard that lives in the response body, so "done" means hyper has finished
/// with the bytes, not that a handler returned.
#[derive(Default)]
struct Drain {
    open: std::sync::atomic::AtomicUsize,
    idle: tokio::sync::Notify,
}

struct DrainGuard(Arc<Drain>);

impl Drain {
    fn enter(self: &Arc<Self>) -> DrainGuard {
        self.open.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        DrainGuard(Arc::clone(self))
    }

    /// Wait until no held answer is still being written, or `bound` passes.
    async fn settle(&self, bound: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + bound;
        loop {
            // Created BEFORE the count is read: a `Notified` receives `notify_waiters` from the
            // moment it exists, so a guard dropped between the read and the await is not missed.
            let idle = self.idle.notified();
            if self.open.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                return;
            }
            if tokio::time::timeout_at(deadline, idle).await.is_err() {
                return;
            }
        }
    }
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        if self.0.open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

/// A response body that keeps its [`DrainGuard`] until hyper is done with it.
///
/// **The size hint is forwarded**, so hyper still writes `Content-Length` rather than switching
/// the answer to chunked framing: a held answer is byte for byte the answer an unheld ask gets.
struct GuardedBody {
    inner: BoxBody,
    _guard: DrainGuard,
}
impl hyper::body::Body for GuardedBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)
        -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        std::pin::Pin::new(&mut self.inner).poll_frame(cx)
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

impl Listener {
    /// Bind and serve.
    ///
    /// **Binding happens synchronously, before anything is spawned**, so a port already in use is
    /// an error the caller can show him rather than a failure inside a task nobody is watching. If
    /// any address fails to bind, every socket already bound is dropped and the whole start fails
    /// — a half-bound listener that answers on one interface and not another is worse than one
    /// that says it could not start.
    /// `https_port` is a parameter and not the constant, for one reason: the end-to-end test in
    /// this file stands a REAL listener up on an ephemeral port so it can prove the handshake and
    /// the routes against a real TLS client, and a test that fought the shipped app for port 8443
    /// would be a test that fails when he has the app open.
    /// Pass zero to let the OS assign a port while keeping the socket bound; `bound` reports
    /// the assigned address so clients never need a bind-and-release port probe.
    /// **[`super::PhoneRuntime::start`] is the only production caller and it passes
    /// [`HTTPS_PORT`].**
    ///
    /// **THE SECOND SOCKET IS GONE — CEO §61, 2026-09-19.** Every address also carried a plain
    /// HTTP listener on the neighboring port, which served one file: an Apple `.mobileconfig`
    /// for the phone to install, so that a certificate this Mac made for itself would be trusted
    /// on the local network. §61 rules that a phone app inside the home network is *"utterly
    /// useless"* and that the Tailscale path is the product, and on that path the certificate is
    /// publicly trusted and nothing is installed on the phone. So there is nothing for that port
    /// to serve, and a port bound with nothing behind it is an open port asking to be explained.
    pub fn start(
        channel: Arc<Channel>,
        tls: Arc<rustls::ServerConfig>,
        addresses: &[IpAddr],
        https_port: u16,
    ) -> Result<Self, PhoneError> {
        check_addresses(addresses)?;
        let mut https: Vec<StdTcpListener> = Vec::new();
        let mut bound: Vec<SocketAddr> = Vec::new();
        for address in addresses {
            let socket = SocketAddr::new(*address, https_port);
            let listener = bind_listener(socket)
                .map_err(|e| PhoneError::Io(format!("could not listen on {socket}: {e}")))?;
            listener.set_nonblocking(true)?;
            bound.push(listener.local_addr()?);
            https.push(listener);
        }

        let (shutdown, _rx) = watch::channel(false);
        let stop = shutdown.clone();
        let devices = Arc::clone(&channel.devices);
        channel.hub.set_live(true);
        let thread = std::thread::Builder::new()
            .name("richos-phone-channel".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[richos] the phone channel could not start its runtime: {e}");
                        channel.hub.set_live(false);
                        return;
                    }
                };
                runtime.block_on(serve_all(channel, tls, https, stop.subscribe()));
                end_runtime(runtime);
            })
            .map_err(|e| PhoneError::Io(format!("could not start the phone channel thread: {e}")))?;

        Ok(Listener { shutdown, thread: Some(thread), bound, devices })
    }

    /// The managed tunnel's final hop. No LAN address or alternate origin is accepted.
    pub fn start_connect(channel: Arc<Channel>, port: u16) -> Result<Self, PhoneError> {
        let listener = bind_listener(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)))?;
        listener.set_nonblocking(true)?;
        let bound = vec![listener.local_addr()?];
        let (shutdown, rx) = watch::channel(false);
        let devices = Arc::clone(&channel.devices);
        let thread = std::thread::Builder::new().name("richos-connect-listener".into()).spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async {
                let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { return };
                let slots = Arc::new(tokio::sync::Semaphore::new(32));
                let drain = Arc::new(Drain::default());
                channel.hub.set_live(true);
                while !*rx.borrow() {
                    let Some((stream, _)) = accept_one(&listener, &rx).await else { continue };
                    let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else { continue };
                    let channel = Arc::clone(&channel);
                    let drain = Arc::clone(&drain);
                    tokio::spawn(async move {
                        let _permit = permit;
                        let service = service_fn(move |request| {
                            let channel = Arc::clone(&channel);
                            let drain = Arc::clone(&drain);
                            async move { Ok::<_, std::convert::Infallible>(handle(channel, drain, request).await) }
                        });
                        if let Err(error) = hyper::server::conn::http1::Builder::new()
                            .timer(hyper_util::rt::TokioTimer::new())
                            .header_read_timeout(std::time::Duration::from_secs(10))
                            .max_buf_size(16 * 1024)
                            .serve_connection(hyper_util::rt::TokioIo::new(stream), service).await
                        {
                            log_connect_connection_error(&error);
                        }
                    });
                }
                // The port is closed; the answers this Mac just released still go out.
                drop(listener);
                drain.settle(std::time::Duration::from_millis(HELD_ANSWER_DRAIN_MS)).await;
                channel.hub.set_live(false);
            });
            end_runtime(runtime);
        })?;
        Ok(Self { shutdown, thread: Some(thread), bound, devices })
    }

    /// Stop serving and wait for the sockets to be gone.
    pub fn stop(&mut self) {
        // A phone being held hears the state it is in now, rather than a connection that drops
        // (Sage's pair-v2 hypotheses review §1: "released … by channel teardown").
        self.devices.release_holds();
        let _ = self.shutdown.send(true);
        if let Some(thread) = self.thread.take() {
            // Joined rather than detached: "the listener stops when he unpairs the last phone" has
            // to mean the port is free when that call returns, or the next pairing fails to bind
            // for reasons nobody can see.
            let _ = thread.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.thread.is_some()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Rebind after a stopped listener's accepted connections enter TIME_WAIT.
/// Unix requires SO_REUSEADDR on both the old and new sockets. It still
/// rejects a second listener while the first is alive; SO_REUSEPORT is absent.
/// Windows keeps its existing exclusive binding behavior.
fn bind_listener(address: SocketAddr) -> std::io::Result<StdTcpListener> {
    #[cfg(unix)]
    {
        use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
        let socket = if address.is_ipv4() {
            tokio::net::TcpSocket::new_v4()?
        } else {
            tokio::net::TcpSocket::new_v6()?
        };
        socket.set_reuseaddr(true)?;
        socket.bind(address)?;
        if unsafe { libc::listen(socket.as_raw_fd(), 128) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { StdTcpListener::from_raw_fd(socket.into_raw_fd()) })
    }
    #[cfg(not(unix))]
    {
        StdTcpListener::bind(address)
    }
}

/// **End a listener thread's runtime without waiting for its route handlers** (hunt 2026-09-29
/// part 1, finding 39). Every route runs under `spawn_blocking` ([`run`]), and dropping a Tokio
/// runtime waits for every blocking task to return (tokio 1.53.1 `Runtime::shutdown_background`
/// doc: "dropping a runtime will block indefinitely for spawned blocking tasks to complete").
/// So [`Listener::stop`], which joins this thread, waited for whatever the phone had asked for
/// last, a voice note being transcribed for up to 90 s among them, and "Forget this phone" waited
/// with it.
///
/// What `stop` promises is kept: the listening sockets are closed inside `block_on`, and the
/// connection tasks are still dropped here (`Runtime::drop` shuts the current-thread scheduler
/// down), so the port is free when `stop` returns. The held answers still get their drain, which
/// is awaited before `block_on` returns. A handler that is still running finishes on its own
/// thread and its answer goes nowhere; the voice route re-checks the paired phone after
/// transcribing, so a phone forgotten meanwhile cannot submit a turn from it.
fn end_runtime(runtime: tokio::runtime::Runtime) {
    runtime.shutdown_background();
}

/// **The one guard that makes a wildcard bind impossible.** Plan §2.5 item 2: *"It binds the LAN
/// interfaces only — never `0.0.0.0` as a shortcut, never an interface it was not told about."*
///
/// A separate function because the only plausible way a wildcard could creep in is an empty
/// address list quietly meaning "all of them", and that has to be testable without building a
/// whole channel.
pub fn check_addresses(addresses: &[IpAddr]) -> Result<(), PhoneError> {
    if addresses.is_empty() {
        return Err(PhoneError::Malformed(
            "this Mac has no address the phone could reach, so there is nothing to listen on".into(),
        ));
    }
    for address in addresses {
        if address.is_unspecified() {
            return Err(PhoneError::Malformed(format!(
                "{address} is every interface at once, which this channel never binds; give it \
                 the addresses this Mac actually answers on"
            )));
        }
        if address.is_multicast() {
            return Err(PhoneError::Malformed(format!("{address} is not an address a server binds")));
        }
    }
    Ok(())
}

/// **The key, in whichever of the two encodings it actually arrived in.**
///
/// MEASURED ON THIS MAC, 2026-09-19, and it is the reason this function exists rather than a
/// `PrivateKeyDer::Pkcs8` at the call site. Counting the PEM labels that
/// `tailscale cert --cert-file - --key-file -` printed for this Mac's real tailnet name:
/// **four `CERTIFICATE` blocks and one `EC PRIVATE KEY` block.**
///
/// `EC PRIVATE KEY` is **SEC1**, not PKCS#8. Wrapped as PKCS#8 — which is what this file did
/// until this line — `ring` refuses it and the whole tailnet configuration fails to build with
/// "rustls refused a private key". `tailnet::KeyDer` has carried the distinction since the parser
/// was written; the TLS side threw it away. Nothing caught it because every test fed it a PKCS#8
/// key minted by our own authority, and the one encoding a real `tailscale cert` produces is the
/// other one.
fn private_key_der(key: &super::tailnet::KeyDer) -> rustls::pki_types::PrivateKeyDer<'static> {
    match key {
        super::tailnet::KeyDer::Pkcs8(der) => {
            rustls::pki_types::PrivateKeyDer::Pkcs8(der.clone().into())
        }
        super::tailnet::KeyDer::Sec1(der) => {
            rustls::pki_types::PrivateKeyDer::Sec1(der.clone().into())
        }
    }
}

/// Turn a leaf (or a chain, leaf first) and its key into something rustls can present.
///
/// Separated from [`tls_config`] because the listener now holds **two** of these and the awkward
/// part — a key that is SEC1 where rustls wants PKCS#8 — is identical for both.
fn certified_key(
    chain_der: Vec<Vec<u8>>,
    key: &super::tailnet::KeyDer,
) -> Result<Arc<rustls::sign::CertifiedKey>, PhoneError> {
    let chain: Vec<rustls::pki_types::CertificateDer<'static>> =
        chain_der.into_iter().map(rustls::pki_types::CertificateDer::from).collect();
    if chain.is_empty() {
        return Err(PhoneError::Crypto("a certificate chain with nothing in it".into()));
    }
    let signing_key = rustls::crypto::ring::sign::any_supported_type(&private_key_der(key))
        .map_err(|e| PhoneError::Crypto(format!("rustls refused a private key: {e}")))?;
    Ok(Arc::new(rustls::sign::CertifiedKey::new(chain, signing_key)))
}

/// **Which certificate this Mac presents, decided by the name the phone asked for.**
///
/// The Tailscale path (CEO §61, reach document §2.3) gives this Mac a name —
/// `mm1.<tailnet>.ts.net` — with a **publicly trusted** certificate, and that is what the phone
/// meets. It is the only name the product ever hands out.
///
/// # The Mac's own leaf is the fallback for every OTHER name, and that is a safety property
///
/// [`resolve`](CertDesk::resolve) returns the tailnet key **only** for an exact,
/// case-insensitive match on the tailnet name, and the Mac-CA leaf for everything else — another
/// name, a bare IP address (which carries no SNI at all), or a client too old to send one.
/// Nothing the product does points anything at those, so what the fallback buys is that a
/// connection which arrives anyway gets a handshake and a 404 rather than a TLS alert nobody can
/// read. The same authority is what the six words on the pairing screen name, which is why it
/// outlived the path that once asked a phone to install it (CEO §61).
pub struct CertDesk {
    /// The Mac's own leaf, from [`super::ca`]. Always present: the listener does not start
    /// without it, and it is what anything that does not ask for the tailnet name is served.
    own: Arc<rustls::sign::CertifiedKey>,
    /// The tailnet name, normalized, and the publicly trusted chain for it. `None` until Tailscale
    /// is signed in, certificates are enabled for the tailnet, and `tailscale cert` has answered.
    tailnet: Option<(String, Arc<rustls::sign::CertifiedKey>)>,
}

impl CertDesk {
    /// The name this desk will answer to with the tailnet certificate, if it has one.
    pub fn tailnet_name(&self) -> Option<&str> {
        self.tailnet.as_ref().map(|(name, _)| name.as_str())
    }
}

impl std::fmt::Debug for CertDesk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No key material, ever, in a formatter. The name is the only interesting part.
        f.debug_struct("CertDesk").field("tailnet", &self.tailnet_name()).finish()
    }
}

impl rustls::server::ResolvesServerCert for CertDesk {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        if let (Some(asked), Some((name, key))) = (client_hello.server_name(), &self.tailnet) {
            // ASCII-case-insensitive, because a DNS name is, and the phone is not the only thing
            // that will ever open this socket. `super::tailnet::normalize_name` already lowercased
            // our side.
            if asked.eq_ignore_ascii_case(name) {
                return Some(Arc::clone(key));
            }
        }
        Some(Arc::clone(&self.own))
    }
}

/// Build the TLS configuration from the leaf the certificate authority issued, with no tailnet
/// certificate beside it.
///
/// **No production caller** since CEO §61: [`super::PhoneRuntime::start`] does not start a
/// channel without a tailnet certificate. It is the shape the unit tests in this file need — a
/// real `ServerConfig` over a real leaf, with no Tailscale on the machine running them.
pub fn tls_config(
    leaf_der: &[u8],
    leaf_key_pkcs8: &[u8],
) -> Result<Arc<rustls::ServerConfig>, PhoneError> {
    tls_config_with_tailnet(leaf_der, leaf_key_pkcs8, None)
}

/// [`tls_config`], plus the publicly trusted certificate for this Mac's tailnet name.
///
/// `tailnet` is `(name, chain leaf-first, PKCS#8 key)`. The chain has more than one member here
/// and exactly one in the home case: `tailscale cert` returns a leaf **and its issuing
/// intermediate**, and a public chain that omits the intermediate is the classic "works in curl,
/// fails on a phone" certificate — desktop trust stores often cache intermediates and a freshly
/// wiped phone does not.
pub fn tls_config_with_tailnet(
    leaf_der: &[u8],
    leaf_key_pkcs8: &[u8],
    tailnet: Option<(String, Vec<Vec<u8>>, super::tailnet::KeyDer)>,
) -> Result<Arc<rustls::ServerConfig>, PhoneError> {
    // The same `ring` provider `voice_provision.rs` installs, through the same idempotent function,
    // for the same reason it does: a process-level provider must exist before a config is built, and
    // depending on which unrelated feature ran first is a defect waiting for one first run.
    crate::voice_provision::ensure_crypto_provider();

    // The ROOT is deliberately not in this chain. Nothing that reaches this leaf trusts the
    // root anyway — §61 removed the path that put it on a phone — so sending it would be a
    // bigger handshake that proves nothing.
    let own = certified_key(
        vec![leaf_der.to_vec()],
        &super::tailnet::KeyDer::Pkcs8(leaf_key_pkcs8.to_vec()),
    )
    .map_err(|e| PhoneError::Crypto(format!("rustls refused our own leaf: {e}")))?;

    let tailnet = match tailnet {
        Some((name, chain, key)) => {
            Some((super::tailnet::normalize_name(&name), certified_key(chain, &key)?))
        }
        None => None,
    };

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(CertDesk { own, tailnet }));
    // HTTP/1.1 only, matching plan §2.6's *"No WebSocket on either side. Upgrade later only if a
    // measurement demands it"* — one protocol, one code path.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

async fn serve_all(
    channel: Arc<Channel>,
    tls: Arc<rustls::ServerConfig>,
    https: Vec<StdTcpListener>,
    shutdown: watch::Receiver<bool>,
) {
    let acceptor = tokio_rustls::TlsAcceptor::from(tls);
    let drain = Arc::new(Drain::default());
    let mut tasks = Vec::new();
    for listener in https {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { continue };
        tasks.push(tokio::spawn(accept_https(
            listener,
            acceptor.clone(),
            Arc::clone(&channel),
            Arc::clone(&drain),
            shutdown.clone(),
        )));
    }
    for task in tasks {
        let _ = task.await;
    }
    // Every port is closed by now (each accept loop owned its socket). What remains is the held
    // answers this Mac just released, which are waited for and nothing else.
    drain.settle(std::time::Duration::from_millis(HELD_ANSWER_DRAIN_MS)).await;
    channel.hub.set_live(false);
}

/// Race one `accept()` against the shutdown signal, with no macro and no polling.
///
/// The `borrow()` check at the top is not decoration: a fresh `watch::Receiver` clone inherits the
/// version it was cloned from, so if shutdown had already been sent, `changed()` would wait for a
/// send that is never coming. Reading the current value first is what closes that window.
async fn accept_one(
    listener: &tokio::net::TcpListener,
    shutdown: &watch::Receiver<bool>,
) -> Option<(tokio::net::TcpStream, SocketAddr)> {
    if *shutdown.borrow() {
        return None;
    }
    let accepting = Box::pin(listener.accept());
    let mut rx = shutdown.clone();
    let stopping = Box::pin(async move {
        let _ = rx.changed().await;
    });
    match select(accepting, stopping).await {
        Either::Left((Ok(pair), _)) => Some(pair),
        // `None` for BOTH "that accept failed" and "we are shutting down", and the caller tells
        // them apart by reading the shutdown value again. One failed accept is not the end of a
        // listener — a client that vanished between the kernel queueing it and us taking it is
        // ordinary — so the loop continues in that case and returns in the other.
        Either::Left((Err(_), _)) | Either::Right(_) => None,
    }
}

async fn accept_https(
    listener: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    channel: Arc<Channel>,
    drain: Arc<Drain>,
    shutdown: watch::Receiver<bool>,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(32));
    while !*shutdown.borrow() {
        let Some((stream, _peer)) = accept_one(&listener, &shutdown).await else {
            if *shutdown.borrow() {
                return;
            }
            continue;
        };
        let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else { continue };
        let acceptor = acceptor.clone();
        let channel = Arc::clone(&channel);
        let drain = Arc::clone(&drain);
        tokio::spawn(async move {
            // A handshake that fails is a device on his Wi-Fi that does not hold the certificate
            // authority. Nothing is logged per connection: a TV that probes the port every minute
            // would otherwise fill his log with noise.
            let _permit = permit;
            let Ok(Ok(tls_stream)) = tokio::time::timeout(std::time::Duration::from_secs(10), acceptor.accept(stream)).await else { return };
            let io = hyper_util::rt::TokioIo::new(tls_stream);
            let service = service_fn(move |request| {
                let channel = Arc::clone(&channel);
                let drain = Arc::clone(&drain);
                async move { Ok::<_, std::convert::Infallible>(handle(channel, drain, request).await) }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .timer(hyper_util::rt::TokioTimer::new())
                .header_read_timeout(std::time::Duration::from_secs(10))
                .max_buf_size(16 * 1024).serve_connection(io, service).await;
        });
    }
}


/// A RichOS Connect connection that ends in an error — a client that hung up mid-request, a
/// header that never finished inside its ten seconds — is routine on a public edge, and one
/// line per connection would let anyone on the internet fill his log. So it is said at most
/// once per ten seconds, under the same bound `routes::log_refusal` puts on refused requests.
fn log_connect_connection_error(error: &hyper::Error) {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<std::time::Instant>>> = std::sync::OnceLock::new();
    let mut last = LAST.get_or_init(|| std::sync::Mutex::new(None)).lock().unwrap();
    if super::routes::refusal_log_due(&mut last, std::time::Instant::now()) {
        eprintln!("[richos] a RichOS Connect connection ended with an error: {error} (sampled)");
    }
}

async fn handle(channel: Arc<Channel>, drain: Arc<Drain>, request: Request<HyperBody>) -> Response<BoxBody> {
    let method = request.method().as_str().to_string();
    // Authenticated signatures cover the wire path. Decode an audio ID only after
    // verification in its route; decoding here changes signed %3A into ':' first.
    let wire_path = request.uri().path();
    let path = if wire_path.starts_with("/api/") { wire_path.to_string() } else { percent_decode(wire_path) };
    let query = request.uri().query().unwrap_or("").to_string();
    let header = |name: &str| {
        request.headers().get(name).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
    };
    let authorization = header("authorization");
    let last_event_id = header("last-event-id");
    let content_type = header("content-type");
    // `pair-wait` (Sage's pair-v2 hypotheses review §1): how long the phone asked this Mac to hold
    // the answer. A request that asked is counted by the drain from here, before its answer is
    // computed, so a teardown that lands while it is being dispatched still waits for it.
    let wait = hold_seconds(header("prefer").as_deref());
    let guard = wait.map(|_| drain.enter());

    // THE LIMIT IS APPLIED BEFORE THE BODY IS READ, which is the difference between a limit and a
    // check. `Limited` fails the read rather than buffering four gigabytes and then measuring it.
    let (body_limit, upload_seconds) =
        read_limits(&channel, &method, &path, &query, content_type.as_deref(), authorization.as_deref());
    let body = match tokio::time::timeout(std::time::Duration::from_secs(upload_seconds), Limited::new(request.into_body(), body_limit).collect()).await {
        Ok(Ok(collected)) => collected.to_bytes().to_vec(),
        _ => return render(&channel, Outcome::PayloadTooLarge),
    };

    let incoming =
        Arc::new(Incoming { method, path, query, authorization, last_event_id, content_type, body });
    // Read BEFORE the request runs, so a press that lands between its answer and the hold below
    // ends the hold at once rather than being missed for fourteen seconds.
    let since = channel.devices.hold_generation();
    let mut outcome = run(&channel, &incoming).await;

    // **THE HOLD** — Sage's pair-v2 hypotheses review §1, "The fix" point 2. Only the phone's own
    // signed "They match" answered "still waiting" is ever held (`routes::holdable`), at most one
    // at a time (`DeviceDesk::hold_answer`), and for at most `PAIR_WAIT_MAX_SECONDS`. No blocking
    // thread is held: this is one async task waiting on the desk's release signal.
    if let Some(seconds) = wait.filter(|_| super::routes::holdable(&incoming, &outcome)) {
        let ended = tokio::time::timeout(
            std::time::Duration::from_secs(seconds),
            channel.devices.hold_answer(since),
        )
        .await;
        // Something changed — the press, a refusal, an expiry, a teardown — so the request is
        // run again and the phone is sent THAT answer. A hold that was superseded or ran its full
        // time is "still waiting", as before, unless a release landed in its last instant.
        let released = matches!(ended, Ok(super::device::HoldEnd::Released))
            || channel.devices.hold_generation() != since;
        if released {
            outcome = run(&channel, &incoming).await;
        }
    }
    match (outcome, guard) {
        // A stream is long-lived; the drain never waits on one.
        (Outcome::Stream { opening, mark, device, .. }, _) => open_stream(channel, &device, opening, mark),
        (other, Some(guard)) => {
            render(&channel, other).map(|inner| GuardedBody { inner, _guard: guard }.boxed())
        }
        (other, None) => render(&channel, other),
    }
}

/// Run the route table off the async thread: it signs, verifies and may write to disk.
async fn run(channel: &Arc<Channel>, incoming: &Arc<Incoming>) -> Outcome {
    let (target, request) = (Arc::clone(channel), Arc::clone(incoming));
    tokio::task::spawn_blocking(move || dispatch(&target, &request)).await.unwrap_or(Outcome::NotFound)
}

/// **How many bytes, and how long, this request's body may take — decided before one is read.**
///
/// The route's own limits ([`super::routes::body_limit`], [`super::routes::upload_seconds`])
/// say what a voice note or a file may be. Sage's pairing review F3: they were granted from the
/// request line alone, so on the public Connect route anybody could make this Mac hold about
/// 1.92 GB (32 slots × 60 MB) and every slot for up to 300 s. A large limit is now granted only
/// to a credential naming the paired, confirmed device with a live challenge
/// ([`super::device::DeviceDesk::admits_large_body`]); everything else gets
/// [`super::MAX_BODY_BYTES`] (64 KiB) and 15 s, and a larger body is refused 413 unread.
fn read_limits(
    channel: &Channel,
    method: &str,
    path: &str,
    query: &str,
    content_type: Option<&str>,
    authorization: Option<&str>,
) -> (usize, u64) {
    let limit = super::routes::body_limit(method, path, query, content_type);
    if limit <= super::MAX_BODY_BYTES || !channel.devices.admits_large_body(authorization) {
        return (limit.min(super::MAX_BODY_BYTES), 15);
    }
    (limit, super::routes::upload_seconds(method, path, query, content_type))
}

/// Turn a described response into an HTTP one, with a fresh challenge attached.
///
/// **Every response carries `X-RichOS-Challenge`, including a 404 and a 429.** That is how the
/// phone gets the next thing to sign (`web/web-app/lib/api.js` reads the header on every response),
/// and it is why the contract needs no challenge route. A refusal that omitted it would leave a
/// phone whose challenge had aged out with no way back.
fn render(channel: &Channel, outcome: Outcome) -> Response<BoxBody> {
    let challenge = channel.devices.issue_challenge().ok();
    render_with(outcome, challenge)
}

fn render_with(outcome: Outcome, challenge: Option<String>) -> Response<BoxBody> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(outcome.status()).unwrap_or(StatusCode::NOT_FOUND))
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        // The phone app is same-origin with the API today. The header is here because the
        // reachability seam (plan §10.7) means the API base can move to another origin later, and
        // `*` is refused even now: a credentialed request from any page on the internet is exactly
        // what this must not allow. `null` is what a same-origin fetch needs and nothing more.
        .header("access-control-allow-origin", "null");
    if let Some(challenge) = challenge {
        builder = builder.header("x-richos-challenge", challenge);
    }
    let body = match outcome {
        Outcome::Json { body, .. } => {
            builder = builder.header("content-type", "application/json; charset=utf-8");
            Bytes::from(body)
        }
        Outcome::Bytes { content_type, body, download_as, .. } => {
            builder = builder.header("content-type", content_type);
            if let Some(name) = download_as {
                builder =
                    builder.header("content-disposition", format!("attachment; filename=\"{name}\""));
            }
            Bytes::from(body)
        }
        // A flat refusal carries no body at all: one answer to every question.
        Outcome::NotFound | Outcome::PayloadTooLarge => Bytes::new(),
        // The one refusal with a body, because the phone acts on it: it clears its credential and
        // stops instead of retrying against a Mac that has forgotten it.
        Outcome::Revoked => {
            builder = builder.header("content-type", "application/json; charset=utf-8");
            Bytes::from_static(b"{\"revoked\":true}")
        }
        Outcome::RateLimited => {
            builder = builder.header("retry-after", "60");
            Bytes::new()
        }
        // Sage F1: the key's holder is told the pairing is waiting for the press on this Mac, and
        // told it in a body every client reads as "try again" rather than as a final refusal.
        Outcome::AwaitingMac => {
            builder = builder.header("content-type", "application/json; charset=utf-8");
            Bytes::from_static(super::routes::AWAITING_MAC_BODY.as_bytes())
        }
        Outcome::Stream { .. } => Bytes::new(),
    };
    builder
        .body(Full::new(body).boxed())
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

// The HTTP response owns presence. A producer waiting for a heartbeat can outlive a
// closed socket; letting that task own the slot suppresses the phone's next push.
struct StreamBody {
    inner: BoxBody,
    _slot: super::device::StreamSlot,
}
impl hyper::body::Body for StreamBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)
        -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        std::pin::Pin::new(&mut self.inner).poll_frame(cx)
    }
}

/// Open an event stream: write the opening frames, then follow the hub until the phone goes away
/// or the channel stops.
///
/// `mark` is the hub position the opening frames were prepared at. The stream subscribes FROM
/// there ([`super::stream::PhoneHub::subscribe_from`]): whatever was published between preparing
/// the opening and this subscription is sent after it, once, and everything later follows live.
/// Subscribing afterwards instead lost that window (hunt 2026-09-29, part 1, finding 13).
///
/// `device` is the authenticated device the stream is for. Its older streams are ended when this
/// one is admitted ([`super::device::DeviceDesk::claim_stream`], 2026-10-01 iPhone walk D4): the
/// producer of a replaced stream stops at `ended`, which drops its sender and ends its response.
fn open_stream(channel: Arc<Channel>, device: &str, opening: Vec<String>, mark: u64) -> Response<BoxBody> {
    let slot = match channel.devices.claim_stream(device) {
        Ok(slot) => slot,
        Err(_) => return render(&channel, Outcome::RateLimited),
    };
    let ended = slot.ended();
    let (mut sender, body) =
        http_body_util::channel::Channel::<Bytes, std::convert::Infallible>::new(32);
    // `None`: the position can no longer be answered exactly (the hub's memory rolled past it).
    // The phone is sent the opening, told to start again, and the stream ends.
    let (missed, live) = match channel.hub.subscribe_from(mark) {
        Ok((missed, live)) => (missed, Some(live)),
        Err(()) => (Vec::new(), None),
    };

    let produce = async move {
        // **THE FIRST BYTES GO OUT THE MOMENT THE STREAM IS ACCEPTED, WHATEVER THE OPENING HOLDS**
        // (2026-10-01, isaac-opus-reopen1's headless-lab measurement: a reply 15.3 s after the
        // iPhone app reopened). A resume whose `since` names nothing held (the last frame was the
        // `hello` right after pairing, id 0, or the hub's cursor sits above what it holds) is
        // answered with an EMPTY opening. The head left at once, but no body byte did until the first
        // keep-alive, `KEEPALIVE_MS` = 15,000 ms later, and the iPhone's `URLSession` hands its core
        // the response only with the first body bytes: the lab logged "answered 200 after 15059 ms",
        // and for those 15 s the phone was not connected, so a waiting message was not sent and a
        // waiting reply was not shown. One keep-alive written first puts a body chunk on the wire
        // with the head on every stream, so no client, transport or proxy depends on which trigger
        // it hit or on whether it reports a response before its first body bytes. It is a
        // comment in the vocabulary every client already reads and ignores (iPhone `SSEParser`
        // returns it apart from events and acts only on `re-snapshot`; Android `SseParser` yields
        // `SseItem.KeepAlive`, which `RichCore` drops; `EventSource` discards comment lines).
        let first = std::iter::once(keepalive_comment());
        for frame in first.chain(opening).chain(missed.iter().map(|f| f.to_wire())) {
            if sender.send_data(Bytes::from(frame)).await.is_err() {
                return;
            }
        }
        let Some(mut live) = live else {
            sender.send_data(Bytes::from(": re-snapshot 0\n\n")).await.ok();
            return;
        };
        loop {
            // `timeout` rather than a three-way `select!`: the keep-alive IS the timeout, and
            // shutdown needs no arm here because dropping the channel's runtime ends this task.
            match tokio::time::timeout(std::time::Duration::from_millis(KEEPALIVE_MS), live.recv())
                .await
            {
                Ok(Ok(frame)) => {
                    if sender.send_data(Bytes::from(frame.to_wire())).await.is_err() {
                        return;
                    }
                }
                // The phone fell far enough behind that frames were dropped. It is told to start
                // again rather than handed a thinned stream: a gap it cannot see is the one
                // failure this whole design refuses.
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(missed))) => {
                    let notice = format!(": re-snapshot {missed}\n\n");
                    let _ = sender.send_data(Bytes::from(notice)).await;
                    return;
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => return,
                Err(_elapsed) => {
                    // A comment, not an event: it keeps the connection warm without inventing
                    // anything for the phone to render, and it never wakes the Mac — it only
                    // writes to a socket that is already open.
                    if sender.send_data(Bytes::from(keepalive_comment())).await.is_err() {
                        return;
                    }
                }
            }
        }
    };
    // Whichever comes first: the stream ending by itself, or a newer stream from the same device
    // replacing it. Polled by hand because this crate's tokio has no `macros` feature for
    // `select!`. Either way `produce` is dropped here, and its sender with it, which ends the body.
    tokio::spawn(async move {
        use std::future::Future;
        let mut produce = std::pin::pin!(produce);
        let mut ended = std::pin::pin!(ended);
        std::future::poll_fn(|cx| {
            if ended.as_mut().poll(cx).is_ready() {
                return std::task::Poll::Ready(());
            }
            produce.as_mut().poll(cx)
        })
        .await;
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .header("access-control-allow-origin", "null")
        // Named because a reverse proxy in front of this would buffer an event stream into
        // uselessness. There is no proxy today and this design refuses to acquire one, so the
        // header is a note to whoever later thinks about putting one there.
        .header("x-accel-buffering", "no")
        .body(StreamBody { inner: body.boxed(), _slot: slot }.boxed())
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

/// The keep-alive comment: written first on every accepted stream, and after every
/// `KEEPALIVE_MS` without a frame. A comment, never an event, so no client renders anything.
fn keepalive_comment() -> String {
    format!(": keep-alive {}\n\n", super::now_millis())
}

/// Percent-decode a path.
///
/// Written out rather than adding `percent-encoding` as a direct dependency: it is a dozen lines,
/// and the only thing this needs is a path or one query value. **A malformed escape is left alone
/// rather than guessed at** — and it does not matter either way, because a credential-free `GET`
/// resolves by exact match against two fixed paths (`routes::retired_page`), so a decode that
/// produced a `..` component would name nothing and get the same flat 404.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percent_escape_is_decoded_and_a_broken_one_is_left_alone() {
        assert_eq!(percent_decode("/app.js"), "/app.js");
        assert_eq!(percent_decode("/%2e%2e/private.txt"), "/../private.txt");
        assert_eq!(percent_decode("/a%20b.js"), "/a b.js");
        assert_eq!(percent_decode("RichOS-Device%20dev_a.CH.AAAA"), "RichOS-Device dev_a.CH.AAAA");
        // Broken escapes: left as they are, and the lookup misses on them either way.
        assert_eq!(percent_decode("/a%2"), "/a%2");
        assert_eq!(percent_decode("/a%zz"), "/a%zz");
        assert_eq!(percent_decode("%"), "%");
    }

    #[test]
    fn decoding_happens_before_the_lookup_so_an_escaped_dot_dot_is_still_refused() {
        // The pair that matters: this function turns `%2e%2e` into `..`, and the route layer then
        // has nothing by that name. Either half alone would let `/%2e%2e/private.txt` through —
        // decoding after the lookup would let the escaped form reach a later consumer intact.
        let decoded = percent_decode("/%2e%2e/private.txt");
        assert_eq!(decoded, "/../private.txt");
        assert!(decoded.split('/').any(|part| part == ".."));
        // A credential-free GET matches two fixed paths exactly (`routes::retired_page`), so the
        // decoded path is refused by not existing rather than by inspection.
        for path in ["/../private.txt", decoded.as_str()] {
            assert!(!matches!(path, "/" | "/index.html" | "/sw.js"), "{path} names a served path");
        }
    }

    #[test]
    fn the_port_is_the_one_the_plan_pinned() {
        // The port is part of the origin (plan §2.1), so changing it means the phone app has to
        // be re-installed. It is asserted so a change is a decision.
        //
        // THERE IS ONE PORT NOW. The neighboring one served the trust page, and CEO §61 removed
        // the path that needed it — see `Listener::start`.
        assert_eq!(HTTPS_PORT, 8443);
    }

    /// **SAGE F3: A LARGE BODY IS GRANTED BEFORE AUTHENTICATION ONLY TO THE PAIRED, CONFIRMED
    /// DEVICE.** His test, as he wrote it: an unsigned `POST /api/messages` with `Content-Type:
    /// audio/wav` and a 1 MiB body is answered 413, and so is one signed by an unknown device id;
    /// the same upload signed by the active device is read in full and reaches the route. Over the
    /// real Connect listener (plain HTTP on loopback, the public route's last hop). On `main`
    /// before this change the unsigned upload was granted 60,000,000 bytes and 120 s.
    #[test]
    fn a_large_body_is_read_only_for_the_confirmed_device_and_refused_unread_for_anyone_else() {
        use crate::phone::{api_base::ApiBaseDesk, device::{DeviceDesk, PairedVia, Platform, PublicKeyForm, signing_string}, routes::{Bridge, Accepted, StopSwitch}, stream::PhoneHub};
        use std::io::{Read as _, Write as _};
        struct Quiet;
        impl Bridge for Quiet {
            fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> { Err("unused".into()) }
            fn snapshot(&self, _: Option<&str>) -> Result<serde_json::Value, String> { Ok(serde_json::json!({})) }
            fn current_thread(&self) -> Option<(String, String)> { None }
            fn threads(&self) -> Vec<(String, String)> { vec![] }
        }
        let dir = std::env::temp_dir().join(format!("f3-limits-{}-{}", std::process::id(), super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let phone = crate::phone::device::tests::Phone::new();
        let window = devices.open_pairing().unwrap();
        let device = devices.complete_pairing(&window.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        let challenge = devices.issue_challenge().unwrap();
        let channel = Arc::new(Channel {
            devices: Arc::clone(&devices), rejected: StopSwitch::unwired(),
            api_base: Arc::new(ApiBaseDesk::only("https://example.invalid")), hub: PhoneHub::new(), bridge: Arc::new(Quiet),
           vapid_public: String::new(), fingerprint_hex: String::new(),
            pairing_path: std::sync::Mutex::new(PairedVia::CONNECT),
        });
        let credential = |id: &str, body: &[u8]| {
            let sig = super::super::b64url(&phone.sign(&signing_string(&challenge, "POST", "/api/messages", body)));
            format!("RichOS-Device {id}.{challenge}.{sig}")
        };
        let small = (super::super::MAX_BODY_BYTES, 15);
        let wav = Some("audio/wav");

        // The decision, case by case.
        assert_eq!(read_limits(&channel, "POST", "/api/messages", "", wav, None), small, "unsigned");
        assert_eq!(read_limits(&channel, "POST", "/api/messages", "kind=attachment", Some("image/jpeg"), None), small, "unsigned file");
        assert_eq!(read_limits(&channel, "POST", "/api/messages", "", wav, Some(&credential("dev_000000000000", b""))), small, "unknown device");
        assert_eq!(read_limits(&channel, "POST", "/api/messages", "", wav, Some(&credential(&device.id, b""))), small, "a device nobody confirmed on the Mac");
        devices.confirm_on_mac().unwrap();
        assert_eq!(
            read_limits(&channel, "POST", "/api/messages", "", wav, Some(&credential(&device.id, b""))),
            (super::super::voice::MAX_UPLOAD, 120),
            "the confirmed device was not granted its voice note"
        );
        assert_eq!(
            read_limits(&channel, "POST", "/api/messages", "kind=attachment", Some("image/jpeg"), Some(&credential(&device.id, b""))),
            (super::super::attachments::MAX_FILE_BYTES, super::super::attachments::UPLOAD_SECONDS)
        );
        let invented = format!("RichOS-Device {}.{}.AAAA", device.id, super::super::b64url(&[9u8; 24]));
        assert_eq!(read_limits(&channel, "POST", "/api/messages", "", wav, Some(&invented)), small, "a challenge this Mac never issued");

        // And over the wire.
        let mut listener = Listener::start_connect(Arc::clone(&channel), 0).expect("the Connect listener did not start");
        let port = listener.bound[0].port();
        let body = vec![0u8; 1024 * 1024];
        let post = |authorization: Option<String>| -> u16 {
            let mut socket = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            socket.set_write_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
            let mut head = format!("POST /api/messages HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n", body.len());
            if let Some(a) = authorization { head.push_str(&format!("Authorization: {a}\r\n")); }
            head.push_str("\r\n");
            let _ = socket.write_all(head.as_bytes());
            // The server may stop reading at 64 KiB and close; a write that fails then is the point.
            for chunk in body.chunks(16 * 1024) {
                if socket.write_all(chunk).is_err() { break; }
            }
            let mut raw = Vec::new();
            let _ = socket.read_to_end(&mut raw);
            let text = String::from_utf8_lossy(&raw);
            text.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
        };
        assert_eq!(post(None), 413, "an unsigned 1 MiB upload was not refused");
        assert_eq!(post(Some(credential("dev_000000000000", &body))), 413, "an unknown device's 1 MiB upload was not refused");
        let accepted = post(Some(credential(&device.id, &body)));
        assert_ne!(accepted, 413, "the confirmed device's upload was refused");
        assert_ne!(accepted, 0, "the confirmed device's upload got no answer");
        listener.stop();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// **STOPPING THE LISTENER DOES NOT WAIT FOR A REQUEST STILL BEING HANDLED** (hunt
    /// 2026-09-29 part 1, finding 39). Route handlers run under `spawn_blocking`, and dropping a
    /// Tokio runtime waits for every blocking task to return, so "Forget this phone" waited for
    /// whatever the phone had asked for last: a voice note can be transcribing for up to 90 s.
    ///
    /// Over the real Connect listener: the confirmed phone sends a message, and the bridge
    /// holds it (a stand-in for the transcriber) until the test lets go. Stop is given 8 s,
    /// eight times the 1 s held-answer drain; a listener that waits for the handler cannot
    /// return inside it. What `stop` promises is kept: the port is free when it returns.
    #[test]
    fn stopping_the_listener_does_not_wait_for_a_request_still_being_handled() {
        use crate::phone::{api_base::ApiBaseDesk, device::{DeviceDesk, PairedVia, Platform, PublicKeyForm, signing_string}, routes::{Bridge, Accepted, StopSwitch}, stream::PhoneHub};
        use std::io::{Read as _, Write as _};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Condvar, Mutex};
        type Gate = Arc<(Mutex<bool>, Condvar)>;
        struct Held { entered: Arc<AtomicBool>, gate: Gate }
        impl Bridge for Held {
            fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> {
                self.entered.store(true, Ordering::SeqCst);
                let (open, changed) = &*self.gate;
                let mut open = open.lock().unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
                while !*open {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() { break; }
                    open = changed.wait_timeout(open, left).unwrap().0;
                }
                Err("the test let this request go".into())
            }
            fn snapshot(&self, _: Option<&str>) -> Result<serde_json::Value, String> { Ok(serde_json::json!({})) }
            fn current_thread(&self) -> Option<(String, String)> { None }
            fn threads(&self) -> Vec<(String, String)> { vec![] }
        }
        let dir = std::env::temp_dir().join(format!("stop-held-{}-{}", std::process::id(), super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let phone = crate::phone::device::tests::Phone::new();
        let window = devices.open_pairing().unwrap();
        let device = devices.complete_pairing(&window.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        devices.confirm_on_mac().unwrap();
        let challenge = devices.issue_challenge().unwrap();
        let entered = Arc::new(AtomicBool::new(false));
        let gate: Gate = Arc::new((Mutex::new(false), Condvar::new()));
        let channel = Arc::new(Channel {
            devices: Arc::clone(&devices), rejected: StopSwitch::unwired(),
            api_base: Arc::new(ApiBaseDesk::only("https://example.invalid")), hub: PhoneHub::new(),
            bridge: Arc::new(Held { entered: Arc::clone(&entered), gate: Arc::clone(&gate) }),
           vapid_public: String::new(), fingerprint_hex: String::new(),
            pairing_path: std::sync::Mutex::new(PairedVia::CONNECT),
        });
        let mut listener = Listener::start_connect(Arc::clone(&channel), 0).expect("the Connect listener did not start");
        let port = listener.bound[0].port();
        assert!(bind_listener(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_err(),
                "reuse of a retired socket must not permit two live listeners");
        #[cfg(unix)]
        assert_eq!(listening_fds_in_this_process(port).map(|fds| fds.len()), Ok(1),
                   "the descriptor scan does not see the running listener, so its empty answer after stop would prove nothing");
        let body = br#"{"client_id":"held-1","text":"hello from the phone"}"#.to_vec();
        let sig = super::super::b64url(&phone.sign(&signing_string(&challenge, "POST", "/api/messages", &body)));
        let authorization = format!("RichOS-Device {}.{challenge}.{sig}", device.id);
        let sender = std::thread::spawn(move || {
            let mut socket = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(70))).unwrap();
            let head = format!(
                "POST /api/messages HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nAuthorization: {authorization}\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).unwrap();
            socket.write_all(&body).unwrap();
            // The connection is dropped when the listener stops; how it ends is not the test.
            let mut raw = Vec::new();
            socket.read_to_end(&mut raw).ok();
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !entered.load(Ordering::SeqCst) {
            assert!(std::time::Instant::now() < deadline, "the phone's message never reached the bridge");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let (done, stopped) = std::sync::mpsc::channel();
        let began = std::time::Instant::now();
        std::thread::spawn(move || {
            listener.stop();
            done.send(()).ok();
        });
        let returned = stopped.recv_timeout(std::time::Duration::from_secs(8)).is_ok();
        let took = began.elapsed();
        // Use the same socket options as the next real pairing. A default
        // socket also rejects retired accepted connections in TIME_WAIT.
        let port_free = if returned {
            port_released_by_stop(port, || bind_listener(SocketAddr::from((Ipv4Addr::LOCALHOST, port))))
        } else {
            Ok(())
        };
        {
            let (open, changed) = &*gate;
            *open.lock().unwrap() = true;
            changed.notify_all();
        }
        sender.join().unwrap();
        assert!(returned, "stopping the listener was still waiting after {took:?} for a request it was handling");
        if let Err(why) = port_free {
            panic!("the port was still held when stop returned: {why}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// How long a port may stay bound by a child process that is still being started.
    ///
    /// **WHY THERE IS A BOUND AT ALL** (`stopping_the_listener_does_not_wait_for_a_request_still_being_handled`
    /// failed twice in full `cargo test --bin richos-tauri` merge gates on 2026-10-08, and passed
    /// alone). Starting a child copies the parent's whole descriptor table into it, and the copy
    /// lets go of the close-on-exec descriptors only when the child's `exec` completes. A listening
    /// socket this process closes inside that window stays bound to its port until then, although
    /// nothing in this process holds it any more. Other tests in the same binary start children
    /// (`/bin/sleep`, `/bin/kill`, `lsof`, helper scripts), so a test that rebinds the instant
    /// `stop` returns loses that race now and then. Measured on this Mac (2026-10-08) by
    /// `a_port_stop_released_can_stay_bound_while_another_thread_starts_a_child`: instant rebinds
    /// after the real Connect listener's `stop` were refused 0 of 300 times with no child starting
    /// and 28 of 300 beside four threads starting children, and in every refused round the
    /// descriptor scan found no socket of this process on the port. A one-off probe timed the
    /// refusals: every port was free again within 1 ms (the longest 953 µs of 30 refusals).
    /// macOS gives no way to take a port back from
    /// that copy: `SHUT_RDWR` on a listening socket returns `ENOTCONN` there. Two seconds is the
    /// margin for a heavily loaded Mac, and far below how long a socket `stop` really leaked
    /// would hold the port.
    const CHILD_START_HOLD_BOUND: std::time::Duration = std::time::Duration::from_secs(2);

    /// **THE PORT IS RELEASED BY `stop`: checked as what `stop` controls, then as what the next
    /// pairing sees.** First, with no timing in it, no socket left in THIS process listens on
    /// the port. Then `rebind` must succeed within [`CHILD_START_HOLD_BOUND`], which only a child
    /// still being started by another thread can stretch past the first try.
    fn port_released_by_stop(port: u16, rebind: impl Fn() -> std::io::Result<StdTcpListener>) -> Result<(), String> {
        #[cfg(unix)]
        {
            let ours = listening_fds_in_this_process(port)?;
            if !ours.is_empty() {
                return Err(format!("this process still has a socket listening on port {port} (descriptors {ours:?})"));
            }
        }
        let began = std::time::Instant::now();
        loop {
            match rebind() {
                Ok(_) => return Ok(()),
                Err(error) if began.elapsed() >= CHILD_START_HOLD_BOUND => {
                    return Err(format!(
                        "no socket in this process listens on port {port}, yet it could not be bound again for {:?}: {error}",
                        began.elapsed()
                    ))
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        }
    }

    /// The descriptors of this process that are TCP sockets bound to `port` with no peer: a
    /// listener, or anything else that would refuse the next bind. (`SO_ACCEPTCONN`, the direct
    /// question, answers `ENOPROTOOPT` on macOS.)
    #[cfg(unix)]
    fn listening_fds_in_this_process(port: u16) -> Result<Vec<i32>, String> {
        let entries = std::fs::read_dir("/dev/fd").map_err(|e| format!("could not list this process's descriptors: {e}"))?;
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let Some(fd) = entry.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else { continue };
            // SAFETY: a zeroed sockaddr_storage is valid, and both calls write at most the length
            // passed. A descriptor closed or reused by another thread meanwhile only fails the
            // call or describes that other socket.
            let mut address: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut length = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            if unsafe { libc::getsockname(fd, (&mut address as *mut libc::sockaddr_storage).cast(), &mut length) } != 0 {
                continue;
            }
            let bound = match i32::from(address.ss_family) {
                libc::AF_INET => u16::from_be(unsafe { (*(&address as *const libc::sockaddr_storage).cast::<libc::sockaddr_in>()).sin_port }),
                libc::AF_INET6 => u16::from_be(unsafe { (*(&address as *const libc::sockaddr_storage).cast::<libc::sockaddr_in6>()).sin6_port }),
                _ => continue,
            };
            if bound != port {
                continue;
            }
            let mut kind: libc::c_int = 0;
            let mut size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            let stream = unsafe {
                libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_TYPE, (&mut kind as *mut libc::c_int).cast(), &mut size)
            } == 0 && kind == libc::SOCK_STREAM;
            // No peer is what makes a bound socket block the bind: a connection on the same port
            // (one the listener accepted, now in TIME_WAIT) does not.
            let mut peer: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut peer_length = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let connected = unsafe { libc::getpeername(fd, (&mut peer as *mut libc::sockaddr_storage).cast(), &mut peer_length) } == 0;
            if stream && !connected {
                found.push(fd);
            }
        }
        Ok(found)
    }

    /// **THE RACE ITSELF, ON DEMAND.** Not part of the normal run: it starts hundreds of
    /// children, which is exactly the disturbance the rest of the binary must not be given.
    /// Run: `cargo test --bin richos-tauri a_port_stop_released -- --ignored --nocapture`.
    /// The real Connect listener is started and stopped 300 times alone, then 300 times beside
    /// four threads that keep starting `/bin/sleep`. It prints how many instant rebinds were
    /// refused in each (the check the merge gates tripped on), and fails if
    /// [`port_released_by_stop`] is refused even once.
    #[test]
    #[ignore]
    #[cfg(unix)]
    fn a_port_stop_released_can_stay_bound_while_another_thread_starts_a_child() {
        use crate::phone::{api_base::ApiBaseDesk, device::{DeviceDesk, PairedVia}, routes::{Bridge, Accepted, StopSwitch}, stream::PhoneHub};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Quiet;
        impl Bridge for Quiet {
            fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> { Err("unused".into()) }
            fn snapshot(&self, _: Option<&str>) -> Result<serde_json::Value, String> { Ok(serde_json::json!({})) }
            fn current_thread(&self) -> Option<(String, String)> { None }
            fn threads(&self) -> Vec<(String, String)> { vec![] }
        }
        let dir = std::env::temp_dir().join(format!("stop-spawn-race-{}-{}", std::process::id(), super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let channel = Arc::new(Channel {
            devices: Arc::new(DeviceDesk::open(&dir).unwrap()), rejected: StopSwitch::unwired(),
            api_base: Arc::new(ApiBaseDesk::only("https://example.invalid")), hub: PhoneHub::new(),
            bridge: Arc::new(Quiet),
            vapid_public: String::new(), fingerprint_hex: String::new(),
            pairing_path: std::sync::Mutex::new(PairedVia::CONNECT),
        });
        let rounds = 300;
        let run = || {
            let (mut instant_refusals, mut failures) = (0, Vec::new());
            for _ in 0..rounds {
                let mut listener = Listener::start_connect(Arc::clone(&channel), 0).expect("the Connect listener did not start");
                let port = listener.bound[0].port();
                assert_eq!(listening_fds_in_this_process(port).map(|fds| fds.len()), Ok(1), "the scan does not see the running listener");
                listener.stop();
                let rebind = || bind_listener(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
                if rebind().is_err() {
                    instant_refusals += 1;
                }
                if let Err(why) = port_released_by_stop(port, rebind) {
                    failures.push(why);
                }
            }
            (instant_refusals, failures)
        };
        // THE CONTROL: the same rounds with no child being started anywhere in the test.
        let (quiet_refusals, quiet_failures) = run();
        let done = Arc::new(AtomicBool::new(false));
        let starters: Vec<_> = (0..4).map(|_| {
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                let mut children = Vec::new();
                while !done.load(Ordering::SeqCst) {
                    if let Ok(child) = std::process::Command::new("/bin/sleep").arg("1")
                        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null()).spawn()
                    {
                        children.push(child);
                    }
                    if children.len() > 40 {
                        for mut child in children.drain(..20) { child.kill().ok(); child.wait().ok(); }
                    }
                }
                for mut child in children { child.kill().ok(); child.wait().ok(); }
            })
        }).collect();
        let (instant_refusals, failures) = run();
        done.store(true, Ordering::SeqCst);
        for starter in starters { starter.join().unwrap(); }
        std::fs::remove_dir_all(dir).unwrap();
        println!("no child starting: instant rebind refused in {quiet_refusals} of {rounds}; port_released_by_stop refused in {} of {rounds}", quiet_failures.len());
        println!("children starting: instant rebind refused in {instant_refusals} of {rounds}; port_released_by_stop refused in {} of {rounds}", failures.len());
        assert!(quiet_failures.is_empty() && failures.is_empty(), "port_released_by_stop refused: {quiet_failures:?} {failures:?}");
    }

    #[test]
    fn closing_the_http_body_releases_presence_before_the_next_heartbeat() {
        use crate::phone::{api_base::ApiBaseDesk, device::DeviceDesk, routes::{Bridge,Accepted,StopSwitch}, stream::PhoneHub};
        struct Quiet;
        impl Bridge for Quiet {
            fn submit_text(&self,_:Option<&str>,_:&str,_client_id:&str)->Result<Accepted,String>{Err("unused".into())}
            fn snapshot(&self,_:Option<&str>)->Result<serde_json::Value,String>{Ok(serde_json::json!({}))}
            fn current_thread(&self)->Option<(String,String)>{None}
            fn threads(&self)->Vec<(String,String)>{vec![]}
        }
        let dir=std::env::temp_dir().join(format!("stream-presence-{}-{}",std::process::id(),super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let devices=Arc::new(DeviceDesk::open(&dir).unwrap());
        let channel=Arc::new(Channel {devices:Arc::clone(&devices),rejected:StopSwitch::unwired(),
            api_base:Arc::new(ApiBaseDesk::only("https://example.invalid")),hub:PhoneHub::new(),bridge:Arc::new(Quiet),
           vapid_public:String::new(),fingerprint_hex:String::new(),
            pairing_path:std::sync::Mutex::new(super::super::device::PairedVia::CONNECT)});
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let response=open_stream(channel.clone(),"dev_test",vec![],0);
            assert_eq!(devices.open_streams(),1,"A live body must still suppress duplicate foreground push");
            tokio::task::yield_now().await; // Producer is now waiting for its next frame.
            drop(response);
            assert_eq!(devices.open_streams(),0,"A closed body must not suppress background push for 15 seconds");
            for _ in 0..10 {drop(open_stream(channel.clone(),"dev_test",vec![],0));}
            assert_eq!(devices.open_streams(),0,"Rapid reopen/close must not exhaust the stream slots");
        });
        drop(runtime);std::fs::remove_dir_all(dir).unwrap();
    }

    /// **AN EVENT PUBLISHED BETWEEN PREPARING THE OPENING AND OPENING THE STREAM REACHES THE
    /// PHONE** (hunt 2026-09-29, part 1, finding 13). The opening frames are built in the route
    /// and the subscription is made afterwards, when the response is written; a frame published
    /// in that gap was in neither. It arrives now, after the opening, exactly once, and the
    /// frame after that arrives live.
    #[test]
    fn an_event_published_between_preparing_the_opening_and_opening_the_stream_is_delivered_once() {
        use crate::phone::{api_base::ApiBaseDesk, device::DeviceDesk, routes::{Bridge,Accepted,StopSwitch}, stream::{PhoneHub, Replay}};
        struct Quiet;
        impl Bridge for Quiet {
            fn submit_text(&self,_:Option<&str>,_:&str,_client_id:&str)->Result<Accepted,String>{Err("unused".into())}
            fn snapshot(&self,_:Option<&str>)->Result<serde_json::Value,String>{Ok(serde_json::json!({}))}
            fn current_thread(&self)->Option<(String,String)>{None}
            fn threads(&self)->Vec<(String,String)>{vec![]}
        }
        let dir=std::env::temp_dir().join(format!("stream-gap-{}-{}",std::process::id(),super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let hub=PhoneHub::new();
        hub.set_live(true);
        let channel=Arc::new(Channel {devices:Arc::new(DeviceDesk::open(&dir).unwrap()),rejected:StopSwitch::unwired(),
            api_base:Arc::new(ApiBaseDesk::only("https://example.invalid")),hub:Arc::clone(&hub),bridge:Arc::new(Quiet),
           vapid_public:String::new(),fingerprint_hex:String::new(),
            pairing_path:std::sync::Mutex::new(super::super::device::PairedVia::CONNECT)});

        hub.publish("message",1,"{\"n\":1}".into()).unwrap();
        // The route: the phone is up to date as of now.
        let (replay,mark)=hub.replay_after_marked(Some(1));
        assert_eq!(replay,Replay::Tail(Vec::new()));
        // THE GAP: the route has answered and the response is not written yet.
        hub.publish("message",2,"{\"n\":\"in the gap\"}".into()).unwrap();

        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let response=open_stream(channel.clone(),"dev_test",vec![],mark);
            let mut body=response.into_body();
            let mut seen=String::new();
            async fn read(body:&mut BoxBody,seen:&mut String)->bool {
                use http_body_util::BodyExt;
                match tokio::time::timeout(std::time::Duration::from_secs(2),body.frame()).await {
                    Ok(Some(Ok(frame))) => { if let Some(data)=frame.data_ref() { seen.push_str(&String::from_utf8_lossy(data)); } true }
                    _ => false,
                }
            }
            // Every stream's first chunk is a keep-alive, written whatever the opening holds (see
            // `a_resume_with_nothing_to_replay_is_answered_at_once_and_not_at_the_first_keepalive`).
            assert!(read(&mut body,&mut seen).await,"the stream wrote nothing at all when it opened");
            assert!(seen.starts_with(": keep-alive "),"the first chunk was not the keep-alive comment: {seen:?}");
            assert!(read(&mut body,&mut seen).await,"nothing arrived: the event published in the gap was lost");
            assert!(seen.contains("in the gap"),"the stream did not carry the event published in the gap: {seen:?}");
            // And what follows is live, and the gap's event is not repeated.
            hub.publish("message",3,"{\"n\":\"live\"}".into()).unwrap();
            assert!(read(&mut body,&mut seen).await,"a live event after the gap did not arrive");
            assert!(seen.contains("live"),"{seen:?}");
            assert_eq!(seen.matches("in the gap").count(),1,"the gap's event was delivered twice: {seen:?}");
        });
        drop(runtime);std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wildcard_bind_is_refused_and_so_is_an_empty_address_list() {
        // Plan §2.5 item 2. The only plausible way a wildcard could creep in is an empty list
        // quietly meaning "all interfaces", so an empty list is an error — and `0.0.0.0` is
        // refused by name in case somebody ever passes it deliberately.
        use std::net::Ipv4Addr;
        assert!(check_addresses(&[]).is_err(), "an empty address list was accepted");
        assert!(check_addresses(&[IpAddr::V4(Ipv4Addr::UNSPECIFIED)]).is_err(), "0.0.0.0");
        assert!(check_addresses(&[IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)]).is_err(), "::");
        assert!(check_addresses(&[IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1))]).is_err(), "multicast");
        // One bad address in an otherwise good list is still a refusal — a half-bound listener is
        // worse than one that says it could not start.
        assert!(check_addresses(&[
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 249)),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        ])
        .is_err());
        // POSITIVE CONTROL: the addresses this Mac actually answers on are accepted.
        assert!(check_addresses(&[
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 249)),
        ])
        .is_ok());
    }

    #[test]
    fn every_answer_carries_a_fresh_challenge_and_a_refusal_carries_no_body() {
        // The phone reads `X-RichOS-Challenge` on every response, which is why there is no
        // challenge route. A 404 that omitted it would leave a phone whose challenge had aged out
        // with no way back.
        for outcome in [
            Outcome::NotFound,
            Outcome::RateLimited,
            Outcome::PayloadTooLarge,
            Outcome::Json { status: 200, body: "{}".into() },
        ] {
            let expected = outcome.status();
            let response = render_with(outcome, Some("CHALLENGE".into()));
            assert_eq!(response.status().as_u16(), expected);
            assert_eq!(response.headers().get("x-richos-challenge").unwrap(), "CHALLENGE");
            assert_eq!(
                response.headers().get("access-control-allow-origin").unwrap(),
                "null",
                "a wildcard origin would let any page on the internet make a credentialed request"
            );
            assert_eq!(response.headers().get("x-content-type-options").unwrap(), "nosniff");
        }
    }

    #[test]
    fn a_revoked_phone_gets_the_one_body_the_phone_acts_on() {
        // `web/web-app/lib/api.js` looks for exactly `{"revoked":true}` and treats it as final.
        let response = render_with(Outcome::Revoked, None);
        assert_eq!(response.status().as_u16(), 403);
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json; charset=utf-8");
    }

    #[test]
    fn a_rate_limited_answer_says_when_to_come_back() {
        let response = render_with(Outcome::RateLimited, None);
        assert_eq!(response.status().as_u16(), 429);
        assert_eq!(response.headers().get("retry-after").unwrap(), "60");
    }

    #[test]
    fn the_tls_configuration_is_built_from_our_own_leaf_and_speaks_http_1_1_only() {
        // Built from a REAL certificate rather than a fixture, so "rustls accepts what our
        // certificate authority issues" is a fact about the two of them together. This is the
        // assertion that would have caught a SEC1-versus-PKCS#8 mismatch, which is the one thing
        // about this handshake that is easy to get wrong and impossible to see.
        use crate::phone::ca::PhoneCa;
        use crate::phone::names::LocalNames;
        use crate::phone::secrets::MemorySecrets;
        use std::net::Ipv4Addr;

        let dir = std::env::temp_dir().join(format!(
            "richos-phone-tls-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = MemorySecrets::default();
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: "mm1.local".into(),
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 249))],
        };
        let ca = PhoneCa::open(&dir, &secrets, names).unwrap();
        let config = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).expect("rustls refused our leaf");
        assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ONE LISTENER, TWO NAMES, TWO AUTHORITIES — the whole of the Tailscale path's TLS half,
    /// proved without a tailnet.**
    ///
    /// CEO §61 and reach document §2.3: the Tailscale path gives this Mac a second name with a
    /// **publicly trusted** certificate, and the brief requires that `mm1.local` keep working
    /// unchanged beside it. Those are two claims about one socket, and this is where they are
    /// settled.
    ///
    /// **The stand-in is honest about what it stands in for.** There is no Tailscale on this Mac
    /// (see `phone/tailnet.rs`'s header for the eight checks), so the "publicly trusted" chain here
    /// is a SECOND `PhoneCa`, issued for the tailnet name under its own separate root. What matters
    /// for SNI selection is exactly the property that makes it a good stand-in: **it is signed by
    /// an authority the home root knows nothing about**, so a client that trusts only the home root
    /// cannot accept it, and the third case below turns that into the control.
    ///
    /// `unverified:` that a real Let's Encrypt chain from `tailscale cert` is accepted. That is a
    /// claim about certificate *contents*, which this test does not make; it makes a claim about
    /// *which* certificate the resolver hands out, which is the part we wrote.
    #[test]
    fn the_tailnet_name_gets_the_tailnet_certificate_and_every_other_name_still_gets_our_own() {
        use crate::phone::ca::PhoneCa;
        use crate::phone::names::LocalNames;
        use crate::phone::secrets::MemorySecrets;
        use std::net::Ipv4Addr;

        const TAILNET: &str = "mm1.tail1a2b3c.ts.net";

        /// **A temporary directory that goes away even when the test panics.**
        ///
        /// CEO §54: *"a ROCK-SOLID … mechanism that always guarantees that garbage like this will
        /// be always cleaned up afterwards."* A trailing `remove_dir_all` is not that mechanism —
        /// it is skipped by the very failure that makes somebody run the test in the first place.
        /// **Proved on 2026-09-18 by defeating it:** a deliberate mutation probe of this test left
        /// two authority directories behind in `$TMPDIR`, because the panic jumped over the
        /// cleanup at the bottom. `Drop` runs during unwinding; that line does not.
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn authority(bonjour: &str, tag: &str) -> (PhoneCa, Scratch) {
            let dir = std::env::temp_dir().join(format!(
                "richos-phone-sni-{tag}-{}-{}",
                std::process::id(),
                super::super::now_millis()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let names = LocalNames {
                host: "MM1".into(),
                bonjour: bonjour.into(),
                // LOOPBACK, so case 4 below can connect to a bare address and have the leaf it is
                // served actually cover it. `names.rs` includes loopback in the real list for the
                // same kind of reason.
                addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            };
            // A FRESH secret store per authority. Sharing one would hand the second `PhoneCa` the
            // first's key, and the two roots would silently be one root — which would make the
            // control below pass for the wrong reason.
            let ca = PhoneCa::open(&dir, &MemorySecrets::default(), names).unwrap();
            (ca, Scratch(dir))
        }

        let (home, _home_dir) = authority("mm1.local", "home");
        let (tailnet, _tailnet_dir) = authority(TAILNET, "tailnet");
        assert_ne!(home.ca_der, tailnet.ca_der, "the two authorities are the same authority");

        let config = tls_config_with_tailnet(
            &home.leaf_der,
            &home.leaf_key_pkcs8,
            Some((
            TAILNET.to_string(),
            vec![tailnet.leaf_der.clone()],
            crate::phone::tailnet::KeyDer::Pkcs8(tailnet.leaf_key_pkcs8.clone()),
        )),
        )
        .expect("rustls refused a two-certificate configuration");

        // A socket that accepts three connections and does nothing but the handshake.
        let listener = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_config = Arc::clone(&config);
        let server = std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut socket, _)) = listener.accept() else { return };
                socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok();
                let Ok(mut connection) = rustls::ServerConnection::new(Arc::clone(&server_config))
                else {
                    continue;
                };
                // Errors are expected on the refusal case: the client sends an alert and goes.
                let _ = connection.complete_io(&mut socket);
            }
        });

        /// Handshake only — no HTTP, because what is being proved is which certificate came back.
        fn handshake(port: u16, root_der: &[u8], sni: &str) -> Result<(), String> {
            let mut roots = rustls::RootCertStore::empty();
            roots.add(rustls::pki_types::CertificateDer::from(root_der.to_vec())).unwrap();
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let name = rustls::pki_types::ServerName::try_from(sni).unwrap().to_owned();
            let mut connection =
                rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
            let mut socket =
                std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).map_err(|e| e.to_string())?;
            socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok();
            while connection.is_handshaking() {
                connection.complete_io(&mut socket).map_err(|e| e.to_string())?;
            }
            Ok(())
        }

        // 1. THE HOME PATH IS UNTOUCHED. `mm1.local`, validated against the Mac's own root.
        assert!(
            handshake(port, &home.ca_der, "mm1.local").is_ok(),
            "the home name stopped working when the tailnet certificate was added"
        );

        // 2. THE TAILNET NAME GETS THE OTHER CERTIFICATE. Validated against the OTHER root, which
        //    is only possible if the resolver actually switched on the SNI name.
        assert!(
            handshake(port, &tailnet.ca_der, TAILNET).is_ok(),
            "the tailnet name was not served the tailnet certificate"
        );

        // 3. THE CONTROL, and the reason cases 1 and 2 mean anything. The SAME tailnet name,
        //    validated against the HOME root, must FAIL — because what came back was signed by an
        //    authority that root has never heard of. If the resolver had ignored SNI and served the
        //    home leaf for everything, this would have succeeded and case 2 would have failed; if
        //    it had served the tailnet leaf for everything, case 1 would have failed. Only the
        //    resolver actually working makes all three come out as asserted.
        assert!(
            handshake(port, &home.ca_der, TAILNET).is_err(),
            "the home root accepted the tailnet certificate, so the two authorities are not distinct"
        );

        // 4. A CONNECTION THAT NAMES NOTHING STILL GETS THE MAC'S OWN CERTIFICATE. An IP-literal
        //    `ServerName` sends NO SNI extension at all — which is what reaching the Mac by its
        //    address does, and what a client too old to send one does. This is why `resolve`
        //    falls back to home instead of returning `None`: `None` would turn "connect by
        //    address" into a handshake failure, breaking the home path for the one user who took
        //    the Tailscale path.
        assert!(
            handshake(port, &home.ca_der, "127.0.0.1").is_ok(),
            "a connection carrying no server name was not served the Mac's own certificate"
        );

        // The two authority directories need no line here: `Scratch` removes them on the way out,
        // including on the way out through a panic.
        let _ = server.join();
    }


    // -----------------------------------------------------------------------------------
    // THE WHOLE CHANNEL, END TO END, OVER REAL TLS
    // -----------------------------------------------------------------------------------
    //
    // Every layer this slice built, in one test, with nothing stubbed between them:
    //
    //   * a REAL certificate authority and leaf from `openssl`, and a REAL `rustls` client that
    //     validates the leaf against that root — so "the certificate is acceptable" is a fact
    //     about the two of them rather than about the fields we asked for;
    //   * the REAL `hyper` listener on a real socket, reached by writing HTTP/1.1 bytes;
    //   * the REAL route table, the REAL signature check, the REAL SSE stream;
    //   * a REAL `Spine` behind the bridge, with a mock lease standing in for `claude` and
    //     nothing else standing in for anything.
    //
    // What it does not cover, named so the coverage claim is honest: `claude` itself (a mock
    // lease), the browser (the client is `rustls` and hand-written HTTP), the macOS Keychain (an
    // in-memory secret store, because a test must not write to his login keychain), and Apple's
    // push service (nothing outbound happens here).

    use crate::phone::api_base::ApiBaseDesk;
    use crate::phone::ca::PhoneCa;
    use crate::phone::device::{signing_string, DeviceDesk};
    use crate::phone::names::LocalNames;
    use crate::phone::routes::{Accepted, Bridge, Channel};
    use crate::phone::secrets::MemorySecrets;
    use crate::phone::stream::PhoneHub;
    use richos_core::cognition::MockLeaseFactory;
    use richos_core::entity::{Entity, EntityId, EntityRegistry};
    use richos_core::ledger::Ledger;
    use richos_core::spine::Spine;
    use richos_core::steering::TurnControl;
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::Ipv4Addr;
    use std::sync::Mutex as StdMutex;

    /// A bridge over a real spine, with the same two-part shape the Tauri one has: the words go to
    /// the durable intake log, and the drain runs on its own thread because it runs the turn.
    struct SpineBridge {
        spine: Arc<StdMutex<Spine>>,
        control: TurnControl,
        thread: String,
        entity: EntityId,
    }

    impl Bridge for SpineBridge {
        fn submit_text(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {
            let thread = thread_id.unwrap_or(&self.thread).to_string();
            let record = self
                .control
                .submit_from_channel_as(&thread, Some(self.entity.clone()), text, "phone", Some(client_id))
                .map_err(|e| e.to_string())?;
            let spine = Arc::clone(&self.spine);
            std::thread::spawn(move || {
                let _ = spine.lock().unwrap().poll_intake();
            });
            Ok(Accepted {
                message_id: format!("intake_{}", record.id()),
                thread_id: thread,
                at: super::super::now_millis(),
            })
        }
        fn snapshot(&self, thread_id: Option<&str>) -> Result<Value, String> {
            let thread = thread_id.unwrap_or(&self.thread).to_string();
            let spine = self.spine.lock().unwrap();
            crate::timeline_view::timeline_payload(&*spine, &thread)
        }
        fn current_thread(&self) -> Option<(String, String)> {
            Some((self.thread.clone(), "the proposal".into()))
        }
        fn threads(&self) -> Vec<(String, String)> {
            vec![(self.thread.clone(), "the proposal".into())]
        }
    }

    /// A real TLS client that validates against our own root, and writes HTTP/1.1 by hand.
    struct TlsClient {
        config: Arc<rustls::ClientConfig>,
        port: u16,
        /// The name this client asks for and validates against — **taken from the fixture's own
        /// [`LocalNames`] rather than written out again here.** It was a literal in three places
        /// in this file, which is three chances for a test to validate a name the Mac it is
        /// talking to was never issued.
        server: String,
    }

    impl TlsClient {
        fn new(ca_der: &[u8], port: u16, server: &str) -> Self {
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(rustls::pki_types::CertificateDer::from(ca_der.to_vec()))
                .expect("our own root was refused by rustls as a root");
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            TlsClient { config: Arc::new(config), port, server: server.to_string() }
        }

        /// The name this client asks for, as rustls wants it. One place, so a fixture that
        /// changes its Mac's name changes what is validated too.
        fn server_name(&self) -> rustls::pki_types::ServerName<'static> {
            rustls::pki_types::ServerName::try_from(self.server.clone())
                .expect("the fixture's own name is not a server name")
        }

        /// One request, one response, one connection. Returns `(status, headers, body)`.
        ///
        /// **The handshake is the first assertion in this function**: the client validates our leaf
        /// against our root, for the server name `mm1.local`, with the connection made to
        /// 127.0.0.1. If the SAN, the extended key usage, the signature algorithm or the chain were
        /// wrong, this is where it would fail.
        fn request(
            &self,
            method: &str,
            path: &str,
            headers: &[(&str, &str)],
            body: &[u8],
        ) -> (u16, Vec<(String, String)>, Vec<u8>) {
            let mut connection =
                rustls::ClientConnection::new(Arc::clone(&self.config), self.server_name()).unwrap();
            let mut socket =
                std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).expect("connect");
            socket.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
            let mut tls = rustls::Stream::new(&mut connection, &mut socket);

            let mut request = format!(
                "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
                self.server
            );
            for (name, value) in headers {
                request.push_str(&format!("{name}: {value}\r\n"));
            }
            request.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            tls.write_all(request.as_bytes()).expect("write request");
            tls.write_all(body).expect("write body");
            tls.flush().ok();

            let mut raw = Vec::new();
            // `Connection: close`, so the read ends when the server closes — no chunk parsing.
            let _ = tls.read_to_end(&mut raw);
            parse_response(&raw)
        }

        /// Open a stream and read whatever arrives within `wait`, then drop the connection.
        fn stream(&self, path: &str, wait: std::time::Duration) -> String {
            let mut connection =
                rustls::ClientConnection::new(Arc::clone(&self.config), self.server_name()).unwrap();
            let mut socket =
                std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).expect("connect");
            socket.set_read_timeout(Some(wait)).unwrap();
            let mut tls = rustls::Stream::new(&mut connection, &mut socket);
            let request = format!(
                "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\n\r\n",
                self.server
            );
            tls.write_all(request.as_bytes()).expect("write request");
            tls.flush().ok();
            let mut raw = Vec::new();
            let deadline = std::time::Instant::now() + wait;
            let mut buffer = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                match tls.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => raw.extend_from_slice(&buffer[..n]),
                    Err(_) => break,
                }
            }
            String::from_utf8_lossy(&raw).to_string()
        }

        /// **THE SAME STREAM, WITH A CLOCK ON IT.** Open the stream and record, for each
        /// marker, the first instant at which the bytes carrying it had arrived at this client.
        /// Returns `(arrival per marker, the whole transcript)`.
        ///
        /// **Why a second method rather than a flag on the one above.** [`Self::stream`] answers
        /// "did this ever reach the phone", which is a question about bytes; this answers
        /// "when", which is a question about the path they took — and the two want opposite read
        /// timeouts. `stream` waits once for as long as it is willing to wait; this polls on a
        /// short one, so an arrival is dated to within that poll rather than to whenever the
        /// socket happened to fill.
        ///
        /// `opened` is rung the moment `event: hello` is on the wire, so a caller can be certain
        /// the stream is live BEFORE it does the thing it is timing. Without it the measurement
        /// would silently include a TLS handshake on some runs and not on others.
        ///
        /// **The resolution is `POLL` and is stated rather than implied.** Every assertion made
        /// on these numbers is two orders of magnitude above it.
        fn stream_marked(
            &self,
            path: &str,
            markers: &[String],
            opened: std::sync::mpsc::Sender<()>,
            wait: std::time::Duration,
        ) -> (Vec<Option<u64>>, String) {
            const POLL: std::time::Duration = std::time::Duration::from_millis(20);
            let mut connection =
                rustls::ClientConnection::new(Arc::clone(&self.config), self.server_name()).unwrap();
            let mut socket =
                std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).expect("connect");
            // The handshake is a round trip and must not be timed out by the polling interval.
            // It is shortened the moment the request is on the wire.
            socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
            let mut tls = rustls::Stream::new(&mut connection, &mut socket);
            let request = format!(
                "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\n\r\n",
                self.server
            );
            tls.write_all(request.as_bytes()).expect("write request");
            tls.flush().ok();
            tls.sock.set_read_timeout(Some(POLL)).unwrap();

            let mut text = String::new();
            let mut at: Vec<Option<u64>> = vec![None; markers.len()];
            let mut rung = false;
            let deadline = std::time::Instant::now() + wait;
            let mut buffer = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                match tls.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => text.push_str(&String::from_utf8_lossy(&buffer[..n])),
                    // A read that timed out is the ORDINARY case on a live stream with nothing
                    // happening on it. `WouldBlock`/`TimedOut` is not a dead socket, and
                    // treating it as one is how a measurement quietly becomes "nothing arrived".
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => break,
                }
                let now = super::super::now_millis();
                if !rung && text.contains("event: hello") {
                    rung = true;
                    let _ = opened.send(());
                }
                for (i, marker) in markers.iter().enumerate() {
                    if at[i].is_none() && text.contains(marker.as_str()) {
                        at[i] = Some(now);
                    }
                }
                if at.iter().all(|a| a.is_some()) {
                    break;
                }
            }
            (at, text)
        }
    }

    fn parse_response(raw: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
        let text = String::from_utf8_lossy(raw);
        let split = text.find("\r\n\r\n").unwrap_or(text.len());
        let head = &text[..split];
        let body = raw[(split + 4).min(raw.len())..].to_vec();
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let headers = lines
            .filter_map(|l| l.split_once(": "))
            .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
            .collect();
        (status, headers, body)
    }

    fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// **The API base these tests hand the phone.** A fixed string rather than something
    /// derived from the listener's own port: what is under test here is that what the desk was
    /// given is what comes back on the wire, and a value that moved with the port would be
    /// asserting the format call instead. It is a tailnet name because that is the only kind of
    /// origin the product hands out (CEO §61).
    const TEST_API_BASE: &str = "https://mm1.tail9a3b2.ts.net:8443";

    /// **A live proof whose prerequisites are missing has not passed.** The failure names what
    /// was missing, so the person who asked for `--ignored` reads "not proven" and why, never a
    /// green result with nothing behind it.
    fn cannot_prove(missing: &str) -> ! {
        panic!("NOT PROVEN: {missing}. The live proof did not run, so nothing is claimed.");
    }

    #[test]
    #[should_panic(expected = "NOT PROVEN: this Mac is `off`, not `ready`")]
    fn a_live_proof_with_missing_prerequisites_is_a_failure_not_a_pass() {
        cannot_prove("this Mac is `off`, not `ready`");
    }

    /// **THE LIVE PROOF: this Mac, its real tailnet name, a publicly trusted certificate, and
    /// `curl` with no `-k` and no `--cacert`.**
    ///
    /// `#[ignore]` for the same reason the detection one is: its answer depends on somebody's
    /// Tailscale account, and a suite may not depend on that. Run it deliberately:
    ///
    /// ```text
    /// cargo test --bin richos-tauri phone::listen::tests::live -- --ignored --nocapture
    /// ```
    ///
    /// It FAILS, loudly, on any state but `ready` or when the shipping port will not bind
    /// (`cannot_prove`) — Rust has no "skipped" result for a test selected with `--ignored`, so
    /// a return there is recorded as PASSED, and a proof that quietly passes on a Mac with no
    /// Tailscale would be worse than no proof. On a ready Mac it proves, in one run:
    ///
    /// 1. detection reports `ready` with the tailnet name;
    /// 2. `tailscale cert` hands over a chain and a key **through a pipe** — the key is never
    ///    written to disk, which is the whole reason `fetch_cert` asks for stdout;
    /// 3. the listener stands up on the tailnet address at the shipping port;
    /// 4. **curl, using the system trust store and nothing of ours, reaches it by NAME** — which
    ///    is the entire promise of this path, and the thing that deletes the sixteen taps;
    /// 5. the API layer answers over that origin, not just the static app;
    /// 6. the pairing URL a QR would carry is the tailnet origin.
    ///
    /// It binds the SHIPPING port, 8443, deliberately: an ephemeral port would prove a handshake
    /// and not the thing a phone will actually dial. So it refuses to run against a RichOS that is
    /// already serving, rather than fighting it for the socket.
    #[test]
    #[ignore = "depends on this machine's real Tailscale state; run with --ignored"]
    fn live_the_tailnet_name_is_served_with_a_publicly_trusted_certificate() {
        use crate::phone::tailnet;

        let (state, diagnostic) = tailnet::detect();
        println!("state      = {} {:?}", state.token(), diagnostic.map(|d| d.label()));
        let (Some(name), Some(origin)) = (state.name(), state.origin()) else {
            cannot_prove(&format!("this Mac is `{}`, not `ready`", state.token()));
        };
        println!("name       = {name}");
        println!("origin     = {origin}");
        println!("addresses  = {:?}", state.addresses());
        println!("account    = {:?}", state.account().map(|a| a.described()));
        println!("phone      = {:?}", state.phone());

        let cli = tailnet::find_cli().expect("ready with no command line is not a reachable state");
        let cert = tailnet::fetch_cert(&cli, name, super::super::TAILNET_MIN_VALIDITY)
            .expect("tailscale cert refused");
        println!(
            "cert       = {} certificate(s) in the chain, key is {}",
            cert.chain_der.len(),
            match cert.key {
                tailnet::KeyDer::Pkcs8(_) => "PKCS#8",
                tailnet::KeyDer::Sec1(_) => "SEC1 (`EC PRIVATE KEY`)",
            }
        );

        // --- the Mac, built the way `PhoneRuntime::start` builds it ---------------------------
        let dir = std::env::temp_dir().join(format!(
            "richos-phone-live-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = MemorySecrets::default();
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: "mm1.local".into(),
            addresses: state.addresses().to_vec(),
        };
        let ca = PhoneCa::open(&dir, &secrets, names.clone()).unwrap();

        let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
        spine.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()
            ])
            .unwrap(),
        );
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = spine.create_thread("the proposal", &entity).unwrap();
        spine.switch_thread(&thread).unwrap();
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["On it!"])));
        let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
        spine.set_turn_control(control.clone());
        let hub = PhoneHub::new();
        spine.set_live_observer(Box::new(crate::phone::stream::PhoneLiveEmitter::new(
            Arc::clone(&hub),
        )));
        let spine = Arc::new(StdMutex::new(spine));

        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let vapid = crate::phone::push::VapidKey::generate().unwrap();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::unwired(),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(origin.clone())),
            hub: Arc::clone(&hub),
            bridge: Arc::new(SpineBridge {
                spine: Arc::clone(&spine),
                control,
                thread: thread.clone(),
                entity,
            }) as Arc<dyn Bridge>,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: std::sync::Mutex::new(crate::phone::device::PairedVia::HOME),
        });

        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config_with_tailnet(
            &ca.leaf_der,
            &ca.leaf_key_pkcs8,
            Some((name.to_string(), cert.chain_der.clone(), cert.key.clone())),
        )
        .expect("rustls refused the tailnet certificate");

        let mut listener = match Listener::start(
            Arc::clone(&channel),
            tls,
            state.addresses(),
            HTTPS_PORT,
        ) {
            Ok(listener) => listener,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                cannot_prove(&format!("port {HTTPS_PORT} would not bind ({e}); quit RichOS and retry"));
            }
        };
        println!(
            "listening  = {}",
            listener.bound.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")
        );

        // --- curl, with NOTHING of ours in its trust store ------------------------------------
        //
        // No `--insecure`, no `--cacert`, no `--resolve`: the name is resolved by MagicDNS and the
        // certificate is validated against the system's own roots. If either half were missing
        // this would fail, which is exactly why it is the proof.
        let curl = |args: &[&str]| -> (String, String, bool) {
            let out = std::process::Command::new("/usr/bin/curl")
                .args(["--silent", "--show-error", "--max-time", "20"])
                .args(args)
                .output()
                .expect("curl did not run");
            (
                String::from_utf8_lossy(&out.stdout).to_string(),
                String::from_utf8_lossy(&out.stderr).to_string(),
                out.status.success(),
            )
        };

        let app_url = format!("{origin}/");
        let (body, stderr, ok) = curl(&[&app_url]);
        println!("GET {app_url} -> {} bytes, curl {}", body.len(), if ok { "ok" } else { "failed" });
        assert!(ok && stderr.is_empty(), "curl refused the public certificate or the name: {stderr}");
        assert!(
            body.contains("<script src=\"/app.js\""),
            "what was served over the tailnet name is not the phone app: {body}"
        );

        // AND THE API, not only the static app: an unsigned, unpaired POST is the one request the
        // channel answers without a credential, so it proves the route layer is reached.
        let phone = crate::phone::device::tests::Phone::new();
        let pair_body = serde_json::json!({
            "code": code,
            "public_key_jwk": phone.jwk(),
            "device_name": "the live proof",
        })
        .to_string();
        let pair_url = format!("{origin}/api/pair");
        let (body, stderr, ok) = curl(&[
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &pair_body,
            &pair_url,
        ]);
        println!("POST {pair_url} -> {body}");
        assert!(ok && stderr.is_empty(), "curl could not reach the API over the tailnet: {stderr}");
        let paired: Value = serde_json::from_str(&body).expect("the API did not answer JSON");
        assert!(paired.get("device_id").is_some(), "pairing over the tailnet was refused: {body}");

        // 6. AND THE CODE A QR WOULD CARRY IS THIS ORIGIN. Built the same way `status()` builds it.
        println!("pair url   = {origin}/#pair={code}");
        assert!(
            format!("{origin}/#pair={code}").starts_with(&format!("https://{name}:")),
            "the pairing URL is not the tailnet origin"
        );

        listener.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_phone_pairs_posts_a_message_and_reads_richs_reply_over_real_tls() {
        // --- the Mac -------------------------------------------------------------------------
        let dir = std::env::temp_dir().join(format!(
            "richos-phone-e2e-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = MemorySecrets::default();
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: "mm1.local".into(),
            addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        };
        let ca = PhoneCa::open(&dir, &secrets, names.clone()).unwrap();

        let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
        spine.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()
            ])
            .unwrap(),
        );
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = spine.create_thread("the proposal", &entity).unwrap();
        spine.switch_thread(&thread).unwrap();
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![
            "On it! The proposal is with legal.",
        ])));
        let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
        spine.set_turn_control(control.clone());

        let hub = PhoneHub::new();
        spine.set_live_observer(Box::new(crate::phone::stream::PhoneLiveEmitter::new(Arc::clone(&hub))));
        let spine = Arc::new(StdMutex::new(spine));

        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let vapid = crate::phone::push::VapidKey::generate().unwrap();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::unwired(),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(TEST_API_BASE)),
            hub: Arc::clone(&hub),
            bridge: Arc::new(SpineBridge {
                spine: Arc::clone(&spine),
                control,
                thread: thread.clone(),
                entity,
            }) as Arc<dyn Bridge>,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: std::sync::Mutex::new(crate::phone::device::PairedVia::HOME),
        });

        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).unwrap();
        let mut listener = Listener::start(
            Arc::clone(&channel),
            tls,
            &[IpAddr::V4(Ipv4Addr::LOCALHOST)],
            0,
        )
        .expect("the listener did not start");
        let https_port = listener.bound[0].port();

        // --- the phone -----------------------------------------------------------------------
        let client = TlsClient::new(&ca.ca_der, https_port, &names.bonjour);
        let phone = crate::phone::device::tests::Phone::new();

        // 0. THE PHONE WEB PAGE IS RETIRED (CEO, 2026-10-30), over a real TLS socket. A phone
        //    camera that scans the pairing QR code opens this origin in a browser: it gets the
        //    plain answer, never the old web app, and none of the web app's files are served.
        //    On `main` before this change, `/` was the web app and every path below was a 200.
        let (status, headers, body) = client.request("GET", "/", &[], b"");
        assert_eq!(status, 200, "the plain answer did not load: {}", String::from_utf8_lossy(&body));
        assert_eq!(header_of(&headers, "content-type"), Some("text/html; charset=utf-8"));
        let page = String::from_utf8_lossy(&body).into_owned();
        assert!(page.contains("Use the RichConnect app"), "what loaded is not the plain answer: {page}");
        assert!(!page.contains("<script"), "the plain answer carries a script: {page}");
        for path in [
            "/app.js",
            "/styles.css",
            "/manifest.webmanifest",
            "/icons/icon-192.png",
            "/lib/api.js",
            "/README.md",
            "/package.json",
            "/test/api.test.js",
            "/../private.txt",
        ] {
            let (status, _h, _b) = client.request("GET", path, &[], b"");
            assert_eq!(status, 404, "{path} was served");
        }
        let (status, headers, body) = client.request("GET", "/sw.js", &[], b"");
        assert_eq!(status, 200, "a phone that installed the page gets no worker to remove it");
        assert_eq!(header_of(&headers, "content-type"), Some("text/javascript; charset=utf-8"));
        assert!(String::from_utf8_lossy(&body).contains("registration.unregister()"));

        // 1. PAIR. No signature: this is the request that establishes the credential.
        let pair_body = serde_json::json!({
            "code": code,
            "public_key_jwk": phone.jwk(),
            "device_name": "iPhone",
        })
        .to_string();
        let (status, _headers, body) = client.request(
            "POST",
            "/api/pair",
            &[("Content-Type", "application/json")],
            pair_body.as_bytes(),
        );
        assert_eq!(status, 200, "pairing: {}", String::from_utf8_lossy(&body));
        let paired: Value = serde_json::from_slice(&body).unwrap();
        let device_id = paired["device_id"].as_str().unwrap().to_string();
        let mut challenge = paired["challenge"].as_str().unwrap().to_string();
        assert_eq!(paired["ca_fingerprint_sha256"], ca.fingerprint_hex());
        assert_eq!(paired["api_base"], TEST_API_BASE);

        // 1b. SAGE F1, OVER THE WIRE: the key just registered reaches nothing until a person at
        //     this Mac presses "They match" — a 409 carrying the awaiting body and a challenge,
        //     never a 200 and never the flat 404 the phone would read as final.
        let sig = super::super::b64url(&phone.sign(&signing_string(&challenge, "GET", "/api/events?before=0&limit=1", b"")));
        let auth = format!("RichOS-Device {device_id}.{challenge}.{sig}").replace(' ', "%20");
        let (status, headers, body) = client.request("GET", &format!("/api/events?before=0&limit=1&auth={auth}"), &[], b"");
        assert_eq!(status, 409, "an unconfirmed device was answered: {}", String::from_utf8_lossy(&body));
        assert_eq!(body, super::super::routes::AWAITING_MAC_BODY.as_bytes());
        assert!(header_of(&headers, "x-richos-challenge").is_some(), "the awaiting answer carried no challenge");
        devices.confirm_on_mac().unwrap();

        // Real URL encoding must survive TLS/HTTP parsing before signature verification.
        let audio_file=dir.join("signed-audio.wav");std::fs::write(&audio_file,b"RIFF....WAVE").unwrap();
        devices.mint_audio("turn_audio:text:0",audio_file);
        let audio_path="/api/audio/turn_audio%3Atext%3A0";
        let sig=super::super::b64url(&phone.sign(&signing_string(&challenge,"GET",audio_path,b"")));
        let auth=format!("RichOS-Device {device_id}.{challenge}.{sig}");
        let (status,_,bytes)=client.request("GET",audio_path,&[("Authorization",&auth)],b"");
        assert_eq!(status,200);assert_eq!(bytes,b"RIFF....WAVE");

        // 2. AN UNPAIRED CALLER STILL SEES NOTHING, over the same real socket. The positive
        //    control for every 404 in the unit tests, asserted against the wire this time.
        let (status, _h, body) = client.request("POST", "/api/messages", &[], b"{}");
        assert_eq!(status, 404, "an unsigned POST was answered");
        assert!(body.is_empty(), "a refusal carried a body: {}", String::from_utf8_lossy(&body));

        // 3. POST HIS WORDS, signed exactly as `web/web-app/lib/api.js` signs them.
        let words = "where are we on the proposal?";
        let message_body = serde_json::json!({
            "client_id": "01JE2E",
            "thread_id": thread,
            "kind": "text",
            "text": words,
            "sent_at": "2026-09-18T13:00:00.000Z",
        })
        .to_string();
        let signature = super::super::b64url(&phone.sign(&signing_string(
            &challenge,
            "POST",
            "/api/messages",
            message_body.as_bytes(),
        )));
        let authorization = format!("RichOS-Device {device_id}.{challenge}.{signature}");
        let (status, headers, body) = client.request(
            "POST",
            "/api/messages",
            &[("Content-Type", "application/json"), ("Authorization", &authorization)],
            message_body.as_bytes(),
        );
        assert_eq!(status, 200, "posting: {}", String::from_utf8_lossy(&body));
        let accepted: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(accepted["duplicate"], false);
        assert!(accepted["message_id"].as_str().unwrap().starts_with("intake_"));
        // EVERY response carries the next challenge, which is why there is no challenge route.
        challenge = header_of(&headers, "x-richos-challenge").expect("no challenge header").to_string();

        // 4. THE WORDS REACH THE LEDGER AS HIS OWN, and the turn runs. The drain is on its own
        //    thread, so this waits for the ledger rather than for the response.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut seen = Vec::new();
        while std::time::Instant::now() < deadline {
            {
                let guard = spine.lock().unwrap();
                let binding = guard.ledger().thread_binding(&thread).unwrap();
                seen = guard
                    .ledger()
                    .thread_turns_scoped(&binding)
                    .unwrap()
                    .into_iter()
                    .filter(|t| t.source == richos_core::ledger::Source::Text)
                    .map(|t| t.user_text.clone())
                    .collect();
            }
            if !seen.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(seen, vec![words.to_string()], "his words did not reach the ledger");

        // 5. THE STREAM CARRIES RICH'S REPLY. A fresh ticket, and a real SSE read.
        let stream_path = format!("/api/events?thread_id={thread}");
        let stream_sig = super::super::b64url(&phone.sign(&signing_string(
            &challenge,
            "GET",
            &stream_path,
            b"",
        )));
        let stream_auth = format!("RichOS-Device {device_id}.{challenge}.{stream_sig}")
            .replace(' ', "%20");
        let wire = client.stream(
            &format!("{stream_path}&auth={stream_auth}"),
            std::time::Duration::from_secs(3),
        );
        assert!(wire.contains("200 OK"), "the stream was refused: {wire}");
        assert!(wire.contains("text/event-stream"), "{wire}");
        assert!(wire.contains("event: hello"), "no hello on the stream: {wire}");
        assert!(
            wire.contains("On it! The proposal is with legal."),
            "Rich's reply is not on the stream: {wire}"
        );
        assert!(wire.contains(words), "his own words are not on the stream: {wire}");
        // AND UNDER THE PHONE'S OWN ID for them (adoption ledger §2.8 row C5): the id the POST
        // carried came back on the Mac's row for it, so the phone holding "01JE2E" on screen
        // recognizes this row as that line whether or not its receipt has arrived yet.
        assert!(wire.contains(r#""client_id":"01JE2E""#), "his row does not carry the phone's own id: {wire}");

        // 6 AND 7 WERE THE TRUST ENDPOINT — CEO §61, 2026-09-19. They stood a plain-HTTP
        //    socket up on the neighboring port, fetched `/ca`, asserted the Apple content type
        //    and `com.apple.security.root` in the body, then asserted a 404 for everything else
        //    on that port. §61 removed the path that needed a phone to install anything, and
        //    `Listener::start` no longer binds a second port at all. What used to prove "the
        //    trust page serves one file and nothing else" is now proved by there being no
        //    second socket: `listener.bound` is asserted below.
        assert_eq!(
            listener.bound,
            vec![std::net::SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), https_port)],
            "the channel bound something other than the one HTTPS socket"
        );

        // 8. STOPPING THE CHANNEL FREES THE PORT. "Off means no socket, not a closed door" — the
        //    proof is that no socket of ours listens on it once `stop()` returns, and that it can
        //    be bound again (past a child another test may be starting: `port_released_by_stop`).
        #[cfg(unix)]
        assert_eq!(listening_fds_in_this_process(https_port).map(|fds| fds.len()), Ok(1),
                   "the descriptor scan does not see the running listener, so its empty answer after stop would prove nothing");
        listener.stop();
        if let Err(why) = port_released_by_stop(https_port, || std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, https_port))) {
            panic!("the port was still held after stop() returned: {why}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------------------
    // THE TWO LEGS, TIMED — Ray's `.20260920.1` defect 1
    // -----------------------------------------------------------------------------------
    //
    // **WHAT WAS MEASURED IN THE VM, and it is the asymmetry rather than either number that
    // is the finding.** `docs/verification/2026-09-20-nightly-1.2.0-nightly.20260920.1-mac-to-
    // phone-in-the-vm-audit.md`, step 3: Rich's reply crossed from the Mac's window to the
    // phone's page in under a second on all five turns (tightest turn: the two screens within
    // 167 ms). The CEO's OWN typed message took **1.3 s to 6.1 s on four of those same five
    // turns** — same channel, same thread, same second.
    //
    // **WHAT THIS TEST CAN AND CANNOT SETTLE, said before the numbers so they are not read as
    // more than they are.** It measures the leg from the Mac's LEDGER to the bytes on the
    // phone's socket, for both directions of one turn, over real TLS through the shipped
    // listener. That is the whole of what `src/phone/` owns. It does NOT measure the leg from
    // his keypress to the ledger — that is the window, the Tauri command and the spine's one
    // mutex (`main.rs`'s `take_the_spine`), and it is measured in the VM with `RICHOS_HOP_TRACE`
    // and not here. So a GREEN run here does not say "the defect is fixed"; it says the phone
    // path is not where the asymmetry is, which is a thing worth being able to prove in
    // twenty seconds rather than in a VM walk.
    //
    // **THE ONE STRUCTURAL FACT IT PINS**, which is what makes it a regression test rather than
    // a benchmark: his message and Rich's reply leave the Mac through the SAME
    // `LiveObserver` -> `PhoneHub::publish` -> one open SSE socket (`stream.rs`), so no future
    // version can quietly put his own words on a poll, a batch or a refresh-after-the-reply
    // without this failing. That is exactly the shape the fix for
    // `esc-20260919T003541Z-6885f74b` removed, and nothing until now would have noticed it
    // coming back.

    /// One Mac, one paired phone, one thread — everything the two timing tests below need, and
    /// nothing either of them measures. Built once so the measurement is the only thing that
    /// differs between them.
    struct Timed {
        dir: std::path::PathBuf,
        spine: Arc<StdMutex<Spine>>,
        /// The lock-free durable road the shell's send path takes.
        control: TurnControl,
        hub: Arc<PhoneHub>,
        thread: String,
        entity: EntityId,
        listener: Listener,
        /// The signed stream URL, ready to open.
        url: String,
        ca_der: Vec<u8>,
        server: String,
        port: u16,
    }

    impl Timed {
        /// Open the phone's stream on its own thread and watch for `markers`. The handle yields
        /// `(arrival per marker, transcript)`; `opened` is rung when the stream says `hello`.
        fn watch(
            &self,
            markers: Vec<String>,
            opened: std::sync::mpsc::Sender<()>,
        ) -> std::thread::JoinHandle<(Vec<Option<u64>>, String)> {
            let client = TlsClient::new(&self.ca_der, self.port, &self.server);
            let url = self.url.clone();
            std::thread::spawn(move || {
                client.stream_marked(&url, &markers, opened, std::time::Duration::from_secs(20))
            })
        }

        fn finish(mut self) {
            self.listener.stop();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn a_mac_with_a_paired_phone(tag: &str, reply: &'static str) -> Timed {
        let dir = std::env::temp_dir().join(format!(
            "richos-phone-{tag}-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = MemorySecrets::default();
        // **THE FIXTURE MAC IS NAMED THE WAY A REAL ONE IS NOW** — its tailnet name, the same
        // origin `TEST_API_BASE` hands the phone, because CEO §61 leaves the product one path
        // and the older fixtures in this file still carry the name of the one it lost. Nothing
        // about the measurement depends on which name it is; the leaf covers both this name and
        // the loopback address, and the client below validates against the certificate either
        // way.
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: "mm1.tail9a3b2.ts.net".into(),
            addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        };
        let ca = PhoneCa::open(&dir, &secrets, names.clone()).unwrap();

        let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
        spine.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()
            ])
            .unwrap(),
        );
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = spine.create_thread("the proposal", &entity).unwrap();
        spine.switch_thread(&thread).unwrap();
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![reply])));
        let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
        spine.set_turn_control(control.clone());
        // The handle the SHELL holds. `send_message` reaches the log through its own
        // `AppState`, never through the spine, and that is the whole point of it.
        let for_the_desk = control.clone();
        let for_the_desk_entity = entity.clone();

        let hub = PhoneHub::new();
        spine.set_live_observer(Box::new(crate::phone::stream::PhoneLiveEmitter::new(Arc::clone(
            &hub,
        ))));
        let spine = Arc::new(StdMutex::new(spine));

        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let vapid = crate::phone::push::VapidKey::generate().unwrap();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::unwired(),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(TEST_API_BASE)),
            hub: Arc::clone(&hub),
            bridge: Arc::new(SpineBridge {
                spine: Arc::clone(&spine),
                control,
                thread: thread.clone(),
                entity,
            }) as Arc<dyn Bridge>,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: std::sync::Mutex::new(crate::phone::device::PairedVia::HOME),
        });

        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).unwrap();
        let mut listener = Listener::start(
            Arc::clone(&channel),
            tls,
            &[IpAddr::V4(Ipv4Addr::LOCALHOST)],
            0,
        )
        .expect("the listener did not start");
        let https_port = listener.bound[0].port();

        // --- the phone pairs ----------------------------------------------------------------
        let client = TlsClient::new(&ca.ca_der, https_port, &names.bonjour);
        let phone = crate::phone::device::tests::Phone::new();
        let pair_body = serde_json::json!({
            "code": code, "public_key_jwk": phone.jwk(), "device_name": "iPhone",
        })
        .to_string();
        let (status, _headers, body) = client.request(
            "POST",
            "/api/pair",
            &[("Content-Type", "application/json")],
            pair_body.as_bytes(),
        );
        assert_eq!(status, 200, "pairing: {}", String::from_utf8_lossy(&body));
        let paired: Value = serde_json::from_slice(&body).unwrap();
        let device_id = paired["device_id"].as_str().unwrap().to_string();
        let challenge = paired["challenge"].as_str().unwrap().to_string();
        // The person at this Mac presses "They match" (Sage F1); this test measures what follows.
        devices.confirm_on_mac().unwrap();

        // --- and OPENS ITS STREAM BEFORE ANYTHING HAPPENS -----------------------------------
        //
        // The phone is on the thread, watching, the way it is when it sits beside his keyboard.
        // The measurement does not start until this socket has said `hello`.
        let stream_path = format!("/api/events?thread_id={thread}");
        let stream_sig = super::super::b64url(&phone.sign(&signing_string(
            &challenge,
            "GET",
            &stream_path,
            b"",
        )));
        let stream_auth =
            format!("RichOS-Device {device_id}.{challenge}.{stream_sig}").replace(' ', "%20");

        Timed {
            dir,
            spine,
            control: for_the_desk,
            hub,
            thread,
            entity: for_the_desk_entity,
            listener,
            url: format!("{stream_path}&auth={stream_auth}"),
            ca_der: ca.ca_der.clone(),
            server: names.bonjour.clone(),
            port: https_port,
        }
    }

    /// **THE WORDS THE TWO TESTS USE.** Distinct sentences with no substring of one in the
    /// other, because every arrival below is decided by `contains`.
    const HIS_WORDS: &str = "where are we on the proposal?";
    const THE_REPLY: &str = "It is with legal, and I will chase it today.";

    #[test]
    fn his_own_message_and_richs_reply_reach_the_phone_within_one_bound() {
        let mac = a_mac_with_a_paired_phone("legs", THE_REPLY);

        // The phone is on the thread, watching, the way it is when it sits beside his keyboard.
        // Nothing is measured until this socket has said `hello`.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let watcher = mac.watch(vec![HIS_WORDS.to_string(), THE_REPLY.to_string()], ready_tx);
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the phone's stream never said hello — nothing below would mean anything");

        // --- THE PRESS ----------------------------------------------------------------------
        //
        // BOTH ledger writes happen inside `[pressed, settled]` — his message at the top of
        // `Spine::accept_prompt` and the reply's completion at the end of the same call — so
        // each leg is an INTERVAL and not a point, exactly as Ray's own table is. Quoting one
        // end of it as the measurement would be inventing precision the instrument has not got.
        let pressed = super::super::now_millis();
        mac.spine
            .lock()
            .unwrap()
            .submit_prompt(HIS_WORDS, richos_core::ledger::Source::Text)
            .expect("the turn was refused");
        let settled = super::super::now_millis();

        let (at, transcript) = watcher.join().expect("the watching phone panicked");

        let his = at[0].expect("HIS OWN MESSAGE NEVER REACHED THE PHONE AT ALL");
        let rich = at[1].expect("RICH'S REPLY NEVER REACHED THE PHONE AT ALL");
        let his_hi = his.saturating_sub(pressed);
        let his_lo = his.saturating_sub(settled);
        let rich_hi = rich.saturating_sub(pressed);
        let rich_lo = rich.saturating_sub(settled);
        let turn_span = settled.saturating_sub(pressed);
        println!("the turn itself, ledger to ledger : {turn_span} ms");
        println!("his own message -> the phone      : ({his_lo}, {his_hi}) ms");
        println!("Rich's reply    -> the phone      : ({rich_lo}, {rich_hi}) ms");
        println!("the asymmetry between the two legs: {} ms", his_hi.abs_diff(rich_hi));

        // **THE BOUND, AND WHERE THE NUMBER COMES FROM.** One second — chosen as the figure the
        // VM walk measured for the leg that IS fast ("under a second on all five turns"), so a
        // leg that fails this is a leg that would have been filed as the defect. It is roughly
        // two orders of magnitude above what this path costs on loopback, which is what keeps it
        // from being a load test of whatever else the machine is doing.
        const BOUND_MS: u64 = 1_000;
        assert!(
            his_hi <= BOUND_MS,
            "his own message took ({his_lo}, {his_hi}) ms to reach the phone — the wire, not the \
             window. Transcript:\n{transcript}"
        );
        assert!(
            rich_hi <= BOUND_MS,
            "Rich's reply took ({rich_lo}, {rich_hi}) ms to reach the phone. Transcript:\n{transcript}"
        );

        // **AND THE TWO LEGS ARE ONE PATH.** This is the assertion that would have failed if his
        // own message had been left to a poll, a batch or a refresh-after-the-reply: those all
        // produce a leg that is fine in absolute terms and WRONG beside the other one.
        const ASYMMETRY_MS: u64 = 500;
        assert!(
            his_hi.abs_diff(rich_hi) <= ASYMMETRY_MS,
            "the two legs are not on one path: his ({his_lo}, {his_hi}) ms, the reply \
             ({rich_lo}, {rich_hi}) ms. Transcript:\n{transcript}"
        );
        assert!(
            his <= rich,
            "the phone was told what Rich said before it was told what he said: his at {his}, \
             the reply at {rich}"
        );

        // POSITIVE CONTROLS, so a green run cannot be green because nothing was measured: both
        // rows really are on this stream, with the roles the page merges on.
        assert!(transcript.contains("\"role\":\"ceo\""), "no CEO row on the wire:\n{transcript}");
        assert!(transcript.contains("\"role\":\"rich\""), "no Rich row on the wire:\n{transcript}");

        mac.finish();
    }

    /// **WHERE THE 1.3-6.1 SECONDS GO, DEMONSTRATED RATHER THAN ARGUED.**
    ///
    /// The test above shows the two legs are one path and that the path is fast. This one shows
    /// what that path is BEHIND, and it is the thing the Mac's own window is not behind.
    ///
    /// **THE CHAIN, WITH ITS FILE AND LINE AT THE TIME OF WRITING.**
    ///
    ///   1. `ui/main.js` paints his bubble from the webview's own model — `addPendingUserMessage`
    ///      then `scheduleRender()` (`:1814-1821`) — and only THEN calls
    ///      `Bridge.invoke("send_message")` (`:1824`). So the Mac's screen shows his sentence
    ///      without waiting for anything at all, which is why Ray measured it at 43-504 ms.
    ///   2. `send_message` (`src-tauri/src/main.rs`) reaches `take_the_spine(&state.spine)`
    ///      (`:1187`) and **waits there for the one process-wide spine mutex**.
    ///   3. Only past that lock does `Spine::accept_prompt` write his words to the ledger and
    ///      emit `rich://ceo-message` one statement later (`crates/richos-core/src/spine.rs`
    ///      `:2336`). The write itself cannot be moved out from under the lock:
    ///      `Ledger::record_prompt_received` is `&mut self` over in-memory projections
    ///      (`ledger.rs:1624-1625`).
    ///
    /// **So every millisecond anything else holds that mutex is charged to the phone's copy of
    /// his sentence and to nothing else on his screen.** The longest holder by far is a turn:
    /// `send_message`'s own documentation says it "holds that mutex for the entire turn"
    /// (`main.rs:557`), and it keeps holding it past the moment the window is told the turn is
    /// over — `TurnStatus::Completed` goes out at `spine.rs:2898`, and `after_turn_boundary`
    /// (`spine.rs:3450`: proactive flush, `drain_intake`, stop claim, context pressure),
    /// `drain_queue`, and then `spine.messages(&thread)` (`main.rs:1263`, a full projection of
    /// the thread) all run before the guard is dropped. The composer is live through all of it,
    /// because `anyLiveTurn()` went false when the window heard `Completed`.
    ///
    /// **This test is that window, stood up deliberately and measured.** A holder takes the
    /// spine for [`HELD_MS`] and lets go; his sentence is submitted while it is held. The
    /// assertion is not "this is slow" — it is that HIS leg inherits the wait and the shape
    /// matches what Ray saw on a real Mac.
    #[test]
    fn a_busy_spine_delays_the_phones_copy_of_his_words_and_nothing_else_he_can_see() {
        const HELD_MS: u64 = 1_500;
        let mac = a_mac_with_a_paired_phone("held", THE_REPLY);

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let watcher = mac.watch(vec![HIS_WORDS.to_string()], ready_tx);
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the phone's stream never said hello");

        // SOMETHING ELSE HAS THE SPINE — the tail of the turn before his, a window command, a
        // prime. Which one it is does not matter to the arithmetic and is exactly what the log
        // line at `main.rs`'s `spine_wait_notice` names on a real Mac.
        let holding = Arc::clone(&mac.spine);
        let (let_go_tx, let_go_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let guard = holding.lock().unwrap();
            let _ = let_go_tx.send(());
            std::thread::sleep(std::time::Duration::from_millis(HELD_MS));
            drop(guard);
        });
        let_go_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the holder never took the spine");

        // THE PRESS. The window has already painted his bubble by this point in the real app;
        // this line is everything that happens after `Bridge.invoke`.
        let pressed = super::super::now_millis();
        mac.spine
            .lock()
            .unwrap()
            .submit_prompt(HIS_WORDS, richos_core::ledger::Source::Text)
            .expect("the turn was refused");
        let _ = holder.join();

        let (at, transcript) = watcher.join().expect("the watching phone panicked");
        let his = at[0].expect("his message never reached the phone at all");
        let waited = his.saturating_sub(pressed);
        println!("the spine was held for          : {HELD_MS} ms");
        println!("his message -> the phone, behind it: {waited} ms");

        // **THE WAIT IS INHERITED, MEASURABLY.** A slack of 300 ms below the holder's own sleep,
        // because `now_millis` is a wall clock, the holder's sleep is a floor rather than an
        // exact span, and the marker is dated to within the watcher's 20 ms poll. It is nowhere
        // near tight enough to flake and nowhere near loose enough to pass if the wait were not
        // inherited: without it this leg is single-digit milliseconds, which the test above
        // measures on the same fixture.
        assert!(
            waited + 300 >= HELD_MS,
            "his message reached the phone in {waited} ms while the spine was held for \
             {HELD_MS} ms — if that is real, the lock is no longer on this path and the comment \
             above this test is out of date. Transcript:\n{transcript}"
        );
        mac.finish();
    }

    /// **AND THE ROAD THE SHELL TAKES NOW, MEASURED AGAINST THE SAME HELD LOCK.**
    ///
    /// The test above is the defect: his row is behind the spine because it can only be built
    /// out of the ledger, and the ledger is behind that mutex. This is the fix, and it is the
    /// same situation with one thing changed — `send_message` writes his words to the DURABLE
    /// intake log (no lock, one `fsync`) and announces them from the record, so the phone has
    /// his sentence while the mutex is still held by somebody else.
    ///
    /// **The two tests are deliberately one pair.** Either alone could be satisfied by a
    /// trick: the one above by a slow machine, this one by announcing something the Mac had
    /// not committed to. Together they say the only thing worth saying — the wait was real,
    /// the wait is gone, and what the phone is shown is a record that is already on disk.
    ///
    /// **What this does NOT assert, said rather than implied.** It does not exercise
    /// `send_message` itself: that is a `#[tauri::command]` and needs a window. It exercises
    /// the seam the command calls, with the lock in the state the command finds it in.
    #[test]
    fn his_words_reach_the_phone_while_the_spine_is_still_held_by_something_else() {
        let mac = a_mac_with_a_paired_phone("announced", THE_REPLY);

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let watcher = mac.watch(vec![HIS_WORDS.to_string()], ready_tx);
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the phone's stream never said hello");

        // THE HOLDER KEEPS THE SPINE UNTIL THE PHONE HAS HIS WORDS (2026-09-29, the load-sensitive
        // checks audit). It used to hold it for 1,500 ms and the test then required the words to
        // arrive within 300 ms of wall clock, which a descheduled test process fails while the
        // words went past the lock exactly as they should. Now the lock is released only after
        // the watcher has returned, so "past the lock, not behind it" is a fact: if the
        // announcement were behind the lock, the watcher would reach its own 20 s deadline with
        // nothing, and the holder's 60 s bound is only there so a regression cannot hang the run.
        let holding = Arc::clone(&mac.spine);
        let (took_tx, took_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let guard = holding.lock().unwrap();
            let _ = took_tx.send(());
            if release_rx.recv_timeout(std::time::Duration::from_secs(60)).is_err() {
                eprintln!("[test] the holder was never told to let go; letting go at its 60 s bound");
            }
            drop(guard);
        });
        took_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the holder never took the spine");

        // THE PRESS, on the road `send_message` takes: the durable record first — which this
        // fixture's `TurnControl` writes to a real file — then the announcement, and only then
        // the spine.
        let pressed = super::super::now_millis();
        let record = mac
            .control
            .submit_from_desk(&mac.thread, Some(mac.entity.clone()), HIS_WORDS)
            .expect("his words could not be written to the intake log");
        let announced = crate::phone::stream::announce_his_words(
            &mac.hub,
            &mac.thread,
            &format!("intake_{}", record.id()),
            HIS_WORDS,
            super::super::now_millis(),
        );
        assert!(announced.is_some(), "nothing was published for his sentence");

        let (at, transcript) = watcher.join().expect("the watching phone panicked");
        // **PAST THE LOCK, NOT BEHIND IT.** The watcher has returned and the holder has not been
        // told to let go, so the spine is still held right now; the phone either has his words
        // already or never got them inside its 20 s deadline.
        assert!(
            mac.spine.try_lock().is_err(),
            "the spine is free, so this proves nothing about getting past a held lock"
        );
        let his = at[0].unwrap_or_else(|| {
            panic!(
                "his message never reached the phone while the spine was held — the announcement \
                 is back behind the lock. Transcript:\n{transcript}"
            )
        });
        let took = his.saturating_sub(pressed);
        // Reported, not judged: on this Mac's idle runs single-digit milliseconds, against the
        // 1,507 ms the test above measures behind the lock.
        println!("the spine was still held when the phone had his words");
        println!("his message -> the phone, past it : {took} ms");
        assert!(
            transcript.contains("intake_"),
            "the row on the wire does not carry the intake id the page retires it by:\n{transcript}"
        );

        // AND HIS WORDS ARE ON DISK, which is what makes the announcement honest rather than a
        // guess the Mac might contradict (plan §6).
        assert!(
            mac.control.pending_intake().iter().any(|r| r.id() == record.id()),
            "his sentence was announced to the phone and is not on the log"
        );

        release_tx.send(()).expect("the holder let go of the spine before it was told to");
        let _ = holder.join();
        // The turn itself still runs, in order, off the log — the same call `send_message`
        // makes once it has the spine.
        mac.spine.lock().unwrap().poll_intake().expect("the spine could not take it off the log");
        mac.finish();
    }

    // -----------------------------------------------------------------------------------
    // THE SAME THING AGAIN, WITH AN OUTSIDE CLIENT
    // -----------------------------------------------------------------------------------
    //
    // The test above uses `rustls` as the client, which is the same TLS implementation the server
    // uses. That is a strong test of the routes and a WEAK test of the certificate: an
    // implementation agreeing with itself is the one class of certificate defect a self-test
    // cannot find.
    //
    // So this does it again with **`/usr/bin/curl`** — a different TLS stack (SecureTransport /
    // LibreSSL on this Mac), a different X.509 parser, and a different name-verification
    // implementation, validating our leaf against our root **by name** with `--resolve`. If our
    // certificate were acceptable only to rustls, this is where that shows up.
    //
    // `curl` is on every Mac, so this is not a fragile dependency and the test is not skipped.
    // `--resolve` points the real name at loopback, so nothing touches the LAN and nothing fights
    // the shipped app for port 8443.

    #[test]
    fn curl_a_different_tls_stack_entirely_accepts_our_certificate_and_our_routes() {
        let dir = std::env::temp_dir().join(format!(
            "richos-phone-curl-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = MemorySecrets::default();
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: "mm1.local".into(),
            addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        };
        let ca = PhoneCa::open(&dir, &secrets, names.clone()).unwrap();
        let ca_path = dir.join("phone/ca.crt");

        let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
        spine.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()
            ])
            .unwrap(),
        );
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = spine.create_thread("the proposal", &entity).unwrap();
        spine.switch_thread(&thread).unwrap();
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Legal has it. Two days."])));
        let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
        spine.set_turn_control(control.clone());
        let hub = PhoneHub::new();
        spine.set_live_observer(Box::new(crate::phone::stream::PhoneLiveEmitter::new(Arc::clone(&hub))));
        let spine = Arc::new(StdMutex::new(spine));

        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let vapid = crate::phone::push::VapidKey::generate().unwrap();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::unwired(),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(TEST_API_BASE)),
            hub: Arc::clone(&hub),
            bridge: Arc::new(SpineBridge {
                spine: Arc::clone(&spine),
                control,
                thread: thread.clone(),
                entity,
            }) as Arc<dyn Bridge>,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: std::sync::Mutex::new(crate::phone::device::PairedVia::HOME),
        });
        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).unwrap();
        let mut listener = Listener::start(
            Arc::clone(&channel),
            tls,
            &[IpAddr::V4(Ipv4Addr::LOCALHOST)],
            0,
        )
        .unwrap();
        let https_port = listener.bound[0].port();

        let base = format!("https://mm1.local:{https_port}");
        let resolve = format!("mm1.local:{https_port}:127.0.0.1");
        let curl = |args: &[&str]| -> (String, String) {
            let out = std::process::Command::new("/usr/bin/curl")
                .args(["--silent", "--show-error", "--cacert", ca_path.to_str().unwrap()])
                .args(args)
                .output()
                .expect("curl did not run");
            (
                String::from_utf8_lossy(&out.stdout).to_string(),
                String::from_utf8_lossy(&out.stderr).to_string(),
            )
        };

        // 1. PAIR. No `--insecure` anywhere in this test: curl is validating our leaf against our
        //    root, for the name `mm1.local`, with its own X.509 code.
        let phone = crate::phone::device::tests::Phone::new();
        let pair_body = serde_json::json!({
            "code": code,
            "public_key_jwk": phone.jwk(),
            "device_name": "curl",
        })
        .to_string();
        let (stdout, stderr) = curl(&[
            "--resolve",
            &resolve,
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &pair_body,
            &format!("{base}/api/pair"),
        ]);
        assert!(stderr.is_empty(), "curl refused our certificate or our server: {stderr}");
        let paired: Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("pairing answered {stdout:?}: {e}"));
        let device_id = paired["device_id"].as_str().unwrap().to_string();
        let challenge = paired["challenge"].as_str().unwrap().to_string();
        // The person at this Mac presses "They match" (Sage F1).
        devices.confirm_on_mac().unwrap();

        // 2. POST HIS WORDS, signed.
        let words = "curl asking after the proposal";
        let body = serde_json::json!({
            "client_id": "01JCURL",
            "thread_id": thread,
            "kind": "text",
            "text": words,
            "sent_at": "2026-09-18T13:00:00.000Z",
        })
        .to_string();
        let signature = super::super::b64url(&phone.sign(&signing_string(
            &challenge,
            "POST",
            "/api/messages",
            body.as_bytes(),
        )));
        let authorization = format!("RichOS-Device {device_id}.{challenge}.{signature}");
        let (stdout, stderr) = curl(&[
            "--resolve",
            &resolve,
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("Authorization: {authorization}"),
            "--data-binary",
            &body,
            &format!("{base}/api/messages"),
        ]);
        assert!(stderr.is_empty(), "{stderr}");
        let accepted: Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("posting answered {stdout:?}: {e}"));
        assert_eq!(accepted["duplicate"], false, "{stdout}");

        // 3. HIS WORDS REACH THE LEDGER.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut landed = false;
        while std::time::Instant::now() < deadline && !landed {
            {
                let guard = spine.lock().unwrap();
                let binding = guard.ledger().thread_binding(&thread).unwrap();
                landed = guard
                    .ledger()
                    .thread_turns_scoped(&binding)
                    .unwrap()
                    .iter()
                    .any(|t| t.user_text == words);
            }
            if !landed {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
        assert!(landed, "his words did not reach the ledger");

        // 4. AN UNSIGNED CALLER GETS 404 WITH NOTHING IN IT, as seen by an outside client.
        let (stdout, stderr) = curl(&[
            "--resolve",
            &resolve,
            "-o",
            "/dev/null",
            "-w",
            "%{http_code} %{size_download}",
            "-X",
            "POST",
            "--data-binary",
            "{}",
            &format!("{base}/api/messages"),
        ]);
        assert!(stderr.is_empty(), "{stderr}");
        assert_eq!(stdout.trim(), "404 0", "an unsigned POST was answered: {stdout}");

        // 5 WAS THE PROFILE, fetched off the plain-HTTP neighbor and checked for Apple's own
        //    content type. CEO §61 removed the path that asked a phone to install one, and with
        //    it the second socket — so what stands in its place is that there is no second
        //    socket to fetch anything from.
        assert_eq!(
            listener.bound.len(),
            1,
            "the channel bound more than the one HTTPS socket: {:?}",
            listener.bound
        );

        listener.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }


    // -----------------------------------------------------------------------------------
    // "THEY DO NOT MATCH", OVER THE WIRE, AND THE PORT GOES WITH IT
    // -----------------------------------------------------------------------------------
    //
    // **The defect this is the red/green for.** Ray's nightly `.8` walk in the test VM, defect 1
    // (HIGH), `docs/verification/2026-09-20-nightly-1.2.0-nightly.20260919.8-phone-path-in-the-vm-audit.md`:
    // pressing `They do not match` on the phone dropped the device record and left the Mac
    // answering on 8443 at t+10, 20, 30, 40, 50 and 60 s, and two minutes later. The route had
    // `channel.devices` and no handle on the listener at all.
    //
    // **What is real here and what is not**, so the coverage claim is honest:
    //
    //   * REAL certificate authority and leaf, a REAL `rustls` client validating one against the
    //     other, the REAL `hyper` listener on a real socket, the REAL route table and the REAL
    //     signature check — the rejection arrives the way a phone sends it;
    //   * REAL `phone::watch_for_rejection`, the shipped watcher, on its own thread;
    //   * the OWNER is this test rather than `PhoneRuntime`, because building one far enough to
    //     start a channel needs a `tauri::AppHandle`, the login Keychain and a working tailnet.
    //     The owner's body here is the first two lines of `PhoneRuntime::forget`; the shipped
    //     closure calls `forget()` itself and is one line long.
    //
    // **The positive control is step 2**: the same connect, on the same port, a moment earlier,
    // succeeding. Without it a refused connection proves nothing — a listener that never started
    // refuses just as convincingly.
    //
    // **It serves the tailnet name and no other**, because that is the one path there is (CEO
    // §61) — which is also why it does not use `TlsClient` above, whose server name is pinned to
    // the Mac's own leaf.
    #[test]
    fn they_do_not_match_over_the_wire_stops_the_listener() {
        const TAILNET: &str = "mm1.tail7f4e2d.ts.net";

        /// The same drop guard the SNI test uses, for the same CEO §54 reason: a trailing
        /// `remove_dir_all` is skipped by the very panic that makes somebody run the test.
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// The rejection is answered inside the route table and never reaches the bridge, so
        /// this exists to satisfy the type rather than to be called.
        struct NoBridge;
        impl Bridge for NoBridge {
            fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> {
                Err("this test never sends a message".into())
            }
            fn snapshot(&self, _: Option<&str>) -> Result<Value, String> {
                Err("this test never reads a timeline".into())
            }
            fn current_thread(&self) -> Option<(String, String)> {
                None
            }
            fn threads(&self) -> Vec<(String, String)> {
                Vec::new()
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "richos-phone-reject-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let _scratch = Scratch(dir.clone());

        // A MEMORY SECRET STORE, NEVER THE LOGIN KEYCHAIN. A test may not write to his.
        let secrets = MemorySecrets::default();
        let names = LocalNames {
            host: "MM1".into(),
            bonjour: TAILNET.into(),
            // Loopback, so the leaf covers the address the client actually connects to.
            addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        };
        let ca = PhoneCa::open(&dir, &secrets, names).unwrap();
        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let hub = PhoneHub::new();
        let vapid = crate::phone::push::VapidKey::generate().unwrap();

        // --- the handle the route did not have, and the owner on the other end of it ----------
        let (doorbell, rejections) = std::sync::mpsc::channel::<&'static str>();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::to(doorbell),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(TEST_API_BASE)),
            hub: Arc::clone(&hub),
            bridge: Arc::new(NoBridge) as Arc<dyn Bridge>,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: std::sync::Mutex::new(crate::phone::device::PairedVia::TAILNET),
        });

        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).unwrap();
        let listener = Listener::start(
            Arc::clone(&channel),
            tls,
            &[IpAddr::V4(Ipv4Addr::LOCALHOST)],
            0,
        )
        .expect("the listener did not start");
        let https_port = listener.bound[0].port();
        assert!(hub.is_live(), "starting a listener did not put the hub live");

        // **THE OWNER HOLDS THE LISTENER AND NOTHING ELSE MAY.** `Listener::stop` joins the
        // serving thread, so a route reaching it would join its own thread and hang there — the
        // whole reason the switch is a doorbell rather than a teardown.
        let held = Arc::new(StdMutex::new(Some(listener)));
        let owner_listener = Arc::clone(&held);
        let owner_hub = Arc::clone(&hub);
        let (torn_down_tx, torn_down_rx) = std::sync::mpsc::channel();
        crate::phone::watch_for_rejection(rejections, move |why| {
            // `PhoneRuntime::forget`'s first two lines. In production they are reached through
            // `forget()` itself — one call, one copy of the sequence.
            if let Some(mut listener) = owner_listener.lock().unwrap().take() {
                listener.stop();
            }
            owner_hub.set_live(false);
            torn_down_tx.send(why).expect("the test stopped waiting for teardown");
        });

        // A real TLS client for the tailnet name, validating our leaf against our own root.
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(ca.ca_der.clone())).unwrap();
        let client_config = Arc::new(
            rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth(),
        );
        let request = |method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]| {
            let server_name =
                rustls::pki_types::ServerName::try_from(TAILNET).unwrap().to_owned();
            let mut connection =
                rustls::ClientConnection::new(Arc::clone(&client_config), server_name).unwrap();
            let mut socket =
                std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, https_port)).expect("connect");
            socket.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
            let mut tls = rustls::Stream::new(&mut connection, &mut socket);
            let mut head =
                format!("{method} {path} HTTP/1.1\r\nHost: {TAILNET}\r\nConnection: close\r\n");
            for (name, value) in headers {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            tls.write_all(head.as_bytes()).expect("write request");
            tls.write_all(body).expect("write body");
            tls.flush().ok();
            let mut raw = Vec::new();
            let _ = tls.read_to_end(&mut raw);
            parse_response(&raw)
        };

        // --- 1. a phone pairs, over real TLS ---------------------------------------------------
        let phone = crate::phone::device::tests::Phone::new();
        let pair_body = serde_json::json!({
            "code": code,
            "public_key_jwk": phone.jwk(),
            "device_name": "iPhone",
        })
        .to_string();
        let (status, _headers, body) = request(
            "POST",
            "/api/pair",
            &[("Content-Type", "application/json")],
            pair_body.as_bytes(),
        );
        assert_eq!(status, 200, "pairing was refused: {}", String::from_utf8_lossy(&body));
        let paired: Value = serde_json::from_slice(&body).unwrap();
        let device_id = paired["device_id"].as_str().unwrap().to_string();
        let challenge = paired["challenge"].as_str().unwrap().to_string();
        assert!(devices.is_paired(), "the desk does not think a phone is paired");

        // --- 2. THE POSITIVE CONTROL: the port answers, right now ------------------------------
        //
        // A refused connection at the end means nothing unless the same connection succeeded
        // first. This is Ray's `curl` on 8443, in the shape this test can assert.
        assert!(
            std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, https_port)).is_ok(),
            "the port was not answering BEFORE the rejection, so a refusal after it proves nothing"
        );

        // --- 3. `They do not match` ------------------------------------------------------------
        //
        // The same route and the same field the phone sends — `web/web-app/lib/api.js`,
        // `confirmFingerprint(false)`.
        //
        // **THE RESPONSE TO THIS REQUEST IS DELIBERATELY NOT ASSERTED.** The socket it would be
        // written on is the socket being closed, and the two race: connections are spawned tasks
        // on the serving thread's runtime, so when `serve_all` returns the runtime is dropped and
        // an in-flight write goes with it. That costs nothing, and it is checked rather than
        // assumed — the phone has already thrown its own key away on the same press and ignores
        // the outcome (`richos/web/web-app/app.js:287`:
        // `try { await api.confirmFingerprint(false); } catch { }`). Closing sooner is the safe
        // side of this race.
        let reject = serde_json::json!({
            "device_id": device_id,
            "fingerprint_confirmed": false,
        })
        .to_string();
        let signature = super::super::b64url(&phone.sign(&signing_string(
            &challenge,
            "POST",
            "/api/pair",
            reject.as_bytes(),
        )));
        let authorization = format!("RichOS-Device {device_id}.{challenge}.{signature}");
        let sent_at = std::time::Instant::now();
        let _ = request(
            "POST",
            "/api/pair",
            &[("Content-Type", "application/json"), ("Authorization", authorization.as_str())],
            reject.as_bytes(),
        );

        // --- 4. THE PORT GOES, AND THE CRITERION IS RAY'S ---------------------------------------
        //
        // His measurement was `curl` on 8443 from outside the guest, refused within seconds.
        // Five seconds is the ceiling; what is actually measured is printed, so a regression that
        // merely gets slower is visible rather than silently passing.
        let deadline = sent_at + std::time::Duration::from_secs(5);
        let mut refused_after = None;
        while std::time::Instant::now() < deadline {
            match std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, https_port)) {
                Ok(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(e) => {
                    assert_eq!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionRefused,
                        "the port failed for some reason other than being closed: {e}"
                    );
                    refused_after = Some(sent_at.elapsed());
                    break;
                }
            }
        }
        let refused_after = refused_after.expect(
            "the Mac was still answering five seconds after the phone said the six words did not \
             match - this is Ray's nightly .8 defect 1",
        );
        eprintln!(
            "[test] the port stopped answering {} ms after the rejection went out",
            refused_after.as_millis()
        );

        // --- 5. AND THE REST OF THE STATE MATCHES `Forget this phone` ---------------------------
        // A refused connection proves the socket closed, not that its thread has been joined
        // and the owner finished updating the hub. Wait for that completion on the same deadline.
        let why = torn_down_rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("the shipped watcher did not finish the owner's teardown within five seconds");
        assert_eq!(why, crate::phone::device::StoppedBy::PHONE, "the doorbell did not say the phone refused");
        assert!(!devices.is_paired(), "a rejected phone is still paired");
        assert!(
            !hub.is_live(),
            "the hub is still live, so the surface would still be told a phone is there"
        );
        assert!(
            held.lock().unwrap().is_none(),
            "the owner finished teardown but still holds the listener"
        );
    }

    // ---- pair-wait: the held ask, over a real socket (Sage's pair-v2 hypotheses review §1) ----
    //
    // A real listener on an ephemeral loopback port, a real TLS client, the real route table and
    // the real desk. What each test measures is printed, so a hold that merely gets slower is
    // visible rather than silently passing.

    #[test]
    fn prefer_wait_is_read_as_capped_seconds_and_anything_else_is_no_hold() {
        assert_eq!(hold_seconds(Some("wait=14")), Some(14));
        assert_eq!(hold_seconds(Some("wait=5")), Some(5));
        assert_eq!(hold_seconds(Some("wait=600")), Some(PAIR_WAIT_MAX_SECONDS), "a longer ask is capped, not refused");
        assert_eq!(hold_seconds(Some("respond-async, WAIT=\"9\"; x=1")), Some(9), "RFC 7240: case-insensitive, quoted, among others");
        for none in [None, Some(""), Some("wait=0"), Some("wait=soon"), Some("return=minimal"), Some("wait")] {
            assert_eq!(hold_seconds(none), None, "{none:?}");
        }
    }

    /// The teardown's wait for released answers: none open returns at once, an open one is waited
    /// for until its body is done, and one that never finishes costs at most the bound.
    ///
    /// **DECIDED BY WHAT ENDED THE WAIT, NOT BY A STOPWATCH** (2026-09-29, the load-sensitive
    /// checks audit). This used to time each `settle` against `< 50 ms` and `< 2 s`, which a
    /// descheduled test process fails while the drain does exactly the right thing. Now each wait
    /// is given a bound it cannot reach — `NEVER_MS` — so returning at all proves something
    /// other than the bound ended it, and `HANG_GUARD` only stops a broken drain from hanging
    /// the run: an hour-long bound that is waited out is caught 3,570 s early.
    #[test]
    fn the_drain_waits_for_held_answers_and_no_longer_than_its_bound() {
        const NEVER_MS: u64 = 3_600_000;
        const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);
        let never = std::time::Duration::from_millis(NEVER_MS);
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let drain = Arc::new(Drain::default());

            // NOTHING OPEN: it returns without waiting for its (hour-long) bound.
            tokio::time::timeout(HANG_GUARD, drain.settle(never))
                .await
                .expect("nothing was open, and the drain sat out its bound instead of returning");

            // ONE OPEN: it returns because the answer finished, and not before.
            let guard = drain.enter();
            let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let marked = Arc::clone(&finished);
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                marked.store(true, std::sync::atomic::Ordering::SeqCst);
                drop(guard);
            });
            tokio::time::timeout(HANG_GUARD, drain.settle(never))
                .await
                .expect("the open answer finished, and the drain sat out its bound instead of returning");
            assert!(
                finished.load(std::sync::atomic::Ordering::SeqCst),
                "the drain returned while the open answer was still being written"
            );

            // ONE THAT NEVER FINISHES: only the bound can end this wait, so returning at all is
            // the bound holding; and a tokio timer never fires early, so the lower bound is safe
            // on any host.
            let _stuck = drain.enter();
            let started = tokio::time::Instant::now();
            tokio::time::timeout(HANG_GUARD, drain.settle(std::time::Duration::from_millis(300)))
                .await
                .expect("an answer that never finishes held the drain past its bound");
            let waited = started.elapsed();
            assert!(waited >= std::time::Duration::from_millis(290), "the drain gave up before its bound: {waited:?}");
        });
    }

    const HOLD_TAILNET: &str = "mm1.tail5d2c1b.ts.net";

    /// The held-ask tests never send a message or read a timeline.
    struct QuietBridge;
    impl Bridge for QuietBridge {
        fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> {
            Err("this test never sends a message".into())
        }
        fn snapshot(&self, _: Option<&str>) -> Result<Value, String> {
            Err("this test never reads a timeline".into())
        }
        fn current_thread(&self) -> Option<(String, String)> {
            None
        }
        fn threads(&self) -> Vec<(String, String)> {
            Vec::new()
        }
    }

    /// A Mac with one phone that has redeemed its code over real TLS and pressed nothing yet.
    struct HoldWire {
        dir: std::path::PathBuf,
        devices: Arc<DeviceDesk>,
        listener: StdMutex<Option<Listener>>,
        client: Arc<rustls::ClientConfig>,
        port: u16,
        phone: crate::phone::device::tests::Phone,
        device_id: String,
        challenge: String,
    }

    impl Drop for HoldWire {
        /// CEO §54: the listener and the scratch go however the test ends.
        fn drop(&mut self) {
            if let Some(mut listener) = self.listener.lock().unwrap().take() {
                listener.stop();
            }
            if let Err(error) = std::fs::remove_dir_all(&self.dir) {
                eprintln!("[test] could not remove {}: {error}", self.dir.display());
            }
        }
    }

    fn hold_wire(tag: &str) -> HoldWire {
        wire_with(tag, Arc::new(QuietBridge))
    }

    /// [`hold_wire`] with the bridge chosen by the caller: a stream's `hello` reads the thread,
    /// which [`QuietBridge`] refuses.
    fn wire_with(tag: &str, bridge: Arc<dyn Bridge>) -> HoldWire {
        let dir = std::env::temp_dir().join(format!("richos-phone-hold-{tag}-{}-{}", std::process::id(), super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        // A MEMORY SECRET STORE, NEVER THE LOGIN KEYCHAIN. A test may not write to his.
        let names = LocalNames { host: "MM1".into(), bonjour: HOLD_TAILNET.into(), addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)] };
        let ca = PhoneCa::open(&dir, &MemorySecrets::default(), names).unwrap();
        let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
        let vapid = crate::phone::push::VapidKey::generate().unwrap();
        // Nothing owns this channel's stop switch: these tests end the channel themselves.
        let (doorbell, _nobody) = std::sync::mpsc::channel::<&'static str>();
        let channel = Arc::new(Channel {
            rejected: crate::phone::routes::StopSwitch::to(doorbell),
            devices: Arc::clone(&devices),
            api_base: Arc::new(ApiBaseDesk::only(TEST_API_BASE)),
            hub: PhoneHub::new(),
            bridge,
           
            vapid_public: vapid.application_server_key(),
            fingerprint_hex: ca.fingerprint_hex(),
            pairing_path: StdMutex::new(crate::phone::device::PairedVia::TAILNET),
        });
        devices.open_pairing().unwrap();
        let code = devices.pairing_window().unwrap().code;
        let tls = tls_config(&ca.leaf_der, &ca.leaf_key_pkcs8).unwrap();
        let listener = Listener::start(Arc::clone(&channel), tls, &[IpAddr::V4(Ipv4Addr::LOCALHOST)], 0)
            .expect("the listener did not start");
        let port = listener.bound[0].port();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(ca.ca_der.clone())).unwrap();
        let client = Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        let mut wire = HoldWire {
            dir,
            devices,
            listener: StdMutex::new(Some(listener)),
            client,
            port,
            phone: crate::phone::device::tests::Phone::new(),
            device_id: String::new(),
            challenge: String::new(),
        };
        let pair = serde_json::json!({
            "code": code, "public_key_jwk": wire.phone.jwk(), "device_name": "Android phone",
            "platform": "android", "pairing_version": 2,
        })
        .to_string();
        let (status, _, body) = wire.send("POST", "/api/pair", &[("Content-Type", "application/json")], pair.as_bytes());
        assert_eq!(status, 200, "pairing was refused: {}", String::from_utf8_lossy(&body));
        let answer: Value = serde_json::from_slice(&body).unwrap();
        assert!(answer["capabilities"].as_array().unwrap().contains(&serde_json::json!("pair-wait")), "{answer}");
        wire.device_id = answer["device_id"].as_str().unwrap().to_string();
        wire.challenge = answer["challenge"].as_str().unwrap().to_string();
        wire
    }

    impl HoldWire {
        fn send(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
            let server_name = rustls::pki_types::ServerName::try_from(HOLD_TAILNET).unwrap().to_owned();
            let mut connection = rustls::ClientConnection::new(Arc::clone(&self.client), server_name).unwrap();
            let mut socket = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).expect("connect");
            socket.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();
            let mut tls = rustls::Stream::new(&mut connection, &mut socket);
            let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {HOLD_TAILNET}\r\nConnection: close\r\n");
            for (name, value) in headers {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            tls.write_all(head.as_bytes()).expect("write request");
            tls.write_all(body).expect("write body");
            tls.flush().ok();
            let mut raw = Vec::new();
            // A close without close_notify ends the read with an error after the whole answer has
            // arrived; what was read is what is parsed, so the error is said and not acted on.
            if let Err(error) = tls.read_to_end(&mut raw) {
                eprintln!("[test] the answer's read ended with: {error}");
            }
            parse_response(&raw)
        }

        /// The phone's own signed "They match" — the ask Sage's fix makes the wait send — signed
        /// by `signer`, optionally asking this Mac to hold it. Returns the status, the body and
        /// how long the answer took.
        fn they_match_signed_by(&self, signer: &crate::phone::device::tests::Phone, prefer: Option<&str>) -> (u16, Value, std::time::Duration) {
            let body = serde_json::json!({ "device_id": self.device_id, "fingerprint_confirmed": true }).to_string();
            let signature = super::super::b64url(&signer.sign(&signing_string(&self.challenge, "POST", "/api/pair", body.as_bytes())));
            let authorization = format!("RichOS-Device {}.{}.{signature}", self.device_id, self.challenge);
            let mut headers = vec![("Content-Type", "application/json"), ("Authorization", authorization.as_str())];
            if let Some(prefer) = prefer {
                headers.push(("Prefer", prefer));
            }
            let started = std::time::Instant::now();
            let (status, _, answer) = self.send("POST", "/api/pair", &headers, body.as_bytes());
            (status, serde_json::from_slice(&answer).unwrap_or(Value::Null), started.elapsed())
        }

        fn they_match(&self, prefer: Option<&str>) -> (u16, Value, std::time::Duration) {
            self.they_match_signed_by(&self.phone, prefer)
        }

        /// **WAIT FOR THE MAC TO BE HOLDING, NOT FOR A NUMBER OF MILLISECONDS** (2026-09-29, the
        /// load-sensitive checks audit). These tests used to sleep 500-600 ms and bet that the
        /// ask had connected, shaken hands and reached the hold by then; on a busy host it had
        /// not, and a press, a newer ask or a teardown landed BEFORE the hold it was meant to
        /// end. The desk issues a ticket the moment a hold begins; this waits for the `n`th. The
        /// bound is a hang guard only.
        fn wait_until_held(&self, n: u64) {
            let guard = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while self.devices.holds_begun() < n {
                assert!(std::time::Instant::now() < guard, "hold {n} never began within 30 s");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    /// The longest a hold can last, so the one wrong answer a held-ask test must tell apart from
    /// the right one: a hold that was not ended early runs to at least this. A held ask that
    /// answers inside it was ended by something else, however slow the host was on the way.
    const HOLD_LIMIT: std::time::Duration = std::time::Duration::from_secs(PAIR_WAIT_MAX_SECONDS);

    fn still_waiting() -> Value {
        serde_json::json!({ "ok": true, "awaiting_mac_confirmation": true })
    }

    /// **THE PHONE HEARS THE PRESS ON THE MAC WITHIN ONE ROUND TRIP** — hypothesis 1, fixed. On
    /// `main` the Mac answered this ask at once and the phone learned of the press only at its
    /// next scheduled ask: 2, 3, 5, 8, 13, then 15 s later.
    #[test]
    fn a_held_they_match_is_answered_the_moment_the_mac_is_pressed() {
        let wire = hold_wire("press");

        // CONTROLS: without `Prefer` the answer is immediate and unchanged, and a caller without
        // the key is refused at once however long it asks to wait.
        // "At once" is "no hold began", read off the desk rather than off a stopwatch.
        let (status, body, _) = wire.they_match(None);
        assert_eq!((status, &body), (200, &still_waiting()), "the unheld answer changed");
        assert_eq!(wire.devices.holds_begun(), 0, "an ask that did not ask to be held was held");
        let (status, _, _) = wire.they_match_signed_by(&crate::phone::device::tests::Phone::new(), Some("wait=14"));
        assert_eq!(status, 404, "a signature from another key was not refused");
        assert_eq!(wire.devices.holds_begun(), 0, "a caller without the key was held");

        std::thread::scope(|scope| {
            let asked = scope.spawn(|| wire.they_match(Some("wait=14")));
            wire.wait_until_held(1);
            assert!(!asked.is_finished(), "the Mac answered at once instead of holding the ask");
            let pressed = std::time::Instant::now();
            wire.devices.confirm_on_mac().unwrap();
            let (status, body, took) = asked.join().unwrap();
            let after_press = pressed.elapsed();
            assert_eq!((status, &body), (200, &serde_json::json!({ "ok": true })), "the held ask was not told the Mac had pressed");
            // THE PRESS ENDED THE HOLD, not its fourteen seconds running out: a hold nobody woke
            // would answer `{ok: true}` too (the press moved the generation), but only at the
            // limit. `took` runs from before the ask was sent, so it cannot be under the limit
            // unless something ended the hold early — and the press is the only thing here.
            assert!(took < HOLD_LIMIT, "the held ask answered after {took:?}: the press did not wake it, its limit did");
            eprintln!("[test] held {} ms; answered {} ms after the press on the Mac", took.as_millis(), after_press.as_millis());
        });
    }

    /// A hold nobody releases ends at the time the phone asked for with "still waiting" — the
    /// answer an unheld ask gets — so the phone's own schedule takes over from there.
    #[test]
    fn a_hold_nobody_releases_ends_on_time_with_still_waiting() {
        let wire = hold_wire("timeout");
        let (status, body, took) = wire.they_match(Some("wait=1"));
        assert_eq!((status, &body), (200, &still_waiting()));
        assert_eq!(wire.devices.holds_begun(), 1, "the ask was answered without being held");
        assert!(took >= std::time::Duration::from_millis(900), "a one-second hold answered after {took:?}");
        // The time the phone ASKED for, not the most a hold may last: the one wrong answer this
        // tells apart is a hold that ignored `wait=1` and ran to the limit.
        assert!(took < HOLD_LIMIT, "a one-second hold answered after {took:?}: it ran to the limit, not to the time asked for");
        eprintln!("[test] a one-second hold answered after {} ms", took.as_millis());
    }

    /// **ONE HOLD AT A TIME.** A second ask answers the first at once, so a phone that asks again
    /// never has two connections parked on this Mac.
    #[test]
    fn a_second_ask_answers_the_held_one_at_once() {
        let wire = hold_wire("supersede");
        std::thread::scope(|scope| {
            let first = scope.spawn(|| wire.they_match(Some("wait=14")));
            // The newer ask must arrive while the older one IS held, or there is nothing for it
            // to supersede and the order of the two holds is the scheduler's.
            wire.wait_until_held(1);
            let second_started = std::time::Instant::now();
            let second = scope.spawn(|| wire.they_match(Some("wait=2")));
            let (status, body, took) = first.join().unwrap();
            let first_answered = second_started.elapsed();
            assert_eq!((status, &body), (200, &still_waiting()));
            assert_eq!(wire.devices.holds_begun(), 2, "the older hold ended before the newer ask was held");
            // Ended by the newer hold, not by its own fourteen seconds (see `HOLD_LIMIT`).
            assert!(took < HOLD_LIMIT, "the older hold was not answered early: {took:?}");
            eprintln!("[test] the older hold answered {} ms after the newer ask was sent", first_answered.as_millis());
            let (status, body, took) = second.join().unwrap();
            assert_eq!((status, &body), (200, &still_waiting()));
            assert!(took >= std::time::Duration::from_millis(1_900), "the newer ask was not the one held: {took:?}");
        });
    }

    /// **"They do not match" ON THE MAC REACHES A HELD PHONE BEFORE THE CHANNEL GOES.**
    /// `reject_on_mac` forgets the device and then stops the listener, and the held phone must
    /// hear the refusal rather than a dropped connection.
    ///
    /// **What this does NOT prove, measured rather than assumed:** with the drain's `settle`
    /// removed from `serve_all`, this test still passed on 2026-09-24 (0.56 s), because the
    /// released task is woken before the shutdown and this machine's scheduler runs it first. So
    /// it holds the outcome, and `the_drain_waits_for_held_answers_and_no_longer_than_its_bound`
    /// holds the mechanism that makes that outcome a guarantee instead of a scheduling order.
    #[test]
    fn a_held_ask_hears_they_do_not_match_on_the_mac_before_the_channel_closes() {
        let wire = hold_wire("reject");
        #[cfg(unix)]
        assert_eq!(listening_fds_in_this_process(wire.port).map(|fds| fds.len()), Ok(1),
            "the descriptor scan must see this listener before Stop");
        std::thread::scope(|scope| {
            let sent = std::time::Instant::now();
            let asked = scope.spawn(|| wire.they_match(Some("wait=14")));
            wire.wait_until_held(1);
            assert!(!asked.is_finished());
            // `PhoneRuntime::reject_on_mac`, in order: the device goes, then the channel.
            let stopping = std::time::Instant::now();
            wire.devices.forget().unwrap();
            wire.listener.lock().unwrap().take().expect("the listener").stop();
            let stopped_after = stopping.elapsed();
            let stopped_since_sent = sent.elapsed();
            let (status, body, took) = asked.join().unwrap();
            assert_eq!((status, &body), (403, &serde_json::json!({ "revoked": true })), "the held phone did not hear the Mac's refusal");
            // THE REFUSAL ENDED THE HOLD AND THE TEARDOWN DID NOT SIT IT OUT. Both are counted
            // from before the ask was sent, so neither can be under the hold's limit if the hold
            // ran to it (`HOLD_LIMIT`). The teardown's own wait for the answer is bounded at
            // `HELD_ANSWER_DRAIN_MS`, and that bound is proven by value in
            // `the_drain_waits_for_held_answers_and_no_longer_than_its_bound`; this used to re-time
            // it here against 2 s of wall clock, which a busy host fails with the answer delivered.
            assert!(took < HOLD_LIMIT, "the held ask answered after {took:?}: the refusal did not end it, its limit did");
            assert!(stopped_since_sent < HOLD_LIMIT, "the teardown returned {stopped_since_sent:?} after the ask was sent: it sat out the held ask");
            eprintln!("[test] the channel stopped {} ms after They do not match, with the answer delivered", stopped_after.as_millis());
        });
        // Use the existing descriptor check and child-start bound, as the other Stop tests do.
        port_released_by_stop(wire.port, || bind_listener(SocketAddr::from((Ipv4Addr::LOCALHOST, wire.port))))
            .expect("the port was still held after Stop");
    }

    /// A thread with nothing in it, so a stream's `hello` can be built.
    struct EmptyThread;
    impl Bridge for EmptyThread {
        fn submit_text(&self, _: Option<&str>, _: &str, _client_id: &str) -> Result<Accepted, String> {
            Err("this test never sends a message".into())
        }
        fn snapshot(&self, _: Option<&str>) -> Result<Value, String> {
            Ok(serde_json::json!({}))
        }
        fn current_thread(&self) -> Option<(String, String)> {
            None
        }
        fn threads(&self) -> Vec<(String, String)> {
            Vec::new()
        }
    }

    /// One event stream held open from the phone's side: the connection and its socket, kept
    /// together so the test decides when it goes, the way a killed phone process never does.
    struct HeldStream {
        connection: rustls::ClientConnection,
        socket: std::net::TcpStream,
    }

    impl HeldStream {
        /// Read until the Mac ends this response (the last chunk, or the connection closing), or
        /// until `wait` passes. True when it ended. `wait` is a hang guard only.
        fn ended_within(&mut self, wait: std::time::Duration) -> bool {
            let deadline = std::time::Instant::now() + wait;
            self.socket.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();
            let mut tls = rustls::Stream::new(&mut self.connection, &mut self.socket);
            let mut seen = Vec::new();
            let mut buffer = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                match tls.read(&mut buffer) {
                    Ok(0) => return true,
                    Ok(n) => {
                        seen.extend_from_slice(&buffer[..n]);
                        if seen.windows(7).any(|w| w == b"\r\n0\r\n\r\n") {
                            return true;
                        }
                    }
                    Err(ref e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(_) => return true,
                }
            }
            false
        }
    }

    impl HoldWire {
        /// Open `GET url` as an event stream and return its status once the head has arrived,
        /// with the connection still open.
        fn open_held_stream(&self, url: &str) -> (u16, HeldStream) {
            let server_name = rustls::pki_types::ServerName::try_from(HOLD_TAILNET).unwrap().to_owned();
            let mut held = HeldStream {
                connection: rustls::ClientConnection::new(Arc::clone(&self.client), server_name).unwrap(),
                socket: std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).expect("connect"),
            };
            held.socket.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();
            let mut tls = rustls::Stream::new(&mut held.connection, &mut held.socket);
            let head = format!("GET {url} HTTP/1.1\r\nHost: {HOLD_TAILNET}\r\nAccept: text/event-stream\r\n\r\n");
            tls.write_all(head.as_bytes()).expect("write request");
            tls.flush().ok();
            let mut raw = Vec::new();
            let mut buffer = [0u8; 4096];
            while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                match tls.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => raw.extend_from_slice(&buffer[..n]),
                }
            }
            (parse_response(&raw).0, held)
        }
    }

    /// **A PHONE THAT RELAUNCHES IS NEVER LOCKED OUT BY ITS OWN EARLIER STREAMS** — the
    /// 2026-10-01 iPhone walk, defect D4. A phone process that is killed sends no close, so its
    /// stream stays counted while this Mac writes keep-alives into it. Four quick relaunches
    /// filled every slot, and the fifth launch was answered 429 for a minute ("Reconnecting…
    /// your Mac is out of reach") while this Mac was healthy.
    ///
    /// Four streams are held open here exactly as those four dead processes left them, and the
    /// fifth open from the same phone must be answered 200. The ones it replaced must END, so
    /// their slots are not merely forgotten while their producers keep running.
    #[test]
    fn a_phone_that_reopens_its_stream_replaces_its_own_old_ones_and_is_never_refused() {
        let wire = wire_with("relaunch", Arc::new(EmptyThread));
        wire.devices.confirm_on_mac().unwrap();
        let path = "/api/events?thread_id=thr_relaunch";
        let signature = super::super::b64url(&wire.phone.sign(&signing_string(&wire.challenge, "GET", path, b"")));
        let auth = format!("RichOS-Device {}.{}.{signature}", wire.device_id, wire.challenge).replace(' ', "%20");
        let url = format!("{path}&auth={auth}");

        let mut earlier = Vec::new();
        for launch in 1..=4 {
            let (status, held) = wire.open_held_stream(&url);
            assert_eq!(status, 200, "launch {launch} was refused");
            earlier.push(held);
        }
        let (status, _newest) = wire.open_held_stream(&url);
        assert_eq!(status, 200, "the fifth launch was refused while its four earlier streams were still held: the phone is locked out by itself");
        for (i, held) in earlier.iter_mut().enumerate() {
            assert!(held.ended_within(std::time::Duration::from_secs(10)), "stream {} was replaced but never ended", i + 1);
        }
        assert_eq!(wire.devices.open_streams(), 1, "only the newest stream may still be counted");
    }

    /// **A RESUME WITH NOTHING TO REPLAY IS ANSWERED AT ONCE, NOT AT THE FIRST KEEP-ALIVE**
    /// (2026-10-01, isaac-opus-reopen1: a reply 15.3 s after the iPhone app reopened, measured in
    /// the headless lab against this listener). Right after pairing the phone's last frame is the
    /// `hello`, id 0, and it resumes with `since=0`. The hub holds nothing at or after 0, so the
    /// opening is EMPTY (`PhoneHub::replay_after_marked`, `Replay::Tail(vec![])`). This listener
    /// wrote the head at once but no body byte until `KEEPALIVE_MS` = 15,000 ms later, and the
    /// iPhone's `URLSession` reports the response only with the first body bytes, so the phone sat
    /// unconnected for those 15 s.
    ///
    /// Asserted over real TLS from the request being on the wire: the head AND a first body chunk
    /// arrive within 1,000 ms (KEEPALIVE_MS / 15), and that chunk is an SSE comment every client
    /// ignores. The read gives up at 3,000 ms so the old behavior fails here in 3 s, not 15
    /// (on the code before this fix: the head alone, then nothing, for the whole 3 s).
    #[test]
    fn a_resume_with_nothing_to_replay_is_answered_at_once_and_not_at_the_first_keepalive() {
        const LIMIT: std::time::Duration = std::time::Duration::from_millis(1_000);
        const GIVE_UP: std::time::Duration = std::time::Duration::from_millis(3_000);
        assert!(LIMIT.as_millis() * 15 <= u128::from(KEEPALIVE_MS), "the bound must sit far below the keep-alive");

        let wire = wire_with("firstbyte", Arc::new(EmptyThread));
        wire.devices.confirm_on_mac().unwrap();
        // The pose: a phone whose last frame was the pairing `hello`, id 0, resuming from it.
        let path = "/api/events?thread_id=thr_firstbyte&since=0";
        let signature = super::super::b64url(&wire.phone.sign(&signing_string(&wire.challenge, "GET", path, b"")));
        let auth = format!("RichOS-Device {}.{}.{signature}", wire.device_id, wire.challenge).replace(' ', "%20");
        let url = format!("{path}&auth={auth}");

        let server_name = rustls::pki_types::ServerName::try_from(HOLD_TAILNET).unwrap().to_owned();
        let mut connection = rustls::ClientConnection::new(Arc::clone(&wire.client), server_name).unwrap();
        let mut socket = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, wire.port)).expect("connect");
        // The handshake is completed by the request write below, under a generous timeout; only
        // then is the read shortened to a poll and the clock started.
        socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let mut tls = rustls::Stream::new(&mut connection, &mut socket);
        let head = format!("GET {url} HTTP/1.1\r\nHost: {HOLD_TAILNET}\r\nAccept: text/event-stream\r\n\r\n");
        tls.write_all(head.as_bytes()).expect("write request");
        tls.flush().ok();
        tls.sock.set_read_timeout(Some(std::time::Duration::from_millis(10))).unwrap();
        let sent = std::time::Instant::now();

        let mut raw: Vec<u8> = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut first_body_at: Option<std::time::Duration> = None;
        while sent.elapsed() < GIVE_UP {
            match tls.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => raw.extend_from_slice(&buffer[..n]),
                Err(ref e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
            let text = String::from_utf8_lossy(&raw);
            if let Some(split) = text.find("\r\n\r\n") {
                // Chunked: the body is `<size>\r\n<data>\r\n`. A comment line in the data is enough.
                if text[split + 4..].contains("\n:") || text[split + 4..].contains("\r\n:") {
                    first_body_at = Some(sent.elapsed());
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&raw).to_string();
        let Some(took) = first_body_at else {
            panic!("no head and first chunk within {GIVE_UP:?} of the request (keep-alive is {KEEPALIVE_MS} ms): {text:?}");
        };
        eprintln!("[test] empty-opening stream: head and first chunk {} ms after the request", took.as_millis());
        assert_eq!(parse_response(&raw).0, 200, "{text}");
        assert!(took < LIMIT, "the first bytes took {took:?}, limit {LIMIT:?}: {text:?}");
        let body = &text[text.find("\r\n\r\n").unwrap() + 4..];
        assert!(body.contains(": keep-alive "), "the first chunk is not the comment every client ignores: {body:?}");
        assert!(!body.contains("event:"), "an empty opening invented an event: {body:?}");
    }
}
