//! File and sequence publication for configured oriented-element cases.
//! Numerical preparation and result shape are declared elsewhere; this module
//! does not choose a scenario or route production requests.
use std::{fs, path::{Component,Path,PathBuf}};
use serde_json::{json,Value};
use crate::{SolverCache,prepared_configured::ConfiguredCase,
    result_contract::{CsvVariable,ResultData,ResultPlan}};

struct Case {
    data:ResultData, plan:ResultPlan, debug_plan:ResultPlan, output:PathBuf,
    field_name:String, sweep_name:String, debug_name:String, field_files:bool,
    variable_names:Vec<String>,
}

fn safe_name(value:&str, extension:Option<&str>) -> bool {
    !value.is_empty() && value.len()<=120 && !value.starts_with('.')
        && extension.map_or(true,|suffix|value.ends_with(suffix))
        && value.chars().all(|c|c.is_ascii_alphanumeric() || matches!(c,'.'|'_'|'-'))
}
fn safe_csv(value:&str) -> bool {
    safe_name(value,Some(".csv")) || value.starts_with(".fullmeta-case-") && value.len()<=120
        && value.ends_with(".csv") && value.chars().all(|c|c.is_ascii_alphanumeric() || matches!(c,'.'|'_'|'-'))
}
fn variable_columns(cases:&[Case]) -> Result<Vec<CsvVariable>,String> {
    let names=cases.first().ok_or("No configured result cases.")?.variable_names.as_slice();
    if cases.iter().any(|case|case.variable_names!=names) {return Err("Sequence cases declare different global variables.".into());}
    names.iter().map(|name| {
        let mut real_only=true;
        for case in cases {
            let value=case.data.complex.get(name).ok_or_else(||format!("Missing global variable {name:?}."))?;
            if !value.re.is_finite() || !value.im.is_finite() {return Err(format!("Global variable {name:?} must be finite."));}
            real_only &= value.im.abs()<=1e-12*value.re.abs().max(1.);
        }
        Ok(CsvVariable{name:name.clone(),real_only})
    }).collect()
}
fn compute(request_path:&Path,profile:&str,cache:&mut SolverCache) -> Result<Option<Case>,String> {
    let package=crate::scenario_package::load_installed(profile)?;
    let case=ConfiguredCase::load(request_path,&package,profile)?;
    let mesh=fs::read_to_string(&case.mesh_path).map_err(|e|format!("Could not read mesh: {e}"))?;
    if case.request.get("countOnly").and_then(Value::as_bool)==Some(true) {
        let (nodes,cells,first,second)=crate::prepared_edge_execution::count_dofs(&case,&package,&mesh)?;
        println!("Mesh: {nodes} nodes, {cells} tetrahedra");
        println!("Nedelec order 1: {first} periodic DOFs");
        println!("Nedelec order 2: {second} periodic DOFs");
        return Ok(None);
    }
    if case.request.get("countOnly").and_then(Value::as_bool)!=Some(true)
        && (!safe_csv(&case.sweep_csv) || !safe_csv(&case.debug_csv) || !safe_name(&case.field_prefix,None)) {
        return Err("Configured output filenames must be safe, local filenames.".into());
    }
    let mut numerical=crate::prepared_edge_execution::solve(&case,&package,&mesh,cache)?;
    numerical.data.values.insert("meshNodes".into(),json!(numerical.node_count));
    numerical.data.values.insert("cellCount".into(),json!(numerical.cell_count));
    numerical.data.values.insert("elementOrder".into(),json!(case.element_order));
    let spec=package.get("resultPlans").and_then(|v|v.get(&case.formulation)).ok_or("Edge result plan is required.")?;
    let debug=package.get("preparedEdge").and_then(|v|v.get(&case.formulation)).and_then(|v|v.get("output"))
        .and_then(|v|v.get("debugResult")).ok_or("Edge debug result plan is required.")?;
    let plan=ResultPlan::from_json(spec)?; let debug_plan=ResultPlan::from_json(debug)?;
    let variable_names=case.configuration.get("variables").and_then(Value::as_array).ok_or("Configured variables must be an array.")?
        .iter().map(|row|row.get("symbol").and_then(Value::as_str).filter(|s|!s.is_empty()).map(str::to_owned)
            .ok_or_else(||"Configured variable symbol is required.".to_string())).collect::<Result<Vec<_>,_>>()?;
    let field_name=format!("{}_{:.6}Hz.json",case.field_prefix,case.frequency_hz);
    if !safe_name(&field_name,Some(".json")) {return Err("Generated field output filename is unsafe.".into());}
    Ok(Some(Case{data:numerical.data,plan,debug_plan,output:case.output_directory,
        field_name,sweep_name:case.sweep_csv,debug_name:case.debug_csv,field_files:case.field_files,
        variable_names}))
}
fn publish(case:&Case,variables:&[CsvVariable]) -> Result<(),String> {
    let encoded=if case.field_files {Some(serde_json::to_vec(&case.plan.document(&case.data)?).map_err(|e|format!("Could not encode field output: {e}"))?)} else {None};
    let csv=case.plan.csv_with_variables(std::slice::from_ref(&case.data),variables)?;
    let debug=case.debug_plan.csv_with_variables(std::slice::from_ref(&case.data),variables)?;
    fs::create_dir_all(&case.output).map_err(|e|format!("Could not create output directory: {e}"))?;
    crate::solver_common::stage_started("output");
    if let Some(data)=encoded {fs::write(case.output.join(&case.field_name),data).map_err(|e|format!("Could not write field output: {e}"))?;}
    fs::write(case.output.join(&case.sweep_name),csv).map_err(|e|format!("Could not write sweep CSV: {e}"))?;
    fs::write(case.output.join(&case.debug_name),debug).map_err(|e|format!("Could not write debug CSV: {e}"))?;
    Ok(())
}
fn compact(case:&mut Case) -> Result<(),String> {
    ResultPlan::retain_csv_data_across(&[&case.plan,&case.debug_plan],&mut case.data,&case.variable_names)
}
pub(crate) fn run_request(path:&Path,profile:&str) -> Result<(),String> {
    let Some(case)=compute(path,profile,&mut SolverCache::default())? else {return Ok(());};
    let columns=variable_columns(std::slice::from_ref(&case))?; publish(&case,&columns)
}
pub(crate) fn run_sequence(path:&Path,profile:&str) -> Result<(),String> {
    let manifest:Value=serde_json::from_slice(&fs::read(path).map_err(|e|format!("Could not read sequence manifest: {e}"))?)
        .map_err(|e|format!("Invalid sequence manifest: {e}"))?;
    let requests=manifest.get("requestFiles").and_then(Value::as_array).filter(|a|!a.is_empty()).ok_or("Sequence requires requestFiles.")?;
    let base=path.parent().unwrap_or(Path::new(".")); let mut cache=SolverCache::default(); let mut cases=Vec::new();
    for (index,item) in requests.iter().enumerate() {
        let name=item.as_str().filter(|s|!s.is_empty()).ok_or("Sequence request path must be nonempty.")?;
        let request=Path::new(name);
        if request.components().any(|c|matches!(c,Component::ParentDir)) {return Err("Sequence request must not traverse parent directories.".into());}
        let request=if request.is_absolute(){request.to_path_buf()}else{base.join(request)};
        let raw:Value=serde_json::from_slice(&fs::read(&request).map_err(|e|format!("Could not read sequence request: {e}"))?)
            .map_err(|e|format!("Invalid sequence request: {e}"))?;
        let frequency=crate::solver_request::single_case_frequency(&raw)?;
        println!("{}",crate::solver_orchestration::solver_case_progress_line("case-start",index,requests.len(),frequency));
        let mut case=compute(&request,profile,&mut cache)?.ok_or("countOnly is not supported in a solver sequence.")?;
        let provisional=variable_columns(std::slice::from_ref(&case))?; publish(&case,&provisional)?;
        compact(&mut case)?;
        println!("{}",crate::solver_orchestration::solver_case_progress_line("case-complete",index,requests.len(),frequency));
        cases.push(case);
    }
    let columns=variable_columns(&cases)?;
    for case in &cases {
        let csv=case.plan.csv_with_variables(std::slice::from_ref(&case.data),&columns)?;
        let debug=case.debug_plan.csv_with_variables(std::slice::from_ref(&case.data),&columns)?;
        fs::write(case.output.join(&case.sweep_name),csv).map_err(|e|format!("Could not normalize sweep CSV: {e}"))?;
        fs::write(case.output.join(&case.debug_name),debug).map_err(|e|format!("Could not normalize debug CSV: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::C;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize,Ordering};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT:AtomicUsize=AtomicUsize::new(0);
            for _ in 0..100 {
                let path=std::env::temp_dir().join(format!("edge-publication-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
                match fs::create_dir(&path) {Ok(())=>return Self(path),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>panic!("{e}")}
            }
            panic!("Could not create unique test directory")
        }
    }
    impl Drop for Temp {fn drop(&mut self) {
        let target=self.0.canonicalize().expect("test directory must exist");
        let parent=std::env::temp_dir().canonicalize().expect("temporary directory must exist");
        assert_eq!(target.parent(),Some(parent.as_path()),"test cleanup target escaped temporary directory");
        assert!(target.file_name().and_then(|v|v.to_str()).is_some_and(|v|v.starts_with("edge-publication-")));
        fs::remove_dir_all(target).unwrap();
    }}
    fn fake_case(output:&Path, frequency:f64, value:C) -> Case {
        let spec=json!({"document":{"literal":{}},"csvVariables":{"realSuffix":"_re","imaginarySuffix":"_im","precision":6},
            "csv":[{"header":"frequency_Hz","input":"frequency","format":"fixed","precision":6}]});
        let debug=json!({"document":{"literal":{}},"csvVariables":{"realSuffix":"_re","imaginarySuffix":"_im","precision":6},
            "csv":[{"header":"frequency_Hz","input":"frequency","format":"fixed","precision":6},
                {"header":"residual","input":"residual","format":"scientific","precision":6},
                {"header":"mesh_nodes","input":"meshNodes","format":"integer"},
                {"header":"tetrahedra","input":"cellCount","format":"integer"},
                {"header":"nedelec_dofs","input":"unknowns","format":"integer"},
                {"header":"element_order","input":"elementOrder","format":"integer"}]});
        Case {data:ResultData{values:HashMap::from([("frequency".into(),json!(frequency)),("residual".into(),json!(1e-9)),
                ("meshNodes".into(),json!(4)),("cellCount".into(),json!(1)),("unknowns".into(),json!(6)),("elementOrder".into(),json!(1)),
                ("nodes".into(),json!([[0,0,0],[1,0,0]]))]),
            complex:HashMap::from([("v".into(),value)]),series:HashMap::from([("large".into(),vec![value;100])]),matrices:HashMap::new()},
            plan:ResultPlan::from_json(&spec).unwrap(),debug_plan:ResultPlan::from_json(&debug).unwrap(),output:output.into(),
            field_name:format!("field_{frequency:.6}Hz.json"),sweep_name:format!("s{frequency}.csv"),debug_name:format!("d{frequency}.csv"),
            field_files:false,variable_names:vec!["v".into()]}
    }
    #[test]
    fn optional_field_and_sequence_csv_compaction_preserve_diagnostics() {
        let dir=Temp::new();
        let mut cases=vec![fake_case(&dir.0,1.,C{re:1.,im:0.}),fake_case(&dir.0,2.,C{re:2.,im:3.})];
        for case in &cases {let flags=variable_columns(std::slice::from_ref(case)).unwrap(); publish(case,&flags).unwrap();}
        assert!(!dir.0.join(&cases[0].field_name).exists());
        assert!(fs::read_to_string(dir.0.join(&cases[0].sweep_name)).unwrap().starts_with("frequency_Hz,v\n"));
        for case in &mut cases {compact(case).unwrap(); assert!(case.data.series.is_empty()); assert!(!case.data.values.contains_key("nodes"));}
        let flags=variable_columns(&cases).unwrap(); assert!(!flags[0].real_only);
        for case in &cases {
            let csv=case.plan.csv_with_variables(std::slice::from_ref(&case.data),&flags).unwrap();
            let debug=case.debug_plan.csv_with_variables(std::slice::from_ref(&case.data),&flags).unwrap();
            assert!(csv.starts_with("frequency_Hz,v_re,v_im\n"));
            assert!(debug.starts_with("frequency_Hz,residual,mesh_nodes,tetrahedra,nedelec_dofs,element_order,v_re,v_im\n"));
            assert!(debug.contains(",4,1,6,1,"));
        }
    }
    #[test]
    fn filename_and_column_checks_reject_ambiguous_or_unsafe_inputs() {
        assert!(safe_csv(".fullmeta-case-001.csv")); assert!(safe_csv("response.csv"));
        assert!(!safe_csv("../response.csv")); assert!(!safe_csv(".hidden.csv"));
        assert!(!safe_name("../field",None)); assert!(!safe_name("dir/field",None));
        let dir=Temp::new(); let first=fake_case(&dir.0,1.,C{re:1.,im:0.}); let mut second=fake_case(&dir.0,2.,C{re:2.,im:0.});
        second.variable_names.push("missing".into());
        assert!(variable_columns(&[first,second]).is_err());
    }
    
}
