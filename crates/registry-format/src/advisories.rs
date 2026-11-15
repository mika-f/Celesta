use std::cmp::Ordering;

use semver::{Comparator, Op, Version};
use time::{Duration, OffsetDateTime};

use crate::error::FormatError;
use crate::files::{Advisory, AdvisoryList};

/// How far ahead of the client's clock a list's `generated` may be.
const CLOCK_SKEW: Duration = Duration::minutes(10);

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
    // Deserializing refuses partial versions, but an `Advisory` built in code
    // can carry one. Treat it as matching, so it blocks.
    if c.minor.is_none() || c.patch.is_none() {
        return true;
    }
    let ord = version.cmp_precedence(&bound(c));
    match c.op {
        Op::Exact => ord == Ordering::Equal,
        Op::Less => ord == Ordering::Less,
        Op::LessEq => ord != Ordering::Greater,
        Op::Greater => ord == Ordering::Greater,
        Op::GreaterEq => ord != Ordering::Less,
        // Deserializing refuses every other operator, but an `Advisory` built
        // in code can carry one. Treat it as matching, so it blocks.
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

    /// Checks a freshly verified list before it replaces `cached`.
    ///
    /// Refuses a list whose `generated` is more than 10 minutes after `now`
    /// (`FutureDated`), so a list dated in the future cannot be cached and
    /// then refuse every later list as a rollback. Then refuses a list older
    /// than the cached one (`Rollback`); an equal timestamp is accepted.
    pub fn check_newer(
        &self,
        cached: Option<&AdvisoryList>,
        now: OffsetDateTime,
    ) -> Result<(), FormatError> {
        if self.generated > now + CLOCK_SKEW {
            return Err(FormatError::FutureDated);
        }
        match cached {
            Some(c) if self.generated < c.generated => Err(FormatError::Rollback),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use semver::VersionReq;

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
        for range in [
            "^0.2.2",
            "*",
            "~0.2.2",
            "0.2.*",
            "=0.2",
            "<=0.2",
            ">=1, <0.2",
        ] {
            let a = advisory(range);
            for v in ["0.2.2", "9.9.9"] {
                assert!(a.blocks("vrm", &Version::parse(v).unwrap()), "{range} {v}");
            }
            assert!(!a.blocks("other", &Version::new(0, 2, 2)));
        }
    }
}
