//! Warnings of one kind, listed up to [`LISTED`] and counted past it: a
//! file can repeat one mistake for a few bytes each (a primitive that is
//! not triangles, a bind to a target the mesh lacks), and every notice
//! goes back in `open`'s answer.

use celesta_plugin::{Notice, NoticeLevel};

/// The most warnings of one kind `open` lists; one more line counts the
/// rest.
pub(crate) const LISTED: usize = 20;

/// Warnings of one kind, as they are found.
pub(crate) struct Warnings {
    /// What they are, for the line counting the rest: `primitives that are
    /// not drawn`.
    kind: &'static str,
    listed: Vec<Notice>,
    more: usize,
}

impl Warnings {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            listed: Vec::new(),
            more: 0,
        }
    }

    /// A warning, whose message is made only when it is listed.
    pub(crate) fn push(&mut self, message: impl FnOnce() -> String) {
        if self.listed.len() < LISTED {
            self.listed.push(Notice {
                level: NoticeLevel::Warning,
                message: message(),
            });
        } else {
            self.more += 1;
        }
    }

    /// Adds the warnings listed to `notices`, then `and {n} more {kind}`
    /// when some were not.
    pub(crate) fn finish(self, notices: &mut Vec<Notice>) {
        notices.extend(self.listed);
        if self.more > 0 {
            notices.push(Notice {
                level: NoticeLevel::Warning,
                message: format!("and {} more {}", self.more, self.kind),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(count: usize) -> Vec<String> {
        let mut warnings = Warnings::new("things that are wrong");
        for index in 0..count {
            warnings.push(|| {
                assert!(index < LISTED, "warning {index} was made");
                format!("thing {index} is wrong")
            });
        }
        let mut notices = Vec::new();
        warnings.finish(&mut notices);
        notices
            .into_iter()
            .map(|notice| {
                assert_eq!(notice.level, NoticeLevel::Warning);
                notice.message
            })
            .collect()
    }

    #[test]
    fn warnings_past_the_listed_are_counted_in_one_line() {
        assert!(finished(0).is_empty());
        let all: Vec<String> = (0..LISTED).map(|i| format!("thing {i} is wrong")).collect();
        assert_eq!(finished(LISTED), all);
        let mut capped = all.clone();
        capped.push("and 1 more things that are wrong".into());
        assert_eq!(finished(LISTED + 1), capped);
        assert_eq!(
            finished(250_000).last().unwrap(),
            &format!("and {} more things that are wrong", 250_000 - LISTED)
        );
    }
}
