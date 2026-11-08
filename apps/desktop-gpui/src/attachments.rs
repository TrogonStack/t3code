//! The composer's file attachments, ported from the web: how a picked file is classified and
//! validated (`apps/web/src/components/chat/composerAttachmentFiles.ts` and
//! `useComposerAttachmentIntake.ts`), the upload state a tile shows
//! (`apps/web/src/lib/attachmentUploadState.ts`), and the turn attachments and message context
//! a send carries (`composerContextRecords.ts`, `snapshotComposerSend`). Everything here is pure.

use serde_json::{Value, json};
use std::path::PathBuf;

const MIB: u64 = 1024 * 1024;
/// `PROVIDER_SEND_TURN_MAX_ATTACHMENTS`.
pub const MAX_ATTACHMENTS: usize = 100;
/// `PROVIDER_SEND_TURN_MAX_FILE_BYTES`.
pub const MAX_FILE_BYTES: u64 = 50 * MIB;
/// `PROVIDER_SEND_TURN_MAX_IMAGE_BYTES`.
pub const MAX_IMAGE_BYTES: u64 = 10 * MIB;
/// `COMPOSER_CONTEXT_LABEL_MAX_CHARS`, counted in UTF-16 units like the web's string length.
const LABEL_MAX_UNITS: usize = 200;

/// `ATTACHMENT_ONLY_BOOTSTRAP_PROMPT`.
pub const BOOTSTRAP_PROMPT: &str = "[User attached one or more files without additional text. Respond using the conversation context and the attached files.]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    File,
}

impl Kind {
    fn wire(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Image,
    File,
    UnsupportedImage,
}

/// The MIME types the providers take as images, `PROVIDER_SEND_TURN_SUPPORTED_IMAGE_MIME_TYPES`.
const IMAGE_MIME_BY_EXTENSION: [(&str, &str); 5] = [
    ("gif", "image/gif"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
];

/// `VIDEO_MIME_TYPE_BY_EXTENSION` in `packages/shared/src/video.ts`.
const VIDEO_MIME_BY_EXTENSION: [(&str, &str); 7] = [
    ("avi", "video/x-msvideo"),
    ("m4v", "video/mp4"),
    ("mkv", "video/x-matroska"),
    ("mov", "video/quicktime"),
    ("mp4", "video/mp4"),
    ("ogv", "video/ogg"),
    ("webm", "video/webm"),
];

/// `GENERIC_MIME_TYPES` in `packages/shared/src/image.ts`.
const GENERIC_MIMES: [&str; 5] = [
    "application/octet-stream",
    "binary/octet-stream",
    "application/unknown",
    "video/mp2t",
    "video/vnd.dlna.mpeg-tts",
];

/// `WORKSPACE_IMAGE_PREVIEW_EXTENSIONS` in `packages/shared/src/mediaPreview.ts`.
const PREVIEW_EXTENSIONS: [&str; 13] = [
    ".avif", ".bmp", ".gif", ".ico", ".heic", ".heif", ".jpeg", ".jpg", ".png", ".svg", ".tif",
    ".tiff", ".webp",
];

/// What a browser would report as `File.type` for a name: the system has only a path here.
/// An unknown extension gives the empty type a browser reports for it.
pub fn guess_mime(name: &str) -> &'static str {
    const OTHER: [(&str, &str); 14] = [
        ("svg", "image/svg+xml"),
        ("heic", "image/heic"),
        ("heif", "image/heif"),
        ("avif", "image/avif"),
        ("bmp", "image/bmp"),
        ("ico", "image/x-icon"),
        ("tif", "image/tiff"),
        ("tiff", "image/tiff"),
        ("pdf", "application/pdf"),
        ("json", "application/json"),
        ("zip", "application/zip"),
        ("txt", "text/plain"),
        ("csv", "text/csv"),
        ("md", "text/markdown"),
    ];
    const MORE: [(&str, &str); 5] = [
        ("html", "text/html"),
        ("css", "text/css"),
        ("js", "text/javascript"),
        ("xml", "text/xml"),
        ("wav", "audio/wav"),
    ];
    let Some(extension) = extension(name, true) else {
        return "";
    };
    IMAGE_MIME_BY_EXTENSION
        .iter()
        .chain(&VIDEO_MIME_BY_EXTENSION)
        .chain(&OTHER)
        .chain(&MORE)
        .find(|(known, _)| *known == extension)
        .map_or("", |(_, mime)| mime)
}

/// The lowercased text after the last dot. The web's image inference ignores a leading dot
/// (`dotIndex <= 0`); its video lookup does not (`dotIndex < 0`).
fn extension(name: &str, leading_dot_counts: bool) -> Option<String> {
    let dot = name.rfind('.')?;
    if dot == 0 && !leading_dot_counts {
        return None;
    }
    Some(name[dot + 1..].trim().to_lowercase())
}

fn base_mime(mime: &str) -> String {
    mime.split(';').next().unwrap_or("").trim().to_lowercase()
}

fn is_generic(mime: &str) -> bool {
    GENERIC_MIMES.contains(&mime)
}

/// `isSvgAttachment`.
fn is_svg(name: &str, mime: &str) -> bool {
    let mime = base_mime(mime);
    mime == "image/svg+xml"
        || ((mime.is_empty() || is_generic(&mime)) && name.to_lowercase().ends_with(".svg"))
}

/// `isDngFile`.
fn is_dng(name: &str, mime: &str) -> bool {
    let mime = base_mime(mime);
    name.to_lowercase().ends_with(".dng")
        || matches!(
            mime.as_str(),
            "image/dng" | "image/x-dng" | "image/x-adobe-dng"
        )
}

/// `isHeicImageFile`.
fn is_heic(name: &str, mime: &str) -> bool {
    let lower = mime.to_lowercase();
    if lower == "image/heic" || lower == "image/heif" {
        return true;
    }
    let lower_name = name.to_lowercase();
    (mime.is_empty() || lower == "application/octet-stream")
        && (lower_name.ends_with(".heic") || lower_name.ends_with(".heif"))
}

/// `inferImageMimeTypeForUnknownFile`.
fn infer_image_mime(name: &str, mime: &str) -> Option<&'static str> {
    let lower = mime.to_lowercase();
    if !lower.is_empty() && lower != "application/octet-stream" {
        return None;
    }
    let extension = extension(name, false)?;
    IMAGE_MIME_BY_EXTENSION
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| *mime)
}

/// `classifyComposerAttachmentFile`.
pub fn classify(name: &str, mime: &str) -> Class {
    if is_svg(name, mime) || is_dng(name, mime) {
        return Class::File;
    }
    if is_heic(name, mime) || infer_image_mime(name, mime).is_some() {
        return Class::Image;
    }
    let lower = mime.to_lowercase();
    if !lower.starts_with("image/") {
        return Class::File;
    }
    if IMAGE_MIME_BY_EXTENSION
        .iter()
        .any(|(_, known)| *known == lower)
    {
        return Class::Image;
    }
    let lower_name = name.to_lowercase();
    if PREVIEW_EXTENSIONS
        .iter()
        .any(|ext| lower_name.ends_with(ext))
    {
        Class::File
    } else {
        Class::UnsupportedImage
    }
}

/// `normalizeComposerImageFileMimeType`: the inferred type for an image whose own is unknown.
fn image_mime(name: &str, mime: &str) -> String {
    infer_image_mime(name, mime).map_or_else(|| mime.to_owned(), str::to_owned)
}

/// `composerFileAttachmentMimeType`, with `videoMimeType` from `packages/shared/src/video.ts`.
pub fn file_mime(name: &str, mime: &str) -> String {
    if is_dng(name, mime) {
        return "image/x-adobe-dng".to_owned();
    }
    if is_svg(name, mime) {
        return "image/svg+xml".to_owned();
    }
    video_mime(name, mime).unwrap_or_else(|| {
        if mime.is_empty() {
            "application/octet-stream".to_owned()
        } else {
            mime.to_owned()
        }
    })
}

fn video_mime(name: &str, mime: &str) -> Option<String> {
    let mime = base_mime(mime);
    if mime.starts_with("video/") && !is_generic(&mime) {
        return Some(mime);
    }
    if !mime.is_empty() && !is_generic(&mime) {
        return None;
    }
    let extension = extension(name, true)?;
    VIDEO_MIME_BY_EXTENSION
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| (*mime).to_owned())
}

/// `fileAttachmentStagingLimit`. `config` is the server's advertised
/// `capabilities.fileAttachments.maxUploadBytes` once its config is known: the outer `None` is
/// "not loaded yet", the inner `None` is "this server takes no files".
pub fn staging_limit(config: Option<Option<u64>>) -> Option<u64> {
    match config {
        None => Some(MAX_FILE_BYTES),
        Some(None) => None,
        Some(Some(advertised)) => Some(advertised.min(MAX_FILE_BYTES)),
    }
}

/// `formatAttachmentSize`.
pub fn format_size(bytes: u64) -> String {
    let kilobytes = bytes.div_ceil(1024).max(1);
    if kilobytes >= 1024 {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{kilobytes} KB")
    }
}

/// `formatAttachmentUploadProgress`.
pub fn format_progress(progress: f32) -> String {
    let bounded = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    format!("{}%", (bounded * 100.0).floor() as u32)
}

/// `fileAttachmentTooLargeMessage`.
pub fn too_large_message(name: &str, max_bytes: u64) -> String {
    let limit = if max_bytes >= MIB && max_bytes.is_multiple_of(MIB) {
        format!("{} MB", max_bytes / MIB)
    } else if max_bytes >= 1024 && max_bytes.is_multiple_of(1024) {
        format!("{} KB", max_bytes / 1024)
    } else {
        format!("{max_bytes} byte{}", if max_bytes == 1 { "" } else { "s" })
    };
    format!("'{name}' exceeds the {limit} attachment limit.")
}

/// A file the user picked, as the picker and the file system describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    pub path: PathBuf,
    pub name: String,
    /// Zero when the file could not be read.
    pub size: u64,
}

/// A picked file that passed validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub path: PathBuf,
    pub kind: Kind,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

/// `addComposerAttachments`' validation. `reserved` is how many attachments the draft holds.
/// Like the web, one message reports: the last intake error, replaced by the last image
/// preparation error. The web's image preparation compresses and converts HEIC; here an image
/// is taken as it is, so an image over the limit or a HEIC one is refused with its messages.
pub fn intake(
    picked: &[Picked],
    reserved: usize,
    staging_limit: Option<u64>,
) -> (Vec<Accepted>, Option<String>) {
    let mut reserved = reserved;
    let mut accepted = Vec::new();
    let mut error = None;
    let mut preparation_error = None;
    for file in picked {
        let mime = guess_mime(&file.name);
        let class = classify(&file.name, mime);
        if reserved >= MAX_ATTACHMENTS {
            error = Some(format!(
                "You can attach up to {MAX_ATTACHMENTS} files per message."
            ));
            continue;
        }
        match class {
            Class::UnsupportedImage => {
                error = Some(format!(
                    "'{}' is not a supported image type. Attach GIF, HEIC, HEIF, JPEG, PNG, or WebP images.",
                    file.name
                ));
                continue;
            }
            Class::Image => {
                if file.size == 0 || is_heic(&file.name, mime) {
                    preparation_error =
                        Some(format!("'{}' could not be read as an image.", file.name));
                    continue;
                }
                if file.size > MAX_IMAGE_BYTES {
                    preparation_error = Some(format!(
                        "'{}' is too large to attach, even after compression.",
                        file.name
                    ));
                    continue;
                }
                accepted.push(Accepted {
                    path: file.path.clone(),
                    kind: Kind::Image,
                    name: file.name.clone(),
                    mime: image_mime(&file.name, mime),
                    size: file.size,
                });
            }
            Class::File => {
                let Some(limit) = staging_limit else {
                    error = Some("This server does not support file attachments.".to_owned());
                    continue;
                };
                if file.size == 0 {
                    error = Some(format!("'{}' is empty or could not be read.", file.name));
                    continue;
                }
                if file.size > limit {
                    error = Some(too_large_message(&file.name, limit));
                    continue;
                }
                accepted.push(Accepted {
                    path: file.path.clone(),
                    kind: Kind::File,
                    name: file.name.clone(),
                    mime: file_mime(&file.name, mime),
                    size: file.size,
                });
            }
        }
        reserved += 1;
    }
    (accepted, preparation_error.or(error))
}

/// `AttachmentUploadState` of one attachment.
#[derive(Clone, Debug, PartialEq)]
pub enum Upload {
    Uploading { progress: f32 },
    Ready { attachment_id: String },
    Failed { reason: String },
}

/// One attachment of the draft.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The local id, which the context record's `contextId` is scoped from.
    pub id: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub upload: Upload,
}

impl Item {
    /// The id of the attachment on the server, once it has uploaded.
    pub fn attachment_id(&self) -> Option<&str> {
        match &self.upload {
            Upload::Ready { attachment_id } => Some(attachment_id),
            _ => None,
        }
    }
}

/// `attachmentUploadBlockReason`: a failed upload does not block, since send retries it.
pub fn block_reason(items: &[Item]) -> Option<&'static str> {
    let pending = items
        .iter()
        .filter(|item| matches!(item.upload, Upload::Uploading { .. }))
        .count();
    match pending {
        0 => None,
        1 => Some("Attachment still uploading"),
        _ => Some("Attachments still uploading"),
    }
}

/// `sanitizeComposerContextLabel`.
fn label(name: &str, kind: Kind) -> String {
    let spaced: String = name
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '\\' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let normalized = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut units = 0;
    let mut cleaned = String::new();
    for c in normalized.chars() {
        units += c.len_utf16();
        if units > LABEL_MAX_UNITS {
            break;
        }
        cleaned.push(c);
    }
    if cleaned.is_empty() {
        kind.wire().to_owned()
    } else {
        cleaned
    }
}

/// The items in the order a send carries them: images, then files (`snapshotComposerSend`).
fn ordered(items: &[Item]) -> Vec<&Item> {
    let (images, files): (Vec<_>, Vec<_>) = items.iter().partition(|item| item.kind == Kind::Image);
    images.into_iter().chain(files).collect()
}

/// The turn's `attachments`, each bound to its upload. `None` while one has not uploaded
/// (`getUploadedAttachments`).
pub fn turn_attachments(items: &[Item]) -> Option<Vec<Value>> {
    ordered(items)
        .into_iter()
        .map(|item| {
            Some(json!({
                "type": item.kind.wire(),
                "id": item.attachment_id()?,
                "name": item.name,
                "mimeType": item.mime,
                "sizeBytes": item.size,
            }))
        })
        .collect()
}

/// The message's `context`: one record per attachment (`buildMessageContext`); omitted when
/// there are none.
pub fn context(items: &[Item]) -> Option<Value> {
    let records: Option<Vec<Value>> = ordered(items)
        .into_iter()
        .map(|item| {
            Some(json!({
                "version": 1,
                "kind": item.kind.wire(),
                "contextId": format!("{}_{}", item.kind.wire(), item.id),
                "label": label(&item.name, item.kind),
                "attachmentId": item.attachment_id()?,
                "name": item.name,
                "mimeType": item.mime,
                "sizeBytes": item.size,
            }))
        })
        .collect();
    let records = records?;
    (!records.is_empty()).then(|| json!({"version": 1, "records": records}))
}

/// What goes out as the message text: the typed text, or the bootstrap prompt when there is
/// only attachments.
pub fn message_text(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        BOOTSTRAP_PROMPT
    } else {
        trimmed
    }
}

/// The attachment kinds and names a thread title can be derived from.
pub fn title_seed(items: &[Item]) -> Option<String> {
    let images = items.iter().filter(|item| item.kind == Kind::Image).count();
    match images {
        0 => {
            let name = items
                .first()?
                .name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (!name.is_empty()).then(|| format!("File: {name}"))
        }
        1 => Some("Attached image".to_owned()),
        _ => Some("Attached images".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(name: &str, size: u64) -> Picked {
        Picked {
            path: PathBuf::from(format!("/tmp/{name}")),
            name: name.to_owned(),
            size,
        }
    }

    fn item(id: &str, kind: Kind, name: &str, upload: Upload) -> Item {
        Item {
            id: id.to_owned(),
            path: PathBuf::from(name),
            kind,
            name: name.to_owned(),
            mime: "application/octet-stream".to_owned(),
            size: 2048,
            upload,
        }
    }

    fn ready(id: &str) -> Upload {
        Upload::Ready {
            attachment_id: id.to_owned(),
        }
    }

    #[test]
    fn files_are_classified_like_the_web() {
        let cases = [
            ("a.svg", "image/svg+xml", Class::File),
            ("a.svg", "", Class::File),
            ("a.dng", "image/tiff", Class::File),
            ("a.heic", "image/heic", Class::Image),
            ("a.HEIF", "", Class::Image),
            ("a.jpg", "", Class::Image),
            ("a.png", "application/octet-stream", Class::Image),
            ("a.png", "image/png", Class::Image),
            ("a.pdf", "application/pdf", Class::File),
            ("a", "", Class::File),
            ("a.tiff", "image/tiff", Class::File),
            ("a.xyz", "image/x-xyz", Class::UnsupportedImage),
            (".png", "", Class::File),
        ];
        for (name, mime, expected) in cases {
            assert_eq!(classify(name, mime), expected, "{name} as {mime:?}");
        }
    }

    #[test]
    fn a_file_keeps_its_type_or_gets_the_web_fallbacks() {
        assert_eq!(file_mime("a.dng", ""), "image/x-adobe-dng");
        assert_eq!(file_mime("a.svg", ""), "image/svg+xml");
        assert_eq!(file_mime("a.mov", ""), "video/quicktime");
        assert_eq!(file_mime("a.ts", "video/mp2t"), "video/mp2t");
        assert_eq!(file_mime("a.bin", ""), "application/octet-stream");
        assert_eq!(file_mime("a.pdf", "application/pdf"), "application/pdf");
        assert_eq!(guess_mime("A.PNG"), "image/png");
        assert_eq!(guess_mime("notes"), "");
    }

    #[test]
    fn the_staging_limit_follows_the_server_config() {
        assert_eq!(staging_limit(None), Some(MAX_FILE_BYTES));
        assert_eq!(staging_limit(Some(None)), None);
        assert_eq!(staging_limit(Some(Some(MIB))), Some(MIB));
        assert_eq!(
            staging_limit(Some(Some(MAX_FILE_BYTES * 2))),
            Some(MAX_FILE_BYTES)
        );
    }

    #[test]
    fn sizes_and_progress_read_like_the_web() {
        assert_eq!(format_size(1), "1 KB");
        assert_eq!(format_size(1536), "2 KB");
        assert_eq!(format_size(1024 * 1023), "1023 KB");
        assert_eq!(format_size(1024 * 1023 + 600), "1.0 MB");
        assert_eq!(format_size(5 * MIB / 2), "2.5 MB");
        assert_eq!(format_progress(0.456), "45%");
        assert_eq!(format_progress(2.0), "100%");
        assert_eq!(format_progress(-1.0), "0%");
        assert_eq!(format_progress(f32::NAN), "0%");
    }

    #[test]
    fn the_limit_is_spelled_in_its_own_unit() {
        assert_eq!(
            too_large_message("a", 50 * MIB),
            "'a' exceeds the 50 MB attachment limit."
        );
        assert_eq!(
            too_large_message("a", 2048),
            "'a' exceeds the 2 KB attachment limit."
        );
        assert_eq!(
            too_large_message("a", 1),
            "'a' exceeds the 1 byte attachment limit."
        );
        assert_eq!(
            too_large_message("a", 1500),
            "'a' exceeds the 1500 bytes attachment limit."
        );
    }

    #[test]
    fn intake_accepts_what_fits_and_reports_one_error() {
        let files = [
            picked("notes.txt", 10),
            picked("shot.png", 10),
            picked("empty.txt", 0),
            picked("big.zip", MAX_FILE_BYTES + 1),
        ];
        let (accepted, error) = intake(&files, 0, Some(MAX_FILE_BYTES));
        let names: Vec<_> = accepted
            .iter()
            .map(|file| (file.name.as_str(), file.kind))
            .collect();
        assert_eq!(
            names,
            [("notes.txt", Kind::File), ("shot.png", Kind::Image)]
        );
        assert_eq!(accepted[0].mime, "text/plain");
        assert_eq!(accepted[1].mime, "image/png");
        assert_eq!(
            error.as_deref(),
            Some("'big.zip' exceeds the 50 MB attachment limit.")
        );
    }

    #[test]
    fn intake_refuses_by_the_webs_rules() {
        let one =
            |name: &str, size, reserved, limit| intake(&[picked(name, size)], reserved, limit);
        assert_eq!(
            one("a.txt", 1, MAX_ATTACHMENTS, Some(MIB)).1.as_deref(),
            Some("You can attach up to 100 files per message.")
        );
        assert_eq!(
            one("a.txt", 1, 0, None).1.as_deref(),
            Some("This server does not support file attachments.")
        );
        assert_eq!(
            one("a.txt", 0, 0, Some(MIB)).1.as_deref(),
            Some("'a.txt' is empty or could not be read.")
        );
        assert_eq!(
            one("a.xyz", 1, 0, Some(MIB)).1,
            None,
            "an unknown extension has no type, so it is a file"
        );
        assert_eq!(
            one("big.png", MAX_IMAGE_BYTES + 1, 0, Some(MIB))
                .1
                .as_deref(),
            Some("'big.png' is too large to attach, even after compression.")
        );
        assert_eq!(
            one("a.heic", 5, 0, Some(MIB)).1.as_deref(),
            Some("'a.heic' could not be read as an image.")
        );
        assert!(
            one("a.png", 1, 0, None).0.len() == 1,
            "an image needs no file support"
        );
    }

    #[test]
    fn a_preparation_error_replaces_an_intake_error() {
        let files = [picked("a.heic", 5), picked("b.txt", 0)];
        let (accepted, error) = intake(&files, 0, Some(MIB));
        assert!(accepted.is_empty());
        assert_eq!(
            error.as_deref(),
            Some("'a.heic' could not be read as an image.")
        );
    }

    #[test]
    fn only_uploading_attachments_block_send() {
        let failed = Upload::Failed {
            reason: "Upload failed".to_owned(),
        };
        let uploading = Upload::Uploading { progress: 0.0 };
        let one = |upload: &Upload| item("1", Kind::File, "a", upload.clone());
        assert_eq!(block_reason(&[]), None);
        assert_eq!(block_reason(&[one(&ready("x")), one(&failed)]), None);
        assert_eq!(
            block_reason(&[one(&uploading)]),
            Some("Attachment still uploading")
        );
        assert_eq!(
            block_reason(&[one(&uploading), one(&uploading)]),
            Some("Attachments still uploading")
        );
    }

    #[test]
    fn a_send_carries_images_then_files_bound_to_their_uploads() {
        let items = [
            item("f1", Kind::File, "notes.txt", ready("up-file")),
            item("i1", Kind::Image, "shot.png", ready("up-image")),
        ];
        let attachments = turn_attachments(&items).unwrap();
        assert_eq!(attachments[0]["type"], "image");
        assert_eq!(attachments[0]["id"], "up-image");
        assert_eq!(attachments[1]["id"], "up-file");
        let context = context(&items).unwrap();
        assert_eq!(context["version"], 1);
        assert_eq!(
            context["records"][1],
            json!({
                "version": 1,
                "kind": "file",
                "contextId": "file_f1",
                "label": "notes.txt",
                "attachmentId": "up-file",
                "name": "notes.txt",
                "mimeType": "application/octet-stream",
                "sizeBytes": 2048,
            })
        );
        let pending = [item(
            "f1",
            Kind::File,
            "a",
            Upload::Uploading { progress: 0.5 },
        )];
        assert_eq!(turn_attachments(&pending), None);
        assert_eq!(super::context(&pending), None);
        assert_eq!(super::context(&[]), None);
    }

    #[test]
    fn a_label_is_made_safe_for_a_link_and_cut_at_200_units() {
        assert_eq!(label("  a[b]\\c\r\nd  e ", Kind::File), "a b c d e");
        assert_eq!(label("[]", Kind::Image), "image");
        let long = "x".repeat(250);
        assert_eq!(label(&long, Kind::File).len(), 200);
        // A pair that straddles the cut is dropped whole.
        let pair = format!("{}\u{1f600}", "x".repeat(199));
        assert_eq!(label(&pair, Kind::File), "x".repeat(199));
    }

    #[test]
    fn empty_text_sends_the_bootstrap_prompt() {
        assert_eq!(message_text("  hi \n"), "hi");
        assert_eq!(message_text(" \n"), BOOTSTRAP_PROMPT);
    }

    #[test]
    fn a_title_comes_from_the_attachments_when_there_is_no_text() {
        let image = item("1", Kind::Image, "a.png", ready("x"));
        let file = item("2", Kind::File, "notes  v2.txt", ready("y"));
        assert_eq!(title_seed(&[]), None);
        assert_eq!(
            title_seed(std::slice::from_ref(&image)).as_deref(),
            Some("Attached image")
        );
        assert_eq!(
            title_seed(&[image.clone(), image.clone()]).as_deref(),
            Some("Attached images")
        );
        assert_eq!(title_seed(&[file]).as_deref(), Some("File: notes v2.txt"));
    }
}
