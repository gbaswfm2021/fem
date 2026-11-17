use fullmeta_operation_contracts_sdk::{Contracts, Error};
use serde_json::json;

fn registry() -> Contracts {
    let path = std::env::var_os("FULLMETA_CONTRACTS_DLL").map(std::path::PathBuf::from).unwrap_or_else(||
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../platform/operation-contracts-rust/lib/fullmeta_operation_contracts.dll"));
    Contracts::open(path, include_bytes!("../../native-operations.json")).unwrap()
}

#[test]
fn fem_contract_identity_validation_and_opaque_models() {
    let registry = registry();
    let identity = registry.resolve_binding(&json!({"id":"mesh.check","version":1})).unwrap();
    assert_eq!(identity.digest, registry.contract_digest().unwrap());
    assert!(registry.resolve_binding(&json!({"id":"mesh.check","version":1,"digest":"stale"})).is_err());
    assert!(registry.resolve_binding(&json!({"id":"mesh.check","version":2})).is_err());
    let value = json!({"model":{"arbitrary":{"x-future":[false,null,{"ordered":[2,1]}]}},"componentId":"component"});
    let before = value.clone();
    registry.validate_input("mesh.check", &value).unwrap();
    assert_eq!(value, before);
    let Error::Validation(issue) = registry.validate_input("mesh.check", &json!({"model":null})).unwrap_err() else { panic!("expected validation diagnostics") };
    assert_eq!(issue[0].path.as_deref(), Some("/model"));
    assert!(registry.validate_input("mesh.check", &json!({"model":{},"unexpected":true})).is_err());
    assert!(registry.validate_result("mesh.check", &json!({"ok":true})).is_err());
    assert!(registry.validate_event("fem.studyRun", &json!({"type":"complete","state":"success"})).is_err());
    registry.validate_event("fem.studyRun", &json!({"type":"error","state":"error","message":"Stopped before dispatch"})).unwrap();
}
