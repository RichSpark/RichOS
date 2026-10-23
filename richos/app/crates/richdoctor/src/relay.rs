//! The reporting relay's HTTPS transport ([`richdoctor_core::delivery::Relay`]).
//!
//! What it reports is only what it saw: an answer (status, JSON body, Retry-After), a
//! connection that was never made (nothing reached the service; "offline" only when this Mac
//! has no network at all, [`crate::mac::confirmed_offline`]), or an unclear end (a timeout or a
//! connection lost after the request may have arrived). Each call (register, one screenshot's
//! upload, complete) is bounded by [`SEND_BOUND`].
use richdoctor_core::delivery::{Relay, Transport, SEND_BOUND};
use serde_json::Value;

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
        let request = self.client.post(url).timeout(SEND_BOUND).json(body);
        tauri::async_runtime::block_on(async move { answer(request.send().await).await })
    }

    fn put(&self, url: &str, headers: &[(String, String)], bytes: Vec<u8>) -> Transport {
        let mut request = self.client.put(url).timeout(SEND_BOUND).body(bytes);
        for (k, v) in headers {
            request = request.header(k.as_str(), v.as_str());
        }
        tauri::async_runtime::block_on(async move { answer(request.send().await).await })
    }
}
