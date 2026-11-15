//! Resolution: turns a project's plugin requirements and the registry's
//! indexes into the lock.

use std::collections::{BTreeMap, BTreeSet};

use celesta_plugin_host::{LockedArtifact, LockedPlugin, Lockfile};
use celesta_registry_format::{AdvisoryList, Index, Target, VersionEntry};
use semver::{Version, VersionReq};

/// Which locked plugins may move to a newer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// Keep every lock entry that still holds.
    Keep,
    /// Re-resolve every dependency.
    All,
    /// Re-resolve only these ids.
    Only(BTreeSet<String>),
}

impl Update {
    fn names(&self, id: &str) -> bool {
        match self {
            Update::Keep => false,
            Update::All => true,
            Update::Only(ids) => ids.contains(id),
        }
    }
}

/// What resolution reads.
pub struct ResolveInput<'a> {
    /// The project's requirements, by plugin id.
    pub dependencies: &'a BTreeMap<String, VersionReq>,
    /// The registry's index of every dependency, by plugin id.
    pub indexes: &'a BTreeMap<String, Index>,
    pub advisories: Option<&'a AdvisoryList>,
    /// The running Celesta.
    pub celesta: &'a Version,
    /// The project's targets.
    pub targets: &'a [Target],
    /// The target of the running process.
    pub host: &'a Target,
    /// The registry's base URL, recorded in each lock entry.
    pub registry: &'a str,
    pub previous: Option<&'a Lockfile>,
    pub update: &'a Update,
}

/// The new lock and what the user should be told about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub lock: Lockfile,
    pub warnings: Vec<ResolveWarning>,
}

/// Something worth telling the user that does not stop resolution.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveWarning {
    #[error("{id} {version} has no build for {}", join_targets(targets))]
    MissingTargets {
        id: String,
        version: Version,
        targets: Vec<Target>,
    },
    #[error(
        "{id} {version} is deprecated: {message}{}",
        replacement_note(replacement)
    )]
    Deprecated {
        id: String,
        version: Version,
        message: String,
        replacement: Option<String>,
    },
    #[error("{id} {version} is locked and has been yanked")]
    LockedYanked { id: String, version: Version },
}

fn join_targets(targets: &[Target]) -> String {
    targets
        .iter()
        .map(Target::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn replacement_note(replacement: &Option<String>) -> String {
    match replacement {
        Some(r) => format!(" (use {r} instead)"),
        None => String::new(),
    }
}

/// Why a dependency could not be resolved.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveError {
    #[error("{0} is not in the registry")]
    NotInIndex(String),
    #[error("no version of {id} satisfies {requirement}: {reason}")]
    NoVersion {
        id: String,
        requirement: VersionReq,
        reason: String,
    },
}

/// Resolves every dependency, in id order.
pub fn resolve(input: &ResolveInput) -> Result<Resolution, ResolveError> {
    let mut targets = input.targets.to_vec();
    targets.sort();
    targets.dedup();

    let mut plugins = BTreeMap::new();
    let mut warnings = Vec::new();
    for (id, requirement) in input.dependencies {
        let index = input
            .indexes
            .get(id)
            .ok_or_else(|| ResolveError::NotInIndex(id.clone()))?;
        let (locked, version) = match keep(input, id, requirement, index, &mut warnings) {
            Some(kept) => kept,
            None => choose(input, id, requirement, index, &targets, &mut warnings)?,
        };
        if let Some(entry) = version {
            let deprecation = entry.deprecated.as_ref().or(index.deprecated.as_ref());
            if let Some(d) = deprecation {
                warnings.push(ResolveWarning::Deprecated {
                    id: id.clone(),
                    version: entry.version.clone(),
                    message: d.message.clone(),
                    replacement: d.replacement.clone(),
                });
            }
        }
        plugins.insert(id.clone(), locked);
    }

    let other = input
        .previous
        .map(|previous| previous.other.clone())
        .unwrap_or_default();
    Ok(Resolution {
        lock: Lockfile {
            celesta: input.celesta.clone(),
            targets,
            plugins,
            other,
        },
        warnings,
    })
}

type Chosen<'a> = (LockedPlugin, Option<&'a VersionEntry>);

/// The previous entry, when nothing asks for it to move and it still holds.
fn keep<'a>(
    input: &ResolveInput,
    id: &str,
    requirement: &VersionReq,
    index: &'a Index,
    warnings: &mut Vec<ResolveWarning>,
) -> Option<Chosen<'a>> {
    if input.update.names(id) {
        return None;
    }
    let locked = input.previous?.plugins.get(id)?;
    if !requirement.matches(&locked.version)
        || !locked.celesta.matches(input.celesta)
        || blocked(input, id, &locked.version)
    {
        return None;
    }
    let entry = index.versions.iter().find(|v| v.version == locked.version);
    if let Some(entry) = entry {
        if entry.withdrawn {
            return None;
        }
        if entry.yanked {
            warnings.push(ResolveWarning::LockedYanked {
                id: id.to_owned(),
                version: entry.version.clone(),
            });
        }
    }
    Some((locked.clone(), entry))
}

fn blocked(input: &ResolveInput, id: &str, version: &Version) -> bool {
    input
        .advisories
        .and_then(|list| list.blocking(id, version))
        .is_some()
}

/// Picks the best version from the index.
fn choose<'a>(
    input: &ResolveInput,
    id: &str,
    requirement: &VersionReq,
    index: &'a Index,
    targets: &[Target],
    warnings: &mut Vec<ResolveWarning>,
) -> Result<Chosen<'a>, ResolveError> {
    let mut candidates: Vec<&VersionEntry> = index
        .versions
        .iter()
        .filter(|v| {
            requirement.matches(&v.version)
                && !v.yanked
                && !v.withdrawn
                && !blocked(input, id, &v.version)
                && v.celesta.matches(input.celesta)
        })
        .collect();
    candidates.sort_by(|a, b| b.version.cmp(&a.version));

    let covers = |v: &VersionEntry, target: &Target| {
        v.targets.contains_key(target) || v.targets.contains_key(&Target::ANY)
    };
    let media = index.kind.roles.iter().any(|role| role == "media");

    let mut missing = Vec::new();
    let picked = if media {
        candidates.first().copied()
    } else if let Some(full) = candidates
        .iter()
        .find(|v| targets.iter().all(|t| covers(v, t)))
    {
        Some(*full)
    } else if let Some(partial) = candidates.iter().find(|v| covers(v, input.host)) {
        missing = targets
            .iter()
            .filter(|t| !covers(partial, t))
            .cloned()
            .collect();
        Some(*partial)
    } else {
        None
    };

    let Some(entry) = picked else {
        return Err(ResolveError::NoVersion {
            id: id.to_owned(),
            requirement: requirement.clone(),
            reason: refusal(input, id, requirement, index),
        });
    };
    if !missing.is_empty() {
        warnings.push(ResolveWarning::MissingTargets {
            id: id.to_owned(),
            version: entry.version.clone(),
            targets: missing,
        });
    }
    let locked = LockedPlugin {
        celesta: entry.celesta.clone(),
        registry: input.registry.to_owned(),
        targets: entry
            .targets
            .iter()
            .map(|(target, artifact)| {
                (
                    target.clone(),
                    LockedArtifact {
                        sha256: artifact.sha256,
                        tree_hash: artifact.tree_hash,
                    },
                )
            })
            .collect(),
        version: entry.version.clone(),
        other: Default::default(),
    };
    Ok((locked, Some(entry)))
}

/// Why the newest version matching the requirement was refused.
fn refusal(input: &ResolveInput, id: &str, requirement: &VersionReq, index: &Index) -> String {
    let newest = index
        .versions
        .iter()
        .filter(|v| requirement.matches(&v.version))
        .max_by(|a, b| a.version.cmp(&b.version));
    let Some(v) = newest else {
        return "no version matches".to_owned();
    };
    if v.yanked {
        "yanked".to_owned()
    } else if v.withdrawn {
        "withdrawn".to_owned()
    } else if let Some(advisory) = input.advisories.and_then(|l| l.blocking(id, &v.version)) {
        format!("blocked by {}", advisory.id)
    } else if !v.celesta.matches(input.celesta) {
        format!("works with Celesta {}, not {}", v.celesta, input.celesta)
    } else {
        format!("no build for {}", input.host.as_str())
    }
}
