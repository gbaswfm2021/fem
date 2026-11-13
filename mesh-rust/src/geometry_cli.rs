use crate::sha256;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const ARTIFACT_VERSION: usize = 14;

struct Options {
    command: String,
    input: String,
    output: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    component: Option<String>,
    overwrite: bool,
}

pub fn execute(args: Vec<String>) -> Result<String, String> {
    execute_with_input(args, None)
}

/// Transport adapters supply the same model JSON without replacing compiler semantics.
pub fn execute_with_input(
    args: Vec<String>,
    supplied_input: Option<&str>,
) -> Result<String, String> {
    let options = parse_args(args)?;
    let source = if options.input == "-" {
        match supplied_input {
            Some(value) => value.to_string(),
            None => {
                let mut value = String::new();
                io::stdin()
                    .read_to_string(&mut value)
                    .map_err(|e| format!("Could not read model JSON from stdin: {e}"))?;
                value
            }
        }
    } else {
        fs::read_to_string(&options.input)
            .map_err(|e| format!("Could not read model JSON {}: {e}", options.input))?
    };
    if options.command == "check" {
        let result = check_source(
            &source,
            if options.input == "-" {
                None
            } else {
                Some(Path::new(&options.input))
            },
            options.component.as_deref(),
        )?;
        return serde_json::to_string(&result)
            .map_err(|e| format!("Could not format compiler check result: {e}"));
    }
    let model: Value =
        serde_json::from_str(&source).map_err(|e| format!("Invalid model JSON: {e}"))?;
    crate::schema_metadata::select_model_schema(
        &model,
        if options.input == "-" {
            None
        } else {
            Some(Path::new(&options.input))
        },
    )?;
    crate::schema_metadata::engine_capabilities()?;
    let (component_id, geo, map_data) = match model.get("modelFormat").and_then(Value::as_str) {
        Some("fem-model/2") => {
            let compiled = crate::geometry_v2::compile(&model, options.component.as_deref())?;
            (compiled.component_id, compiled.geo, compiled.map_data)
        }
        Some(model_format) => {
            return Err(format!(
                "The geometry compiler does not support model format {model_format:?}."
            ));
        }
        None => return Err("The model must declare its modelFormat.".to_string()),
    };
    let map_base = serde_json::to_string(&map_data)
        .map_err(|e| format!("Could not serialize mesh map: {e}"))?;
    let mut hash_source = geo.as_bytes().to_vec();
    hash_source.push(0);
    hash_source.extend_from_slice(map_base.as_bytes());
    let geometry_hash = sha256::hex_digest(&hash_source);
    // Keep names short enough for the bundled Windows mesher. The map and
    // cache retain the full digest for content and reuse checks.
    let stem = format!("model-{}", &geometry_hash[..20]);
    let output_dir = if let Some(directory) = options.output_dir {
        directory
    } else if let Some(output) = options.output.as_ref() {
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    } else if options.input == "-" {
        std::env::current_dir().map_err(|e| e.to_string())?
    } else {
        Path::new(&options.input)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    };
    let geo_path = options
        .output
        .unwrap_or_else(|| output_dir.join(format!("{stem}.geo")));
    if geo_path
        .extension()
        .and_then(|x| x.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("geo"))
    {
        return Err("The geometry output filename must end in .geo.".to_string());
    }
    let geo_name = geo_path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or("The geometry output filename is invalid.")?
        .to_string();
    let geo_stem = geo_path
        .file_stem()
        .and_then(|x| x.to_str())
        .ok_or("The geometry output filename is invalid.")?;
    let map_name = format!("{geo_stem}-model-map.json");
    let map_path = geo_path.parent().unwrap_or(Path::new(".")).join(&map_name);
    fs::create_dir_all(geo_path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("Could not create geometry output directory: {e}"))?;
    let mut map = map_data;
    map["geometryHash"] = Value::String(geometry_hash.clone());
    map["geometrySource"] = Value::String(geo_name.clone());
    let map_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&map)
            .map_err(|e| format!("Could not format model map: {e}"))?
    );
    write_artifact(&geo_path, geo.as_bytes(), options.overwrite)?;
    write_artifact(&map_path, map_text.as_bytes(), options.overwrite)?;
    let model_id = model
        .get("metadata")
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    let output = serde_json::json!({
        "modelId": model_id,
        "componentId": component_id,
        "artifactVersion": ARTIFACT_VERSION,
        "geometryHash": geometry_hash,
        "geoPath": geo_path,
        "mapPath": map_path,
        "geoName": geo_name,
        "mapName": map_name,
        "bytes": geo.len()
    });
    serde_json::to_string(&output).map_err(|e| format!("Could not format compiler result: {e}"))
}

pub fn check_source(
    source: &str,
    model_path: Option<&Path>,
    component_id: Option<&str>,
) -> Result<Value, String> {
    let model: Value =
        serde_json::from_str(source).map_err(|e| format!("Invalid model JSON: {e}"))?;
    crate::schema_metadata::select_model_schema(&model, model_path)?;
    crate::schema_metadata::engine_capabilities()?;
    let compiled = match model.get("modelFormat").and_then(Value::as_str) {
        Some("fem-model/2") => crate::geometry_v2::compile(&model, component_id)?,
        Some(model_format) => {
            return Err(format!(
                "The geometry compiler does not support model format {model_format:?}."
            ));
        }
        None => return Err("The model must declare its modelFormat.".to_string()),
    };
    let map_base = serde_json::to_string(&compiled.map_data)
        .map_err(|e| format!("Could not serialize mesh map: {e}"))?;
    let mut hash_source = compiled.geo.as_bytes().to_vec();
    hash_source.push(0);
    hash_source.extend_from_slice(map_base.as_bytes());
    let geometry_hash = sha256::hex_digest(&hash_source);
    let model_id = model
        .get("metadata")
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    Ok(serde_json::json!({
        "modelId": model_id,
        "componentId": compiled.component_id,
        "artifactVersion": ARTIFACT_VERSION,
        "geometryHash": geometry_hash,
        "bytes": compiled.geo.len()
    }))
}

fn parse_args(arguments: Vec<String>) -> Result<Options, String> {
    let mut args = arguments.into_iter();
    let command = args.next().ok_or_else(usage)?;
    if command == "--help" || command == "-h" {
        return Err(usage());
    }
    if command != "compile" && command != "check" {
        return Err(usage());
    }
    let input = args.next().ok_or_else(usage)?;
    let mut output = None;
    let mut output_dir = None;
    let mut component = None;
    let mut overwrite = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--overwrite" => overwrite = true,
            "--component" | "--component-id" => {
                component = Some(args.next().ok_or("--component needs an ID.")?)
            }
            "--output-dir" => {
                output_dir = Some(PathBuf::from(
                    args.next().ok_or("--output-dir needs a directory.")?,
                ))
            }
            "--help" | "-h" => return Err(usage()),
            _ if arg.starts_with('-') => return Err(format!("Unknown option {arg}.\n{}", usage())),
            _ if output.is_none() => output = Some(PathBuf::from(arg)),
            _ => return Err(format!("Unexpected argument {arg}.\n{}", usage())),
        }
    }
    if output.is_some() && output_dir.is_some() {
        return Err("Choose either an explicit .geo path or --output-dir, not both.".to_string());
    }
    if command == "check" && (output.is_some() || output_dir.is_some() || overwrite) {
        return Err(
            "The check command does not write output files and does not accept output options."
                .to_string(),
        );
    }
    Ok(Options {
        command,
        input,
        output,
        output_dir,
        component,
        overwrite,
    })
}

fn usage() -> String {
    "Usage: fullmeta-mesh compile <fem_model.json|-> [output.geo] [--component <id>] [--output-dir <directory>] [--overwrite]\n       fullmeta-mesh check <fem_model.json|-> [--component <id>]".to_string()
}

fn trim_trailing_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(0, |index| index + 1);
    &bytes[..end]
}

fn write_artifact(path: &Path, contents: &[u8], overwrite: bool) -> Result<(), String> {
    let current = match fs::symlink_metadata(path) {
        Ok(info) => Some(info),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Could not inspect output {}: {error}",
                path.display()
            ));
        }
    };
    if let Some(info) = current {
        if info.file_type().is_symlink() {
            return Err(format!(
                "Refusing to write generated artifact through a symbolic link: {}",
                path.display()
            ));
        }
        let existing = fs::read(path)
            .map_err(|e| format!("Could not read existing artifact {}: {e}", path.display()))?;
        // Generated Gmsh and JSON artifacts can be semantically identical while
        // differing only by a terminal newline from an earlier generator version.
        if existing == contents
            || trim_trailing_ascii_whitespace(&existing) == trim_trailing_ascii_whitespace(contents)
        {
            return Ok(());
        }
        if !overwrite {
            return Err(format!(
                "Generated output already exists with different content: {}. Use --overwrite to replace it.",
                path.display()
            ));
        }
        fs::write(path, contents).map_err(|e| {
            format!(
                "Could not replace generated artifact {}: {e}",
                path.display()
            )
        })?;
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            format!(
                "Could not create generated artifact {}: {e}",
                path.display()
            )
        })?;
    file.write_all(contents)
        .map_err(|e| format!("Could not write generated artifact {}: {e}", path.display()))?;
    file.flush()
        .map_err(|e| format!("Could not flush generated artifact {}: {e}", path.display()))?;
    Ok(())
}
