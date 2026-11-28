//! Project favicon images, loaded the way the web's `projectFaviconUrlAtom` does: `assets.createUrl`
//! mints a signed URL for the project's favicon, the URL is fetched, and a URL whose last segment
//! is `project-favicon-missing` or an image that cannot be shown means the monogram stands in.
//! Only the attached environment's projects are looked up; other environments keep the monogram.

use crate::connection::{Action, ActionSender, Call};
use crate::status::CallEvent;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gpui::{App, Global, Image, ImageFormat};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

pub const TOKEN_BASE: u64 = 1 << 59;
/// `PROJECT_FAVICON_FALLBACK_MARKER`.
const FALLBACK_MARKER: &str = "project-favicon-missing";

/// Where a project's favicon is looked up: its workspace root and the path the project
/// projection found, which is only a cache-key hint for the server.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaviconRef {
    pub cwd: String,
    pub path: Option<String>,
}

#[derive(Clone)]
enum State {
    Loading,
    Ready(Arc<Image>),
    /// No favicon, or one that cannot be shown.
    Missing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Url,
    Bytes,
}

#[derive(Default)]
struct Inner {
    states: HashMap<FaviconRef, State>,
    pending: HashMap<u64, (FaviconRef, Stage)>,
    next: u64,
}

pub struct Favicons {
    inner: RefCell<Inner>,
    actions: ActionSender,
}

impl Global for Favicons {}

/// Registers the favicon loader; the window's connection answers it through `on_call`.
pub fn init(actions: ActionSender, cx: &mut App) {
    cx.set_global(Favicons {
        inner: RefCell::default(),
        actions,
    });
}

/// The image of `target`, starting its load on first use; `None` while it loads or when the
/// monogram stands in.
pub fn lookup(target: &FaviconRef, cx: &App) -> Option<Arc<Image>> {
    let favicons = cx.try_global::<Favicons>()?;
    let mut inner = favicons.inner.borrow_mut();
    match inner.states.get(target) {
        Some(State::Ready(image)) => return Some(image.clone()),
        Some(_) => return None,
        None => {}
    }
    let token = TOKEN_BASE + inner.next;
    inner.next += 1;
    let sent = favicons.actions.try_send(Action::Call {
        token,
        call: Call::CreateAssetUrl,
        payload: create_url_payload(target),
    });
    if sent.is_ok() {
        inner.pending.insert(token, (target.clone(), Stage::Url));
        inner.states.insert(target.clone(), State::Loading);
    }
    None
}

/// A new connection drops the requests in flight, so those lookups start over.
pub fn reset(cx: &mut App) {
    if let Some(favicons) = cx.try_global::<Favicons>() {
        let mut inner = favicons.inner.borrow_mut();
        inner.pending.clear();
        inner
            .states
            .retain(|_, state| !matches!(state, State::Loading));
    }
}

/// Takes one answer to a favicon request; repaints when an image or its absence is known.
pub fn on_call(token: u64, event: CallEvent, cx: &mut App) {
    let Some(favicons) = cx.try_global::<Favicons>() else {
        return;
    };
    let mut inner = favicons.inner.borrow_mut();
    let Some((target, stage)) = inner.pending.get(&token).cloned() else {
        return;
    };
    let outcome = match (stage, event) {
        (_, CallEvent::Progress(_)) => return,
        (Stage::Url, CallEvent::Done(value)) => match relative_url(&value) {
            Some(url) if !is_fallback_url(url) => {
                let sent = favicons.actions.try_send(Action::Fetch {
                    token,
                    relative_url: url.to_owned(),
                });
                if sent.is_ok() {
                    inner.pending.insert(token, (target, Stage::Bytes));
                    return;
                }
                State::Missing
            }
            _ => State::Missing,
        },
        (Stage::Bytes, CallEvent::Done(value)) => {
            decode(&value).map_or(State::Missing, |image| State::Ready(Arc::new(image)))
        }
        (_, CallEvent::Failed(_)) => State::Missing,
    };
    inner.pending.remove(&token);
    inner.states.insert(target, outcome);
    drop(inner);
    cx.refresh_windows();
}

fn create_url_payload(target: &FaviconRef) -> Value {
    let mut resource = json!({ "_tag": "project-favicon", "cwd": target.cwd });
    if let Some(path) = &target.path {
        resource["path"] = Value::String(path.clone());
    }
    json!({ "resource": resource })
}

fn relative_url(value: &Value) -> Option<&str> {
    value.get("relativeUrl")?.as_str()
}

/// `isProjectFaviconFallbackUrl`: the last path segment names the marker.
fn is_fallback_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    path.rsplit('/').next() == Some(FALLBACK_MARKER)
}

/// What the server sent, when GPUI can draw it (ICNS and AVIF it cannot, as with a browser that
/// fails to load the image).
fn decode(value: &Value) -> Option<Image> {
    let bytes = STANDARD.decode(value.get("data")?.as_str()?).ok()?;
    let content_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let format = image_format(&bytes, content_type)?;
    Some(Image::from_bytes(format, bytes))
}

fn image_format(bytes: &[u8], content_type: &str) -> Option<ImageFormat> {
    let head = |prefix: &[u8]| bytes.starts_with(prefix);
    if head(&[0x89, b'P', b'N', b'G']) {
        Some(ImageFormat::Png)
    } else if head(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if head(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if head(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(ImageFormat::Webp)
    } else if head(&[0, 0, 1, 0]) {
        Some(ImageFormat::Ico)
    } else if head(b"BM") {
        Some(ImageFormat::Bmp)
    } else if content_type.contains("svg") || looks_like_svg(bytes) {
        Some(ImageFormat::Svg)
    } else {
        None
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let text = text.trim_start();
    text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_segment_means_no_favicon() {
        assert!(is_fallback_url("/api/assets/project-favicon-missing"));
        assert!(is_fallback_url("/api/assets/project-favicon-missing?t=1"));
        assert!(!is_fallback_url("/api/assets/abc123"));
        assert!(!is_fallback_url("/api/project-favicon-missing/abc"));
    }

    #[test]
    fn formats_come_from_the_bytes() {
        assert_eq!(
            image_format(&[0x89, b'P', b'N', b'G', 0], ""),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format(b"RIFF1234WEBPVP8", ""),
            Some(ImageFormat::Webp)
        );
        assert_eq!(
            image_format(b"<?xml version=\"1.0\"?><svg/>", "text/plain"),
            Some(ImageFormat::Svg)
        );
        assert_eq!(image_format(b"icns....", "image/icns"), None);
    }

    #[test]
    fn the_request_carries_the_path_only_when_known() {
        let target = FaviconRef {
            cwd: "/w".into(),
            path: None,
        };
        assert_eq!(
            create_url_payload(&target),
            json!({ "resource": { "_tag": "project-favicon", "cwd": "/w" } })
        );
        let target = FaviconRef {
            cwd: "/w".into(),
            path: Some("public/f.svg".into()),
        };
        assert_eq!(
            create_url_payload(&target)["resource"]["path"],
            "public/f.svg"
        );
    }
}
