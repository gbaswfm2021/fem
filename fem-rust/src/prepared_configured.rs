//! Normalize a configured single-frequency request without a model-file shim.
//!
//! The prepared configuration and its selection arrays remain intact. This
//! layer validates the common envelope, SI parameter dimensions and map
//! identity; element, boundary, integration and output adapters must still
//! validate their own declared capabilities before numerical execution.
use std::{collections::HashMap, fs, path::{Path,PathBuf}};
use serde_json::{Value,json};
use crate::model_expression::Scalar;

pub(crate) struct ConfiguredCase {
    pub(crate) request: Value,
    pub(crate) profile: String,
    pub(crate) study_id: String,
    pub(crate) formulation: String,
    pub(crate) element_order: u64,
    pub(crate) quadrature: String,
    pub(crate) backend: String,
    pub(crate) ordering: String,
    pub(crate) threads: usize,
    pub(crate) frequency_hz: f64,
    /// Original case, including overrides and any future case metadata.
    pub(crate) frequency_case: Value,
    /// Values in SI, with declared dimensions (missing dimensions default to
    /// dimensionless as in the existing configured-request adapter).
    pub(crate) parameters: HashMap<String, Scalar>,
    pub(crate) configuration: Value,
    pub(crate) mesh_map: Value,
    pub(crate) mesh_path: PathBuf,
    pub(crate) mesh_map_path: PathBuf,
    pub(crate) output_directory: PathBuf,
    pub(crate) sweep_csv: String,
    pub(crate) debug_csv: String,
    pub(crate) field_prefix: String,
    pub(crate) field_files: bool,
}

fn required<'a>(object:&'a Value,key:&str,path:&str) -> Result<&'a str,String> {
    object.get(key).and_then(Value::as_str).filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("{path}.{key} must be a nonempty string."))
}
fn resolve(base:&Path,value:&str) -> PathBuf {
    let path = PathBuf::from(value); if path.is_absolute() { path } else { base.join(path) }
}

impl ConfiguredCase {
    /// Read the request and its mesh map, but do not create outputs, load a
    /// project model or synthesize a model file. `package` determines the
    /// profile's declared preparation and mesh dimension.
    pub(crate) fn load(path:&Path,package:&Value,expected_profile:&str) -> Result<Self,String> {
        let request:Value = serde_json::from_slice(&fs::read(path).map_err(|e| format!("Could not read configured request: {e}"))?)
            .map_err(|e| format!("Could not parse configured request: {e}"))?;
        let base = path.parent().unwrap_or(Path::new("."));
        Self::validate_envelope(&request,package,expected_profile)?;
        let map_path = resolve(base,required(&request,"meshMap","request")?);
        let mesh_map:Value = serde_json::from_slice(&fs::read(&map_path).map_err(|e| format!("Could not read mesh map: {e}"))?)
            .map_err(|e| format!("Could not parse mesh map: {e}"))?;
        Self::from_parts(request,package,expected_profile,base,mesh_map)
    }

    fn validate_envelope(request:&Value,package:&Value,profile:&str) -> Result<(),String> {
        if package.get("id").and_then(Value::as_str) != Some(profile) { return Err("Configured request and scenario package identities differ.".into()); }
        if package.pointer("/schemaDocument/x-fem-capabilities/study/preparationByProfile").and_then(|p| p.get(profile)).and_then(|p| p.get("kind")).and_then(Value::as_str) != Some("configuredFrequencyRequest") {
            return Err("Scenario does not declare configuredFrequencyRequest preparation.".into());
        }
        crate::solver_request::validate_single_case_request(request,profile)?;
        crate::solver_request::single_case_frequency(request)?;
        Ok(())
    }

    pub(crate) fn from_parts(request:Value,package:&Value,profile:&str,base:&Path,mesh_map:Value) -> Result<Self,String> {
        Self::validate_envelope(&request,package,profile)?;
        let configuration = request.get("modelConfiguration").filter(|value| value.is_object()).ok_or("request.modelConfiguration must be an object.")?.clone();
        if required(&configuration,"scenarioProfileId","modelConfiguration")? != profile { return Err("modelConfiguration.scenarioProfileId does not match the selected profile.".into()); }
        let capability = crate::model_schema::engine_capabilities()?;
        let declared_schema = capability.pointer("/study/requestConfigurationSchema").and_then(Value::as_str).ok_or("Configured request schema is not declared.")?;
        if declared_schema != "fem-solver-configuration/6" { return Err(format!("Unsupported configured-request capability schema {declared_schema:?}.")); }
        if required(&configuration,"schema","modelConfiguration")? != declared_schema { return Err("modelConfiguration.schema does not match the supported configuration contract.".into()); }
        let dimension = package.pointer("/schemaDocument/x-fem-scenario/profile/dimension").and_then(Value::as_u64)
            .filter(|value| matches!(*value,2 | 3)).ok_or("Scenario must declare a supported spatial dimension.")?;
        if mesh_map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6") || mesh_map.get("dimension").and_then(Value::as_u64) != Some(dimension) {
            return Err("Mesh map schema or dimension does not match the configured scenario.".into());
        }
        let frequency_hz = crate::solver_request::single_case_frequency(&request)?;
        let frequency_case = request["frequencyCases"][0].clone();
        let parameter_rows = frequency_case.get("globalParameters").cloned().unwrap_or_else(|| json!({}));
        let parameter_rows = parameter_rows.as_object().ok_or("frequencyCases[0].globalParameters must be an object.")?;
        let dimensions = frequency_case.get("globalParameterDimensions").and_then(Value::as_object);
        let mut parameters = HashMap::new();
        for (symbol,value) in parameter_rows {
            let value = value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| format!("Global parameter {symbol:?} must be finite and numeric."))?;
            let dimension = match dimensions.and_then(|d| d.get(symbol)) {
                Some(value) => fullmeta_core::quantity::dimension_from_json(value).ok_or_else(|| format!("Global parameter {symbol:?} dimension must contain four or five finite exponents."))?,
                None => fullmeta_core::quantity::ZERO_DIMENSION,
            };
            parameters.insert(symbol.clone(),Scalar { value,dimension });
        }
        let study_id = required(&request,"studyId","request")?.to_owned();
        let formulation = required(&request,"fieldFormulation","request")?.to_owned();
        let quadrature = required(&request,"constitutiveQuadrature","request")?.to_owned();
        let backend = required(&request,"solverBackend","request")?.to_owned();
        let ordering = required(&request,"solverOrdering","request")?.to_owned();
        let element_order = request.get("elementOrder").and_then(Value::as_u64).ok_or("request.elementOrder must be an unsigned integer.")?;
        let threads = request.get("solverThreads").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).filter(|v| *v>0).ok_or("request.solverThreads must be positive.")?;
        let field_files = configuration.get("fieldFiles").and_then(Value::as_bool).ok_or("modelConfiguration.fieldFiles must be boolean.")?;
        let mesh_path = resolve(base,required(&request,"meshFile","request")?);
        let mesh_map_path = resolve(base,required(&request,"meshMap","request")?);
        let output_directory = resolve(base,required(&request,"outputDirectory","request")?);
        let count_only = request.get("countOnly").and_then(Value::as_bool) == Some(true);
        let output_name = |key| -> Result<String,String> {
            if count_only && request.get(key).is_none() { Ok(String::new()) }
            else { Ok(required(&request,key,"request")?.to_owned()) }
        };
        let sweep_csv = output_name("sweepCsv")?;
        let debug_csv = output_name("debugCsv")?;
        if !count_only && sweep_csv == debug_csv { return Err("request.sweepCsv and request.debugCsv must be different.".into()); }
        let field_prefix = output_name("fieldFilePrefix")?;
        Ok(Self { request,profile:profile.into(),study_id,formulation,element_order,quadrature,backend,ordering,threads,
            frequency_hz,frequency_case,parameters,configuration,mesh_map,mesh_path,mesh_map_path,output_directory,
            sweep_csv,debug_csv,field_prefix,field_files })
    }
}


