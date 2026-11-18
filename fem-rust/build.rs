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

// Direct Cargo builds do not run the Node activation command. Fail rather than
// compile a runner table from a registry which omits a new or changed package.
fn verify_scenario_index(directory: &Path, registry: &Value) {
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    let hint = "Run `node prebuilt-tools/scenario-index.mjs` from fem-rust, then retry.";
    let mut packages: Vec<(String, String, bool, Option<u64>)> = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let mut orders = std::collections::HashSet::new();
    for entry in fs::read_dir(directory).expect("Could not scan FEM scenario packages") {
        let entry = entry.expect("Could not inspect FEM scenario directory entry");
        let name = entry
            .file_name()
            .into_string()
            .expect("Scenario filename must be UTF-8");
        if !name.ends_with(".json") || name == "registry.json" {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("Could not inspect FEM scenario package");
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            panic!("Scenario package {name:?} must be a regular file, not a link. {hint}");
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let package = read_json(&path);
        if package.get("contractVersion").and_then(Value::as_str) != Some("fullmeta-fem-scenario/1")
        {
            panic!("Scenario package {name:?} has an unsupported contract version. {hint}");
        }
        let id = package
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| {
                let mut chars = id.bytes();
                matches!(chars.next(), Some(b'a'..=b'z'))
                    && id.len() <= 64
                    && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            })
            .unwrap_or_else(|| panic!("Scenario package {name:?} has an invalid ID. {hint}"));
        if name != format!("{}.json", id.replace('-', "")) || !ids.insert(id.to_string()) {
            panic!("Scenario package {name:?} has a mismatched filename or duplicate ID. {hint}");
        }
        let discovery = package.get("discovery").filter(|v| !v.is_null());
        if discovery.is_some_and(|v| !v.is_object()) {
            panic!("Scenario package {name:?} discovery must be an object. {hint}");
        }
        let default_value = discovery.and_then(|v| v.get("default"));
        if default_value.is_some_and(|v| !v.is_boolean()) {
            panic!("Scenario package {name:?} discovery.default must be a boolean. {hint}");
        }
        let is_default = default_value.and_then(Value::as_bool).unwrap_or(false);
        let order_value = discovery.and_then(|v| v.get("order"));
        let order = order_value.map(|v| v.as_f64().filter(|order|
                order.is_finite() && *order >= 0.0 && *order <= MAX_SAFE_INTEGER as f64 && order.fract() == 0.0)
            .map(|order| order as u64)
            .unwrap_or_else(|| panic!("Scenario package {name:?} discovery.order must be a non-negative safe integer. {hint}")));
        if order.is_some_and(|value| !orders.insert(value)) {
            panic!("Scenario package {name:?} has a duplicate discovery.order. {hint}");
        }
        packages.push((id.to_string(), name, is_default, order));
    }
    if packages.is_empty() || packages.iter().filter(|p| p.2).count() != 1 {
        panic!("Exactly one scenario must declare discovery.default true. {hint}");
    }
    packages.sort_by(|a, b| {
        a.3.unwrap_or(MAX_SAFE_INTEGER)
            .cmp(&b.3.unwrap_or(MAX_SAFE_INTEGER))
            .then_with(|| a.0.cmp(&b.0))
    });
    let expected = serde_json::json!({
        "format": "fullmeta-fem-scenario-registry/1",
        "packages": packages.iter().map(|(id, file, default, _)|
            serde_json::json!({"id": id, "file": file, "default": default})).collect::<Vec<_>>()
    });
    if *registry != expected {
        panic!("Scenario registry is stale or has an unsupported format. {hint}");
    }
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    generate_scenario_runners(&manifest, &output);
}

pub(crate) fn generate_scenario_runners(manifest: &Path, output: &Path) {
    let local_operations = manifest.join("native-operations.json");
    let operations = if local_operations.is_file() {
        local_operations
    } else {
        manifest.join("..").join("native-operations.json")
    };
    println!("cargo:rerun-if-changed={}", operations.display());
    fs::copy(&operations, output.join("native-operations.json"))
        .unwrap_or_else(|error| panic!("Could not copy {}: {error}", operations.display()));
    let scenario_root = manifest.join("src").join("scenarios");
    let registry_path = scenario_root.join("registry.json");
    // Monitoring the directory also detects added or removed package files.
    println!("cargo:rerun-if-changed={}", scenario_root.display());
    println!("cargo:rerun-if-changed={}", registry_path.display());

    let registry_metadata =
        fs::symlink_metadata(&registry_path).expect("Could not inspect FEM scenario registry");
    if registry_metadata.file_type().is_symlink() || !registry_metadata.is_file() {
        panic!("FEM scenario registry must be a regular file, not a link.");
    }
    let registry = read_json(&registry_path);
    verify_scenario_index(&scenario_root, &registry);
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
        let metadata = package.pointer("/schemaDocument/x-fem-scenario");
        if let Some(execution) = metadata.and_then(|value| value.get("execution")) {
            let kind = execution
                .as_object()
                .and_then(|value| value.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    panic!("Scenario package {filename} execution.kind must be a supported string")
                });
            if !matches!(kind, "scalarSimplexP1" | "vectorSimplexP1" | "orientedTetrahedronNedelec") {
                panic!("Scenario package {filename} declares unsupported execution kind {kind:?}");
            }
            if metadata
                .and_then(|value| value.get("solverModule"))
                .is_some()
            {
                panic!(
                    "Scenario package {filename} cannot declare both execution and solverModule"
                );
            }
            continue;
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
        let source = manifest.join("src/scenarios").join(format!("{module}.rs"));
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
