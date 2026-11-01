//! The content plugins an open project draws with. They are found when the
//! project opens and again on File > Reload Plugins, in every location
//! Celesta searches (`celesta_plugin_host::search_paths`), and the one set
//! serves the preview's renderer, every React runtime the editor starts, and
//! exports, so they all agree on what is installed. Plugins that were
//! skipped (a bad manifest, a folder other users can change) show in
//! Problems.

use crate::problems::{Problem, Source};
use crate::view::EditorView;
use celesta_plugin_host::InstalledPlugins;

/// What one discovery found.
#[derive(Clone, Debug, Default)]
pub(crate) struct FoundPlugins {
    pub(crate) installed: InstalledPlugins,
    /// Why each skipped plugin or folder was skipped.
    pub(crate) skipped: Vec<String>,
}

impl FoundPlugins {
    /// Reads every plugin location; blocking, so off the UI thread.
    pub(crate) fn find() -> Self {
        let (installed, errors) = InstalledPlugins::find(&[]);
        Self {
            installed,
            skipped: errors.iter().map(ToString::to_string).collect(),
        }
    }

    /// The Problems entry for the skipped plugins, if any were.
    pub(crate) fn problem(&self) -> Option<Problem> {
        match self.skipped.as_slice() {
            [] => None,
            [one] => Some(Problem::new("A plugin was skipped", one.clone())),
            many => Some(Problem::new(
                format!("{} plugins were skipped", many.len()),
                many.join("\n"),
            )),
        }
    }
}

impl EditorView {
    /// Takes `found` as this view's plugins and reports what was skipped.
    pub(crate) fn set_plugins(&mut self, found: FoundPlugins) {
        self.problems.set(Source::Plugins, found.problem());
        self.plugins = found.installed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_plugins_make_one_problem() {
        assert_eq!(FoundPlugins::default().problem(), None);
        let found = |skipped: &[&str]| FoundPlugins {
            skipped: skipped.iter().map(|line| (*line).to_owned()).collect(),
            ..FoundPlugins::default()
        };
        assert_eq!(
            found(&["a was skipped: bad id"]).problem(),
            Some(Problem::new(
                "A plugin was skipped",
                "a was skipped: bad id"
            ))
        );
        assert_eq!(
            found(&["a was skipped", "b was skipped"]).problem(),
            Some(Problem::new(
                "2 plugins were skipped",
                "a was skipped\nb was skipped"
            ))
        );
    }
}
