//! Machine-readable reports for plugin commands.

use std::fmt::Display;
use std::process::ExitCode;

use celesta_plugin_host::LockProblem;
use celesta_registry_client::{ClientError, ProjectError};
use semver::Version;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{USAGE, report};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    Project,
    Global,
}

#[derive(Clone, Copy)]
pub(crate) struct Output {
    pub json: bool,
    pub command: &'static str,
    pub scope: Option<Scope>,
}

impl Output {
    pub fn write(self, plugins: impl Serialize, details: Value, error: Option<&Error>) -> ExitCode {
        let mut report = json!({
            "schemaVersion": 1,
            "command": self.command,
            "status": if error.is_some() { "error" } else { "ok" },
            "plugins": plugins,
        });
        let object = report.as_object_mut().expect("report is an object");
        if let Value::Object(details) = details {
            object.extend(details);
        }
        if let Some(scope) = self.scope {
            object.insert("scope".to_owned(), json!(scope));
        }
        if let Some(error) = error {
            object.insert("error".to_owned(), json!(error));
        }
        println!("{report}");
        error.map_or(ExitCode::SUCCESS, Error::exit)
    }

    pub fn fail(self, error: Error) -> ExitCode {
        self.fail_with_details(error, Value::Null)
    }

    pub fn fail_with_details(self, error: Error, details: Value) -> ExitCode {
        if self.json {
            self.write(&error.plugin, details, Some(&error))
        } else {
            report::error(&error.message);
            error.exit()
        }
    }
}

/// A clap usage error occurs before a command can be selected.
pub fn usage_error(message: &str) -> ExitCode {
    println!(
        "{}",
        json!({
            "schemaVersion": 1,
            "status": "error",
            "plugins": [],
            "error": Error::usage("invalid_arguments", message),
        })
    );
    ExitCode::from(USAGE)
}

#[derive(Serialize)]
pub(crate) struct Error {
    code: &'static str,
    message: String,
    #[serde(skip)]
    usage: bool,
    #[serde(skip)]
    plugin: Vec<PluginError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginError {
    scope: Scope,
    id: String,
    version: Version,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    advisory_id: Option<String>,
}

impl Error {
    pub fn new(code: &'static str, message: impl Display) -> Self {
        Self {
            code,
            message: message.to_string(),
            usage: false,
            plugin: Vec::new(),
        }
    }

    pub fn usage(code: &'static str, message: impl Display) -> Self {
        Self {
            usage: true,
            ..Self::new(code, message)
        }
    }

    pub fn project(error: ProjectError, scope: Scope) -> Self {
        let code = match &error {
            ProjectError::Config(_) | ProjectError::LockCheck(LockProblem::Config(_)) => {
                "project_config"
            }
            ProjectError::Lock(_) | ProjectError::LockCheck(LockProblem::Unreadable(_)) => {
                "lockfile"
            }
            ProjectError::LockCheck(LockProblem::NoLock) => "lock_missing",
            ProjectError::LockCheck(LockProblem::OutOfDate { .. }) => "lock_out_of_date",
            ProjectError::LockCheck(LockProblem::OtherCelesta { .. }) => "lock_incompatible",
            ProjectError::LockCheck(LockProblem::Unsatisfied { .. }) => "lock_unsatisfied",
            ProjectError::Client(ClientError::NoSuchPlugin(_)) => "plugin_not_found",
            ProjectError::Client(_) => "registry",
            ProjectError::Resolve(_) => "resolution",
            ProjectError::Install { .. } => "install_failed",
            ProjectError::Missing { .. } => "plugin_missing",
            ProjectError::Withdrawn { .. } => "plugin_withdrawn",
            ProjectError::Blocked { .. } => "plugin_blocked",
            ProjectError::LockMismatch { .. } => "lock_hash_mismatch",
            ProjectError::Settings(_) => "settings",
        };
        let mut result = Self::new(code, &error);
        let (id, version, status, advisory_id) = match error {
            ProjectError::Install { id, version, .. }
            | ProjectError::LockMismatch { id, version } => (id, version, "error", None),
            ProjectError::Missing { id, version } => (id, version, "missing", None),
            ProjectError::Withdrawn { id, version } => (id, version, "withdrawn", None),
            ProjectError::Blocked {
                id,
                version,
                advisory,
            } => (id, version, "blocked", Some(advisory)),
            _ => return result,
        };
        result.plugin.push(PluginError {
            scope,
            id,
            version,
            status,
            advisory_id,
        });
        result
    }

    fn exit(&self) -> ExitCode {
        if self.usage {
            ExitCode::from(USAGE)
        } else {
            ExitCode::FAILURE
        }
    }
}
