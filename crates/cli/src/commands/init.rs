use std::path::Path;
use std::process::ExitCode;

use celesta_react_bridge::{
    ProjectTsconfig, initialize_project, list_templates, project_templates_dir, runtime_paths,
};

use super::finish;

pub fn initialize(directory: &Path, template: Option<&str>) -> ExitCode {
    let (_, cli_script) = runtime_paths();
    let setup = match initialize_project(&cli_script, directory, template) {
        Ok(setup) => setup,
        Err(error) => return finish(Err(error.into())),
    };
    println!(
        "Initialized Celesta project in {}",
        setup.types.root.display()
    );
    if setup.types.tsconfig == ProjectTsconfig::MissingExtends {
        eprintln!(
            "Add \"extends\": \"./.celesta/tsconfig.json\" to your existing tsconfig.json to enable Celesta types."
        );
    }
    println!("{}", setup.template.edit);
    println!("Run pnpm install for TypeScript or external dependencies.");
    ExitCode::SUCCESS
}

pub fn list() -> ExitCode {
    let (_, cli_script) = runtime_paths();
    let templates = match list_templates(&project_templates_dir(&cli_script)) {
        Ok(templates) => templates,
        Err(error) => return finish(Err(error.into())),
    };
    for template in templates {
        println!(
            "{:<12}{} — {}",
            template.name, template.title, template.description
        );
    }
    ExitCode::SUCCESS
}
