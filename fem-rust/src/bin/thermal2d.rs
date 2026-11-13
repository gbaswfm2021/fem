//! Entry point for the stationary thermal 2D solver.
//!
//! This source connects the numerical kernel, common preflight adapter and
//! stationary project runner.

#[path = "../mesh2d.rs"]
pub(crate) mod mesh2d;
#[path = "../model_expression.rs"]
pub(crate) mod model_expression;
#[path = "../model_schema.rs"]
pub(crate) mod model_schema;
#[path = "../preflight_client.rs"]
pub(crate) mod preflight_client;
#[path = "../scalar2d.rs"]
pub(crate) mod scalar2d;
#[path = "../solver_common.rs"]
pub(crate) mod solver_common;
#[path = "../solver_options.rs"]
pub(crate) mod solver_options;
#[path = "../thermal2d.rs"]
pub(crate) mod thermal2d;
#[path = "../thermal2d_project.rs"]
pub(crate) mod thermal2d_project;
#[path = "../validation_receipt.rs"]
pub(crate) mod validation_receipt;

pub(crate) use solver_common::{C, c_add};

use serde_json::Value;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run_native() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("thermal2d: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_native() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => {
            print_usage();
            Err("A validation or solve command is required.".into())
        }
        [option] if option == "--help" || option == "-h" => {
            print_usage();
            Ok(())
        }
        [option, request] if option == "--solve" => {
            crate::model_schema::ensure_scenario_metadata()?;
            thermal2d::run_request(Path::new(request))
        }
        [option] if option == "--prepare-study" => {
            let input = read_stdin_json()?;
            let plan = thermal2d::prepare_study(&input)?;
            println!("{}", serde_json::to_string(&plan)?);
            Ok(())
        }
        [option] if option == "--server" => thermal2d_project::run_server(&read_stdin_json()?),
        [option, model] if option == "--validate" => validate_file(model),
        [project, option] if option == "--validate" => {
            let model = project_model_path(project)?;
            validate_file(
                model
                    .to_str()
                    .ok_or("The project model path is not valid UTF-8.")?,
            )
        }
        [project, rest @ ..] if !project.starts_with('-') => {
            let mut study_id = None;
            let mut csv = None;
            let mut index = 0;
            while index < rest.len() {
                let target = match rest[index].as_str() {
                    "--study" if study_id.is_none() => &mut study_id,
                    "--csv" if csv.is_none() => &mut csv,
                    option => {
                        return Err(format!(
                            "Unknown or duplicate option {option:?}; use --help for usage."
                        )
                        .into());
                    }
                };
                let value = rest.get(index + 1).ok_or("The option requires a value.")?;
                *target = Some(value.as_str());
                index += 2;
            }
            thermal2d_project::run_saved_project(project, study_id, csv)
        }
        _ => Err("Unknown thermal solver command; use --help for usage.".into()),
    }
}

fn print_usage() {
    println!(
        "Usage: thermal2d <project-folder> [--study <id>] [--csv <filename>]\n\
         \x20      thermal2d <project-folder> --validate\n\
         \x20      thermal2d --validate <model.json | ->\n\
         \x20      thermal2d --solve <request.json>\n\
         \x20      thermal2d --prepare-study | --server\n\
         Stationary 2D heat conduction with temperature formulation T.\n\
         Material and boundary quantities accept constant parameter expressions with units."
    );
}

fn read_stdin_json() -> Result<Value, Box<dyn Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(serde_json::from_str(&input)?)
}

fn validate_file(filename: &str) -> Result<(), Box<dyn Error>> {
    let bytes = if filename == "-" {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        bytes
    } else {
        fs::read(filename)?
    };
    let model: Value = serde_json::from_slice(&bytes)?;
    if filename == "-" {
        let report = preflight_client::validation_report(&model)?;
        let valid = report.get("valid").and_then(Value::as_bool) == Some(true);
        println!("{report}");
        return if valid {
            Ok(())
        } else {
            Err("Model validation failed.".into())
        };
    }
    let file = if Path::new(filename).is_absolute() {
        PathBuf::from(filename)
    } else {
        env::current_dir()?.join(filename)
    };
    model_schema::select_schema_for_model_file(&model, &file)?;
    thermal2d::validate_model(&model)?;
    println!("Model validation successful.");
    Ok(())
}

fn project_model_path(project: &str) -> Result<PathBuf, Box<dyn Error>> {
    let normalized = project.replace('\\', "/");
    let folder = normalized.strip_prefix("projects/").unwrap_or(&normalized);
    if folder.is_empty()
        || folder.len() > 64
        || !folder.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphanumeric()
            } else {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            }
        })
    {
        return Err("The project must be one folder name directly inside projects/.".into());
    }
    let root = fullmeta_model::runtime_paths::installation_root()?;
    let mut path = root.to_path_buf();
    for (index, part) in ["projects", folder, "fem_model.json"].iter().enumerate() {
        path.push(part);
        let metadata = fs::symlink_metadata(&path)?;
        if is_link(&metadata)
            || (index < 2 && !metadata.is_dir())
            || (index == 2 && !metadata.is_file())
        {
            return Err("Project model paths must use real directories and a regular file, without symbolic links or reparse points.".into());
        }
    }
    Ok(path)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
