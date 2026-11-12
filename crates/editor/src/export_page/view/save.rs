//! The inline "Save as preset…" form, in the panel's lanes: a name, the
//! file to save to, then `Save` (or `Replace`) and the close button.

use super::super::model::SaveTarget;
use super::choice::Choice;
use super::layout::row;
use super::{ExportPage, blocked_line, problem_line};
use crate::actions::ConfirmExport;
use celesta_editor_theme::metrics;
use celesta_exporter::{Preset, update_presets};
use celesta_plugin_host::create_private_dir_all;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, IndexPath, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, Subscription, Window, div};

/// The inline "Save as preset…" form.
pub(super) struct SaveForm {
    pub(super) name: Entity<InputState>,
    target: Entity<SelectState<Vec<Choice<SaveTarget>>>>,
    /// Why the last save failed.
    error: Option<String>,
    _subscriptions: [Subscription; 2],
}

impl ExportPage {
    pub(super) fn open_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save.is_some() {
            return;
        }
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let targets = [
            (SaveTarget::Project, "This project"),
            (SaveTarget::User, "My presets"),
        ]
        .map(|(target, title)| match self.model.can_save(target) {
            Ok(()) => Choice::new(title, target),
            Err(reason) => Choice {
                title: format!("{title} — {reason}").into(),
                value: target,
                disabled: true,
            },
        });
        let first = targets
            .iter()
            .position(|choice| !choice.disabled)
            .unwrap_or_default();
        let target = cx
            .new(|cx| SelectState::new(targets.to_vec(), Some(IndexPath::new(first)), window, cx));
        let subscriptions = [
            // `Save` becomes `Replace` as the name is typed, and Enter in
            // the name saves.
            cx.subscribe_in(&name, window, |this, _, event, window, cx| match event {
                InputEvent::Change => {
                    if let Some(save) = &mut this.save {
                        save.error = None;
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.save_preset(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&target, window, |_, _, _: &SelectEvent<_>, _, cx| {
                cx.notify();
            }),
        ];
        name.update(cx, |name, cx| name.focus(window, cx));
        self.save = Some(SaveForm {
            name,
            target,
            error: None,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn close_save(&mut self, cx: &mut Context<Self>) {
        self.save = None;
        cx.notify();
    }

    /// The save form's name (trimmed) and target, when it can be saved:
    /// a name, a file that can be written, and a choice `Export…` could
    /// export.
    pub(super) fn save_request(&self, cx: &App) -> Option<(String, SaveTarget)> {
        let save = self.save.as_ref()?;
        let name = save.name.read(cx).value().trim().to_owned();
        let target = *save.target.read(cx).selected_value()?;
        (!name.is_empty() && self.model.can_save(target).is_ok() && self.export_blocked().is_none())
            .then_some((name, target))
    }

    /// `Save`/`Replace`: writes the choice under the name to the chosen
    /// file, takes the file as written, and selects the preset.
    pub(super) fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((name, target)) = self.save_request(cx) else {
            return;
        };
        let state = match target {
            SaveTarget::Project => &mut self.model.project,
            SaveTarget::User => &mut self.model.user,
        };
        let Some(path) = state.path.clone() else {
            return;
        };
        let output = self.model.saved_output();
        // The settings folder may not exist yet; a project's root does.
        let folder = match (target, path.parent()) {
            (SaveTarget::User, Some(folder)) => create_private_dir_all(folder),
            _ => Ok(()),
        };
        let written = folder
            .map_err(|error| format!("{} cannot be written: {error}", path.display()))
            .and_then(|()| {
                update_presets(&path, |file| {
                    file.put(Preset {
                        name: name.clone(),
                        output,
                    });
                })
                .map_err(|error| error.to_string())
            });
        match written {
            Ok(file) => {
                self.model.saved(target, name, file);
                self.save = None;
                self.sync(window, cx);
            }
            Err(error) => {
                if let Some(save) = &mut self.save {
                    save.error = Some(error);
                }
                cx.notify();
            }
        }
    }
}

impl ExportPage {
    /// The open form, drawn above the footer's buttons so it stays in view
    /// however far the panel's body is scrolled; `None` while it is closed.
    pub(super) fn render_save(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let save = self.save.as_ref()?;
        let request = self.save_request(cx);
        // `Save` is disabled for what disables `Export…`, said here as the
        // footer does not while the form is open.
        let blocked = self.export_blocked();
        let replaces = request
            .as_ref()
            .is_some_and(|(name, target)| self.model.replaces(*target, name));
        let name = div()
            .w_full()
            // Enter in the name saves (its `PressEnter`); the input passes
            // the key on as the panel's `ConfirmExport`, which would also
            // export, so it stops here. `Export…` exports on its click.
            .on_action(|_: &ConfirmExport, _, _| {})
            .child(Input::new(&save.name).id("export-save-name").small());
        let buttons = h_flex()
            .pl(metrics::inspector_label_width())
            .gap_2()
            .child(
                Button::new("export-save")
                    .ghost()
                    .small()
                    .label(if replaces { "Replace" } else { "Save" })
                    .disabled(request.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.save_preset(window, cx))),
            )
            .child(
                Button::new("export-save-close")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .tooltip("Close Save as preset")
                    .accessibility_label("Close Save as preset")
                    .on_click(cx.listener(|this, _, _, cx| this.close_save(cx))),
            );
        let mut lines: Vec<AnyElement> = Vec::new();
        lines.extend(blocked.map(|reason| blocked_line(reason, cx).into_any_element()));
        lines.extend(
            save.error
                .clone()
                .map(|error| problem_line(error, cx.theme().danger).into_any_element()),
        );
        Some(
            v_flex()
                .w_full()
                .child(row(
                    "save-name".into(),
                    "Name".into(),
                    name.into_any_element(),
                    Vec::new(),
                    cx,
                ))
                .child(row(
                    "save-target".into(),
                    "Save to".into(),
                    Select::new(&save.target)
                        .id("export-save-target")
                        .small()
                        .accessibility_label("Save to")
                        .into_any_element(),
                    Vec::new(),
                    cx,
                ))
                .child(buttons)
                .when(!lines.is_empty(), |form| {
                    form.child(
                        v_flex()
                            .pt_1()
                            .pl(metrics::inspector_label_width())
                            .gap_1()
                            .children(lines),
                    )
                })
                .into_any_element(),
        )
    }
}
