use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// `Assets` carries only gpui-component's default icons; anything else from the catalog renders blank.
gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        Paperclip, Pin, PinOff, Server, Cloud, Terminal, Monitor, Laptop, HardDrive, Computer
    ]
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit_assets::Assets.list(path)?;
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
    }
}
