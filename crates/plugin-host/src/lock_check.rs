//! Whether a project's `celesta.lock` lets command-line tools run its
//! registry plugins: tools read the lock whether or not the user trusts the
//! project, and stop on a lock that is missing, stale, or not installed
//! rather than run other versions ([`crate::InstalledPlugins::find_for_project`]).

use crate::lock::Lockfile;
use crate::project::ProjectDependencies;
use semver::Version;

/// Why a project's registry plugins cannot run as its lock pins them.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum LockProblem {
    /// `celesta.config.json` could not be read.
    #[error("{0}")]
    Config(String),
    #[error("the project's celesta.lock could not be read: {0}")]
    Unreadable(String),
    #[error(
        "the project declares registry plugins but has no celesta.lock; run `celesta plugin install`"
    )]
    NoLock,
    /// Dependencies the lock misses or locks outside their requirement, and
    /// locked plugins that are no dependency, sorted.
    #[error(
        "celesta.lock does not match celesta.config.json for {}; run `celesta plugin install`",
        ids.join(", ")
    )]
    OutOfDate { ids: Vec<String> },
    #[error(
        "celesta.lock was resolved with Celesta {locked}, and this is Celesta {running}; {} cannot load with it. Run `celesta plugin update` to resolve them again",
        ids.join(", ")
    )]
    OtherCelesta {
        locked: Version,
        running: Version,
        ids: Vec<String>,
    },
    /// Each locked version that cannot run, as id, version, and why, in
    /// discovery's order.
    #[error(
        "the project's locked plugins are not installed: {}; run `celesta plugin install`",
        plugins
            .iter()
            .map(|(id, version, reason)| format!("{id} {version} ({reason})"))
            .collect::<Vec<_>>()
            .join("; ")
    )]
    Unsatisfied {
        plugins: Vec<(String, String, String)>,
    },
}

/// Whether `lock` (`None` without one) pins what `project` declares, for
/// Celesta `celesta`.
pub fn check_lock(
    lock: Option<&Lockfile>,
    project: &ProjectDependencies,
    celesta: &Version,
) -> Result<(), LockProblem> {
    let Some(lock) = lock else {
        return if project.dependencies.is_empty() {
            Ok(())
        } else {
            Err(LockProblem::NoLock)
        };
    };
    let unloadable = lock.unloadable(celesta);
    if !unloadable.is_empty() {
        return Err(LockProblem::OtherCelesta {
            locked: lock.celesta.clone(),
            running: celesta.clone(),
            ids: unloadable,
        });
    }
    let stale = project.dependencies.iter().filter(|(id, requirement)| {
        lock.plugins
            .get(*id)
            .is_none_or(|locked| !requirement.matches(&locked.version))
    });
    let orphans = lock
        .plugins
        .keys()
        .filter(|id| !project.dependencies.contains_key(*id));
    let mut ids: Vec<String> = stale.map(|(id, _)| id).chain(orphans).cloned().collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        Ok(())
    } else {
        Err(LockProblem::OutOfDate { ids })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::LockedPlugin;
    use serde_json::Map;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// A lock resolved with Celesta `celesta`, naming each `(id, version,
    /// celesta range)`.
    fn lock(celesta: &str, plugins: &[(&str, &str, &str)]) -> Lockfile {
        Lockfile {
            celesta: version(celesta),
            plugins: plugins
                .iter()
                .map(|&(id, locked, range)| {
                    let locked = LockedPlugin {
                        celesta: range.parse().unwrap(),
                        registry: "https://registry.celesta.studio".to_owned(),
                        targets: Default::default(),
                        version: version(locked),
                        other: Map::new(),
                    };
                    (id.to_owned(), locked)
                })
                .collect(),
            targets: Vec::new(),
            other: Map::new(),
        }
    }

    /// A project declaring each `(id, requirement)`.
    fn project(dependencies: &[(&str, &str)]) -> ProjectDependencies {
        ProjectDependencies {
            dependencies: dependencies
                .iter()
                .map(|&(id, requirement)| (id.to_owned(), requirement.parse().unwrap()))
                .collect(),
            targets: None,
        }
    }

    #[test]
    fn no_lock_is_fine_without_dependencies() {
        assert_eq!(check_lock(None, &project(&[]), &version("0.9.0")), Ok(()));
    }

    #[test]
    fn no_lock_with_dependencies_is_no_lock() {
        assert_eq!(
            check_lock(None, &project(&[("a", "1")]), &version("0.9.0")),
            Err(LockProblem::NoLock)
        );
    }

    #[test]
    fn a_lock_that_pins_every_dependency_is_fine() {
        let lock = lock("0.9.0", &[("a", "1.2.0", "*"), ("b", "0.3.1", "*")]);
        let project = project(&[("a", "1"), ("b", "0.3")]);
        assert_eq!(check_lock(Some(&lock), &project, &version("0.9.0")), Ok(()));
    }

    #[test]
    fn orphan_entries_are_out_of_date() {
        let lock = lock("0.9.0", &[("a", "1.0.0", "*"), ("b", "1.0.0", "*")]);
        assert_eq!(
            check_lock(Some(&lock), &project(&[("a", "1")]), &version("0.9.0")),
            Err(LockProblem::OutOfDate {
                ids: vec!["b".to_owned()]
            })
        );
    }

    #[test]
    fn a_requirement_the_locked_version_misses_is_out_of_date() {
        let lock = lock("0.9.0", &[("b", "1.0.0", "*"), ("c", "1.0.0", "*")]);
        let project = project(&[("a", "1"), ("b", "2"), ("c", "1")]);
        let problem = check_lock(Some(&lock), &project, &version("0.9.0")).unwrap_err();
        assert_eq!(
            problem,
            LockProblem::OutOfDate {
                ids: vec!["a".to_owned(), "b".to_owned()]
            }
        );
        assert_eq!(
            problem.to_string(),
            "celesta.lock does not match celesta.config.json for a, b; run `celesta plugin install`"
        );
    }

    #[test]
    fn other_celesta_names_both_versions() {
        let lock = lock("0.8.0", &[("a", "1.0.0", ">=0.8.0, <0.9.0")]);
        let problem =
            check_lock(Some(&lock), &project(&[("a", "1")]), &version("0.9.0")).unwrap_err();
        assert_eq!(
            problem,
            LockProblem::OtherCelesta {
                locked: version("0.8.0"),
                running: version("0.9.0"),
                ids: vec!["a".to_owned()],
            }
        );
        assert!(
            problem.to_string().contains("celesta plugin update"),
            "{problem}"
        );
    }

    #[test]
    fn unsatisfied_lists_each_locked_version() {
        let problem = LockProblem::Unsatisfied {
            plugins: vec![
                ("b".into(), "1.0.0".into(), "it is not installed".into()),
                ("a".into(), "2.0.0".into(), "its files differ".into()),
            ],
        };
        assert_eq!(
            problem.to_string(),
            "the project's locked plugins are not installed: b 1.0.0 (it is not installed); a 2.0.0 (its files differ); run `celesta plugin install`"
        );
    }
}
