//! The HTTP half of an attachment upload, as the web's `runAttachmentUploadCycle` does it
//! (`apps/web/src/lib/attachmentUploadQueue.ts`): the bytes are POSTed to the signed URL that
//! `attachments.createUploadUrl` minted, with the attachment's MIME type as `Content-Type`.

use crate::status::{CallEvent, Status, StatusSender, publish};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame, SizeHint};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::Semaphore;

/// `MAX_CONCURRENT_UPLOADS`.
const CONCURRENCY: usize = 3;
/// `UPLOAD_TIMEOUT_MS`.
const TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CHUNK: usize = 64 * 1024;

/// Posts attachments to one environment, a few at a time.
pub struct Uploader {
    client: reqwest::Client,
    base: String,
    slots: Arc<Semaphore>,
}

impl Uploader {
    pub fn new(client: reqwest::Client, base: &str) -> Self {
        Self {
            client,
            base: base.trim_end_matches('/').to_owned(),
            slots: Arc::new(Semaphore::new(CONCURRENCY)),
        }
    }

    /// Posts `path` to `relative_url`, answering on `token` with progress, then done or failed.
    pub fn start(
        &self,
        token: u64,
        relative_url: &str,
        path: PathBuf,
        mime: String,
        status: &StatusSender,
    ) {
        let client = self.client.clone();
        let url = format!("{}{relative_url}", self.base);
        let slots = self.slots.clone();
        let status = status.clone();
        tokio::spawn(async move {
            let _slot = slots.acquire_owned().await;
            let outcome = tokio::time::timeout(
                TIMEOUT,
                post(&client, &url, path, &mime, token, status.clone()),
            )
            .await
            .unwrap_or_else(|_| Err("Upload timed out".to_owned()));
            let event = match outcome {
                Ok(()) => CallEvent::Done(Value::Null),
                Err(reason) => CallEvent::Failed(reason),
            };
            publish(&status, Status::Call { token, event });
        });
    }
}

async fn post(
    client: &reqwest::Client,
    url: &str,
    path: PathBuf,
    mime: &str,
    token: u64,
    status: StatusSender,
) -> Result<(), String> {
    let data = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|_| "Upload failed".to_owned())?
        .map_err(|_| "Upload failed".to_owned())?;
    let body = Progress {
        data: Bytes::from(data),
        sent: 0,
        token,
        status,
    };
    let response = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, mime)
        .body(reqwest::Body::wrap(body))
        .send()
        .await
        .map_err(|_| "Upload failed".to_owned())?;
    let code = response.status();
    if code.is_success() {
        Ok(())
    } else {
        Err(format!("Upload rejected ({})", code.as_u16()))
    }
}

/// The request body, handed out in chunks so each one reports how far the upload has got.
struct Progress {
    data: Bytes,
    sent: usize,
    token: u64,
    status: StatusSender,
}

impl HttpBody for Progress {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let total = self.data.len();
        if self.sent >= total {
            return Poll::Ready(None);
        }
        let end = (self.sent + CHUNK).min(total);
        let chunk = self.data.slice(self.sent..end);
        self.sent = end;
        publish(
            &self.status,
            Status::Call {
                token: self.token,
                event: CallEvent::Progress(json!(end as f64 / total as f64)),
            },
        );
        Poll::Ready(Some(Ok(Frame::data(chunk))))
    }

    fn is_end_stream(&self) -> bool {
        self.sent >= self.data.len()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact((self.data.len() - self.sent) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn the_body_reports_each_chunk_and_ends_with_the_whole_file() {
        let (status, updates) = async_channel::unbounded();
        let total = CHUNK * 2 + 10;
        let body = Progress {
            data: Bytes::from(vec![7u8; total]),
            sent: 0,
            token: 9,
            status,
        };
        assert_eq!(body.size_hint().exact(), Some(total as u64));
        let collected = body.collect().await.unwrap().to_bytes();
        assert_eq!(collected.len(), total);
        let mut fractions = Vec::new();
        while let Ok(Status::Call { token, event }) = updates.try_recv() {
            assert_eq!(token, 9);
            let CallEvent::Progress(value) = event else {
                panic!("not progress");
            };
            fractions.push(value.as_f64().unwrap());
        }
        assert_eq!(fractions.len(), 3);
        assert_eq!(fractions.last(), Some(&1.0));
        assert!(fractions.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
