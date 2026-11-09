use crate::attachments::ChipKind;
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// `Assets` carries only gpui-component's default icons; anything else from the catalog renders blank.
gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        Paperclip,
        Type,
        Box,
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
        RotateCcwClock,
        GitBranch,
        ShieldOff,
        Lock,
        LockOpen,
        PenLine,
        Sparkles,
        Settings,
        GitPullRequest,
        ChartNoAxesColumn,
        CircleArrowUp,
        CircleDashed,
        CircleDotDashed,
        CircleAlert,
        ChevronUp,
        CircleCheck,
        TriangleAlert,
        Gauge,
        TrendingUp,
        TrendingDown,
        Ticket,
        ExternalLink,
        Copy,
        LoaderCircle,
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
        Info,
        SquareSplitHorizontal,
        SquareSplitVertical,
        Plus,
        SlidersHorizontal,
        RotateCcw,
        Trash,
        Square,
        SquareTerminal,
        X,
        Star,
        StarFill,
        Zap,
        Settings2,
        Palette,
        MessageSquareText,
        MessagesSquare,
        Keyboard,
        Bot,
        Globe,
        Smartphone,
        CalendarClock,
        FolderTree,
        ShieldCheck,
        KeyRound,
        Wrench,
        FlaskConical,
        Archive,
        PanelsTopLeft,
        Undo2,
        Apple,
        Ellipsis,
        Sun,
        Moon,
        Paintbrush
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

pub const SNAP_SHOT: &str = "icons/snap-shot.svg";

/// `SnapShotIcon` (SettingsSidebarNav.tsx:63-73), which lucide does not ship.
const SNAP_SHOT_SVG: &str = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" "#,
    r#"fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" "#,
    r#"stroke-linejoin="round">"#,
    r#"<path d="M8 3H6a3 3 0 0 0-3 3v2M16 3h2a3 3 0 0 1 3 3v2M21 16v2a3 3 0 0 1-3 3h-2M8 21H6a3 3 0 0 1-3-3v-2"/>"#,
    r#"<rect width="10" height="8" x="7" y="8" rx="2"/><circle cx="12" cy="12" r="1.5"/></svg>"#,
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

/// The lucide icons the web's `pierre-icons.ts` sprite draws for documents and video, plus the
/// image icon of `ImageChipButton`.
const CHIP_ICONS: &[(&str, &[u8])] = &[
    (
        "chip/pdf.svg",
        include_bytes!("chip-icons/pdf.svg") as &[u8],
    ),
    (
        "chip/word.svg",
        include_bytes!("chip-icons/word.svg") as &[u8],
    ),
    (
        "chip/spreadsheet.svg",
        include_bytes!("chip-icons/spreadsheet.svg") as &[u8],
    ),
    (
        "chip/slides.svg",
        include_bytes!("chip-icons/slides.svg") as &[u8],
    ),
    (
        "chip/epub.svg",
        include_bytes!("chip-icons/epub.svg") as &[u8],
    ),
    (
        "chip/video.svg",
        include_bytes!("chip-icons/video.svg") as &[u8],
    ),
    (
        "chip/image.svg",
        include_bytes!("chip-icons/image.svg") as &[u8],
    ),
];

/// The icon asset of a chip kind, or `None` for a file without a document icon.
pub fn chip_icon(kind: ChipKind) -> Option<&'static str> {
    Some(match kind {
        ChipKind::File => return None,
        ChipKind::Image => "chip/image.svg",
        ChipKind::Video => "chip/video.svg",
        ChipKind::Pdf => "chip/pdf.svg",
        ChipKind::Word => "chip/word.svg",
        ChipKind::Spreadsheet => "chip/spreadsheet.svg",
        ChipKind::Slides => "chip/slides.svg",
        ChipKind::Epub => "chip/epub.svg",
    })
}

/// The version control and hosting marks of `SourceControlSettings` (`VcsIcon`, `SourceControlProviderIcon`).
const SOURCE_CONTROL_ICONS: &[(&str, &[u8])] = &[
    (
        "source-control/github-dark.svg",
        include_bytes!("source-control-icons/github-dark.svg") as &[u8],
    ),
    (
        "source-control/github-light.svg",
        include_bytes!("source-control-icons/github-light.svg") as &[u8],
    ),
    (
        "source-control/gitlab.svg",
        include_bytes!("source-control-icons/gitlab.svg") as &[u8],
    ),
    (
        "source-control/gitcafe.svg",
        include_bytes!("source-control-icons/gitcafe.svg") as &[u8],
    ),
    (
        "source-control/forgejo.svg",
        include_bytes!("source-control-icons/forgejo.svg") as &[u8],
    ),
    (
        "source-control/bitbucket.svg",
        include_bytes!("source-control-icons/bitbucket.svg") as &[u8],
    ),
    (
        "source-control/azure-devops.svg",
        include_bytes!("source-control-icons/azure-devops.svg") as &[u8],
    ),
    (
        "source-control/git.svg",
        include_bytes!("source-control-icons/git.svg") as &[u8],
    ),
    (
        "source-control/jj.svg",
        include_bytes!("source-control-icons/jj.svg") as &[u8],
    ),
];

/// The marks of the secret providers (`SecretProviderKindIcon`).
const SECRET_ICONS: &[(&str, &[u8])] = &[
    (
        "secret/one_password.svg",
        include_bytes!("source-control-icons/one_password.svg") as &[u8],
    ),
    (
        "secret/openbao.svg",
        include_bytes!("source-control-icons/openbao.svg") as &[u8],
    ),
    (
        "secret/aws_secrets_manager.svg",
        include_bytes!("source-control-icons/aws_secrets_manager.svg") as &[u8],
    ),
];

/// The mark of a secret provider kind, or `None` for a kind the web draws as a key.
pub fn secret_provider_icon(kind: &str) -> Option<&'static str> {
    SECRET_ICONS
        .iter()
        .find(|(path, _)| {
            path.strip_prefix("secret/")
                .and_then(|p| p.strip_suffix(".svg"))
                == Some(kind)
        })
        .map(|(path, _)| *path)
}

/// The mark of a version control system or hosting provider kind, or `None` for a kind without one.
pub fn source_control_icon(kind: &str, dark: bool) -> Option<&'static str> {
    let scheme = if dark { "dark" } else { "light" };
    [
        format!("source-control/{kind}-{scheme}.svg"),
        format!("source-control/{kind}.svg"),
    ]
    .iter()
    .find_map(|wanted| SOURCE_CONTROL_ICONS.iter().find(|(path, _)| path == wanted))
    .map(|(path, _)| *path)
}

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == MESSAGE_SQUARE_DASHED {
            return Ok(Some(Cow::Borrowed(MESSAGE_SQUARE_DASHED_SVG.as_bytes())));
        }
        if path == SNAP_SHOT {
            return Ok(Some(Cow::Borrowed(SNAP_SHOT_SVG.as_bytes())));
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
        if let Some((_, bytes)) = SOURCE_CONTROL_ICONS
            .iter()
            .find(|(known, _)| *known == path)
        {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some((_, bytes)) = SECRET_ICONS.iter().find(|(known, _)| *known == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some((_, bytes)) = CHIP_ICONS.iter().find(|(known, _)| *known == path) {
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
        if SNAP_SHOT.starts_with(path) {
            paths.push(SNAP_SHOT.into());
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
    fn every_chip_icon_loads() {
        for (path, _) in CHIP_ICONS {
            assert!(AppAssets.load(path).unwrap().is_some(), "{path}");
        }
        assert!(chip_icon(ChipKind::File).is_none());
        assert!(chip_icon(ChipKind::Pdf).is_some());
    }

    #[test]
    fn every_catalog_icon_the_app_uses_loads() {
        for icon in [
            IconName::Paperclip,
            IconName::Settings2,
            IconName::Palette,
            IconName::Sun,
            IconName::Moon,
            IconName::Paintbrush,
            IconName::MessageSquareText,
            IconName::MessagesSquare,
            IconName::Keyboard,
            IconName::Bot,
            IconName::Globe,
            IconName::Smartphone,
            IconName::CalendarClock,
            IconName::FolderTree,
            IconName::ShieldCheck,
            IconName::KeyRound,
            IconName::Wrench,
            IconName::FlaskConical,
            IconName::Archive,
            IconName::PanelsTopLeft,
            IconName::Undo2,
            IconName::File,
            IconName::SquareMenu,
            IconName::PanelBottom,
            IconName::SquareSplitHorizontal,
            IconName::SquareSplitVertical,
            IconName::Plus,
            IconName::Trash,
            IconName::Square,
            IconName::SquareTerminal,
            IconName::X,
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
            IconName::RotateCcwClock,
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
        assert!(AppAssets.load(SNAP_SHOT).ok().flatten().is_some());
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
