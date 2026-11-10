//! The lists the page's Selects show: a choice per row, sections for
//! the preset list, and the components as their plugins described them.

use super::super::model::{CHECKING, ComponentOption, OptionState};
use gpui_kit::component::IndexPath;
use gpui_kit::component::select::{SelectDelegate, SelectItem, SelectState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Entity, SharedString, Window};

/// One row of a choice list: what it shows and the value it stands for.
#[derive(Clone)]
pub(super) struct Choice<V> {
    pub(super) title: SharedString,
    pub(super) value: V,
    pub(super) disabled: bool,
}

impl<V> Choice<V> {
    pub(super) fn new(title: impl Into<SharedString>, value: V) -> Self {
        Self {
            title: title.into(),
            value,
            disabled: false,
        }
    }
}

impl<V: Clone + PartialEq + 'static> SelectItem for Choice<V> {
    type Value = V;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &V {
        &self.value
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

/// Choices under section titles; a section without one is listed first,
/// as the preset list's "None" is.
pub(super) struct Sections<V>(pub(super) Vec<(Option<SharedString>, Vec<Choice<V>>)>);

impl<V: Clone + PartialEq + 'static> SelectDelegate for Sections<V> {
    type Item = Choice<V>;

    fn sections_count(&self, _: &App) -> usize {
        self.0.len()
    }

    // The adapter wraps this title in its standard section header; the
    // newer hook would have the header drawn here instead.
    fn section(&self, section: usize) -> Option<AnyElement> {
        let title = self.0.get(section)?.0.clone()?;
        Some(title.into_any_element())
    }

    fn items_count(&self, section: usize) -> usize {
        self.0.get(section).map_or(0, |(_, items)| items.len())
    }

    fn item(&self, ix: IndexPath) -> Option<&Choice<V>> {
        self.0.get(ix.section)?.1.get(ix.row)
    }

    fn position<W>(&self, value: &W) -> Option<IndexPath>
    where
        Self::Item: SelectItem<Value = W>,
        W: PartialEq,
    {
        self.0.iter().enumerate().find_map(|(section, (_, items))| {
            let row = items.iter().position(|item| item.value() == value)?;
            Some(IndexPath::default().section(section).row(row))
        })
    }
}

/// A component as its list shows it: an unavailable or checking one is
/// disabled, followed by why.
pub(super) fn component_choice<V>(option: &ComponentOption, value: V) -> Choice<V> {
    let reason = match &option.state {
        OptionState::Available => None,
        OptionState::Checking => Some(CHECKING),
        OptionState::Unavailable(reason) => Some(reason.as_str()),
    };
    Choice {
        title: match reason {
            Some(reason) => format!("{} — {reason}", option.label).into(),
            None => option.label.clone().into(),
        },
        value,
        disabled: reason.is_some(),
    }
}

/// Gives `select` `items` and selects `value` (nothing for `None`).
pub(super) fn sync_select<V: Clone + PartialEq + 'static>(
    select: &Entity<SelectState<Vec<Choice<V>>>>,
    items: Vec<Choice<V>>,
    value: Option<&V>,
    window: &mut Window,
    cx: &mut App,
) {
    select.update(cx, |state, cx| {
        state.set_items(items, window, cx);
        match value {
            Some(value) => state.set_selected_value(value, window, cx),
            None => state.set_selected_index(None, window, cx),
        }
    });
}
