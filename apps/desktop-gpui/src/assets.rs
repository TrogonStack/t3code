use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// `Assets` carries only gpui-component's default icons; anything else from the catalog renders blank.
gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        Paperclip, Pin, PinOff, Server, Cloud, Terminal, Monitor, Laptop, HardDrive, Computer
    ]
);

pub const MESSAGE_SQUARE_DASHED: &str = "icons/message-square-dashed.svg";

/// lucide-react 0.564.0 `message-square-dashed`, the icon the server gives the no-project workspace.
const MESSAGE_SQUARE_DASHED_SVG: &str = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" "#,
    r#"fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" "#,
    r#"stroke-linejoin="round">"#,
    r#"<path d="M14 3h2"/><path d="M16 19h-2"/><path d="M2 12v-2"/>"#,
    r#"<path d="M2 16v5.286a.71.71 0 0 0 1.212.502l1.149-1.149"/>"#,
    r#"<path d="M20 19a2 2 0 0 0 2-2v-1"/><path d="M22 10v2"/>"#,
    r#"<path d="M22 6V5a2 2 0 0 0-2-2"/><path d="M4 3a2 2 0 0 0-2 2v1"/>"#,
    r#"<path d="M8 19h2"/><path d="M8 3h2"/></svg>"#,
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == MESSAGE_SQUARE_DASHED {
            return Ok(Some(Cow::Borrowed(MESSAGE_SQUARE_DASHED_SVG.as_bytes())));
        }
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit_assets::Assets.list(path)?;
        if MESSAGE_SQUARE_DASHED.starts_with(path) {
            paths.push(MESSAGE_SQUARE_DASHED.into());
        }
        paths.extend(ExtraIcons.list(path)?);
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit_assets::IconName;

    #[test]
    fn every_catalog_icon_the_app_uses_loads() {
        for icon in [
            IconName::Paperclip,
            IconName::Pin,
            IconName::PinOff,
            IconName::Search,
            IconName::Server,
            IconName::Cloud,
            IconName::Terminal,
            IconName::Monitor,
            IconName::Laptop,
            IconName::HardDrive,
            IconName::Computer,
        ] {
            let loaded = AppAssets.load(&icon.path()).ok().flatten();
            assert!(loaded.is_some(), "{} is not embedded", icon.path());
        }
        assert!(
            AppAssets
                .load(MESSAGE_SQUARE_DASHED)
                .ok()
                .flatten()
                .is_some()
        );
    }
}
