//! The clock and random APIs an entry was seen calling while a frame was
//! evaluated. The runtime makes them deterministic (`Math.random` reseeded
//! per frame from the frame's time, `Date.now()`, `performance.now()` and
//! `new Date()` frozen at it; see `packages/react/src/determinism.ts`) and
//! names each one once, in the answer after its first call; the bridge
//! gathers them here for a warning.

use std::fmt;

/// The warning's code, which starts its text: stable for scripts and agents
/// to match whatever the wording after it says.
pub const NONDETERMINISTIC_RENDER: &str = "nondeterministic_render";

/// The order the warning lists the APIs in; others the runtime may add
/// follow, sorted.
const ORDER: [&str; 4] = [
    "Math.random()",
    "Date.now()",
    "performance.now()",
    "new Date()",
];

/// The APIs an entry called while rendering, each once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NondeterministicCalls {
    apis: Vec<String>,
}

impl NondeterministicCalls {
    /// The APIs, such as `Math.random()`, in the warning's order.
    pub fn apis(&self) -> &[String] {
        &self.apis
    }

    pub const fn is_empty(&self) -> bool {
        self.apis.is_empty()
    }

    /// The warning for these calls; `None` when there were none.
    pub fn warning(&self) -> Option<String> {
        (!self.is_empty()).then(|| self.to_string())
    }

    pub(crate) fn add(&mut self, apis: impl IntoIterator<Item = String>) {
        for api in apis {
            if !self.apis.contains(&api) {
                self.apis.push(api);
            }
        }
        self.apis
            .sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    }
}

fn rank(api: &str) -> usize {
    ORDER
        .iter()
        .position(|known| *known == api)
        .unwrap_or(ORDER.len())
}

impl fmt::Display for NondeterministicCalls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{NONDETERMINISTIC_RENDER}: the entry calls ")?;
        let last = self.apis.len().saturating_sub(1);
        for (index, api) in self.apis.iter().enumerate() {
            match index {
                0 => {}
                _ if index == last => f.write_str(if last == 1 { " and " } else { ", and " })?,
                _ => f.write_str(", ")?,
            }
            f.write_str(api)?;
        }
        f.write_str(
            " while rendering; Celesta reseeds Math.random() for every frame and freezes the \
             clock at the frame's time, so these values follow the frame and not the run. \
             Use random(seed) from @celesta.studio/math, and useCurrentFrame() or \
             useCurrentTime()",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_each_api_once_in_a_fixed_order() {
        let mut calls = NondeterministicCalls::default();
        assert_eq!(calls.warning(), None);
        calls.add(["new Date()".to_owned(), "Math.random()".to_owned()]);
        calls.add(["Math.random()".to_owned(), "Date.now()".to_owned()]);
        assert_eq!(calls.apis(), ["Math.random()", "Date.now()", "new Date()"]);
        let warning = calls.warning().unwrap();
        assert!(
            warning.starts_with(
                "nondeterministic_render: the entry calls Math.random(), Date.now(), and new Date() while rendering;"
            ),
            "{warning}"
        );
    }

    #[test]
    fn names_two_apis_with_and_and_keeps_unknown_ones() {
        let mut calls = NondeterministicCalls::default();
        calls.add([
            "performance.now()".to_owned(),
            "crypto.randomUUID()".to_owned(),
        ]);
        assert_eq!(calls.apis(), ["performance.now()", "crypto.randomUUID()"]);
        assert!(
            calls
                .to_string()
                .starts_with("nondeterministic_render: the entry calls performance.now() and crypto.randomUUID() while")
        );
    }
}
