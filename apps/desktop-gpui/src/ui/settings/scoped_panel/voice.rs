//! The Voice tab (`VoiceSettingsPanel.tsx`). Device settings persist in `client-settings.json`
//! (`settings_voice::VoiceSettings`); the shared dictionary and the cleanup model are server
//! settings written through `ScopedPanel::write`.

use super::super::components::{row, row_with_preview};
use super::super::devices_panel::widgets::{Variant, button, icon_button};
use super::*;
use crate::settings_voice::{self as voice, Mic, Word};

const SUCCESS: u32 = 0x10b981;
const UNAVAILABLE: &str = "Chosen environment (unavailable)";

impl ScopedPanel {
    pub(super) fn voice(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            self.voice_input(window, cx),
            self.voice_transcription(window, cx),
            self.voice_dictionary(ctx, window, cx),
            self.voice_cleanup(ctx, window, cx),
        ]
    }

    pub(super) fn menu_labels(&self, pick: Pick, cx: &App) -> Vec<String> {
        match pick {
            Pick::Shortcut => voice::SHORTCUTS
                .iter()
                .map(|(_, label, hint)| format!("{label} ({hint})"))
                .collect(),
            Pick::Destination => self
                .destinations(cx)
                .into_iter()
                .map(|(_, label)| label)
                .collect(),
            Pick::Prompt => vec!["Built-in".to_owned(), "Custom".to_owned()],
            _ => pick.labels().into_iter().map(str::to_owned).collect(),
        }
    }

    pub(super) fn menu_icons(&self, pick: Pick, cx: &App) -> Vec<Option<Icons>> {
        match pick {
            Pick::Destination => self
                .destinations(cx)
                .into_iter()
                .map(|(id, label)| match id {
                    None => Some(Icons::MicOff),
                    Some(_) if label == UNAVAILABLE => None,
                    Some(_) => Some(Icons::Monitor),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(super) fn menu_headings(&self, pick: Pick, cx: &App) -> Vec<(usize, SharedString)> {
        match pick {
            Pick::Destination if self.destinations(cx).len() > 1 => {
                vec![(1, "On an environment".into())]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn trigger_icon(&self, pick: Pick) -> Option<Icons> {
        (pick == Pick::Destination).then(|| {
            if self.grants.destination.is_some() {
                Icons::Monitor
            } else {
                Icons::MicOff
            }
        })
    }

    pub(super) fn trigger_label(
        &self,
        pick: Pick,
        selected: usize,
        labels: &[String],
        unset: &'static str,
    ) -> String {
        match pick {
            Pick::Shortcut => voice::SHORTCUTS[selected.min(2)].1.to_owned(),
            _ => labels
                .get(selected)
                .cloned()
                .unwrap_or_else(|| unset.to_owned()),
        }
    }

    /// Off, then each environment; a chosen environment that is gone stays as a last item.
    fn destinations(&self, cx: &App) -> Vec<(Option<String>, String)> {
        let environments = &self.scope.read(cx).data().environments;
        let mut out = vec![(None, "Off".to_owned())];
        out.extend(
            environments
                .iter()
                .map(|e| (Some(e.id.clone()), e.label.clone())),
        );
        if let Some(chosen) = &self.grants.destination
            && !environments.iter().any(|e| &e.id == chosen)
        {
            out.push((Some(chosen.clone()), UNAVAILABLE.to_owned()));
        }
        out
    }

    pub(super) fn pick_voice(
        &mut self,
        pick: Pick,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match pick {
            Pick::Shortcut => {
                let mode = voice::SHORTCUTS[index.min(2)].0.to_owned();
                self.update_voice(|v| v.shortcut = mode, cx);
            }
            Pick::Destination => {
                if let Some((destination, _)) = self.destinations(cx).get(index).cloned() {
                    self.grants.destination = destination;
                    self.grants.save();
                }
            }
            _ => {
                let custom = index == 1;
                self.update_voice(
                    |v| {
                        v.custom_prompt = custom;
                        if custom && v.instructions.trim().is_empty() {
                            v.instructions = voice::DEFAULT_PROMPT.to_owned();
                        }
                    },
                    cx,
                );
            }
        }
        self.close_menu(window, cx);
    }

    fn update_voice(&mut self, change: impl FnOnce(&mut VoiceSettings), cx: &mut Context<Self>) {
        change(&mut self.voice);
        self.voice.save();
        cx.notify();
    }

    /// Puts the saved text into the unfocused fields, as the web's controlled inputs show it.
    pub(super) fn sync_voice(&self, window: &mut Window, cx: &mut Context<Self>) {
        let area = &self.instructions;
        let want = self.voice.instructions.clone();
        let focused = area.read(cx).focus_handle(cx).is_focused(window);
        if !focused && area.read(cx).value().as_ref() != want.as_str() {
            area.update(cx, |state, cx| state.set_value(want, window, cx));
        }
        let want = self.voice.correction_word.clone();
        let focused = self.correction.read(cx).focus_handle(cx).is_focused(window);
        if !focused && self.correction.read(cx).value().as_ref() != want.as_str() {
            self.correction
                .update(cx, |state, cx| state.set_value(want, window, cx));
        }
    }

    pub(super) fn change_instructions(&mut self, cx: &mut Context<Self>) {
        let text: String = self
            .instructions
            .read(cx)
            .value()
            .chars()
            .take(voice::INSTRUCTIONS_MAX)
            .collect();
        if self.voice.custom_prompt && text != self.voice.instructions {
            self.update_voice(|v| v.instructions = text, cx);
        }
    }

    pub(super) fn commit_correction(&mut self, cx: &mut Context<Self>) {
        let text: String = self
            .correction
            .read(cx)
            .value()
            .trim()
            .chars()
            .take(voice::TERM_MAX)
            .collect();
        if text != self.voice.correction_word {
            self.update_voice(|v| v.correction_word = text, cx);
        }
    }

    fn words(&self, which: Dictionary, ctx: &Ctx) -> Vec<Word> {
        match which {
            Dictionary::Mine => self.voice.words.clone(),
            Dictionary::Shared => voice::words_of(&self.value(Field::SpeechWords, ctx)),
        }
    }

    fn dictionary_disabled(&self, which: Dictionary, ctx: &Ctx, cx: &App) -> bool {
        which == Dictionary::Shared
            && (!ctx.connected
                || self.gate(Field::SpeechWords, cx).is_some()
                || (ctx.in_project && ctx.members.is_empty()))
    }

    fn set_words(&mut self, which: Dictionary, words: Vec<Word>, cx: &mut Context<Self>) {
        match which {
            Dictionary::Mine => self.update_voice(|v| v.words = words, cx),
            Dictionary::Shared => self.write(
                vec![(Field::SpeechWords, Some(voice::encode_words(&words)))],
                cx,
            ),
        }
    }

    pub(super) fn add_words(
        &mut self,
        which: Dictionary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ctx = self.ctx(cx);
        let editor = &self.editors[which.index()];
        let draft = editor.draft.read(cx).value().to_string();
        let mut words = self.words(which, &ctx);
        let terms = voice::parse_terms(&draft, &words);
        if !voice::can_add(
            &terms,
            words.len(),
            self.dictionary_disabled(which, &ctx, cx),
        ) {
            return;
        }
        words.extend(terms.into_iter().map(|term| Word {
            term,
            aliases: Vec::new(),
        }));
        self.set_words(which, words, cx);
        self.editors[which.index()]
            .draft
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    pub(super) fn add_alias(
        &mut self,
        which: Dictionary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ctx = self.ctx(cx);
        let editor = &self.editors[which.index()];
        let Some(index) = editor.alias_for else {
            return;
        };
        let draft = editor.alias.read(cx).value().to_string();
        let Some(next) = voice::add_alias(&self.words(which, &ctx), index, &draft) else {
            return;
        };
        self.editors[which.index()].alias_for = None;
        self.set_words(which, next, cx);
        self.editors[which.index()]
            .alias
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    pub(super) fn cancel_alias(&mut self, which: Dictionary, cx: &mut Context<Self>) {
        if self.editors[which.index()].alias_for.take().is_some() {
            cx.notify();
        }
    }

    fn begin_alias(
        &mut self,
        which: Dictionary,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = &mut self.editors[which.index()];
        editor.alias_for = Some(index);
        let input = editor.alias.clone();
        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    fn voice_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let refresh = {
            let view = view.clone();
            icon_button("settings-refresh-microphones", Variant::Outline, true, cx)
                .tooltip(tooltip("Refresh microphones"))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.voice = VoiceSettings::load();
                        cx.notify();
                    })
                })
                .child(glyph(Icons::RefreshCw, 16., theme.muted_foreground))
                .into_any_element()
        };
        let order = self.voice.order();
        let count = order.len();
        let mut list = v_flex()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(0.6));
        for (index, mic) in order.iter().enumerate() {
            let key = mic.key();
            let name = mic.name();
            let forget = (*mic != Mic::Default).then(|| {
                let (view, key) = (view.clone(), key.clone());
                icon_button(
                    ("settings-forget-microphone", index),
                    Variant::GhostMuted,
                    true,
                    cx,
                )
                .tooltip(tooltip(format!("Forget {name}")))
                .on_click(move |_, _, cx| {
                    let key = key.clone();
                    view.update(cx, |this, cx| {
                        let next = voice::forget_microphone(&this.voice.order(), &key);
                        this.update_voice(|v| v.microphones = next, cx)
                    })
                })
                .child(Icon::new(Icons::X).size(px(14.)))
            });
            let step =
                |id: &'static str, steps: isize, icon: Icons, enabled: bool, label: String| {
                    let (view, key) = (view.clone(), key.clone());
                    icon_button((id, index), Variant::GhostMuted, enabled, cx)
                        .size(px(24.))
                        .tooltip(tooltip(label))
                        .when(enabled, |button| {
                            button.on_click(move |_, _, cx| {
                                let key = key.clone();
                                view.update(cx, |this, cx| {
                                    let next =
                                        voice::move_microphone(&this.voice.order(), &key, steps);
                                    this.update_voice(|v| v.microphones = next, cx)
                                })
                            })
                        })
                        .child(Icon::new(icon).size(px(14.)))
                };
            list = list.child(
                h_flex()
                    .id(("settings-microphone", index))
                    .items_center()
                    .gap_2()
                    .py(px(6.))
                    .pl_3()
                    .pr(px(6.))
                    .when(index > 0, |item| {
                        item.border_t_1().border_color(theme.border.opacity(0.6))
                    })
                    .child(
                        div()
                            .w(px(16.))
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_1().min_w_0().text_sm().child(name.clone()))
                    .when(index == 0, |item| {
                        item.child(
                            div()
                                .rounded(px(4.))
                                .bg(gpui::rgb(SUCCESS).opacity(0.12))
                                .px_1()
                                .text_size(px(11.))
                                .text_color(gpui::rgb(if theme.dark { 0x34d399 } else { 0x047857 }))
                                .child("In use"),
                        )
                    })
                    .child(
                        div()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(theme.muted_foreground.opacity(0.7))
                            .child(Icon::new(Icons::GripVertical).size(px(14.))),
                    )
                    .child(
                        h_flex()
                            .gap(px(2.))
                            .child(step(
                                "settings-move-microphone-up",
                                -1,
                                Icons::ArrowUp,
                                index > 0,
                                format!("Move {name} up"),
                            ))
                            .child(step(
                                "settings-move-microphone-down",
                                1,
                                Icons::ArrowDown,
                                index + 1 < count,
                                format!("Move {name} down"),
                            ))
                            .child(match forget {
                                Some(forget) => forget.into_any_element(),
                                None => div().size(px(24.)).into_any_element(),
                            }),
                    ),
            );
        }
        let microphone = row_with_preview(
            "Microphone",
            "Voice input uses the first connected microphone in this list.",
            None,
            refresh,
            div().mt_3().pb_2().child(list).into_any_element(),
            self.wide,
            cx,
        );
        let test = row(
            "Test microphone",
            "Record ten seconds and play it back. The audio stays on this device.",
            None,
            button_disabled("settings-test-microphone", "Record", cx),
            self.wide,
            cx,
        );
        let selected = voice::shortcut_index(&self.voice.shortcut);
        let hint = voice::SHORTCUTS[selected].2;
        let shortcut = row(
            "Dictation shortcut",
            format!("{hint}."),
            None,
            self.select(Pick::Shortcut, selected, 192., None, cx),
            self.wide,
            cx,
        );
        let _ = window;
        section(
            "Input",
            group(
                vec![
                    microphone.into_any_element(),
                    test.into_any_element(),
                    shortcut.into_any_element(),
                ],
                cx,
            ),
        )
        .into_any_element()
    }

    fn voice_transcription(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let destinations = self.destinations(cx);
        let selected = destinations
            .iter()
            .position(|(id, _)| *id == self.grants.destination)
            .unwrap_or(0);
        let destination = row(
            "Transcribe with",
            "Where your speech is turned into text.",
            None,
            self.select(Pick::Destination, selected, 224., None, cx),
            self.wide,
            cx,
        );
        let silence = self.voice.silence_filtering;
        let silence_view = view.clone();
        let silence_row = row(
            "Silence filtering",
            "Skip silence and background noise before transcribing.",
            None,
            switch(
                "settings-speech-silence-filtering",
                silence,
                Rc::new(move |_, cx| {
                    silence_view.update(cx, |this, cx| {
                        this.update_voice(|v| v.silence_filtering = !silence, cx)
                    })
                }),
                window,
                cx,
            ),
            self.wide,
            cx,
        );
        let fillers = self.voice.remove_fillers;
        let filler_row = row(
            "Remove filler words",
            "Drop \u{201c}um\u{201d} and \u{201c}uh\u{201d} from transcripts.",
            None,
            switch(
                "settings-remove-filler-words",
                fillers,
                Rc::new(move |_, cx| {
                    view.update(cx, |this, cx| {
                        this.update_voice(|v| v.remove_fillers = !fillers, cx)
                    })
                }),
                window,
                cx,
            ),
            self.wide,
            cx,
        );
        section(
            "Transcription",
            group(
                vec![
                    destination.into_any_element(),
                    silence_row.into_any_element(),
                    filler_row.into_any_element(),
                ],
                cx,
            ),
        )
        .into_any_element()
    }

    fn voice_dictionary(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mine = row_with_preview(
            "Your dictionary",
            "Names and terms to spell your way, saved on this device.",
            None,
            div().into_any_element(),
            self.dictionary(Dictionary::Mine, ctx, window, cx),
            self.wide,
            cx,
        );
        let (title, description) = if ctx.in_project {
            (
                "Project dictionary",
                format!("Used by everyone who dictates in {}.", ctx.label),
            )
        } else {
            (
                "Shared dictionary",
                "Used by everyone who dictates on this environment.".to_owned(),
            )
        };
        let shared_editor = self.dictionary(Dictionary::Shared, ctx, window, cx);
        let shared = self.setting(
            ids!("speech-project-dictionary"),
            title,
            description,
            Field::SpeechWords,
            None,
            div().into_any_element(),
            Some(shared_editor),
            ctx,
            window,
            cx,
        );
        section(
            "Dictionary",
            group(vec![mine.into_any_element(), shared], cx),
        )
        .into_any_element()
    }

    /// `DictionaryEditor`: an add field, then each word as a chip with its aliases.
    fn dictionary(
        &mut self,
        which: Dictionary,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let disabled = self.dictionary_disabled(which, ctx, cx);
        let words = self.words(which, ctx);
        let editor = &self.editors[which.index()];
        let draft = editor.draft.read(cx).value().to_string();
        let can_add = voice::can_add(&voice::parse_terms(&draft, &words), words.len(), disabled);
        let (add_id, alias_id, remove_id, open_id) = match which {
            Dictionary::Mine => (
                "settings-dictionary-add",
                "settings-dictionary-alias",
                "settings-dictionary-remove",
                "settings-dictionary-alias-open",
            ),
            Dictionary::Shared => (
                "settings-shared-dictionary-add",
                "settings-shared-dictionary-alias",
                "settings-shared-dictionary-remove",
                "settings-shared-dictionary-alias-open",
            ),
        };
        let input = sm_field(&editor.draft, false, false, disabled, window, cx);
        let add_view = view.clone();
        let add = button(add_id, Variant::Outline, can_add, cx)
            .h(px(32.))
            .px(px(11.))
            .gap_2()
            .when(can_add, |button| {
                button.on_click(move |_, window, cx| {
                    add_view.update(cx, |this, cx| this.add_words(which, window, cx))
                })
            })
            .child(glyph(Icons::Plus, 16., theme.muted_foreground).mx(px(-2.)))
            .child("Add");
        let alias_for = editor.alias_for;
        let mut chips = h_flex().flex_wrap().gap_1p5();
        for (index, word) in words.iter().enumerate() {
            let full = word.aliases.len() >= voice::ALIASES_MAX;
            let remove_view = view.clone();
            let open_view = view.clone();
            let mut chip = h_flex()
                .items_center()
                .gap_1()
                .py(px(2.))
                .pl_2()
                .pr(px(2.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border.opacity(0.6))
                .bg(theme.muted.opacity(0.3))
                .text_xs()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(word.term.clone()),
                );
            if !word.aliases.is_empty() {
                chip = chip.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("\u{2190} {}", word.aliases.join(", "))),
                );
            }
            chip = if alias_for == Some(index) {
                chip.child(div().w(px(128.)).child(sm_field(
                    &self.editors[which.index()].alias,
                    false,
                    false,
                    false,
                    window,
                    cx,
                )))
            } else {
                chip.child(
                    button(
                        (open_id, index),
                        Variant::GhostMuted,
                        !full && !disabled,
                        cx,
                    )
                    .h(px(24.))
                    .px(px(7.))
                    .gap_1()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .ml_auto()
                    .when(!full && !disabled, |button| {
                        button.on_click(move |_, window, cx| {
                            open_view
                                .update(cx, |this, cx| this.begin_alias(which, index, window, cx))
                        })
                    })
                    .child("Alias"),
                )
            };
            let _ = alias_id;
            chips = chips.child(
                chip.child(
                    icon_button((remove_id, index), Variant::GhostMuted, !disabled, cx)
                        .size(px(24.))
                        .tooltip(tooltip(format!("Remove {}", word.term)))
                        .when(!disabled, |button| {
                            button.on_click(move |_, _, cx| {
                                remove_view.update(cx, |this, cx| {
                                    let ctx = this.ctx(cx);
                                    let mut words = this.words(which, &ctx);
                                    if index < words.len() {
                                        words.remove(index);
                                        this.set_words(which, words, cx);
                                    }
                                })
                            })
                        })
                        .child(Icon::new(Icons::X).size(px(14.))),
                ),
            );
        }
        v_flex()
            .mt_3()
            .pb_2()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(input))
                    .child(add),
            )
            .when(!words.is_empty(), |editor| editor.child(chips))
            .into_any_element()
    }

    fn voice_cleanup(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let enabled = self.grants.cleanup;
        let mut rows = vec![
            row(
                "Clean up transcripts",
                "Off by default. Sends the transcript and nearby draft text, never audio, to the cleanup model.",
                None,
                switch(
                    "settings-clean-up-transcripts",
                    enabled,
                    Rc::new(move |_, cx| {
                        view.update(cx, |this, cx| {
                            this.grants.cleanup = !enabled;
                            this.grants.save();
                            cx.notify();
                        })
                    }),
                    window,
                    cx,
                ),
                self.wide,
                cx,
            )
            .into_any_element(),
        ];
        if enabled {
            let custom = self.voice.custom_prompt;
            let description = if custom {
                "Your own instructions for cleanup."
            } else {
                "Fix punctuation, numbers, lists, and spoken corrections."
            };
            let select = self.select(Pick::Prompt, usize::from(custom), 128., None, cx);
            rows.push(if custom {
                row_with_preview(
                    "Instructions",
                    description,
                    None,
                    select,
                    self.instructions_area(window, cx),
                    self.wide,
                    cx,
                )
                .into_any_element()
            } else {
                row("Instructions", description, None, select, self.wide, cx).into_any_element()
            });
            rows.push(self.cleanup_model(ctx, window, cx));
            rows.push(
                row(
                    "Correction cue",
                    "The phrase you say to fix a mistake, like \u{201c}scratch that\u{201d}.",
                    None,
                    div()
                        .w(px(160.))
                        .child(sm_field(&self.correction, false, false, false, window, cx))
                        .into_any_element(),
                    self.wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        section("Cleanup", group(rows, cx)).into_any_element()
    }

    fn instructions_area(&self, window: &Window, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let focused = self
            .instructions
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        div()
            .mt_3()
            .pb_2()
            .child(
                div()
                    .w_full()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if focused { theme.ring } else { theme.input })
                    .bg(background)
                    .text_size(px(14.))
                    .child(
                        Textarea::new(&self.instructions)
                            .appearance(false)
                            .bordered(false)
                            .aria_label("Custom cleanup instructions"),
                    ),
            )
            .into_any_element()
    }

    /// `CleanupModelRow`: the web's picker is not ported, so the switch and the model's name.
    fn cleanup_model(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = Field::SpeechCleanupModel;
        const DESCRIPTION: &str = "Model for transcript cleanup, such as a cheaper or faster one. When disabled, use the text generation model configured for this environment.";
        let control = if !ctx.connected {
            Self::label_control("Connect an environment first.", cx)
        } else {
            let selection = self.value(field, ctx);
            let dedicated = !selection.is_null();
            let on = self
                .config
                .text_generation(&ctx.members, ctx.in_project)
                .unwrap_or(Value::Null);
            let switch = self.toggle(
                "settings-use-a-separate-cleanup-model",
                field,
                dedicated,
                on.clone(),
                Value::Null,
                window,
                cx,
            );
            let name = if dedicated {
                sc::model_label(&selection)
            } else if on.is_null() {
                Some("No text generation providers available.".to_owned())
            } else {
                None
            };
            h_flex()
                .gap_3()
                .items_center()
                .children(name.map(|name| Self::label_control(name, cx)))
                .child(switch)
                .into_any_element()
        };
        self.setting(
            ids!("speech-post-processing-model"),
            "Cleanup model",
            DESCRIPTION,
            field,
            None,
            control,
            None,
            ctx,
            window,
            cx,
        )
    }
}

/// `Button size="sm" variant="outline"` with a 16px `MicIcon` at `-mx-0.5`, with no action to run.
fn button_disabled(id: &'static str, label: &'static str, cx: &App) -> AnyElement {
    let muted = Tokens::of(cx).muted_foreground;
    button(id, Variant::Outline, false, cx)
        .child(glyph(Icons::Mic, 16., muted).mx(px(-2.)))
        .child(label)
        .into_any_element()
}

/// An icon in the colour a button gives it (`--control-icon-color`): muted for outline and ghost.
fn glyph(icon: Icons, size: f32, color: Hsla) -> gpui::Div {
    div()
        .flex_shrink_0()
        .text_color(color)
        .child(Icon::new(icon).size(px(size)))
}
