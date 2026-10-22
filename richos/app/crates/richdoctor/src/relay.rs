//! The reporting relay's HTTPS transport ([`richdoctor_core::delivery::Relay`]).
//!
//! What it reports is only what it saw: an answer (status, JSON body, Retry-After), a
//! connection that was never made (nothing reached the service; "offline" only when this Mac
//! has no network at all, [`crate::mac::confirmed_offline`]), or an unclear end (a timeout or a
//! connection lost after the request may have arrived). Register, status, complete and retire
//! are bounded to 30 seconds (PRD §8). A screenshot upload instead stops after 60 seconds with no
//! progress or 5 minutes in all, and says so ([`Transport::Stopped`]).
use richdoctor_core::delivery::{Relay, Transport, REQUEST_BOUND, UPLOAD_CAP, UPLOAD_STALL};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Https {
    client: reqwest::Client,
}

impl Https {
    pub fn new() -> Result<Https, String> {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            // Err only when another thread installed one first, which is the same outcome.
            rustls::crypto::ring::default_provider().install_default().ok();
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .user_agent(format!("RichDoctor/{}", richdoctor_core::VERSION))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Https { client })
    }
}

async fn answer(result: Result<reqwest::Response, reqwest::Error>) -> Transport {
    let response = match result {
        Ok(r) => r,
        Err(e) if e.is_connect() => return Transport::NotConnected { offline: crate::mac::confirmed_offline() },
        Err(_) => return Transport::Unclear,
    };
    let status = response.status().as_u16();
    let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse::<u64>().ok());
    match response.bytes().await {
        Ok(bytes) if bytes.len() <= 256 * 1024 => Transport::Answer { status, body: serde_json::from_slice(&bytes).ok(), retry_after },
        Ok(_) => Transport::Answer { status, body: None, retry_after },
        Err(_) => Transport::Unclear,
    }
}

impl Relay for Https {
    fn post(&self, url: &str, body: &Value) -> Transport {
        let request = self.client.post(url).timeout(REQUEST_BOUND).json(body);
        tauri::async_runtime::block_on(async move { answer(request.send().await).await })
    }

    fn put(&self, url: &str, headers: &[(String, String)], bytes: Vec<u8>, progress: &mut dyn FnMut(u64)) -> Transport {
        let total = bytes.len() as u64;
        let sent = Arc::new(AtomicU64::new(0));
        let last = Arc::new(Mutex::new(Instant::now()));
        let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = bytes.chunks(64 * 1024).map(|c| Ok(bytes::Bytes::copy_from_slice(c))).collect();
        let (s2, l2) = (sent.clone(), last.clone());
        let stream = futures_util::stream::iter(chunks).inspect(move |c| {
            if let Ok(c) = c {
                s2.fetch_add(c.len() as u64, Ordering::SeqCst);
                *l2.lock().unwrap() = Instant::now();
            }
        });
        let mut request = self.client.put(url).timeout(UPLOAD_CAP).header("Content-Length", total.to_string()).body(reqwest::Body::wrap_stream(stream));
        for (k, v) in headers {
            request = request.header(k.as_str(), v.as_str());
        }
        let started = Instant::now();
        tauri::async_runtime::block_on(async move {
            let send = Box::pin(request.send());
            let mut send = send;
            loop {
                let tick = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(1)));
                match futures_util::future::select(send, tick).await {
                    futures_util::future::Either::Left((result, _)) => {
                        progress(sent.load(Ordering::SeqCst));
                        return match &result {
                            Err(e) if e.is_timeout() => Transport::Stopped("it did not finish within five minutes.".into()),
                            _ => answer(result).await,
                        };
                    }
                    futures_util::future::Either::Right((_, pending)) => {
                        progress(sent.load(Ordering::SeqCst));
                        let quiet = last.lock().unwrap().elapsed();
                        // Bytes are all handed over and the answer is awaited: that is waiting on the
                        // service, which the five-minute cap bounds, not a stalled upload.
                        if sent.load(Ordering::SeqCst) < total && quiet > UPLOAD_STALL {
                            return Transport::Stopped("it made no progress for 60 seconds.".into());
                        }
                        if started.elapsed() > UPLOAD_CAP {
                            return Transport::Stopped("it did not finish within five minutes.".into());
                        }
                        send = pending;
                    }
                }
            }
        })
    }
}

use futures_util::StreamExt as _;
