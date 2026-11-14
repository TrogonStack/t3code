//! `ThreadsSettingsPanel` (SettingsPanels.tsx:1496-1900), with the new thread rows of
//! `ProjectDefaultsSettings` and `SnoozePresetsRow`.

use super::super::components::disclosure;
use super::super::devices_panel::widgets::{Variant, button, icon_button};
use super::*;
use crate::settings_threads as threads;

const RESET: &str = "Reset to default";

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

    /// A model selection's name; the web's picker is not ported.
    fn model_text(&self, field: Field, ctx: &Ctx, none: &'static str) -> String {
        sc::model_label(&self.value(field, ctx)).unwrap_or_else(|| none.to_owned())
    }

    fn new_threads(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let project = ctx.in_project;
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
            Self::label_control(self.model_text(Field::DefaultModel, ctx, "Automatic"), cx),
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
                Self::label_control(self.model_text(Field::FallbackModel, ctx, "None"), cx),
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

    fn snooze(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let field = Field::SnoozePresets;
        let presets = self
            .value(field, ctx)
            .as_array()
            .cloned()
            .unwrap_or_default();
        let defaults = threads::default_presets()
            .as_array()
            .cloned()
            .unwrap_or_default();
        let reset = (!threads::same_presets(&presets, &defaults) && self.gate(field, cx).is_none())
            .then(|| {
                self.reset_with(
                    "settings-snooze-reset",
                    RESET,
                    vec![(field, None)],
                    window,
                    cx,
                )
            });
        let inert = self.gate(field, cx).is_some();
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let mut list = v_flex().gap(px(6.)).pt_3().pb_2();
        for (index, preset) in presets.iter().enumerate() {
            let view = view.clone();
            let kept: Vec<Value> = presets
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, preset)| preset.clone())
                .collect();
            list = list.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child(threads::preset_label(preset)),
                    )
                    .child(
                        icon_button(
                            ("settings-snooze-remove", index),
                            Variant::GhostMuted,
                            !inert,
                            cx,
                        )
                        .tooltip(tooltip(format!("Remove snooze option {}", index + 1)))
                        .when(!inert, |remove| {
                            remove.on_click(move |_, _, cx| {
                                let kept = kept.clone();
                                view.update(cx, |this, cx| {
                                    this.write(vec![(field, Some(Value::Array(kept)))], cx)
                                })
                            })
                        })
                        .child(Icon::new(Icons::X).size(px(14.))),
                    ),
            );
        }
        let full = presets.len() >= threads::MAX_PRESETS;
        let next = threads::next_candidate(&presets);
        let mut grown = presets.clone();
        grown.push(next);
        let add_view = view.clone();
        list = list.child(
            h_flex()
                .items_center()
                .gap_2()
                .child(
                    button(
                        "settings-snooze-add",
                        Variant::GhostMuted,
                        !full && !inert,
                        cx,
                    )
                    .when(!full && !inert, |add| {
                        add.on_click(move |_, _, cx| {
                            let grown = grown.clone();
                            add_view.update(cx, |this, cx| {
                                this.write(vec![(field, Some(Value::Array(grown)))], cx)
                            })
                        })
                    })
                    .child(Icon::new(Icons::Plus).size(px(14.)))
                    .child("Add option"),
                )
                .when(full, |footer| {
                    footer.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Up to {} options", threads::MAX_PRESETS)),
                    )
                }),
        );
        let row = self.setting(
            ids!("snooze-options"),
            "Snooze options",
            "Choices in every snooze menu, listed soonest first. Weekdays mean the next one after today.",
            field,
            reset,
            div().into_any_element(),
            Some(list.into_any_element()),
            ctx,
            window,
            cx,
        );
        section("Snooze", group(vec![row], cx)).into_any_element()
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
        &self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = if ctx.connected {
            self.model_text(Field::TextModel, ctx, "Automatic")
        } else {
            "Connect an environment to choose its text generation model.".to_owned()
        };
        let mut rows = vec![self.setting(
            ids!("text-generation-model"),
            "Text generation model",
            "Used for thread titles and other generated text on connected devices with this provider. Source control can override it.",
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
            Self::label_control(model, cx),
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
