//! `WorktreesSettingsPanel` (WorktreesSettings.tsx), with `WorktreesDirectoryRow` and
//! `WorktreeCleanupSection` (StorageSettings.tsx), `BranchNamingSettings.tsx` and
//! `WorktreeSubmodulesRow` (ProjectDefaultsSettings.tsx).

use super::super::components::disclosure;
use super::*;
use crate::settings_worktrees as worktrees;

const RESET: &str = "Reset to default";

impl ScopedPanel {
    pub(super) fn worktrees(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            self.creation(ctx, window, cx),
            self.branches(ctx, window, cx),
            self.cleanup(ctx, window, cx),
        ]
    }

    fn creation(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();
        if !ctx.in_project {
            let field = Field::WorktreesDir;
            rows.push(self.setting(
                ids!("storage-worktrees-location"),
                "Worktree location",
                "Folder where new worktrees are created, on any drive, such as D:\\worktrees or ~/worktrees. Existing worktrees stay where they are. Leave empty to use the T3 home folder.",
                field,
                self.reset("settings-worktrees-location-reset", RESET, &[field], ctx, window, cx),
                self.line(field, 288., self.gate(field, cx).is_some(), window, cx),
                None,
                ctx,
                window,
                cx,
            ));
        }
        rows.push(self.setting(
            ids!("start-from-origin"),
            "Start from remote",
            "Uses the latest commit of the base branch’s tracked remote branch, even if its name differs. Otherwise uses the same-named branch on origin. Uses your local branch if that remote branch is missing.",
            Field::StartFromOrigin,
            self.reset(
                "settings-start-from-origin-reset",
                RESET,
                &[Field::StartFromOrigin],
                ctx,
                window,
                cx,
            ),
            self.flag(
                "settings-start-from-origin",
                Field::StartFromOrigin,
                ctx,
                window,
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        ));
        let submodules = self.submodules(ctx, window, cx);
        let view = cx.entity();
        rows.push(disclosure(
            "settings-worktree-creation-advanced",
            self.advanced_open,
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })
            }),
            vec![submodules],
            window,
            cx,
        ));
        section("Creation", group(rows, cx)).into_any_element()
    }

    fn submodules(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let field = Field::Submodules;
        let value = self.value(field, ctx);
        let reset = if ctx.in_project {
            None
        } else {
            self.reset(
                "settings-worktree-submodules-reset",
                RESET,
                &[field],
                ctx,
                window,
                cx,
            )
        };
        self.setting(
            ids!("worktree-submodules"),
            "Submodules",
            worktrees::submodules_description(ctx.in_project),
            field,
            reset,
            self.select(
                Pick::Submodules,
                Pick::Submodules.selected(&value),
                160.,
                self.gate(field, cx),
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        )
    }

    fn branches(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.value(Field::BranchMode, ctx);
        let semantic = mode.as_str() == Some("semantic");
        let description = v_flex()
            .gap_1()
            .child("Choose how new worktree branches are named from your first message.")
            .when(semantic, |column| column.child(worktrees::SEMANTIC_STATUS));
        let mut rows = vec![self.setting(
            ids!("worktree-branch-naming"),
            "Worktree branch naming",
            description,
            Field::BranchMode,
            self.reset(
                "settings-worktree-branch-naming-reset",
                RESET,
                &[Field::BranchMode],
                ctx,
                window,
                cx,
            ),
            self.select(
                Pick::BranchMode,
                Pick::BranchMode.selected(&mode),
                192.,
                self.gate(Field::BranchMode, cx),
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        )];
        match worktrees::BRANCH_MODES[Pick::BranchMode.selected(&mode)].0 {
            "static" => {
                let field = Field::BranchPrefix;
                rows.push(self.setting(
                    ids!("worktree-branch-prefix"),
                    "Branch prefix",
                    "For example, t3 or t3/ produces t3/add-search. Leave empty for no prefix.",
                    field,
                    self.reset(
                        "settings-worktree-branch-prefix-reset",
                        RESET,
                        &[field],
                        ctx,
                        window,
                        cx,
                    ),
                    self.line(field, 224., self.gate(field, cx).is_some(), window, cx),
                    None,
                    ctx,
                    window,
                    cx,
                ));
            }
            "custom" => {
                let field = Field::BranchInstructions;
                let instructions = self.text_area(
                    field,
                    "Branch naming instructions",
                    self.gate(field, cx).is_some(),
                    window,
                    cx,
                );
                rows.push(self.setting(
                    ids!("worktree-branch-instructions"),
                    "Branch naming instructions",
                    "Appended to the naming prompt. The model returns the complete branch name; no prefix or suffix is added.",
                    field,
                    self.reset(
                        "settings-worktree-branch-instructions-reset",
                        RESET,
                        &[field],
                        ctx,
                        window,
                        cx,
                    ),
                    div().into_any_element(),
                    Some(instructions),
                    ctx,
                    window,
                    cx,
                ));
            }
            _ => {}
        }
        section("Branches", group(rows, cx)).into_any_element()
    }

    /// The field the cleanup rows' markers and inert state follow.
    fn rules_field(ctx: &Ctx) -> Field {
        if ctx.in_project {
            Field::WorktreeCleanup
        } else {
            Field::StorageCleanup
        }
    }

    /// A rule's switch, `aria-label` as its title.
    fn rule_flag(
        &self,
        id: &'static str,
        key: &'static str,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self.rules(ctx)[key].as_bool().unwrap_or(false);
        self.rule_switch(id, key, checked, json!(!checked), ctx, window, cx)
    }

    /// A switch that writes `next` to the rule.
    #[allow(clippy::too_many_arguments)]
    fn rule_switch(
        &self,
        id: &'static str,
        key: &'static str,
        checked: bool,
        next: Value,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let why = self.gate(Self::rules_field(ctx), cx);
        let view = cx.entity();
        let toggle: Toggle = if why.is_some() {
            Rc::new(|_, _| {})
        } else {
            Rc::new(move |_, cx| {
                let next = next.clone();
                view.update(cx, |this, cx| {
                    let ctx = this.ctx(cx);
                    this.write_rules(key, next, &ctx, cx)
                })
            })
        };
        Self::inert(id, why, switch(id, checked, toggle, window, cx))
    }

    /// `RetentionControl`: the days field while on, "Off" otherwise, and the switch.
    fn retention(
        &self,
        id: &'static str,
        rule: Rule,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let value = self.rules(ctx)[rule.wire()].clone();
        let on = !value.is_null();
        let next = if on {
            Value::Null
        } else {
            json!(worktrees::RETENTION_DEFAULT_DAYS)
        };
        let disabled = self.gate(Self::rules_field(ctx), cx).is_some();
        let body = if on {
            h_flex()
                .gap_2()
                .items_center()
                .child(self.line(rule, 72., disabled, window, cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("days"),
                )
                .into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Off")
                .into_any_element()
        };
        h_flex()
            .gap_3()
            .items_center()
            .child(body)
            .child(self.rule_switch(id, rule.wire(), on, next, ctx, window, cx))
            .into_any_element()
    }

    fn cleanup(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let field = Self::rules_field(ctx);
        let policy = self.value(Field::WorktreeCleanup, ctx);
        let mode = worktrees::cleanup_mode(&policy);
        let mut rows = Vec::new();
        if ctx.in_project {
            rows.push(self.setting(
                ids!("worktree-cleanup-mode"),
                "Automatic worktree cleanup",
                worktrees::cleanup_mode_description(mode),
                Field::WorktreeCleanup,
                None,
                self.select(
                    Pick::CleanupMode,
                    Pick::CleanupMode.selected(&policy),
                    128.,
                    self.gate(Field::WorktreeCleanup, cx),
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
        }
        if !ctx.in_project || mode == "custom" {
            let rules = self.rules(ctx);
            rows.push(self.setting(
                ids!("worktree-cleanup-on-delete"),
                "Delete worktrees with deleted threads",
                "Remove unused worktrees when active or archived threads are deleted. Worktrees with local changes are kept.",
                field,
                None,
                self.rule_flag(
                    "settings-worktree-cleanup-on-delete",
                    "worktreeOnDelete",
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
            rows.push(self.setting(
                ids!("worktree-cleanup-on-merge"),
                "Delete merged worktrees",
                "Remove worktrees whose pull request is merged and whose commits are included in the default branch.",
                field,
                None,
                self.rule_flag(
                    "settings-worktree-cleanup-on-merge",
                    "worktreeOnMerge",
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
            rows.push(self.setting(
                ids!("worktree-cleanup-after-days"),
                "Delete inactive worktrees",
                "Remove worktrees after their threads have been inactive for this many days. Branches and thread history are kept.",
                field,
                None,
                self.retention(
                    "settings-worktree-cleanup-after-days",
                    Rule::AfterDays,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ));
            let keep_when = rules["worktreeKeepWhen"].clone();
            let advanced = vec![
                self.setting(
                    ids!("worktree-cleanup-settled-days"),
                    "Delete settled worktrees",
                    "Remove worktrees this many days after their thread settles. Set 0 days to remove them at the next cleanup. Branches and thread history are kept.",
                    field,
                    None,
                    self.retention(
                        "settings-worktree-cleanup-settled-days",
                        Rule::SettledDays,
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
                    ids!("worktree-cleanup-unchanged"),
                    "Delete unchanged worktrees",
                    "Remove worktrees with no commits beyond the default branch.",
                    field,
                    None,
                    self.rule_flag(
                        "settings-worktree-cleanup-unchanged",
                        "worktreeUnchanged",
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
                    ids!("storage-worktree-keep-when"),
                    "Keep worktrees with local changes",
                    worktrees::keep_when_description(keep_when.as_str().unwrap_or_default()),
                    field,
                    None,
                    self.select(
                        Pick::KeepWhen,
                        Pick::KeepWhen.selected(&keep_when),
                        192.,
                        self.gate(field, cx),
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
                "settings-worktree-cleanup-advanced",
                self.cleanup_open,
                Rc::new(move |_, cx| {
                    view.update(cx, |this, cx| {
                        this.cleanup_open = !this.cleanup_open;
                        cx.notify();
                    })
                }),
                advanced,
                window,
                cx,
            ));
        }
        section("Cleanup", group(rows, cx)).into_any_element()
    }
}
