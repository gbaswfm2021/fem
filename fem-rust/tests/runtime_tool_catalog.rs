use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn mesh_command_uses_the_selected_schema_tool_catalog() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fullmeta-runtime-tool-catalog-{}-{nonce}",
        std::process::id()
    ));
    let schema_path = root.join("applications/fem4/fem4.schema.json");
    let geometry_path = root.join("mesh/model.geo");
    fs::create_dir_all(schema_path.parent().expect("schema parent"))
        .expect("create schema directory");
    fs::create_dir_all(root.join("applications/fem4/tools"))
        .expect("create runtime tool directory");
    fs::create_dir_all(geometry_path.parent().expect("geometry parent"))
        .expect("create geometry directory");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut schema: serde_json::Value = serde_json::from_slice(
        &fs::read(source.join("../fem.schema.json")).expect("read FEM schema"),
    )
    .expect("parse FEM schema");
    schema["x-fullmeta-runtime"]["tools"]["gmsh"]["executable"] =
        "tools/fem4-gmsh.exe".into();
    fs::write(&schema_path, serde_json::to_vec(&schema).unwrap()).expect("write schema fixture");
    let scenarios = schema_path.parent().unwrap().join("fem-rust/src/scenarios");
    fs::create_dir_all(&scenarios).expect("create scenario directory");
    for item in fs::read_dir(source.join("src/scenarios")).expect("read scenario directory") {
        let item = item.expect("read scenario entry");
        if item.path().extension().is_some_and(|ext| ext == "json") {
            fs::copy(item.path(), scenarios.join(item.file_name())).expect("copy scenario contract");
        }
    }
    fs::write(&geometry_path, "Mesh 2;\n").expect("write geometry fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_fullmeta-fem"))
        .arg("mesh")
        .arg(&geometry_path)
        .env("FULLMETA_ROOT", &root)
        .env("FULLMETA_MODEL_SCHEMA_FILE", &schema_path)
        .output()
        .expect("launch mesher process");
    let _ = fs::remove_dir_all(&root);

    assert!(
        !output.status.success(),
        "missing fixture executable should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fem4-gmsh.exe"),
        "expected the selected schema catalog path in error; got: {stderr}"
    );
    assert!(
        !stderr.contains("at gmsh: program not found"),
        "mesh command fell back to PATH instead of the schema catalog: {stderr}"
    );
}
