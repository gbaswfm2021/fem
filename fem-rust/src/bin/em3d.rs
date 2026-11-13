#[path = "../main.rs"]
mod fullmeta;

pub(crate) use fullmeta::{
    C, MU0, SolverCache, TET_Q, TRI_Q, c_add, c_mul, c_scale, c_sqrt, em3d, field_output,
    integration, json_schema, material, model_expression, model_schema, preflight_client,
    solve_linear_system, solver_common, solver_request, validation_receipt,
};

fn main() -> std::process::ExitCode {
    match fullmeta::run_native() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("em3d: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
