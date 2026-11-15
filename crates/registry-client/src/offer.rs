//! The version the app offers to install: the newest one `resolve` would
//! accept, so an offer never disagrees with the install that follows.

use std::collections::BTreeMap;

use celesta_registry_format::{AdvisoryList, Index, Target, VersionEntry};
use semver::{Comparator, Op, Prerelease, Version, VersionReq};

use crate::resolve::{ResolveInput, Update, resolve};

/// What the offer is for, which decides the targets a build must cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferScope {
    /// An install for this machine alone, as `install_global` does.
    Global,
    /// An install into the project, which locks for the project's targets.
    Project,
    /// A pin written to the project's requirements, locked for its targets.
    Pin,
}

/// What the offer reads.
pub struct OfferInput<'a> {
    pub index: &'a Index,
    pub advisories: Option<&'a AdvisoryList>,
    /// The running Celesta.
    pub celesta: &'a Version,
    /// The target of the running process.
    pub host: &'a Target,
    /// The project's targets (`ProjectDependencies::project_targets`); ignored for `Global`.
    pub targets: &'a [Target],
    pub include_prereleases: bool,
    pub scope: OfferScope,
}

/// Why no version is offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoOffer {
    /// No candidate has a build for the host (`Global`) or the project's
    /// targets (`Project`, `Pin`).
    NoBuild { host: Target },
    /// The newest otherwise eligible version's range excludes the running
    /// Celesta.
    NeedsCelesta {
        newest: Version,
        celesta: VersionReq,
    },
    /// Only pre-releases are eligible, and they are not included.
    OnlyPrereleases { newest: Version },
    /// Every version is yanked, withdrawn, or blocked.
    NothingPublished,
}

/// Whether the running Celesta is a pre-release, which then offers
/// pre-releases of plugins unless the user turns them off.
pub fn prereleases_by_default(celesta: &Version) -> bool {
    !celesta.pre.is_empty()
}

/// The requirement a project records for an offered version: `^version`.
pub fn offered_requirement(version: &Version) -> VersionReq {
    VersionReq {
        comparators: vec![Comparator {
            op: Op::Caret,
            major: version.major,
            minor: Some(version.minor),
            patch: Some(version.patch),
            pre: version.pre.clone(),
        }],
    }
}

/// The newest version of the plugin that `resolve` accepts for the scope.
pub fn offered_version(input: &OfferInput) -> Result<Version, NoOffer> {
    let mut index = input.index.clone();
    let targets = match input.scope {
        OfferScope::Global => {
            // Installed for this host alone, a media plugin needs a build
            // here, as in `install_global`.
            index.kind.roles.retain(|role| role != "media");
            std::slice::from_ref(input.host)
        }
        OfferScope::Project | OfferScope::Pin => input.targets,
    };

    let mut alive: Vec<&VersionEntry> = index
        .versions
        .iter()
        .filter(|v| !v.yanked && !v.withdrawn && !blocked(input, &index, &v.version))
        .collect();
    alive.sort_by(|a, b| b.version.cmp(&a.version));
    let Some(newest_alive) = alive.first() else {
        return Err(NoOffer::NothingPublished);
    };
    let candidates: Vec<&VersionEntry> = alive
        .iter()
        .filter(|v| input.include_prereleases || v.version.pre == Prerelease::EMPTY)
        .copied()
        .collect();
    let Some(newest) = candidates.first() else {
        return Err(NoOffer::OnlyPrereleases {
            newest: newest_alive.version.clone(),
        });
    };

    let indexes = BTreeMap::from([(index.id.clone(), index.clone())]);
    for candidate in &candidates {
        let dependencies = BTreeMap::from([(
            index.id.clone(),
            VersionReq {
                comparators: vec![Comparator {
                    op: Op::Exact,
                    major: candidate.version.major,
                    minor: Some(candidate.version.minor),
                    patch: Some(candidate.version.patch),
                    pre: candidate.version.pre.clone(),
                }],
            },
        )]);
        let resolved = resolve(&ResolveInput {
            dependencies: &dependencies,
            indexes: &indexes,
            advisories: input.advisories,
            celesta: input.celesta,
            targets,
            host: input.host,
            registry: "",
            previous: None,
            update: &Update::All,
        });
        if resolved.is_ok() {
            return Ok(candidate.version.clone());
        }
    }

    if newest.celesta.matches(input.celesta) {
        Err(NoOffer::NoBuild {
            host: input.host.clone(),
        })
    } else {
        Err(NoOffer::NeedsCelesta {
            newest: newest.version.clone(),
            celesta: newest.celesta.clone(),
        })
    }
}

fn blocked(input: &OfferInput, index: &Index, version: &Version) -> bool {
    input
        .advisories
        .is_some_and(|list| list.blocking(&index.id, version).is_some())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::testing::{TestRegistry, at};

    const ALPHA: &str = ">=0.10.0-alpha.3, <0.11.0";

    fn host() -> Target {
        Target::parse("macos-aarch64").unwrap()
    }

    fn version(text: &str) -> Version {
        text.parse().unwrap()
    }

    fn index_of(registry: &TestRegistry, id: &str) -> Index {
        let cache = TempDir::new().unwrap();
        registry
            .client(cache.path())
            .index(id, at("2026-12-01T00:00:00Z"))
            .unwrap()
    }

    fn set_celesta(registry: &TestRegistry, id: &str, versions: &[&str], range: &str) {
        for v in versions {
            registry.edit(id, v, |entry| entry["celesta"] = json!(range));
        }
    }

    fn advisories(list: serde_json::Value) -> AdvisoryList {
        serde_json::from_value(json!({
            "generated": "2026-11-20T00:00:00Z",
            "advisories": list,
        }))
        .unwrap()
    }

    fn offer(
        index: &Index,
        celesta: &str,
        include_prereleases: bool,
        scope: OfferScope,
        targets: &[Target],
        advisories: Option<&AdvisoryList>,
    ) -> Result<Version, NoOffer> {
        offered_version(&OfferInput {
            index,
            advisories,
            celesta: &version(celesta),
            host: &host(),
            targets,
            include_prereleases,
            scope,
        })
    }

    fn global(index: &Index, celesta: &str, include_prereleases: bool) -> Result<Version, NoOffer> {
        offer(
            index,
            celesta,
            include_prereleases,
            OfferScope::Global,
            &[],
            None,
        )
    }

    #[test]
    fn a_prerelease_is_offered_only_when_included() {
        let registry = TestRegistry::new();
        registry.publish("acme.one", "0.10.0-alpha.3", &["macos-aarch64"]);
        set_celesta(&registry, "acme.one", &["0.10.0-alpha.3"], ALPHA);
        let index = index_of(&registry, "acme.one");

        assert_eq!(
            global(&index, "0.10.0-alpha.3", true),
            Ok(version("0.10.0-alpha.3"))
        );
        assert_eq!(
            global(&index, "0.10.0-alpha.3", false),
            Err(NoOffer::OnlyPrereleases {
                newest: version("0.10.0-alpha.3")
            })
        );
    }

    #[test]
    fn the_newest_stable_wins_over_an_older_prerelease() {
        let registry = TestRegistry::new();
        for v in ["1.0.0", "1.1.0-beta.1", "0.9.0"] {
            registry.publish("acme.one", v, &["macos-aarch64"]);
            set_celesta(&registry, "acme.one", &[v], ">=0.10.0");
        }
        let index = index_of(&registry, "acme.one");

        assert_eq!(global(&index, "0.10.0", false), Ok(version("1.0.0")));
        assert_eq!(global(&index, "0.10.0", true), Ok(version("1.1.0-beta.1")));
    }

    #[test]
    fn yanked_and_blocked_versions_are_skipped() {
        let registry = TestRegistry::new();
        for v in ["1.2.0", "1.1.0", "1.0.0"] {
            registry.publish("acme.one", v, &["macos-aarch64"]);
            set_celesta(&registry, "acme.one", &[v], ">=0.10.0");
        }
        registry.edit("acme.one", "1.2.0", |entry| entry["yanked"] = json!(true));
        let index = index_of(&registry, "acme.one");
        let list = advisories(json!([{
            "id": "CELESTA-2026-0009",
            "plugin": "acme.one",
            "versions": "<1.2.0, >=1.1.0",
            "title": "Bad",
            "description": "Does bad things.",
            "fixed": "1.2.0",
            "published": "2026-11-20T00:00:00Z",
        }]));

        let host = host();
        let at_host = |index: &Index, advisories: Option<&AdvisoryList>| {
            offer(
                index,
                "0.10.0",
                false,
                OfferScope::Global,
                std::slice::from_ref(&host),
                advisories,
            )
        };
        assert_eq!(at_host(&index, Some(&list)), Ok(version("1.0.0")));

        registry.edit("acme.one", "1.0.0", |entry| {
            entry["withdrawn"] = json!(true)
        });
        let index = index_of(&registry, "acme.one");
        assert_eq!(at_host(&index, Some(&list)), Err(NoOffer::NothingPublished));
    }

    #[test]
    fn a_version_for_another_celesta_is_not_offered() {
        let registry = TestRegistry::new();
        registry.publish("acme.one", "2.0.0", &["macos-aarch64"]);
        set_celesta(&registry, "acme.one", &["2.0.0"], ">=0.11.0");
        let index = index_of(&registry, "acme.one");

        assert_eq!(
            global(&index, "0.10.0", false),
            Err(NoOffer::NeedsCelesta {
                newest: version("2.0.0"),
                celesta: ">=0.11.0".parse().unwrap(),
            })
        );
    }

    #[test]
    fn global_needs_a_host_build_but_pin_does_not() {
        let registry = TestRegistry::new();
        registry.publish_with_roles("acme.media", "1.0.0", &["windows-x86_64"], &["media"]);
        set_celesta(&registry, "acme.media", &["1.0.0"], ">=0.10.0");
        let index = index_of(&registry, "acme.media");
        let project = [host(), Target::parse("windows-x86_64").unwrap()];

        assert_eq!(
            global(&index, "0.10.0", false),
            Err(NoOffer::NoBuild { host: host() })
        );
        assert_eq!(
            offer(&index, "0.10.0", false, OfferScope::Pin, &project, None),
            Ok(version("1.0.0"))
        );
    }

    #[test]
    fn prereleases_by_default_follows_the_running_celesta() {
        assert!(prereleases_by_default(&version("0.10.0-alpha.3")));
        assert!(!prereleases_by_default(&version("0.10.0")));
    }

    #[test]
    fn the_offered_requirement_is_a_caret() {
        let requirement = offered_requirement(&version("0.10.0-alpha.3"));

        assert_eq!(requirement.to_string(), "^0.10.0-alpha.3");
        assert!(requirement.matches(&version("0.10.0-alpha.4")));
        assert!(requirement.matches(&version("0.10.2")));
        assert!(!requirement.matches(&version("0.11.0")));
    }
}
