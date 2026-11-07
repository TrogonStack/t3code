use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// `Assets` carries only gpui-component's default icons; anything else from the catalog renders blank.
gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        Paperclip,
        Pin,
        PinOff,
        Server,
        Cloud,
        Terminal,
        Monitor,
        Laptop,
        HardDrive,
        Computer,
        ListFilter,
        FolderPlus,
        SquarePen,
        Folder,
        FolderGit,
        FolderGit2,
        GitBranch,
        ShieldOff,
        Lock,
        PenLine,
        Sparkles,
        Settings,
        GitPullRequest,
        ChartNoAxesColumn,
        CircleArrowUp,
        CircleDashed,
        CircleDotDashed,
        CircleAlert,
        Clock,
        Check,
        GitPullRequestArrow,
        GitPullRequestDraft,
        GitPullRequestClosed,
        GitMerge,
        Layers,
        ArrowRightLeft,
        SquareMenu,
        PanelBottom,
        ChevronDown,
        SquareArrowOutUpRight,
        FileDiff,
        CloudUpload,
        GitBranchPlus,
        GitCommitHorizontal,
        CloudDownload,
        Info
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

/// The web's dev `SidebarStageBackdrop` art, dark palette, faded into the sidebar.
pub const STAGE_DEV: &str = "images/stage-dev.svg";
/// The same art cropped like the web's `StageBackdropButtonArt`, for the send button.
pub const STAGE_DEV_BUTTON: &str = "images/stage-dev-button.svg";

/// The web's provider icons (`ProviderInstanceIcon`), one file per color scheme where the fills differ.
const PROVIDER_ICONS: &[(&str, &[u8])] = &[
    (
        "providers/antigravity.png",
        include_bytes!("provider-icons/antigravity.png") as &[u8],
    ),
    (
        "providers/claudeAgent.svg",
        include_bytes!("provider-icons/claudeAgent.svg") as &[u8],
    ),
    (
        "providers/codex-dark.svg",
        include_bytes!("provider-icons/codex-dark.svg") as &[u8],
    ),
    (
        "providers/codex-light.svg",
        include_bytes!("provider-icons/codex-light.svg") as &[u8],
    ),
    (
        "providers/cursor-dark.svg",
        include_bytes!("provider-icons/cursor-dark.svg") as &[u8],
    ),
    (
        "providers/cursor-light.svg",
        include_bytes!("provider-icons/cursor-light.svg") as &[u8],
    ),
    (
        "providers/fx-dark.svg",
        include_bytes!("provider-icons/fx-dark.svg") as &[u8],
    ),
    (
        "providers/fx-light.svg",
        include_bytes!("provider-icons/fx-light.svg") as &[u8],
    ),
    (
        "providers/grok-dark.svg",
        include_bytes!("provider-icons/grok-dark.svg") as &[u8],
    ),
    (
        "providers/grok-light.svg",
        include_bytes!("provider-icons/grok-light.svg") as &[u8],
    ),
    (
        "providers/kiro.svg",
        include_bytes!("provider-icons/kiro.svg") as &[u8],
    ),
    (
        "providers/muse.svg",
        include_bytes!("provider-icons/muse.svg") as &[u8],
    ),
    (
        "providers/opencode-dark.svg",
        include_bytes!("provider-icons/opencode-dark.svg") as &[u8],
    ),
    (
        "providers/opencode-light.svg",
        include_bytes!("provider-icons/opencode-light.svg") as &[u8],
    ),
    (
        "providers/pi-dark.svg",
        include_bytes!("provider-icons/pi-dark.svg") as &[u8],
    ),
    (
        "providers/pi-light.svg",
        include_bytes!("provider-icons/pi-light.svg") as &[u8],
    ),
];

/// The icon asset for a driver, or `None` for the drivers the web draws as initials.
pub fn provider_icon(driver: &str, dark: bool) -> Option<&'static str> {
    let scheme = if dark { "dark" } else { "light" };
    [
        format!("providers/{driver}-{scheme}.svg"),
        format!("providers/{driver}.svg"),
        format!("providers/{driver}.png"),
    ]
    .iter()
    .find_map(|wanted| PROVIDER_ICONS.iter().find(|(path, _)| path == wanted))
    .map(|(path, _)| *path)
}

/// The editor brand marks the web's `OpenInPicker` draws that the provider icons do not already cover.
const EDITOR_ICONS: &[(&str, &[u8])] = &[
    (
        "editors/finder.svg",
        include_bytes!("editor-icons/finder.svg") as &[u8],
    ),
    (
        "editors/idea.svg",
        include_bytes!("editor-icons/idea.svg") as &[u8],
    ),
    (
        "editors/vscode.svg",
        include_bytes!("editor-icons/vscode.svg") as &[u8],
    ),
    (
        "editors/zed-dark.svg",
        include_bytes!("editor-icons/zed-dark.svg") as &[u8],
    ),
    (
        "editors/zed-light.svg",
        include_bytes!("editor-icons/zed-light.svg") as &[u8],
    ),
];

/// The brand icon for an editor id, or `None` where the web falls back to a generic glyph.
pub fn editor_icon(editor: &str, dark: bool) -> Option<&'static str> {
    let scheme = if dark { "dark" } else { "light" };
    match editor {
        "cursor" | "kiro" | "antigravity" => provider_icon(editor, dark),
        "vscode" => Some("editors/vscode.svg"),
        "idea" => Some("editors/idea.svg"),
        "file-manager" => Some("editors/finder.svg"),
        "zed" => EDITOR_ICONS
            .iter()
            .map(|(path, _)| *path)
            .find(|path| *path == format!("editors/zed-{scheme}.svg")),
        _ => None,
    }
}

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == MESSAGE_SQUARE_DASHED {
            return Ok(Some(Cow::Borrowed(MESSAGE_SQUARE_DASHED_SVG.as_bytes())));
        }
        if path == STAGE_DEV {
            return Ok(Some(Cow::Borrowed(include_bytes!("stage-dev.svg"))));
        }
        if path == STAGE_DEV_BUTTON {
            return Ok(Some(Cow::Borrowed(include_bytes!("stage-dev-button.svg"))));
        }
        if let Some((_, bytes)) = PROVIDER_ICONS.iter().find(|(known, _)| *known == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some((_, bytes)) = EDITOR_ICONS.iter().find(|(known, _)| *known == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
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
            IconName::SquareMenu,
            IconName::PanelBottom,
            IconName::ChevronDown,
            IconName::SquareArrowOutUpRight,
            IconName::FileDiff,
            IconName::Pin,
            IconName::PinOff,
            IconName::Search,
            IconName::Server,
            IconName::Cloud,
            IconName::Terminal,
            IconName::Monitor,
            IconName::Laptop,
            IconName::HardDrive,
            IconName::ListFilter,
            IconName::FolderPlus,
            IconName::SquarePen,
            IconName::Folder,
            IconName::FolderGit,
            IconName::FolderGit2,
            IconName::GitBranch,
            IconName::ShieldOff,
            IconName::Lock,
            IconName::PenLine,
            IconName::Sparkles,
            IconName::GitPullRequest,
            IconName::ChartNoAxesColumn,
            IconName::CircleArrowUp,
            IconName::CircleDashed,
            IconName::CircleDotDashed,
            IconName::CircleAlert,
            IconName::Clock,
            IconName::Check,
            IconName::GitPullRequestArrow,
            IconName::GitPullRequestDraft,
            IconName::GitPullRequestClosed,
            IconName::GitMerge,
            IconName::Layers,
            IconName::ArrowRightLeft,
            IconName::Computer,
            IconName::CloudUpload,
            IconName::GitBranchPlus,
            IconName::GitCommitHorizontal,
            IconName::CloudDownload,
            IconName::Info,
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

    #[test]
    fn editor_icons_resolve_and_load() {
        assert_eq!(editor_icon("zed", true), Some("editors/zed-dark.svg"));
        assert_eq!(editor_icon("zed", false), Some("editors/zed-light.svg"));
        assert_eq!(
            editor_icon("cursor", true),
            Some("providers/cursor-dark.svg")
        );
        assert_eq!(editor_icon("trae", true), None);
        for id in [
            "cursor",
            "kiro",
            "vscode",
            "zed",
            "antigravity",
            "idea",
            "file-manager",
        ] {
            for dark in [true, false] {
                let path = editor_icon(id, dark).expect(id);
                assert!(AppAssets.load(path).ok().flatten().is_some(), "{path}");
            }
        }
    }

    #[test]
    fn provider_icons_follow_the_color_scheme() {
        assert_eq!(
            provider_icon("codex", true),
            Some("providers/codex-dark.svg")
        );
        assert_eq!(
            provider_icon("codex", false),
            Some("providers/codex-light.svg")
        );
        assert_eq!(
            provider_icon("claudeAgent", true),
            Some("providers/claudeAgent.svg")
        );
        assert_eq!(
            provider_icon("antigravity", false),
            Some("providers/antigravity.png")
        );
        assert_eq!(provider_icon("acpRegistry", false), None);
        for (path, _) in PROVIDER_ICONS {
            assert!(AppAssets.load(path).ok().flatten().is_some(), "{path}");
        }
    }
}
