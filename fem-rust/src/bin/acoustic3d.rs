#[path = "../main.rs"]
mod fullmeta;

pub(crate) use fullmeta::{
    C, MU0, SolverCache, TET_Q, TRI_Q, acoustic2d, acoustic3d, c_add, c_mul, c_scale, c_sqrt,
    field_output, integration, json_schema, material, mesh2d, model_expression, model_schema,
    preflight_client, scalar2d, solve_linear_system, solver_common, solver_request,
    validation_receipt,
};

fn main() -> std::process::ExitCode {
    match fullmeta::run_native() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("acoustic3d: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
