//! FEM adapter for the platform operation-contracts DLL.
use fullmeta_operation_contracts_sdk::{Contracts, Error};
pub use fullmeta_operation_contracts_sdk::{ContractIdentity, Diagnostic};
use serde_json::Value;
use std::{path::PathBuf, sync::OnceLock};

static CONTRACTS: OnceLock<Contracts> = OnceLock::new();
static CONFIGURED_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn configure_library_path(path: PathBuf) -> Result<(), String> {
    if CONTRACTS.get().is_some() { return Err("Operation-contracts DLL is already initialized".into()); }
    CONFIGURED_PATH.set(path).map_err(|_| "Operation-contracts DLL path is already configured".into())
}

fn dll_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("FULLMETA_CONTRACTS_DLL") { return Ok(path.into()); }
    if let Some(path) = CONFIGURED_PATH.get() { return Ok(path.clone()); }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory=executable.parent().ok_or("Cannot find the FEM executable directory")?;
    let library = std::path::Path::new("platform/operation-contracts-rust/lib/fullmeta_operation_contracts.dll");
    if let Some(path) = std::env::var_os("FULLMETA_ROOT").map(PathBuf::from)
        .into_iter().chain(executable.ancestors().map(std::path::Path::to_path_buf))
        .map(|root| root.join(library)).find(|candidate| candidate.is_file()) { return Ok(path); }
    Ok(directory.join("fullmeta_operation_contracts.dll"))
}

pub fn install_default_registry(registry: &[u8]) -> Result<(), String> {
    if let Some(contracts) = CONTRACTS.get() {
        let digest = contracts.digest(registry).map_err(|error| error.to_string())?;
        return if digest == contracts.contract_digest().map_err(|error| error.to_string())? { Ok(()) } else { Err("A different operation registry is already installed.".into()) };
    }
    let contracts = Contracts::open(dll_path()?, registry).map_err(|error| error.to_string())?;
    contracts.install_default_registry().map_err(|error| error.to_string())?;
    let _ = CONTRACTS.set(contracts);
    Ok(())
}

fn contracts() -> &'static Contracts {
    CONTRACTS.get().expect("FEM operation contracts were not initialized")
}
fn diagnostic(error: Error) -> Diagnostic {
    match error {
        Error::Diagnostic(issue) => issue,
        Error::Validation(mut issues) if !issues.is_empty() => issues.remove(0),
        error => Diagnostic::new("OPERATION_CONTRACT_DLL_ERROR", error.to_string(), None, "contract"),
    }
}
fn diagnostics(error: Error) -> Vec<Diagnostic> {
    match error { Error::Validation(issues) => issues, error => vec![diagnostic(error)] }
}
pub fn describe(program: Option<&str>) -> Value { contracts().describe(program).expect("operation contracts DLL describe failed") }
pub fn contract_digest() -> String { contracts().contract_digest().expect("operation contracts DLL digest failed") }
pub fn digest(bytes: &[u8]) -> String { contracts().digest(bytes).expect("operation contracts DLL hash failed") }
pub fn descriptor(id: &str) -> Result<Value, Diagnostic> { contracts().descriptor(id).map_err(diagnostic) }
pub fn resolve_binding(binding: &Value) -> Result<ContractIdentity, Diagnostic> { contracts().resolve_binding(binding).map_err(diagnostic) }
pub fn binding_from_environment(program: &str, entry: &str) -> Result<Option<ContractIdentity>, Diagnostic> { contracts().binding_from_environment(program, entry).map_err(diagnostic) }
pub fn validate_input(id: &str, value: &Value) -> Result<(), Vec<Diagnostic>> { contracts().validate_input(id, value).map_err(diagnostics) }
pub fn validate_result(id: &str, value: &Value) -> Result<(), Vec<Diagnostic>> { contracts().validate_result(id, value).map_err(diagnostics) }
pub fn validate_event(id: &str, value: &Value) -> Result<(), Vec<Diagnostic>> { contracts().validate_event(id, value).map_err(diagnostics) }
pub fn error_response(issues: &[Diagnostic]) -> Value { contracts().error_response(issues).expect("operation contracts DLL error response failed") }
pub fn error_event(issues: &[Diagnostic]) -> Value { contracts().error_event(issues).expect("operation contracts DLL error event failed") }
