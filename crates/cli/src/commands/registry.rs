//! The commands that read the registry: `search`, `info`, and `audit`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use celesta_plugin_host::{Lockfile, UserSettings};
use celesta_registry_client::ResolveWarning;
use celesta_registry_format::{AdvisoryList, Index, SearchEntry, VersionEntry};
use semver::Version;

use super::{Failure, finish};
use crate::session::Session;
use crate::{advisories, report};

/// Lists the registry plugins whose id or description contains `text`,
/// ignoring case.
pub fn search(session: &Session, text: &str) -> ExitCode {
    advisories::refresh(session);
    finish(search_registry(session, text))
}

fn search_registry(session: &Session, text: &str) -> Result<(), Failure> {
    let list = session.registry.search(session.now)?;
    let lines = search_lines(&list.plugins, text);
    if lines.is_empty() {
        println!("no plugin matches {text}");
    }
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// `{id}  {latest or "-"}  {description}` for each of `plugins` whose id
/// or description contains `text`, ignoring case, with ` (deprecated)`
/// after a deprecated one.
fn search_lines(plugins: &[SearchEntry], text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    plugins
        .iter()
        .filter(|plugin| {
            plugin.id.to_lowercase().contains(&text)
                || plugin.description.to_lowercase().contains(&text)
        })
        .map(|plugin| {
            let latest = plugin
                .latest
                .as_ref()
                .map_or_else(|| "-".to_owned(), Version::to_string);
            let deprecated = if plugin.deprecated.is_some() {
                " (deprecated)"
            } else {
                ""
            };
            format!(
                "{}  {latest}  {}{deprecated}",
                plugin.id, plugin.description
            )
        })
        .collect()
}

/// Shows `id`'s publisher, kind, and versions with their states, and
/// whether the project in `root` locks it or it is installed globally.
pub fn info(session: &Session, root: &Path, id: &str) -> ExitCode {
    let advisories = advisories::refresh(session);
    finish(show_info(session, root, id, advisories.as_ref()))
}

fn show_info(
    session: &Session,
    root: &Path,
    id: &str,
    advisories: Option<&AdvisoryList>,
) -> Result<(), Failure> {
    let lock = Lockfile::load(root)?;
    let settings = UserSettings::load(&session.settings_path)?;
    let index = session.registry.index(id, session.now)?;
    for line in info_lines(&index, advisories) {
        println!("{line}");
    }
    let locked = lock.as_ref().and_then(|lock| lock.plugins.get(id));
    let global = settings
        .registry_plugins
        .iter()
        .find(|plugin| plugin.id == id);
    if let Some(locked) = locked {
        println!("source: locked in this project: {}", locked.version);
    }
    if let Some(global) = global {
        println!("source: installed globally: {}", global.version);
    }
    if locked.is_none() && global.is_none() {
        println!("source: not locked in this project or installed globally");
    }
    Ok(())
}

/// The header, roles, and one line per version of `index`, newest first:
/// `{version}  celesta {range}  {targets}  {state}`.
fn info_lines(index: &Index, advisories: Option<&AdvisoryList>) -> Vec<String> {
    let roles = if index.kind.roles.is_empty() {
        "-".to_owned()
    } else {
        index.kind.roles.join(", ")
    };
    let mut lines = vec![
        format!("{} by {}: {}", index.id, index.publisher, index.description),
        format!("roles: {roles}"),
    ];
    let mut versions: Vec<&VersionEntry> = index.versions.iter().collect();
    versions.sort_by(|a, b| b.version.cmp(&a.version));
    for entry in versions {
        let targets: Vec<&str> = entry.targets.keys().map(|target| target.as_str()).collect();
        lines.push(format!(
            "{}  celesta {}  {}  {}",
            entry.version,
            entry.celesta,
            targets.join(", "),
            state(index, entry, advisories)
        ));
    }
    lines
}

/// What `entry` of `index` is: `blocked ({advisory})`, `withdrawn`,
/// `yanked`, `deprecated`, or `published`, the first that holds.
fn state(index: &Index, entry: &VersionEntry, advisories: Option<&AdvisoryList>) -> String {
    if let Some(advisory) = advisories.and_then(|list| list.blocking(&index.id, &entry.version)) {
        return format!("blocked ({})", advisory.id);
    }
    let word = if entry.withdrawn {
        "withdrawn"
    } else if entry.yanked {
        "yanked"
    } else if entry.deprecated.is_some() || index.deprecated.is_some() {
        "deprecated"
    } else {
        "published"
    };
    word.to_owned()
}

/// Checks every plugin the project in `root` locks, and every global one,
/// against the advisories and its index. Fails when one is blocked or no
/// longer distributed.
pub fn audit(session: &Session, root: &Path) -> ExitCode {
    let advisories = advisories::refresh(session);
    match audit_plugins(session, root, advisories.as_ref()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(failure) => finish(Err(failure)),
    }
}

/// Prints the findings; returns whether none stops a plugin from running.
fn audit_plugins(
    session: &Session,
    root: &Path,
    advisories: Option<&AdvisoryList>,
) -> Result<bool, Failure> {
    let lock = Lockfile::load(root)?;
    let settings = UserSettings::load(&session.settings_path)?;
    let locked = lock
        .iter()
        .flat_map(|lock| &lock.plugins)
        .map(|(id, locked)| (id.as_str(), &locked.version));
    let global = settings
        .registry_plugins
        .iter()
        .map(|plugin| (plugin.id.as_str(), &plugin.version));
    let mut seen = BTreeSet::new();
    let mut indexes = BTreeMap::new();
    let mut findings = Vec::new();
    let mut passes = true;
    for (id, version) in locked.chain(global) {
        if !seen.insert((id, version)) {
            continue;
        }
        let index = indexes
            .entry(id)
            .or_insert_with(|| audit_index(session, id))
            .as_ref();
        let found = audit_findings(id, version, index, advisories);
        passes &= found.iter().all(|finding| !finding.fails);
        findings.extend(found);
    }
    if findings.is_empty() {
        println!("no problems found");
    }
    for finding in findings {
        println!("{}", finding.text);
    }
    Ok(passes)
}

/// `id`'s index fetched now, else the cached one with a warning; `None`
/// when there is neither.
fn audit_index(session: &Session, id: &str) -> Option<Index> {
    let error = match session.registry.index(id, session.now) {
        Ok(index) => return Some(index),
        Err(error) => error,
    };
    match session.registry.cached_index(id) {
        Some(index) => {
            report::warn(format_args!("using the cached index of {id}: {error}"));
            Some(index)
        }
        None => {
            report::warn(format_args!(
                "could not check {id} for yanked, withdrawn, or deprecated versions: {error}"
            ));
            None
        }
    }
}

/// One line `audit` prints.
#[derive(Debug, PartialEq)]
struct Finding {
    text: String,
    /// Whether it stops the plugin from running.
    fails: bool,
}

/// What is wrong with `version` of `id` by `advisories` and `index`.
fn audit_findings(
    id: &str,
    version: &Version,
    index: Option<&Index>,
    advisories: Option<&AdvisoryList>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(advisory) = advisories.and_then(|list| list.blocking(id, version)) {
        let fixed = advisory
            .fixed
            .as_ref()
            .map(|fixed| format!("; {fixed} fixes it"))
            .unwrap_or_default();
        findings.push(Finding {
            text: format!(
                "{id} {version}: blocked by {}: {}{fixed}",
                advisory.id, advisory.title
            ),
            fails: true,
        });
    }
    let Some(index) = index else {
        return findings;
    };
    let entry = index
        .versions
        .iter()
        .find(|entry| entry.version == *version);
    let Some(entry) = entry.filter(|entry| !entry.withdrawn) else {
        findings.push(Finding {
            text: format!("{id} {version} is no longer distributed"),
            fails: true,
        });
        return findings;
    };
    if entry.yanked {
        findings.push(Finding {
            text: format!("{id} {version} has been yanked"),
            fails: false,
        });
    }
    if let Some(deprecation) = entry.deprecated.as_ref().or(index.deprecated.as_ref()) {
        let warning = ResolveWarning::Deprecated {
            id: id.to_owned(),
            version: version.clone(),
            message: deprecation.message.clone(),
            replacement: deprecation.replacement.clone(),
        };
        findings.push(Finding {
            text: warning.to_string(),
            fails: false,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use celesta_registry_format::Kind;
    use time::OffsetDateTime;

    use super::*;

    fn index(versions: &[&str]) -> Index {
        Index {
            id: "acme.charts".to_owned(),
            generated: OffsetDateTime::UNIX_EPOCH,
            publisher: "acme".to_owned(),
            kind: Kind {
                roles: Vec::new(),
                program: false,
                module: true,
            },
            description: "Charts.".to_owned(),
            homepage: None,
            repository: None,
            deprecated: None,
            versions: versions
                .iter()
                .map(|version| VersionEntry {
                    version: Version::parse(version).unwrap(),
                    celesta: semver::VersionReq::STAR,
                    protocol: 1,
                    published: OffsetDateTime::UNIX_EPOCH,
                    yanked: false,
                    withdrawn: false,
                    deprecated: None,
                    source: None,
                    targets: BTreeMap::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_version_missing_from_its_index_is_no_longer_distributed() {
        let index = index(&["1.0.0"]);
        let findings = audit_findings("acme.charts", &Version::new(2, 0, 0), Some(&index), None);
        assert_eq!(
            findings,
            [Finding {
                text: "acme.charts 2.0.0 is no longer distributed".to_owned(),
                fails: true,
            }]
        );
        assert!(
            audit_findings("acme.charts", &Version::new(1, 0, 0), Some(&index), None).is_empty()
        );
        // Without an index, only the advisories are checked.
        assert!(audit_findings("acme.charts", &Version::new(2, 0, 0), None, None).is_empty());
    }

    #[test]
    fn a_plugin_without_roles_shows_a_dash() {
        let lines = info_lines(&index(&[]), None);
        assert_eq!(lines, ["acme.charts by acme: Charts.", "roles: -"]);
    }
}
