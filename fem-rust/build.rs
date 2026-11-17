use serde_json::Value;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

fn safe_module_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && value.len() <= 64
}

fn read_json(path: &Path) -> Value {
    let bytes =
        fs::read(path).unwrap_or_else(|error| panic!("Could not read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("Invalid JSON in {}: {error}", path.display()))
}

fn rust_string(value: &str) -> String {
    serde_json::to_string(value).expect("Rust-compatible JSON string")
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let local_operations = manifest.join("native-operations.json");
    let operations = if local_operations.is_file() { local_operations } else { manifest.join("..").join("native-operations.json") };
    println!("cargo:rerun-if-changed={}", operations.display());
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::copy(&operations, output.join("native-operations.json"))
        .unwrap_or_else(|error| panic!("Could not copy {}: {error}", operations.display()));
    let scenario_root = manifest.join("src").join("scenarios");
    let registry_path = scenario_root.join("registry.json");
    println!("cargo:rerun-if-changed={}", registry_path.display());

    let registry = read_json(&registry_path);
    let packages = registry
        .get("packages")
        .and_then(Value::as_array)
        .expect("FEM scenario registry packages must be an array");
    let mut modules = BTreeMap::<String, PathBuf>::new();
    for item in packages {
        let package_id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| {
                let mut bytes = id.bytes();
                matches!(bytes.next(), Some(b'a'..=b'z'))
                    && bytes.all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && id.len() <= 64
            })
            .expect("Scenario registry package requires a safe id");
        let filename = item
            .get("file")
            .and_then(Value::as_str)
            .filter(|filename| {
                !filename.is_empty()
                    && !filename.contains(['/', '\\', ':'])
                    && *filename != "."
                    && *filename != ".."
            })
            .expect("Scenario registry package file must be a safe sibling filename");
        if filename != format!("{}.json", package_id.replace('-', "")) {
            panic!("Scenario package {package_id:?} must use its ID without hyphens as the JSON filename");
        }
        let package_path = scenario_root.join(filename);
        println!("cargo:rerun-if-changed={}", package_path.display());
        let package = read_json(&package_path);
        if package.get("contractVersion").and_then(Value::as_str) != Some("fullmeta-fem-scenario/1")
            || package.get("id").and_then(Value::as_str) != Some(package_id)
        {
            panic!("Scenario package {filename} has an unsupported contract version or ID");
        }
        let Some(module) = package
            .pointer("/schemaDocument/x-fem-scenario/solverModule")
            .and_then(Value::as_str)
        else {
            continue;
        };
        if !safe_module_name(module) {
            panic!("Scenario solverModule {module:?} is not a safe Rust module name");
        }
        if filename != format!("{module}.json") {
            panic!("Scenario package {filename} must use the same filename stem as solverModule {module:?}");
        }
        let source = manifest
            .join("src/scenarios")
            .join(format!("{module}.rs"));
        println!("cargo:rerun-if-changed={}", source.display());
        if !source.is_file() {
            panic!(
                "Scenario package {} declares solverModule {module:?}, but {} does not exist",
                filename,
                source.display()
            );
        }
        if modules.insert(module.to_string(), source).is_some() {
            panic!("More than one scenario package declares solverModule {module:?}");
        }
    }

    let mut generated = String::new();
    for (module, source) in &modules {
        let rust_module = format!("scenario_runner_{module}");
        generated.push_str(&format!(
            "#[path = {}]\npub(crate) mod {rust_module};\n#[allow(unused_imports)]\npub(crate) use {rust_module} as {module};\n",
            rust_string(&source.to_string_lossy())
        ));
    }
    generated.push_str("pub(crate) fn scenario_runner_contains(module: &str) -> bool {\n");
    if modules.is_empty() {
        generated.push_str("    let _ = module;\n    false\n");
    } else {
        generated.push_str("    matches!(module, ");
        for (index, module) in modules.keys().enumerate() {
            if index > 0 {
                generated.push_str(" | ");
            }
            generated.push_str(&rust_string(module));
        }
        generated.push_str(")\n");
    }
    generated.push_str("}\n\n");
    generated.push_str(
        "pub(crate) fn run_scenario_runner(module: &str, arguments: &[String]) -> std::result::Result<i32, String> {\n    let _ = arguments;\n    match module {\n",
    );
    for module in modules.keys() {
        generated.push_str(&format!(
            "        {} => scenario_runner_{module}::run(arguments),\n",
            rust_string(module)
        ));
    }
    generated.push_str(
        "        _ => Err(format!(\"Scenario runner module {module:?} is not registered in this build.\")),\n    }\n}\n",
    );

    fs::write(output.join("scenario_runners.rs"), generated)
        .expect("Could not write generated scenario runner registry");
}
