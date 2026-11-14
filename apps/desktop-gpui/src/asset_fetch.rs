//! The HTTP half of an asset load: the signed URL `assets.createUrl` minted is fetched and the
//! bytes come back through `Status::Call`, as the web's `<img src>` fetches a favicon.

use crate::status::{CallEvent, Status, StatusSender, publish};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);
/// A favicon is a few KiB; anything bigger is not one.
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// Fetches assets of one environment.
pub struct Fetcher {
    client: reqwest::Client,
    base: String,
}

impl Fetcher {
    pub fn new(client: reqwest::Client, base: &str) -> Self {
        Self {
            client,
            base: base.to_owned(),
        }
    }

    /// Answers on `token` with `{ "type", "data" }` (base64), or a failure.
    pub fn start(&self, token: u64, relative_url: &str, status: &StatusSender) {
        let client = self.client.clone();
        let url = resolve(&self.base, relative_url);
        let status = status.clone();
        tokio::spawn(async move {
            let event = match tokio::time::timeout(TIMEOUT, get(&client, url)).await {
                Ok(Ok(value)) => CallEvent::Done(value),
                Ok(Err(reason)) => CallEvent::Failed(reason),
                Err(_) => CallEvent::Failed("The asset timed out".to_owned()),
            };
            publish(&status, Status::Call { token, event });
        });
    }
}

/// `resolveAssetUrl`: the relative URL against the HTTP base.
fn resolve(base: &str, relative_url: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(base)
        .and_then(|base| base.join(relative_url))
        .map_err(|_| "The asset URL is not valid".to_owned())
}

async fn get(
    client: &reqwest::Client,
    url: Result<reqwest::Url, String>,
) -> Result<serde_json::Value, String> {
    let response = client
        .get(url?)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| "The asset could not be loaded".to_owned())?;
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "The asset could not be loaded".to_owned())?;
    if bytes.len() > MAX_BYTES {
        return Err("The asset is too large".to_owned());
    }
    Ok(json!({ "type": kind, "data": STANDARD.encode(&bytes) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_urls_resolve_against_the_base() {
        let url = resolve("http://127.0.0.1:3773/", "/api/assets/x?token=1").unwrap();
        assert_eq!(url.as_str(), "http://127.0.0.1:3773/api/assets/x?token=1");
        assert!(resolve("not a url", "/x").is_err());
    }
}
