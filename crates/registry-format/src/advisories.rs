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
        // `parse` refuses every other operator, but a list built without it can
        // carry one. Treat an unsupported operator as matching, so it blocks.
        _ => true,
    }
}

// An empty comparator list (`*`) matches every version through `all`.
impl Advisory {
    fn blocks(&self, plugin: &str, version: &Version) -> bool {
        self.plugin == plugin && self.versions.comparators.iter().all(|c| holds(c, version))
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

#[cfg(test)]
mod tests {
    use semver::VersionReq;
    use time::OffsetDateTime;

    use super::*;

    fn advisory(range: &str) -> Advisory {
        Advisory {
            id: "X".into(),
            plugin: "vrm".into(),
            versions: VersionReq::parse(range).unwrap(),
            title: "t".into(),
            description: "d".into(),
            fixed: None,
            published: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn unvalidated_ranges_fail_closed() {
        for range in ["^0.2.2", "*", "~0.2.2", "0.2.*"] {
            let a = advisory(range);
            for v in ["0.2.2", "9.9.9"] {
                assert!(a.blocks("vrm", &Version::parse(v).unwrap()), "{range} {v}");
            }
            assert!(!a.blocks("other", &Version::new(0, 2, 2)));
        }
    }
}
