use std::cmp::Ordering;

use semver::{Comparator, Op, Version};

use crate::error::FormatError;
use crate::files::{Advisory, AdvisoryList};

/// The version a comparator names, with build metadata dropped.
fn bound(c: &Comparator) -> Version {
    Version {
        major: c.major,
        minor: c.minor.unwrap_or(0),
        patch: c.patch.unwrap_or(0),
        pre: c.pre.clone(),
        build: semver::BuildMetadata::EMPTY,
    }
}

/// Evaluates one comparator with `Version`'s total ordering. Unlike
/// `VersionReq::matches`, pre-releases are not excluded, so `<0.2.3` matches `0.2.3-rc.1`.
fn holds(c: &Comparator, version: &Version) -> bool {
    let ord = version.cmp_precedence(&bound(c));
    match c.op {
        Op::Exact => ord == Ordering::Equal,
        Op::Less => ord == Ordering::Less,
        Op::LessEq => ord != Ordering::Greater,
        Op::Greater => ord == Ordering::Greater,
        Op::GreaterEq => ord != Ordering::Less,
        // `parse` refuses every other operator; failing closed here is never reached.
        _ => false,
    }
}

impl Advisory {
    fn blocks(&self, plugin: &str, version: &Version) -> bool {
        self.plugin == plugin
            && !self.versions.comparators.is_empty()
            && self.versions.comparators.iter().all(|c| holds(c, version))
    }
}

impl AdvisoryList {
    /// The first advisory, in file order, that forbids this plugin version.
    pub fn blocking(&self, plugin: &str, version: &Version) -> Option<&Advisory> {
        self.advisories.iter().find(|a| a.blocks(plugin, version))
    }

    /// Refuses a list older than the cached one; an equal timestamp is accepted.
    pub fn check_newer(&self, cached: Option<&AdvisoryList>) -> Result<(), FormatError> {
        match cached {
            Some(c) if self.generated < c.generated => Err(FormatError::Rollback),
            _ => Ok(()),
        }
    }
}
