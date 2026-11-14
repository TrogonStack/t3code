//! `ThreadsSettingsPanel` (SettingsPanels.tsx:1496-1900), with the new thread rows of
//! `ProjectDefaultsSettings` and `SnoozePresetsRow`.

use super::super::components::disclosure;
use super::model_chip::Slot;
use super::*;
use crate::settings_threads as threads;

pub(super) const RESET: &str = "Reset to default";

impl ScopedPanel {
    pub(super) fn threads(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut sections = vec![
            self.new_projects(ctx, window, cx),
            self.new_threads(ctx, window, cx),
        ];
        if !ctx.in_project {
            sections.push(self.snooze(ctx, window, cx));
        }
        sections.push(self.usage_limits(ctx, window, cx));
        if ctx.connected {
            sections.push(self.auto_settle(ctx, window, cx));
        }
        sections.push(self.text_generation(ctx, window, cx));
        sections
    }

    fn new_projects(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let field = Field::BaseDirectory;
        let row = self.setting(
            ids!("add-project-starts-in"),
            "Add project starts in",
            "Leave empty to use \"~/\" when the Add Project browser opens.",
            field,
            self.reset(
                "settings-base-directory-reset",
                RESET,
                &[field],
                ctx,
                window,
                cx,
            ),
            self.line(field, 288., self.gate(field, cx).is_some(), window, cx),
            None,
            ctx,
            window,
            cx,
        );
        section("New projects", group(vec![row], cx)).into_any_element()
    }

    fn new_threads(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project = ctx.in_project;
        let default_chips = self.model_chips(Slot::Default, ctx, cx);
        let fallback_chips = self.model_chips(Slot::Fallback, ctx, cx);
        let model = self.value(Field::DefaultModel, ctx);
        let mut rows = vec![self.setting(
            ids!("default-model"),
            "Model",
            if project {
                "Model for new threads in this project."
            } else if model.is_null() {
                "Default model for new threads, selected automatically. Projects can override it."
            } else {
                "Default model for new threads. Projects can override it."
            },
            Field::DefaultModel,
            self.reset(
                "settings-default-model-reset",
                RESET,
                &[Field::DefaultModel],
                ctx,
                window,
                cx,
            ),
            default_chips,
            None,
            ctx,
            window,
            cx,
        )];
        if !project {
            rows.push(self.setting(
                ids!("fallback-model"),
                "Fallback model",
                "Used for new threads when the default model's provider is out of usage.",
                Field::FallbackModel,
                self.reset(
                    "settings-fallback-model-reset",
                    RESET,
                    &[Field::FallbackModel],
                    ctx,
                    window,
                    cx,
                ),
                fallback_chips,
                None,
                ctx,
                window,
                cx,
            ));
        }
        let mode = self.value(Field::RuntimeMode, ctx);
        rows.push(self.setting(
            ids!("default-permissions"),
            "Permissions",
            if project {
                "Permissions for new threads in this project."
            } else {
                "Default permissions for new threads. Projects can override them."
            },
            Field::RuntimeMode,
            self.reset(
                "settings-default-permissions-reset",
                RESET,
                &[Field::RuntimeMode],
                ctx,
                window,
                cx,
            ),
            self.select(
                Pick::Permissions,
                Pick::Permissions.selected(&mode),
                192.,
                self.gate(Field::RuntimeMode, cx),
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        ));
        let workspace = self.value(Field::ThreadEnvMode, ctx);
        rows.push(self.setting(
            ids!("new-threads"),
            "Workspace",
            if project {
                "Where new threads in this project start."
            } else {
                "Where new threads start. Projects and their t3.json can override it."
            },
            Field::ThreadEnvMode,
            if project {
                None
            } else {
                self.reset(
                    "settings-default-workspace-reset",
                    RESET,
                    &[Field::ThreadEnvMode],
                    ctx,
                    window,
                    cx,
                )
            },
            self.select(
                Pick::Workspace,
                Pick::Workspace.selected(&workspace),
                192.,
                self.gate(Field::ThreadEnvMode, cx),
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        ));
        section("New threads", group(rows, cx)).into_any_element()
    }

    fn usage_limits(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let rows = vec![
            self.setting(
                ids!("auto-resume-limited-threads"),
                "Resume after usage limits",
                "Automatically resume threads when their usage limits reset. You can cancel auto-resume in each thread.",
                Field::ResumeLimited,
                None,
                self.flag(
                    "settings-auto-resume-limited-threads",
                    Field::ResumeLimited,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
            self.setting(
                ids!("snooze-limited-threads"),
                "Snooze until limits reset",
                "Snooze threads until their usage limits reset. Enable auto-resume to continue the agent after the reset.",
                Field::SnoozeLimited,
                None,
                self.flag(
                    "settings-snooze-limited-threads",
                    Field::SnoozeLimited,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
        ];
        section("Usage limits", group(rows, cx)).into_any_element()
    }

    fn auto_settle(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let days = self.value(Field::SettleDays, ctx);
        let on = !days.is_null();
        let mut rows = vec![
            self.setting(
                ids!("auto-settle-merged-threads"),
                "Auto-settle merged threads",
                "Settle a thread when its pull request merges. Closed pull requests still settle automatically.",
                Field::SettleOnMerge,
                self.reset(
                    "settings-auto-settle-merged-reset",
                    RESET,
                    &[Field::SettleOnMerge],
                    ctx,
                    window,
                    cx,
                ),
                self.flag(
                    "settings-auto-settle-merged-threads",
                    Field::SettleOnMerge,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
            self.setting(
                ids!("auto-settle-inactive-threads"),
                "Auto-settle inactive threads",
                "Sidebar threads with no activity for this long settle automatically.",
                Field::SettleDays,
                self.reset(
                    "settings-auto-settle-inactive-reset",
                    RESET,
                    &[Field::SettleDays],
                    ctx,
                    window,
                    cx,
                ),
                self.toggle(
                    "settings-auto-settle-inactive-threads",
                    Field::SettleDays,
                    on,
                    json!(threads::AUTO_SETTLE_DEFAULT_DAYS),
                    Value::Null,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
        ];
        if on {
            rows.push(self.setting(
                ids!("days-before-auto-settle"),
                "Days of inactivity before auto-settle",
                "Any new activity un-settles a thread automatically.",
                Field::SettleDays,
                None,
                self.line(
                    Field::SettleDays,
                    112.,
                    self.gate(Field::SettleDays, cx).is_some(),
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
            rows.push(self.setting(
                ids!("keep-threads-with-pull-requests"),
                "Keep threads with a PR",
                "Inactivity does not settle threads with a pull request. Merge and close still do.",
                Field::SettleKeepPr,
                None,
                self.flag(
                    "settings-keep-threads-with-pull-requests",
                    Field::SettleKeepPr,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
        }
        section("Auto-settle", group(rows, cx)).into_any_element()
    }

    fn text_generation(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips = self.model_chips(Slot::Text, ctx, cx);
        let mut rows = vec![self.setting(
            ids!("text-generation-model"),
            "Text generation model",
            "For thread titles and other generated text. Source control has its own setting. Text generators from Providers also appear here.",
            Field::TextModel,
            if ctx.connected {
                self.reset(
                    "settings-text-generation-model-reset",
                    RESET,
                    &[Field::TextModel],
                    ctx,
                    window,
                    cx,
                )
            } else {
                None
            },
            chips,
            None,
            ctx,
            window,
            cx,
        )];
        let instructions = self.text_area(
            Field::TitleInstructions,
            "Thread title instructions",
            self.gate(Field::TitleInstructions, cx).is_some(),
            window,
            cx,
        );
        let mode = self.value(Field::Streaming, ctx);
        let streaming = threads::STREAMING_MODES[Pick::Streaming.selected(&mode)];
        let advanced = vec![
            self.setting(
                ids!("thread-title-instructions"),
                "Thread title instructions",
                "Appended to the prompt for new and regenerated thread titles.",
                Field::TitleInstructions,
                self.reset(
                    "settings-thread-title-instructions-reset",
                    RESET,
                    &[Field::TitleInstructions],
                    ctx,
                    window,
                    cx,
                ),
                div().into_any_element(),
                Some(instructions),
                ctx,
                window,
                cx,
            ),
            self.setting(
                ids!("response-streaming"),
                "Response streaming",
                streaming.2,
                Field::Streaming,
                self.reset(
                    "settings-response-streaming-reset",
                    RESET,
                    &[Field::Streaming],
                    ctx,
                    window,
                    cx,
                ),
                self.select(
                    Pick::Streaming,
                    Pick::Streaming.selected(&mode),
                    224.,
                    self.gate(Field::Streaming, cx),
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
        ];
        let view = cx.entity();
        rows.push(disclosure(
            "settings-threads-advanced",
            self.advanced_open,
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })
            }),
            advanced,
            window,
            cx,
        ));
        section("Text generation", group(rows, cx)).into_any_element()
    }
}
