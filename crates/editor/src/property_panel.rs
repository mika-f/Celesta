//! The Properties section of the React Preview panel: one row per property
//! the entry declares, each committing its value to the entry's props file.
//! This module holds the pure helpers, the panel state, and the commit flow;
//! `property_panel_view.rs` renders it.

use crate::edit_history::{FileEdit, write_recorded};
use crate::element_edit::schema_field;
use crate::element_rows::control_kind;
use crate::preview::{KeptSchema, ReactPreview};
use crate::property_controls::{
    Control, ControlEvent, ControlKind, build_control, parse_color, parse_color_input,
    parse_number_input, take_picked_color,
};
use crate::source::{PropertyArgs, watched_mtime};
use crate::view::EditorView;
use crate::widgets::{format_component_number, json_display};
use celesta_react_bridge::{
    ComponentPropertyField, ENTRY_PROPS_FILE, PropertyInputs, PropertyIssue, effective_values,
    import_path, resolve_props_file, set_property_text,
};
use gpui_kit::{App, Context, PathPromptOptions, Subscription, Window};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Commits waiting for the single writer, oldest first. A key appears at most
/// once: a newer commit to a queued key replaces its value in place.
#[derive(Debug, Default)]
pub(crate) struct CommitQueue {
    commits: VecDeque<(String, Option<Value>)>,
}

impl CommitQueue {
    pub(crate) fn push(&mut self, key: String, value: Option<Value>) {
        match self.commits.iter_mut().find(|(queued, _)| *queued == key) {
            Some((_, queued)) => *queued = value,
            None => self.commits.push_back((key, value)),
        }
    }

    pub(crate) fn pop(&mut self) -> Option<(String, Option<Value>)> {
        self.commits.pop_front()
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.commits.iter().any(|(queued, _)| queued == key)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }
}

/// Splits the last load's issues into messages per property key and messages
/// that concern a whole source (an empty key), which name that source.
pub(crate) fn issues_by_key(
    issues: &[PropertyIssue],
) -> (BTreeMap<String, Vec<String>>, Vec<String>) {
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut general = Vec::new();
    for issue in issues {
        if issue.key.is_empty() {
            general.push(issue.to_string());
        } else {
            by_key
                .entry(issue.key.clone())
                .or_default()
                .push(issue.message.clone());
        }
    }
    (by_key, general)
}

/// The file a commit writes: the props file in use, or a new `props.json`
/// beside the entry when there is none yet.
pub(crate) fn props_target(entry: &Path, args: &PropertyArgs) -> PathBuf {
    resolve_props_file(entry, args.props_file.as_deref()).map_or_else(
        || {
            entry
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(ENTRY_PROPS_FILE)
        },
        |choice| choice.path,
    )
}

/// Equal values, with numbers compared by value (a file's `1` equals `1.0`).
pub(crate) fn same_value(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left == right,
        _ => left == right,
    }
}

/// Equal values for a row; colors also ignore the case of their hex digits
/// (`#aabbcc` is `#AABBCC`).
fn values_match(left: &Value, right: &Value, ignore_case: bool) -> bool {
    match (left.as_str(), right.as_str()) {
        (Some(left), Some(right)) if ignore_case => left.eq_ignore_ascii_case(right),
        _ => same_value(left, right),
    }
}

/// True when `value` is not the field's declared default.
pub(crate) fn differs_from_default(field: &ComponentPropertyField, value: &Value) -> bool {
    let color = matches!(field, ComponentPropertyField::Color { .. });
    !values_match(value, &field.default_json(), color)
}

/// The JSON a committed number is written as: whole numbers without a
/// fraction (`2`, not `2.0`).
pub(crate) fn number_value(value: f64) -> Value {
    // Integers from 2^53 up are not exact in f64, so they stay floats.
    const EXACT: f64 = 9_007_199_254_740_992.0;
    if value.fract() == 0.0 && value.abs() < EXACT {
        return Value::from(value as i64);
    }
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// The text an input shows for a value: strings unquoted, numbers without a
/// needless fraction, anything else (a wrong type in the file) as JSON.
pub(crate) fn value_text(value: &Value) -> String {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map_or_else(|| number.to_string(), format_component_number),
        other => json_display(other),
    }
}

/// The schema's keys in declaration order, then any key the order missed.
pub(crate) fn ordered_keys(schema: &KeptSchema) -> Vec<&str> {
    let mut keys: Vec<&str> = schema
        .order
        .iter()
        .map(String::as_str)
        .filter(|key| schema.fields.contains_key(*key))
        .collect();
    for key in schema.fields.keys() {
        if !keys.contains(&key.as_str()) {
            keys.push(key);
        }
    }
    keys
}

/// The label a row shows: the field's `label`, else its key.
pub(crate) fn field_label<'a>(key: &'a str, field: &'a ComponentPropertyField) -> &'a str {
    let label = match field {
        ComponentPropertyField::String { label, .. }
        | ComponentPropertyField::Number { label, .. }
        | ComponentPropertyField::Boolean { label, .. }
        | ComponentPropertyField::Color { label, .. }
        | ComponentPropertyField::Select { label, .. }
        | ComponentPropertyField::Path { label, .. } => label,
    };
    label.as_deref().unwrap_or(key)
}

/// The value the preview uses for one key, from the inputs last read from disk.
fn effective_value(react: &ReactPreview, key: &str) -> Option<Value> {
    let schema = react.property_schema.as_ref()?;
    effective_values(&schema.fields, &react.disk_properties)
        .remove(key)
        .map(|effective| effective.value)
}

/// What Enter or blur in a row's text does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextSubmit {
    /// The person edited the text: parse and commit it.
    Commit,
    /// Focus left an unedited text: show the row's value, which may have
    /// changed on disk while the person was in the field.
    Refresh,
    /// Enter in an unedited text: nothing to do.
    Ignore,
}

/// A row's value and the decisions about it that need no window: what the
/// controls show, when a loaded value may replace it, and when typed text
/// counts as an edit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowState {
    /// The value the controls show: the loaded value, or the one just
    /// committed while its write and the reload after it run.
    pub(crate) shown: Value,
    /// The row was written before reload `hold_until` was requested; loaded
    /// values from older reloads predate the write and are ignored.
    hold_until: u64,
    /// The person changed the text since it last showed `shown`.
    edited: bool,
    /// A value to put into the controls on the next render.
    update: Option<(Value, TextUpdate)>,
    /// Values are compared ignoring ASCII case (color rows).
    ignore_case: bool,
    /// Why this row's last write failed; cleared by its next successful one,
    /// or by a reload that read the props file after the failure.
    pub(crate) write_error: Option<String>,
    /// The first reload number whose read of the props file clears
    /// `write_error`; `None` keeps it until the next successful write.
    error_released_by: Option<u64>,
}

impl RowState {
    pub(crate) fn new() -> Self {
        Self {
            // Differs from every loaded value, so the first load fills the controls.
            shown: Value::Null,
            hold_until: 0,
            edited: false,
            update: None,
            ignore_case: false,
            write_error: None,
            error_released_by: None,
        }
    }

    /// A row for `field`: colors compare ignoring the case of their digits.
    pub(crate) fn for_field(field: &ComponentPropertyField) -> Self {
        Self {
            ignore_case: matches!(field, ComponentPropertyField::Color { .. }),
            ..Self::new()
        }
    }

    /// Records why a write failed. With `released_by` (the number the next
    /// reload will get), a later reload that read the props file clears it:
    /// the cause may have been fixed there. `None` keeps it until a write
    /// succeeds.
    pub(crate) fn report_error(&mut self, message: String, released_by: Option<u64>) {
        self.write_error = Some(message);
        self.error_released_by = released_by;
    }

    /// The newest applied reload that read the props file is `read_reload`.
    pub(crate) fn offer_read(&mut self, read_reload: u64) {
        if self
            .error_released_by
            .is_some_and(|since| read_reload >= since)
        {
            self.write_error = None;
            self.error_released_by = None;
        }
    }

    /// A reload read `value` from disk. It replaces the shown value unless
    /// a commit for this key is still on its way (`pending`) or the newest
    /// applied reload started before this row was last written.
    pub(crate) fn offer_loaded(&mut self, value: &Value, applied_reload: u64, pending: bool) {
        if pending
            || applied_reload < self.hold_until
            || values_match(value, &self.shown, self.ignore_case)
        {
            return;
        }
        self.shown = value.clone();
        self.update = Some((value.clone(), TextUpdate::UnlessFocused));
    }

    /// Shows `value` (`None`: the default, as the key is removed) and puts
    /// it into the controls. False when the row already shows that value,
    /// so there is nothing to write.
    pub(crate) fn commit(
        &mut self,
        value: Option<&Value>,
        default: Value,
        text: TextUpdate,
    ) -> bool {
        // The text now holds (or is about to show) the committed value.
        self.edited = false;
        let shown = value.cloned().unwrap_or(default);
        if value.is_some() && values_match(&shown, &self.shown, self.ignore_case) {
            return false;
        }
        self.shown = shown.clone();
        self.update = Some((shown, text));
        true
    }

    /// The write succeeded; `next_reload` is the first reload that reads it.
    pub(crate) fn written(&mut self, next_reload: u64) {
        self.hold_until = next_reload;
        self.write_error = None;
        self.error_released_by = None;
    }

    /// The write failed and the file still holds `file_value`. The typed
    /// text stays and counts as an edit, so the next Enter or blur retries.
    pub(crate) fn write_failed(&mut self, file_value: Option<Value>) {
        self.edited = true;
        if let Some(value) = file_value {
            self.shown = value.clone();
            self.update = Some((value, TextUpdate::Keep));
        }
    }

    pub(crate) fn text_edited(&mut self) {
        self.edited = true;
    }

    /// The text was set to the shown value.
    pub(crate) fn text_replaced(&mut self) {
        self.edited = false;
    }

    /// Enter (`blur` false) or blur in the row's text.
    pub(crate) fn submitted(&mut self, blur: bool) -> TextSubmit {
        if self.edited {
            TextSubmit::Commit
        } else if blur {
            self.update = Some((self.shown.clone(), TextUpdate::Always));
            TextSubmit::Refresh
        } else {
            TextSubmit::Ignore
        }
    }

    /// Escape: show the row's value again.
    pub(crate) fn restore(&mut self) {
        self.update = Some((self.shown.clone(), TextUpdate::Always));
    }

    pub(crate) fn take_update(&mut self) -> Option<(Value, TextUpdate)> {
        self.update.take()
    }
}

/// How putting a value into a row's controls treats its text input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextUpdate {
    /// Leave the text as it is: it holds what the person typed.
    Keep,
    /// Replace the text unless the person is typing in it.
    UnlessFocused,
    /// Replace the text: the value was chosen elsewhere in the row.
    Always,
}

/// Sends what the person does in a row's controls to the row for `key`.
pub(crate) fn property_control_handler(
    key: &str,
) -> impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static
{
    let key = key.to_owned();
    move |this, event, _, cx| this.property_control_event(&key, event, cx)
}

/// One declared property's controls and what they show.
pub(crate) struct PropertyRow {
    pub(crate) field: ComponentPropertyField,
    pub(crate) kind: ControlKind,
    /// A switch shows `RowState::shown`.
    pub(crate) control: Control,
    /// The row's value and when it changes; applied to `control` on render,
    /// which has the window the controls need.
    pub(crate) state: RowState,
    /// Why the typed text was not committed (not a number, not a color).
    pub(crate) invalid: Option<String>,
    /// A color was picked since the picker last closed; the pick is
    /// committed when it closes.
    picked: bool,
    _subscriptions: Vec<Subscription>,
}

/// The Properties section's state for the open entry. A new `EditorView`
/// (another entry opened) starts with a fresh one.
#[derive(Default)]
pub(crate) struct PropertyPanelState {
    /// The schema the rows were built for.
    schema: Option<KeptSchema>,
    /// The inputs the rows last showed, and the reload that read them.
    inputs: Option<PropertyInputs>,
    applied: u64,
    pub(crate) rows: BTreeMap<String, PropertyRow>,
    queue: CommitQueue,
    /// The key being written right now; one write runs at a time.
    writing: Option<String>,
    /// A write succeeded since the last reload; reload once the queue drains.
    reload_needed: bool,
}

impl PropertyPanelState {
    /// A write is running or waiting.
    pub(crate) fn is_writing(&self) -> bool {
        self.writing.is_some() || !self.queue.is_empty()
    }
}

/// Puts `value` into a row's controls.
fn update_controls(
    row: &mut PropertyRow,
    value: Value,
    text: TextUpdate,
    window: &mut Window,
    cx: &mut App,
) {
    let replace = match text {
        TextUpdate::Keep => false,
        TextUpdate::UnlessFocused => !row.control.text_focused(window, cx),
        TextUpdate::Always => true,
    };
    if replace && row.control.set_text(value_text(&value), window, cx) {
        row.invalid = None;
        row.state.text_replaced();
    }
    match &row.control {
        Control::Select(select) => {
            let choice = value.as_str().map(str::to_owned);
            select.update(cx, |select, cx| match &choice {
                Some(choice) => select.set_selected_value(choice, window, cx),
                None => select.set_selected_index(None, window, cx),
            });
        }
        // A value that is not a color clears the swatch, which a locked row
        // shows in place of the picker.
        Control::Color { picker, .. } => {
            let color = parse_color(&value);
            picker.update(cx, |picker, cx| match color {
                Some(color) => picker.set_value(color, window, cx),
                None => picker.clear_value(window, cx),
            });
        }
        Control::Text(_) | Control::Textarea(_) | Control::Switch => {}
    }
}

impl EditorView {
    /// Builds rows for a new schema and puts newly read values into the
    /// controls. Runs from `render`, which has the window inputs need, and
    /// does nothing when neither changed.
    pub(crate) fn sync_property_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(react) = self.react_preview.as_ref() else {
            return;
        };
        if self.property_panel.schema != react.property_schema {
            let schema = react.property_schema.clone();
            self.rebuild_property_rows(schema.as_ref(), window, cx);
            self.property_panel.schema = schema;
            self.property_panel.inputs = None;
        }
        let Some(react) = self.react_preview.as_ref() else {
            return;
        };
        let applied = self.react_reloads.applied();
        let panel = &mut self.property_panel;
        // A newer reload can release a row held for its write even when the
        // inputs it read look the same.
        if panel.inputs.as_ref() != Some(&react.disk_properties) || panel.applied != applied {
            if let Some(schema) = panel.schema.as_ref() {
                let values = effective_values(&schema.fields, &react.disk_properties);
                for (key, row) in &mut panel.rows {
                    // A commit on its way to the file shows its own value.
                    let pending =
                        panel.queue.contains(key) || panel.writing.as_deref() == Some(key);
                    if let Some(effective) = values.get(key) {
                        row.state.offer_loaded(&effective.value, applied, pending);
                    }
                }
            }
            panel.inputs = Some(react.disk_properties.clone());
            panel.applied = applied;
        }
        for row in panel.rows.values_mut() {
            row.state.offer_read(react.properties_read_reload);
            if let Some((value, text)) = row.state.take_update() {
                update_controls(row, value, text, window, cx);
            }
        }
    }

    /// Keeps rows whose field did not change, so their controls keep focus and
    /// typed text, and builds the rest.
    fn rebuild_property_rows(
        &mut self,
        schema: Option<&KeptSchema>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut previous = std::mem::take(&mut self.property_panel.rows);
        let Some(schema) = schema else {
            return;
        };
        for key in ordered_keys(schema) {
            let field = &schema.fields[key];
            let row = match previous.remove(key) {
                Some(row) if row.field == *field => row,
                _ => Self::new_property_row(key, field, window, cx),
            };
            self.property_panel.rows.insert(key.to_owned(), row);
        }
    }

    fn new_property_row(
        key: &str,
        field: &ComponentPropertyField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PropertyRow {
        let kind = control_kind(&schema_field(field));
        let built = build_control(&kind, window, cx, property_control_handler(key));
        PropertyRow {
            field: field.clone(),
            kind,
            control: built.control,
            // Differs from every effective value, so the first sync fills the controls.
            state: RowState::for_field(field),
            invalid: None,
            picked: false,
            _subscriptions: built.subscriptions,
        }
    }

    /// What the person did in the controls of the row for `key`.
    fn property_control_event(&mut self, key: &str, event: ControlEvent, cx: &mut Context<Self>) {
        match event {
            ControlEvent::Edited => {
                if let Some(row) = self.property_panel.rows.get_mut(key) {
                    row.state.text_edited();
                }
            }
            ControlEvent::Submit { blur } => self.submit_property_text(key, blur, cx),
            ControlEvent::Escape => self.restore_property_text(key, cx),
            ControlEvent::Toggled(checked) => {
                let value = Some(Value::Bool(checked));
                self.commit_property(key, value, TextUpdate::Keep, cx);
            }
            ControlEvent::Chosen(choice) => {
                let value = Some(Value::String(choice));
                self.commit_property(key, value, TextUpdate::Keep, cx);
            }
            ControlEvent::ColorPicked => {
                if let Some(row) = self.property_panel.rows.get_mut(key) {
                    row.picked = true;
                }
            }
            ControlEvent::ColorPickerClosed => self.color_picker_closed(key, cx),
            ControlEvent::ChoosePath => self.choose_property_path(key, cx),
            ControlEvent::ClearPath => {
                let none = Some(Value::String(String::new()));
                self.commit_property(key, none, TextUpdate::Always, cx);
            }
        }
    }

    /// Enter (`blur` false) or blur in a row's text: commits an edit, or
    /// marks it invalid when it does not parse. Unedited text is never
    /// written; on blur it shows the row's value, which may have changed on
    /// disk meanwhile.
    fn submit_property_text(&mut self, key: &str, blur: bool, cx: &mut Context<Self>) {
        let Some(row) = self.property_panel.rows.get_mut(key) else {
            return;
        };
        match row.state.submitted(blur) {
            TextSubmit::Commit => {}
            TextSubmit::Refresh => return cx.notify(),
            TextSubmit::Ignore => return,
        }
        let Some(text) = row.control.text(cx) else {
            return;
        };
        let parsed = match row.field {
            ComponentPropertyField::Number { .. } => parse_number_input(&text).map(number_value),
            ComponentPropertyField::Color { .. } => parse_color_input(&text).map(Value::String),
            _ => Ok(Value::String(text.to_string())),
        };
        match parsed {
            Ok(value) => {
                row.invalid = None;
                self.commit_property(key, Some(value), TextUpdate::Keep, cx);
            }
            Err(message) => row.invalid = Some(message),
        }
        cx.notify();
    }

    /// Escape in a row's text: show its value again and drop the typed text.
    pub(crate) fn restore_property_text(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.property_panel.rows.get_mut(key) {
            row.state.restore();
            cx.notify();
        }
    }

    /// Commits the picked color when the picker closes.
    fn color_picker_closed(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(row) = self.property_panel.rows.get_mut(key) else {
            return;
        };
        if let Some(color) = take_picked_color(&row.control, &mut row.picked, cx) {
            self.commit_property(key, Some(Value::String(color)), TextUpdate::Always, cx);
        }
    }

    /// Queues a write of `value` (`None` removes the key) and shows it in the
    /// row right away. A value the row already shows is not written again.
    pub(crate) fn commit_property(
        &mut self,
        key: &str,
        value: Option<Value>,
        text: TextUpdate,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.property_panel.rows.get_mut(key) else {
            return;
        };
        // The controls follow the committed value; `Keep` leaves the typed
        // text but still moves the picker or select.
        if !row
            .state
            .commit(value.as_ref(), row.field.default_json(), text)
        {
            return;
        }
        self.property_panel.queue.push(key.to_owned(), value);
        self.drain_property_commits(cx);
        cx.notify();
    }

    /// Starts the next queued write unless one is running. Once the queue is
    /// empty, reloads the entry if any write succeeded.
    pub(crate) fn drain_property_commits(&mut self, cx: &mut Context<Self>) {
        // An undo, redo or canvas write on the history is still running.
        if self.property_panel.writing.is_some() || self.edit_history.is_busy() {
            return;
        }
        let Some(react) = self.react_preview.as_ref() else {
            return;
        };
        let Some((key, value)) = self.property_panel.queue.pop() else {
            if std::mem::take(&mut self.property_panel.reload_needed) {
                self.request_react_reload(cx);
            }
            return;
        };
        let target = props_target(&react.entry, &react.property_args);
        let entry = react.entry.clone();
        let property_args = react.property_args.clone();
        self.property_panel.writing = Some(key.clone());
        let session = self.session;
        cx.spawn(async move |view, cx| {
            let written_key = key.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    write_recorded(&target, |text| {
                        set_property_text(&target, text, &written_key, value.as_ref())
                            .map_err(|error| error.to_string())
                    })
                    // Sampled after the write, so the reload watcher does
                    // not take this write for an edit from elsewhere.
                    .map(|edit| (edit, watched_mtime(&entry, &property_args)))
                })
                .await;
            view.update(cx, |this, cx| {
                // Another file opened meanwhile; its panel never queued this write.
                if this.session == session {
                    this.finish_property_commit(&key, result, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn finish_property_commit(
        &mut self,
        key: &str,
        result: Result<(FileEdit, Option<SystemTime>), String>,
        cx: &mut Context<Self>,
    ) {
        self.property_panel.writing = None;
        match result {
            Ok((edit, watched)) => {
                if edit.changes() {
                    self.edit_history.push(edit.into());
                }
                let next_reload = self.apply_own_write_sample(watched);
                self.property_panel.reload_needed = true;
                if let Some(row) = self.property_panel.rows.get_mut(key) {
                    row.state.written(next_reload);
                }
            }
            Err(message) => {
                let queued_again = self.property_panel.queue.contains(key);
                let effective = self
                    .react_preview
                    .as_ref()
                    .and_then(|react| effective_value(react, key));
                if let Some(row) = self.property_panel.rows.get_mut(key) {
                    // A reload from now on reads the file as it is then.
                    let released_by = self.react_reloads.next();
                    row.state.report_error(message, Some(released_by));
                    // The file still holds the old value: show it again, but
                    // keep the typed text so the person can retry.
                    if !queued_again {
                        row.state.write_failed(effective);
                    }
                }
            }
        }
        self.drain_property_commits(cx);
        self.drain_held_rows(cx);
        cx.notify();
    }

    /// `Choose…` on a path row: imports the chosen file into the project and
    /// commits the path the props file should hold.
    pub(crate) fn choose_property_path(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(react) = self.react_preview.as_ref() else {
            return;
        };
        let project_root = react
            .entry
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let target = props_target(&react.entry, &react.property_args);
        let props_dir = target
            .parent()
            .map_or_else(|| project_root.clone(), Path::to_path_buf);
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let key = key.to_owned();
        let session = self.session;
        cx.spawn(async move |view, cx| {
            let chosen = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    view.update(cx, |this, cx| {
                        if this.session == session {
                            let message = format!("Couldn’t show the file dialog: {error}");
                            this.set_property_write_error(&key, message, cx);
                        }
                    })
                    .ok();
                    None
                }
            };
            let Some(chosen) = chosen else {
                return;
            };
            let imported = cx
                .background_executor()
                .spawn(async move { import_path(&project_root, &props_dir, &chosen) })
                .await;
            view.update(cx, |this, cx| {
                if this.session != session {
                    return;
                }
                match imported {
                    Ok(path) => {
                        let value = Some(Value::String(path));
                        this.commit_property(&key, value, TextUpdate::Always, cx);
                    }
                    Err(error) => {
                        let message = format!("Couldn’t use the chosen file: {error}");
                        this.set_property_write_error(&key, message, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn set_property_write_error(&mut self, key: &str, message: String, cx: &mut Context<Self>) {
        if let Some(row) = self.property_panel.rows.get_mut(key) {
            row.state.report_error(message, None);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Hsla, Rgba};
    use serde_json::json;

    #[test]
    fn queued_commits_to_one_key_keep_the_last_value() {
        let mut queue = CommitQueue::default();
        queue.push("a".to_owned(), Some(json!(1)));
        queue.push("b".to_owned(), Some(json!(2)));
        queue.push("a".to_owned(), Some(json!(3)));

        assert_eq!(queue.pop(), Some(("a".to_owned(), Some(json!(3)))));
        assert_eq!(queue.pop(), Some(("b".to_owned(), Some(json!(2)))));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn issues_are_grouped_by_key() {
        let issue = |key: &str, source: &str, message: &str| PropertyIssue {
            key: key.to_owned(),
            source: source.to_owned(),
            message: message.to_owned(),
        };
        let (by_key, general) = issues_by_key(&[
            issue("title", "props.json", "must be a string"),
            issue("", "--props", "must be a JSON object"),
            issue("opacity", "props.json", "must be at most 1"),
            issue("title", "--props", "must not be empty"),
        ]);

        assert_eq!(
            by_key,
            BTreeMap::from([
                ("opacity".to_owned(), vec!["must be at most 1".to_owned()]),
                (
                    "title".to_owned(),
                    vec![
                        "must be a string".to_owned(),
                        "must not be empty".to_owned()
                    ]
                ),
            ])
        );
        assert_eq!(general, vec!["--props: must be a JSON object".to_owned()]);
    }

    #[test]
    fn the_target_is_the_props_file_in_use_or_a_new_props_json() {
        let dir = std::env::temp_dir().join(format!("celesta-props-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("variants")).unwrap();
        let entry = dir.join("film.tsx");

        let variant = dir.join("variants").join("a.json");
        let with_file = PropertyArgs::new(Some(variant.clone()), None);
        assert_eq!(props_target(&entry, &with_file), variant);

        // No props.json yet: the first commit creates one beside the entry.
        assert_eq!(
            props_target(&entry, &PropertyArgs::default()),
            dir.join("props.json")
        );

        // An existing props.json is the file in use.
        std::fs::write(dir.join("props.json"), "{}").unwrap();
        assert_eq!(
            props_target(&entry, &PropertyArgs::default()),
            dir.join("props.json")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numbers_equal_to_the_default_are_not_a_difference() {
        let number = ComponentPropertyField::Number {
            label: None,
            default_value: 1.0,
            min: None,
            max: None,
            step: None,
        };
        assert!(!differs_from_default(&number, &json!(1)));
        assert!(!differs_from_default(&number, &json!(1.0)));
        assert!(differs_from_default(&number, &json!(0.5)));
        assert!(differs_from_default(&number, &json!("1")));

        let text = ComponentPropertyField::String {
            label: None,
            default_value: "Hello".to_owned(),
        };
        assert!(!differs_from_default(&text, &json!("Hello")));
        assert!(differs_from_default(&text, &json!("Bye")));
    }

    #[test]
    fn whole_numbers_are_written_without_a_fraction() {
        assert_eq!(number_value(2.0), json!(2));
        assert_eq!(number_value(-3.0), json!(-3));
        assert_eq!(number_value(0.25), json!(0.25));
        assert_eq!(number_value(1e300), json!(1e300));
    }

    #[test]
    fn inputs_show_strings_unquoted_and_numbers_plainly() {
        assert_eq!(value_text(&json!("./assets/a.png")), "./assets/a.png");
        assert_eq!(value_text(&json!(2.0)), "2");
        assert_eq!(value_text(&json!(0.5)), "0.5");
        assert_eq!(value_text(&json!(true)), "true");
    }

    #[test]
    fn keys_follow_the_declaration_order() {
        let text = |default: &str| ComponentPropertyField::String {
            label: None,
            default_value: default.to_owned(),
        };
        let schema = KeptSchema {
            fields: BTreeMap::from([
                ("a".to_owned(), text("")),
                ("b".to_owned(), text("")),
                ("c".to_owned(), text("")),
            ]),
            order: vec!["c".to_owned(), "gone".to_owned(), "a".to_owned()],
        };
        assert_eq!(ordered_keys(&schema), vec!["c", "a", "b"]);
    }

    #[test]
    fn a_stale_reload_does_not_revert_a_row_written_after_it_started() {
        let mut row = RowState::new();
        row.offer_loaded(&json!(false), 0, false);
        row.take_update();
        // Reload 1 starts and reads `false`; meanwhile `true` is written, so
        // only reload 2 or later reads it.
        assert!(row.commit(Some(&json!(true)), json!(false), TextUpdate::Keep));
        row.take_update();
        row.written(2);

        row.offer_loaded(&json!(false), 1, false);
        assert_eq!(row.shown, json!(true));
        assert_eq!(row.take_update(), None);

        // Reload 2 read the file after the write; an edit from elsewhere shows.
        row.offer_loaded(&json!(false), 2, false);
        assert_eq!(row.shown, json!(false));
        assert_eq!(
            row.take_update(),
            Some((json!(false), TextUpdate::UnlessFocused))
        );
    }

    #[test]
    fn a_pending_commit_keeps_its_value() {
        let mut row = RowState::new();
        row.commit(Some(&json!("new")), json!(""), TextUpdate::Keep);
        row.offer_loaded(&json!("old"), 5, true);
        assert_eq!(row.shown, json!("new"));
    }

    #[test]
    fn blur_without_an_edit_does_not_commit() {
        let mut row = RowState::new();
        row.offer_loaded(&json!("from disk"), 0, false);
        row.take_update();
        row.text_replaced();

        // Focused while another editor changed the file: the text is stale.
        row.offer_loaded(&json!("changed elsewhere"), 0, false);
        row.take_update();
        assert_eq!(row.submitted(false), TextSubmit::Ignore);
        assert_eq!(row.submitted(true), TextSubmit::Refresh);
        assert_eq!(
            row.take_update(),
            Some((json!("changed elsewhere"), TextUpdate::Always))
        );

        row.text_edited();
        assert_eq!(row.submitted(true), TextSubmit::Commit);
        row.commit(Some(&json!("typed")), json!(""), TextUpdate::Keep);
        assert_eq!(row.submitted(true), TextSubmit::Refresh);
    }

    #[test]
    fn a_failed_write_keeps_the_typed_text_as_an_edit() {
        let mut row = RowState::new();
        row.text_edited();
        row.commit(Some(&json!("typed")), json!(""), TextUpdate::Keep);
        row.take_update();

        row.write_failed(Some(json!("on disk")));
        assert_eq!(row.shown, json!("on disk"));
        assert_eq!(
            row.take_update(),
            Some((json!("on disk"), TextUpdate::Keep))
        );
        assert_eq!(row.submitted(true), TextSubmit::Commit);
    }

    #[test]
    fn a_typed_color_updates_the_picker() {
        let mut row = RowState::new();
        assert!(row.commit(Some(&json!("#AABBCC")), json!("#000000"), TextUpdate::Keep));
        // Keep leaves the typed text alone but still updates the picker.
        let (value, text) = row.take_update().expect("a control update");
        assert_eq!(text, TextUpdate::Keep);
        assert_eq!(
            parse_color(&value),
            Some(Hsla::from(Rgba::try_from("#AABBCC").unwrap()))
        );
    }

    #[test]
    fn committing_the_shown_value_writes_nothing() {
        let mut row = RowState::new();
        row.commit(Some(&json!(2)), json!(0), TextUpdate::Keep);
        assert!(!row.commit(Some(&json!(2.0)), json!(0), TextUpdate::Keep));
        // Reset always writes: it removes the key even when it equals the default.
        assert!(row.commit(None, json!(0), TextUpdate::Always));
    }

    #[test]
    fn a_write_error_clears_once_a_later_reload_read_the_file() {
        let mut row = RowState::new();
        // The write failed while the next reload to be requested was number 4.
        row.report_error("Fix props.json before editing: …".to_owned(), Some(4));

        // Reload 3 started before the failure, so it says nothing about the fix.
        row.offer_read(3);
        assert!(row.write_error.is_some());

        row.offer_read(4);
        assert_eq!(row.write_error, None);
    }

    #[test]
    fn a_write_error_without_a_release_waits_for_a_successful_write() {
        let mut row = RowState::new();
        row.report_error("Couldn’t use the chosen file".to_owned(), None);

        row.offer_read(100);
        assert!(row.write_error.is_some());

        row.written(5);
        assert_eq!(row.write_error, None);
    }

    #[test]
    fn colors_differing_only_in_case_are_the_same_value() {
        let color = ComponentPropertyField::Color {
            label: None,
            default_value: "#AABBCC".to_owned(),
        };
        assert!(!differs_from_default(&color, &json!("#aabbcc")));
        assert!(differs_from_default(&color, &json!("#aabbcd")));

        // Typing the same color in another case writes nothing.
        let mut row = RowState::for_field(&color);
        assert!(row.commit(Some(&json!("#AABBCC")), json!("#000000"), TextUpdate::Keep));
        assert!(!row.commit(Some(&json!("#aabbcc")), json!("#000000"), TextUpdate::Keep));
        assert_eq!(row.shown, json!("#AABBCC"));
        // Nor does a reload that read the other spelling replace the row.
        row.take_update();
        row.offer_loaded(&json!("#aabbcc"), 0, false);
        assert_eq!(row.take_update(), None);

        // A string field keeps case-sensitive comparison.
        let text = ComponentPropertyField::String {
            label: None,
            default_value: "Hello".to_owned(),
        };
        assert!(differs_from_default(&text, &json!("hello")));
    }
}
