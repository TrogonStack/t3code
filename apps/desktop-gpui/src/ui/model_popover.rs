//! The web's `ProviderModelPicker` popover: the rail and model list of `ModelPickerContent.tsx`,
//! the effort column of `ComposerModelEffort.tsx`, and the footer's speed and permissions controls.

use super::*;
use crate::model_picker::{Focus, Listing, Tab};
use crate::providers::{Control, PickerInstance};

/// `h-86.5`: the body, rail and columns, above the footer.
const BODY_HEIGHT: f32 = 346.;
/// `w-screen max-w-90`: the rail and model list together.
const PICKER_WIDTH: f32 = 360.;
/// The effort column's `w-48`.
const EFFORT_WIDTH: f32 = 192.;
/// The rail's `w-11`.
const RAIL_WIDTH: f32 = 44.;
/// `COLUMN_ROW`'s `h-8`.
const ROW_HEIGHT: f32 = 32.;

pub(super) struct Panel {
    pub view: Entity<MainView>,
    pub search: Entity<InputState>,
    pub instances: Vec<PickerInstance>,
    pub trigger: Trigger,
    pub tab: Tab,
    pub expanded: bool,
    pub highlight: usize,
    pub focus: Focus,
    pub effort: usize,
    pub permissions: Entity<PermissionsPanel>,
    pub permissions_open: bool,
    pub mode: RuntimeMode,
}

/// The tokens every part of the popover paints with.
#[derive(Clone, Copy)]
struct Palette {
    foreground: Hsla,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    warning: Hsla,
    fill: Hsla,
    hover: Hsla,
    track: Hsla,
    selected: Hsla,
}

impl Palette {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            foreground: theme.foreground,
            muted: theme.muted_foreground,
            border: theme.border.opacity(0.7),
            primary: theme.primary,
            warning: theme.warning,
            fill: theme.muted.opacity(0.4),
            hover: theme.foreground.opacity(0.09),
            track: theme.input.opacity(0.4),
            selected: theme.input.opacity(0.72),
        }
    }
}

impl Panel {
    pub fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let palette = Palette::of(cx);
        let query = self.search.read(cx).value();
        let listing = model_picker::listing(&self.instances, &self.tab, &query, self.expanded);
        let searching = !query.trim().is_empty();
        let controls = &self.trigger.controls;
        let effort_column = controls.effort.is_some() || !controls.others.is_empty();
        let width = PICKER_WIDTH + if effort_column { EFFORT_WIDTH } else { 0. };
        let search_focused = self.search.focus_handle(cx).is_focused(window);
        v_flex()
            .w(px(width))
            .child(
                h_flex()
                    .h(px(BODY_HEIGHT))
                    .child(self.rail(searching, palette, cx))
                    .child(self.models(&listing, searching, search_focused, palette, cx))
                    .children(effort_column.then(|| self.effort_column(palette))),
            )
            .child(self.footer(palette, cx))
            .into_any_element()
    }

    /// `ModelPickerSidebar`: the favorites star, a divider, then one square per instance.
    fn rail(&self, searching: bool, palette: Palette, cx: &App) -> impl IntoElement {
        let has_favorites = self
            .instances
            .iter()
            .any(|i| i.models.iter().any(|m| m.favorite));
        let favorites = self.rail_item(
            "model-tab-favorites",
            searching,
            self.tab == Tab::Favorites,
            "Favorites".to_owned(),
            true,
            Tab::Favorites,
            palette,
            Icon::new(if has_favorites {
                Icons::StarFill
            } else {
                Icons::Star
            })
            .size(px(20.))
            .text_color(if self.tab == Tab::Favorites && !searching {
                palette.foreground
            } else {
                palette.muted
            })
            .into_any_element(),
        );
        let items = self.instances.iter().enumerate().map(|(index, instance)| {
            let tab = Tab::Instance(instance.instance_id.clone());
            let note = instance
                .unavailable
                .clone()
                .unwrap_or_else(|| instance.name.clone());
            self.rail_item(
                ("model-tab", index),
                searching,
                self.tab == tab,
                note,
                instance.unavailable.is_none(),
                tab,
                palette,
                provider_mark_at(&instance.mark, 24., 20., 1., cx).into_any_element(),
            )
        });
        v_flex()
            .id("model-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .gap_1()
            .p_1()
            .bg(cx.theme().muted.opacity(0.3))
            .overflow_y_scroll()
            .child(favorites)
            .child(div().h_px().w_full().flex_shrink_0().bg(palette.border))
            .children(items)
    }

    #[allow(clippy::too_many_arguments)]
    fn rail_item(
        &self,
        id: impl Into<gpui::ElementId>,
        searching: bool,
        active: bool,
        note: String,
        usable: bool,
        tab: Tab,
        palette: Palette,
        icon: AnyElement,
    ) -> impl IntoElement {
        let view = self.view.clone();
        let current = active && !searching;
        let focused = current && self.focus == Focus::Sidebar;
        div()
            .id(id)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(RAIL_WIDTH - 8.))
            .rounded_md()
            .when(focused, |item| item.bg(palette.hover))
            .when(usable, |item| {
                item.cursor_pointer()
                    .hover(move |style| style.bg(palette.foreground.opacity(0.1)))
            })
            .when(!usable, |item| item.opacity(0.45))
            .tooltip(tooltip(note))
            .child(icon)
            .when(current, |item| {
                // `h-5 w-0.75` at `right-1`, centered on the item.
                item.child(
                    div()
                        .absolute()
                        .right(px(4.))
                        .top(px((RAIL_WIDTH - 8. - 20.) / 2.))
                        .h(px(20.))
                        .w(px(3.))
                        .rounded_full()
                        .bg(palette.primary),
                )
            })
            .when(usable, |item| {
                item.on_click(move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.picker_select_tab(tab.clone(), window, cx)
                    });
                })
            })
    }

    /// The middle column: the search, the rows, and the older-models toggle.
    fn models(
        &self,
        listing: &Listing,
        searching: bool,
        search_focused: bool,
        palette: Palette,
        cx: &App,
    ) -> impl IntoElement {
        let jumps = listing.jump_rows();
        let on_list = self.focus == Focus::List;
        let rows = listing.rows.iter().enumerate().map(|(index, row)| {
            let jump = jumps.iter().position(|&at| at == index);
            self.model_row(
                index,
                row.instance,
                row.model,
                jump,
                on_list && self.highlight == index,
                searching || self.tab == Tab::Favorites,
                palette,
                cx,
            )
        });
        let toggle = listing.older.map(|expanded| {
            let view = self.view.clone();
            let highlighted = on_list && self.highlight == listing.rows.len();
            div()
                .id("model-older")
                .flex()
                .h(px(ROW_HEIGHT))
                .w_full()
                .flex_shrink_0()
                .items_center()
                .px_2()
                .rounded_sm()
                .text_sm()
                .text_color(palette.muted)
                .cursor_pointer()
                .when(highlighted, |row| {
                    row.bg(palette.hover).text_color(palette.foreground)
                })
                .hover(move |style| style.text_color(palette.foreground))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.picker_toggle_older(cx));
                })
                .child(if expanded { "Show less" } else { "Show more" })
        });
        let empty = listing.rows.is_empty().then(|| {
            let text = if searching {
                "No models found"
            } else if self.tab == Tab::Favorites {
                "No favorites yet"
            } else {
                "No models"
            };
            div()
                .px_2()
                .py_3()
                .text_sm()
                .text_color(palette.muted)
                .child(text)
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.fill)
            .border_l_1()
            .border_color(palette.border)
            .child(
                // The search sits in a `px-3 pt-2.5` strip, underlined in the primary tone on focus.
                div().px_3().pt(px(10.)).child(
                    h_flex()
                        .pb(px(6.))
                        .border_b_1()
                        .border_color(if search_focused {
                            palette.primary
                        } else {
                            palette.border
                        })
                        .child(
                            Input::new(&self.search)
                                .appearance(false)
                                .h(px(26.))
                                .px_0()
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size_4()
                                        .text_color(palette.muted.opacity(0.55)),
                                ),
                        ),
                ),
            )
            .child(
                v_flex()
                    .id("model-list")
                    .flex_1()
                    .min_h_0()
                    .gap(px(2.))
                    .overflow_y_scroll()
                    .py(px(6.))
                    .pl_2()
                    .pr_px()
                    .children(rows)
                    .children(toggle)
                    .children(empty),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn model_row(
        &self,
        index: usize,
        instance: &PickerInstance,
        model: &providers::PickerModel,
        jump: Option<usize>,
        highlighted: bool,
        show_provider: bool,
        palette: Palette,
        cx: &App,
    ) -> impl IntoElement {
        let usable = instance.unavailable.is_none();
        let chosen =
            instance.instance_id == self.trigger.instance_id && model.slug == self.trigger.slug;
        let group: SharedString = format!("model-row-{index}").into();
        let (view, id, slug) = (
            self.view.clone(),
            instance.instance_id.clone(),
            model.slug.clone(),
        );
        let star = {
            let (view, id, slug) = (view.clone(), id.clone(), slug.clone());
            let favorite = model.favorite;
            // A starred model shows its star, except in the favorites tab where it is hover-only.
            let resting = favorite && self.tab != Tab::Favorites;
            let tip = if favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            };
            div()
                .id(("model-star", index))
                .flex()
                .size(px(24.))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded_md()
                .cursor_pointer()
                .text_color(palette.muted)
                .when(!resting && !highlighted, |star| {
                    star.opacity(0.)
                        .group_hover(group.clone(), |style| style.opacity(1.))
                })
                .hover(move |style| style.bg(palette.hover).text_color(palette.foreground))
                .tooltip(tooltip(tip))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.picker_toggle_favorite(&id, &slug, cx));
                })
                .child(
                    Icon::new(if favorite {
                        Icons::StarFill
                    } else {
                        Icons::Star
                    })
                    .size(px(12.))
                    .text_color(if favorite {
                        palette.warning
                    } else {
                        palette.muted
                    }),
                )
        };
        h_flex()
            .id(("model-row", index))
            .group(group)
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .gap(px(6.))
            .px_2()
            .items_center()
            .rounded_sm()
            .text_sm()
            .when(highlighted, |row| row.bg(palette.hover))
            .when(usable, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(palette.hover))
            })
            .when(!usable, |row| row.opacity(0.45))
            .tooltip(tooltip(model.title.clone()))
            .when(show_provider, |row| {
                row.child(provider_mark(&instance.mark, 1., cx))
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .line_height(px(22.))
                    .child(model.label.clone()),
            )
            .children(model.sub_provider.clone().map(|name| {
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(palette.muted.opacity(0.7))
                    .child(name)
            }))
            .children(model.new.then(|| {
                div()
                    .flex_shrink_0()
                    .rounded_sm()
                    .px_1()
                    .text_xs()
                    .text_color(palette.primary)
                    .bg(palette.primary.opacity(0.12))
                    .child("New")
            }))
            .child(div().flex_1())
            .when(chosen, |row| {
                row.child(
                    Icon::new(IconName::Check)
                        .size(px(14.))
                        .text_color(palette.foreground),
                )
            })
            .when(usable, |row| row.child(star))
            .children(jump.map(|n| {
                div()
                    .flex_shrink_0()
                    .px(px(4.))
                    .rounded_sm()
                    .border_1()
                    .border_color(palette.border)
                    .text_size(px(10.))
                    .line_height(px(14.))
                    .text_color(palette.muted)
                    .child(format!("\u{2318}{}", n + 1))
            }))
            .when(usable, |row| {
                row.on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.pick_model(&id, &slug, cx));
                })
            })
    }

    /// `ComposerModelEffort`: the header, then a row per level and per further option.
    fn effort_column(&self, palette: Palette) -> impl IntoElement {
        let controls = &self.trigger.controls;
        let on_column = self.focus == Focus::Effort;
        let mut index = 0;
        let mut rows = Vec::new();
        if let Some(effort) = &controls.effort {
            for level in &effort.levels {
                rows.push(
                    self.level_row(
                        index,
                        effort,
                        level,
                        on_column && self.effort == index,
                        palette,
                    )
                    .into_any_element(),
                );
                index += 1;
            }
            if effort.levels.is_empty() {
                rows.push(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child("No effort levels")
                        .into_any_element(),
                );
            }
        }
        for control in &controls.others {
            rows.push(
                self.option_row(index, control, on_column && self.effort == index, palette)
                    .into_any_element(),
            );
            index += 1;
        }
        v_flex()
            .w(px(EFFORT_WIDTH))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(palette.border)
            .child(
                div().px_3().pt(px(10.)).child(
                    h_flex()
                        .h(px(33.))
                        .mt(px(-1.))
                        .gap_2()
                        .pb(px(6.))
                        .items_center()
                        .border_b_1()
                        .border_color(palette.border)
                        .text_sm()
                        .child(div().text_color(palette.muted).child(controls.title()))
                        .child(
                            div()
                                .ml_auto()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .text_color(palette.muted)
                                .child(self.trigger.model.clone()),
                        ),
                ),
            )
            .child(
                v_flex()
                    .id("model-effort-list")
                    .flex_1()
                    .min_h_0()
                    .gap(px(2.))
                    .overflow_y_scroll()
                    .px_2()
                    .pt(px(10.))
                    .pb(px(6.))
                    .children(rows),
            )
    }

    fn level_row(
        &self,
        index: usize,
        control: &Control,
        level: &providers::Level,
        highlighted: bool,
        palette: Palette,
    ) -> impl IntoElement {
        let (view, id, value) = (self.view.clone(), control.id.clone(), level.value.clone());
        h_flex()
            .id(("model-level", index))
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .px_2()
            .items_center()
            .rounded_sm()
            .text_sm()
            .cursor_pointer()
            .when(highlighted, |row| row.bg(palette.hover))
            .hover(move |style| style.bg(palette.hover))
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.picker_set_option(&id, value.clone(), cx)
                });
            })
            .child(div().min_w_0().truncate().child(level.label.clone()))
            .when(level.is_default && !level.selected, |row| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(palette.muted.opacity(0.7))
                        .child("Default"),
                )
            })
            .child(div().flex_1())
            .when(level.selected, |row| {
                row.child(
                    Icon::new(IconName::Check)
                        .size(px(14.))
                        .text_color(palette.foreground),
                )
            })
    }

    /// A further option as one row: its name over the current value, clicking steps to the next.
    fn option_row(
        &self,
        index: usize,
        control: &Control,
        highlighted: bool,
        palette: Palette,
    ) -> impl IntoElement {
        let (view, id) = (self.view.clone(), control.id.clone());
        let next = control.next().map(|level| level.value.clone());
        v_flex()
            .id(("model-option", index))
            .w_full()
            .flex_shrink_0()
            .py_1()
            .px_2()
            .rounded_sm()
            .cursor_pointer()
            .when(highlighted, |row| row.bg(palette.hover))
            .hover(move |style| style.bg(palette.hover))
            .on_click(move |_, _, cx| {
                if let Some(value) = next.clone() {
                    view.update(cx, |this, cx| this.picker_set_option(&id, value, cx));
                }
            })
            .child(
                div()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(control.label.clone()),
            )
            .child(
                div().text_sm().truncate().child(
                    control
                        .current()
                        .map(|level| level.label.clone())
                        .unwrap_or_default(),
                ),
            )
    }

    /// The footer: the speed segments, and the permissions control at the end.
    fn footer(&self, palette: Palette, cx: &App) -> impl IntoElement {
        let speed = self.trigger.controls.speed.as_ref().map(|speed| {
            let last = speed.levels.len().saturating_sub(1);
            let segments = speed.levels.iter().enumerate().map(|(index, level)| {
                let (view, id, value) = (self.view.clone(), speed.id.clone(), level.value.clone());
                let selected = level.selected;
                let bolt = providers::is_fast_label(&level.label);
                h_flex()
                    .id(("model-speed", index))
                    .h(px(24.))
                    .px_2()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(palette.muted)
                    .cursor_pointer()
                    .when(index == 0, |s| s.rounded_l_md())
                    .when(index == last, |s| s.rounded_r_md())
                    .when(selected, |s| {
                        s.bg(palette.selected).text_color(palette.foreground)
                    })
                    .hover(move |style| style.text_color(palette.foreground))
                    .when_some(level.description.clone(), |s, tip| s.tooltip(tooltip(tip)))
                    .when(level.description.is_none() && bolt, |s| {
                        s.tooltip(tooltip("2x speed, increased usage"))
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.picker_set_option(&id, value.clone(), cx)
                        });
                    })
                    .children(bolt.then(|| Icon::new(Icons::Zap).size(px(12.))))
                    .child(level.label.clone())
            });
            h_flex()
                .rounded_lg()
                .bg(palette.track)
                .p(px(2.))
                .children(segments)
        });
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(palette.border)
            .bg(palette.fill)
            .children(speed)
            .child(div().ml_auto().child(permissions_popover(
                &self.permissions,
                self.permissions_open,
                self.mode,
                FooterPermissions {
                    mode: self.mode,
                    open: false,
                    hover: palette.hover,
                    foreground: palette.foreground,
                    muted: palette.muted,
                },
                cx,
            )))
    }
}

/// `PermissionsMenu`'s trigger: `h-7 gap-1.5 rounded-md px-2 text-xs`, the mode's icon, its name,
/// and a chevron.
#[derive(IntoElement)]
struct FooterPermissions {
    mode: RuntimeMode,
    open: bool,
    hover: Hsla,
    foreground: Hsla,
    muted: Hsla,
}

impl Selectable for FooterPermissions {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl RenderOnce for FooterPermissions {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (hover, foreground) = (self.hover, self.foreground);
        h_flex()
            .id("model-permissions")
            .h(px(28.))
            .gap(px(6.))
            .px_2()
            .items_center()
            .rounded_md()
            .text_xs()
            .text_color(self.muted)
            .cursor_pointer()
            .hover(move |style| style.bg(hover).text_color(foreground))
            .when(self.open, move |button| {
                button.bg(hover).text_color(foreground)
            })
            .child(Icon::new(runtime_icon(self.mode)).size(px(14.)))
            .child(self.mode.label())
            .child(Icon::new(Icons::ChevronUp).size(px(12.)))
    }
}
