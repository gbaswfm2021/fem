//! Typed prescribed longitudinal-wavenumber settings shared by harmonic routes.
//!
//! The selected scenario contract declares which formulations use the
//! derivative substitution. Model values remain ordinary typed Study data.
use crate::model_expression::{self, Scalar};
use crate::{C, c_mul};
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn prescribed_wavenumber(
    package: &Value,
    model: &Value,
    study_id: &str,
    formulation: &str,
    parameters: &HashMap<String, Scalar>,
) -> Result<C, String> {
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("Model Study list is required to resolve longitudinal wavenumber.")?;
    let study = studies
        .iter()
        .find(|study| study.get("id").and_then(Value::as_str) == Some(study_id))
        .ok_or_else(|| format!("Study {study_id:?} is not present in the model."))?;
    let use_setting = study.get("useLongitudinalWavenumber");
    let explicit_enabled = match use_setting {
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err("Study useLongitudinalWavenumber must be a boolean.".into()),
        None => false,
    };
    if use_setting == Some(&Value::Bool(false)) {
        return Ok(C::default());
    }
    let Some(value) = study.get("longitudinalWavenumber") else {
        if explicit_enabled {
            validate_harmonic_declaration(package, formulation)?;
        }
        return Ok(C::default());
    };
    let mut parts = [0.0; 2];
    for (index, part) in ["real", "imaginary"].into_iter().enumerate() {
        let quantity = value
            .get(part)
            .ok_or_else(|| format!("Study longitudinalWavenumber.{part} is required."))?;
        let unit = quantity
            .get("unit")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Study longitudinalWavenumber.{part}.unit is required."))?;
        if unit != "rad/m" {
            return Err(format!(
                "Study longitudinalWavenumber.{part} must use unit rad/m."
            ));
        }
        parts[index] = model_expression::evaluate_quantity(
            quantity,
            parameters,
            &format!("Study {study_id}.longitudinalWavenumber.{part}"),
        )?;
    }
    if parts.iter().any(|value| !value.is_finite()) {
        return Err("Study longitudinal wavenumber must be finite.".into());
    }
    let kz = C {
        re: parts[0],
        im: parts[1],
    };
    if kz == C::default() && !explicit_enabled {
        return Ok(kz);
    }

    validate_harmonic_declaration(package, formulation)?;
    Ok(kz)
}

fn validate_harmonic_declaration<'a>(package: &'a Value, formulation: &str) -> Result<&'a Value, String> {
    let declaration = package.get("harmonicLongitudinalWavenumber")
        .and_then(|value| value.get("formulations"))
        .and_then(|value| value.get(formulation))
        .ok_or_else(|| {
            let required=package.get("harmonicLongitudinalWavenumber")
                .and_then(|value|value.get("requiredFormulation"))
                .and_then(Value::as_str);
            required.map(|required|format!(
                "A prescribed longitudinal wavenumber requires the {required} formulation; selected {formulation:?}."
            )).unwrap_or_else(||format!(
                "The selected scenario contract does not support a prescribed longitudinal wavenumber for formulation {formulation:?}."
            ))
        })?;
    let operator_dimension = declaration.get("operatorDimension").and_then(Value::as_u64);
    let operator_formulation = declaration
        .get("operatorFormulation")
        .and_then(Value::as_str)
        .unwrap_or(formulation);
    let dimension = package
        .pointer(&format!("/weakForms/{operator_formulation}/dimension"))
        .and_then(Value::as_u64);
    if operator_dimension != Some(3) || dimension != Some(2) {
        return Err("The selected scenario contract has an invalid two-dimensional longitudinal operator declaration.".into());
    }
    Ok(declaration)
}

/// Resolve the optional study-level Fourier wavenumber used by generic
/// transient and temporal-eigenfrequency forms. These studies initially admit
/// a real fixed input; the temporal eigenvalue remains independent of this
/// spatial parameter.
pub(crate) fn study_wavenumber(
    study: &Value,
    parameters: &HashMap<String, Scalar>,
    path: &str,
) -> Result<C, String> {
    match study.get("useLongitudinalWavenumber") {
        Some(Value::Bool(false)) => return Ok(C::default()),
        Some(Value::Bool(true)) | None => {},
        Some(_) => return Err(format!("{path}.useLongitudinalWavenumber must be a boolean.")),
    }
    let Some(value) = study.get("longitudinalWavenumber") else {
        return Ok(C::default());
    };
    let inverse_length = model_expression::unit("rad/m")?.1;
    let mut parts = [0.0; 2];
    for (index, part) in ["real", "imaginary"].into_iter().enumerate() {
        let quantity = value
            .get(part)
            .ok_or_else(|| format!("{path}.{part} is required."))?;
        let unit = quantity
            .get("unit")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{path}.{part}.unit is required."))?;
        if model_expression::unit(unit)?
            .1
            .iter()
            .zip(inverse_length)
            .any(|(left, right)| (left - right).abs() > 1e-10)
        {
            return Err(format!("{path}.{part} must have inverse-length units (rad/m)."));
        }
        parts[index] = model_expression::evaluate_quantity(
            quantity,
            parameters,
            &format!("{path}.{part}"),
        )?;
    }
    if parts.iter().any(|part| !part.is_finite()) {
        return Err(format!("{path} must be finite."));
    }
    if parts[1] != 0.0 {
        return Err(format!(
            "{path}.imaginary must be zero for transient and temporal eigenfrequency studies."
        ));
    }
    Ok(C {
        re: parts[0],
        im: 0.0,
    })
}

pub(crate) fn use_study_wavenumber(study:&Value,path:&str)->Result<bool,String> {
    match study.get("useLongitudinalWavenumber") {
        Some(Value::Bool(value))=>Ok(*value),
        Some(_)=>Err(format!("{path}.useLongitudinalWavenumber must be a boolean.")),
        None=>Ok(false),
    }
}

/// Resolve the contract-selected numerical formulation for a nonzero
/// longitudinal wavenumber. The displayed Study formulation stays unchanged;
/// a scenario may point to a wider cross-sectional field space in JSON.
pub(crate) fn operator_formulation<'a>(package: &'a Value, formulation: &'a str) -> &'a str {
    package
        .get("harmonicLongitudinalWavenumber")
        .and_then(|value| value.get("formulations"))
        .and_then(|value| value.get(formulation))
        .and_then(|value| value.get("operatorFormulation"))
        .and_then(Value::as_str)
        .unwrap_or(formulation)
}

/// Resolve a finite-`kz` source contract declared separately from the field
/// operator, preserving the selected primary formulation's source meanings.
pub(crate) fn source_formulation<'a>(package: &'a Value, formulation: &'a str) -> &'a str {
    package
        .get("harmonicLongitudinalWavenumber")
        .and_then(|value| value.get("formulations"))
        .and_then(|value| value.get(formulation))
        .and_then(|value| value.get("sourceFormulation"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| operator_formulation(package, formulation))
}

pub(crate) fn boundary_input_formulation<'a>(package: &'a Value, formulation: &'a str) -> &'a str {
    package
        .get("harmonicLongitudinalWavenumber")
        .and_then(|value| value.get("formulations"))
        .and_then(|value| value.get(formulation))
        .and_then(|value| value.get("boundaryInputFormulation"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| operator_formulation(package, formulation))
}

pub(crate) fn derivative_factor(kz: C, trial: bool) -> C {
    if trial {
        C {
            re: kz.im,
            im: -kz.re,
        }
    } else {
        C {
            re: -kz.im,
            im: kz.re,
        }
    }
}

pub(crate) fn derivative_value(kz: C, value: C, trial: bool) -> C {
    c_mul(derivative_factor(kz, trial), value)
}

pub(crate) fn validate_prepared_case_value(case: &Value, resolved: C) -> Result<(), String> {
    let Some(value) = case.get("longitudinalWavenumber") else {
        return Ok(());
    };
    if value.get("unit").and_then(Value::as_str) != Some("rad/m") {
        return Err("Prepared longitudinal wavenumber must use unit rad/m.".into());
    }
    let real = value
        .get("real")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or("Prepared longitudinal wavenumber real part must be finite.")?;
    let imaginary = value
        .get("imaginary")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or("Prepared longitudinal wavenumber imaginary part must be finite.")?;
    let scale = 1.0_f64
        .max(real.abs())
        .max(imaginary.abs())
        .max(resolved.re.abs())
        .max(resolved.im.abs());
    if (real - resolved.re).abs() > 1e-12 * scale || (imaginary - resolved.im).abs() > 1e-12 * scale
    {
        return Err("Prepared longitudinal wavenumber does not match the selected Study expression and parameter case.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn q(expression: &str, unit: &str) -> Value {
        json!({"expression":expression,"unit":unit})
    }

    fn square_mesh(n: usize) -> crate::mesh2d::Mesh2D {
        let mut nodes = Vec::with_capacity((n + 1) * (n + 1));
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([i as f64 / n as f64, j as f64 / n as f64, 0.]);
            }
        }
        let index = |i: usize, j: usize| j * (n + 1) + i;
        let mut triangles = Vec::with_capacity(2 * n * n);
        for j in 0..n {
            for i in 0..n {
                let bl = index(i, j);
                let br = index(i + 1, j);
                let tr = index(i + 1, j + 1);
                let tl = index(i, j + 1);
                triangles.push(([bl, br, tr], 1));
                triangles.push(([bl, tr, tl], 1));
            }
        }
        let mut boundary_edges = Vec::with_capacity(4 * n);
        for i in 0..n {
            boundary_edges.push(([index(i, 0), index(i + 1, 0)], 2));
            boundary_edges.push(([index(i + 1, n), index(i, n)], 2));
        }
        for j in 0..n {
            boundary_edges.push(([index(n, j), index(n, j + 1)], 2));
            boundary_edges.push(([index(0, j + 1), index(0, j)], 2));
        }
        crate::mesh2d::Mesh2D {
            nodes,
            triangles,
            boundary_edges,
            integration_edges: Vec::new(),
        }
    }

    fn map() -> Value {
        json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}],
            "domainCellSelections":{}})
    }

    fn harmonic_case(
        profile: &str,
        formulation: &str,
        frequency_hz: f64,
        kz: f64,
    ) -> crate::prepared_case::PreparedCase {
        use crate::prepared_case::{CaseKind, Output, PreparedCase, SolverSettings};
        PreparedCase {
            request: json!({"studyId":"harmonic"}),
            profile: profile.into(),
            study_id: "harmonic".into(),
            kind: CaseKind::Frequency {
                frequency_hz,
                case: json!({"longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}}),
            },
            settings: SolverSettings {
                formulation: formulation.into(),
                element_order: 1,
                quadrature: "standard".into(),
                backend: "faer".into(),
                ordering: "amd".into(),
                threads: 1,
            },
            output: Output::Frequency {
                sweep_csv: "result.csv".into(),
            },
            model_path: Default::default(),
            mesh_path: Default::default(),
            map_path: Default::default(),
            output_directory: Default::default(),
        }
    }

    fn model(real: &str, imaginary: &str) -> Value {
        json!({"model":{"study":[{"id":"harmonic","longitudinalWavenumber":{
            "real":{"expression":real,"unit":"rad/m"},
            "imaginary":{"expression":imaginary,"unit":"rad/m"}
        }}]}})
    }

    #[test]
    fn resolves_complex_parameterized_inverse_length_from_selected_study() {
        let package = json!({"weakForms":{"pressure":{"dimension":2}},
            "harmonicLongitudinalWavenumber":{"unit":"rad/m","formulations":{"pressure":{"operatorDimension":3}}}});
        let unit = model_expression::unit("rad/m").unwrap().1;
        let parameters = HashMap::from([(
            "kr".into(),
            Scalar {
                value: 4.0,
                dimension: unit,
            },
        )]);
        let value = model("kr/2", "-1");
        let kz =
            prescribed_wavenumber(&package, &value, "harmonic", "pressure", &parameters).unwrap();
        assert_eq!(kz, C { re: 2.0, im: -1.0 });
    }

    #[test]
    fn zero_does_not_require_a_scenario_opt_in() {
        let package = json!({"weakForms":{"pressure":{"dimension":2}}});
        let value = model("0", "0");
        assert_eq!(
            prescribed_wavenumber(&package, &value, "harmonic", "pressure", &HashMap::new())
                .unwrap(),
            C::default()
        );
    }

    #[test]
    fn nonzero_requires_the_selected_formulation_contract() {
        let package = json!({"weakForms":{"pressure":{"dimension":2}}});
        let value = model("1", "0");
        let error =
            prescribed_wavenumber(&package, &value, "harmonic", "pressure", &HashMap::new())
                .unwrap_err();
        assert!(error.contains("does not support a prescribed longitudinal wavenumber"));
    }

    #[test]
    fn elastic_plane_stress_rejects_nonzero_longitudinal_wavenumber() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let error = prescribed_wavenumber(
            &package,
            &model("1", "0"),
            "harmonic",
            "planeStress",
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(error.contains("does not support a prescribed longitudinal wavenumber"));
    }

    #[test]
    fn acoustic_two_dimensional_contract_compiles_longitudinal_volume_source_and_recovery() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let mut plan =
            crate::scenario_plan::ScenarioPlan::from_package(&package, "pressure").unwrap();
        let kz = C { re: 12., im: -0.5 };
        plan.set_longitudinal_wavenumber(kz).unwrap();
        assert_eq!(plan.volume.operator_dimension(), 3);
        assert!(plan.recovery.as_ref().unwrap().has_longitudinal_operator());
        assert!(
            plan.domain_sources["additionalDomainSource"]
                .form
                .has_longitudinal_operator()
        );
    }

    #[test]
    fn elastic_plane_strain_declares_three_component_cross_sectional_form() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let operator = operator_formulation(&package, "planeStrain");
        assert_eq!(operator, "planeStrainLongitudinal");
        let plan = crate::scenario_plan::ScenarioPlan::from_package(&package, operator).unwrap();
        assert_eq!(plan.volume.shape(), (2, 3));
        assert_eq!(plan.volume.operator_dimension(), 3);
        assert_eq!(plan.coefficient_matrices["C"].shape(), (9, 9));
        assert_eq!(
            plan.tensor_recovery.as_ref().unwrap().components(),
            [
                "sigma_xx", "sigma_yy", "sigma_zz", "sigma_xy", "sigma_yz", "sigma_xz"
            ]
        );
        assert_eq!(
            plan.domain_sources["domainSource"]
                .form
                .source_components()
                .unwrap(),
            3
        );
        assert_eq!(
            plan.domain_sources["additionalDomainSource"]
                .form
                .source_components()
                .unwrap(),
            9
        );
        assert!(plan.essential["dirichlet"].require_shape(2, 3).is_ok());
    }

    #[test]
    fn em2d_full_polarization_contract_compiles_sources_boundaries_and_recovery() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let operator = operator_formulation(&package, "Full");
        assert_eq!(operator, "fullPolarization");
        let source = source_formulation(&package, "Full");
        let boundary = boundary_input_formulation(&package, "Full");
        let mut plan = crate::scenario_plan::ScenarioPlan::from_package_with_source_formulation(
            &package, operator, source,
        ).unwrap();
        plan.set_longitudinal_wavenumber(C { re: 7., im: -0.25 }).unwrap();
        assert_eq!(plan.volume.shape(), (2, 3));
        assert_eq!(plan.volume.operator_dimension(), 3);
        assert!(plan.domain_sources.contains_key("domainSource"));
        assert!(plan.domain_sources.contains_key("additionalDomainSource"));
        assert!(plan.recovery.as_ref().unwrap().has_longitudinal_operator());
        assert!(plan.boundaries.contains_key("impedance"));

        let declarations = package.get("boundaryInputs").and_then(|v| v.get(boundary)).unwrap();
        let input=crate::boundary_input::BoundaryInput::compile(&declarations["dirichlet"],&json!({
            "type":"dirichlet","value":{"expression":"0","unit":"V/m"},
            "fieldValues":[{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]
        })).unwrap();
        assert!(input.form_key().is_none());
        let values = input.evaluate(&HashMap::new(),Some([0.,0.,0.]),&HashMap::new(),None).unwrap();
        assert!(values.essential.is_some());
    }

    #[test]
    fn acoustic_finite_kz_transverse_wave_error_decreases_on_refinement() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let kz = 1.3;
        let c = 343.0;
        let kx = std::f64::consts::PI;
        let omega = c * (kx * kx + kz * kz).sqrt();
        let model = json!({"scenario":{"profileId":"acoustic-2d"},"model":{
            "parameters":[{"id":"length","name":"Length","symbol":"L","expression":"1","unit":"m"},
                {"id":"kz","name":"Longitudinal wavenumber","symbol":"kz0","expression":"1.3","unit":"rad/m"}],
            "variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":q("kz0","rad/m"),"imaginary":q("0","rad/m")}}],
            "materials":[{"id":"fluid","selectionId":"domain","density":q("1.225","kg/m^3"),
                "soundSpeed":q("343","m/s"),"lossFactor":"0"}],
            "physics":{"id":"wave","type":"acoustics","formulation":"pressure","dirichlet":{
                "id":"dirichlet","type":"dirichlet","selectionId":"outer","value":q("sin(pi*x/L)","Pa")}}
        }});
        let parameters = crate::model_expression::resolve_parameters(
            model
                .pointer("/model/parameters")
                .unwrap()
                .as_array()
                .unwrap(),
            &model,
        )
        .unwrap();
        let errors = [4, 8].map(|n| {
            let mesh = square_mesh(n);
            let case = harmonic_case("acoustic-2d", "pressure", omega / std::f64::consts::TAU, kz);
            let result = crate::prepared_frequency_scalar::solve(
                &case,
                &package,
                &model,
                &map(),
                crate::prepared_frequency_scalar::Mesh::Triangle {
                    nodes: &mesh.nodes,
                    cells: &mesh.triangles,
                    facets: &mesh.boundary_edges,
                },
                &parameters,
                None,
                &mut crate::SolverCache::default(),
            )
            .unwrap();
            assert!(result.solution.residual < 1e-7);
            let mut squared = 0.0;
            let mut count = 0;
            for j in 1..n {
                for i in 1..n {
                    let node = j * (n + 1) + i;
                    let exact = (std::f64::consts::PI * i as f64 / n as f64).sin();
                    squared += (result.solution.real[node] - exact).powi(2)
                        + result.solution.imaginary[node].powi(2);
                    count += 1;
                }
            }
            (squared / count as f64).sqrt()
        });
        assert!(
            errors[1] < 0.75 * errors[0],
            "transverse pressure errors did not decrease: {errors:?}"
        );
    }

    #[test]
    fn elastic_finite_kz_shear_wave_error_decreases_on_refinement() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let kz: f64 = 0.7;
        let young = 12.0;
        let poisson = 0.25;
        let mu = young / (2.0 * (1.0 + poisson));
        let kx = std::f64::consts::PI;
        let omega = (mu * (kx * kx + kz * kz)).sqrt();
        let model = json!({"scenario":{"profileId":"elastic-2d"},"model":{
            "parameters":[{"id":"length","name":"Length","symbol":"L","expression":"1","unit":"m"},
                {"id":"kz","name":"Longitudinal wavenumber","symbol":"kz0","expression":"0.7","unit":"rad/m"}],
            "variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":q("kz0","rad/m"),"imaginary":q("0","rad/m")}}],
            "materials":[{"id":"solid","selectionId":"domain","density":q("1","kg/m^3"),
                "youngModulus":q("12","Pa"),"poissonRatio":"0.25","densityLossFactor":"0"}],
            "physics":{"id":"wave","type":"elasticity","formulation":"planeStrain","dirichlet":{
                "id":"dirichlet","type":"dirichlet","selectionId":"outer","value":q("0","m"),
                "fieldValues":[q("0","m"),q("sin(pi*x/L)","m"),q("0","m")]}}
        }});
        let parameters = crate::model_expression::resolve_parameters(
            model
                .pointer("/model/parameters")
                .unwrap()
                .as_array()
                .unwrap(),
            &model,
        )
        .unwrap();
        let errors = [4, 8].map(|n| {
            let mesh = square_mesh(n);
            let case = harmonic_case(
                "elastic-2d",
                "planeStrain",
                omega / std::f64::consts::TAU,
                kz,
            );
            let result = crate::prepared_vector_model::solve(
                &case,
                &package,
                &model,
                &map(),
                crate::prepared_vector_model::Mesh::Triangle(&mesh),
                &parameters,
                &mut crate::SolverCache::default(),
            )
            .unwrap();
            assert!(result.solution.residual < 1e-7);
            let mut squared = 0.0;
            let mut count = 0;
            for j in 1..n {
                for i in 1..n {
                    let node = j * (n + 1) + i;
                    let x = i as f64 / n as f64;
                    let expected = [
                        C::default(),
                        C {
                            re: (kx * x).sin(),
                            im: 0.,
                        },
                        C::default(),
                    ];
                    for component in 0..3 {
                        let actual = C {
                            re: result.solution.real[node * 3 + component],
                            im: result.solution.imaginary[node * 3 + component],
                        };
                        squared += (actual.re - expected[component].re).powi(2)
                            + (actual.im - expected[component].im).powi(2);
                    }
                    count += 3;
                }
            }
            (squared / count as f64).sqrt()
        });
        assert!(
            errors[1] < 0.8 * errors[0],
            "plane-strain displacement errors did not decrease: {errors:?}"
        );
    }

    #[test]
    fn elastic_finite_kz_body_force_eigenstress_and_traction_drive_the_solution() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let kz = 0.7;
        let frequency = 0.2;
        let model = json!({"scenario":{"profileId":"elastic-2d"},"model":{
            "parameters":[{"id":"length","name":"Length","symbol":"L","expression":"1","unit":"m"}],
            "variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":q("0.7","rad/m"),"imaginary":q("0","rad/m")}}],
            "materials":[{"id":"solid","selectionId":"domain","density":q("1","kg/m^3"),
                "youngModulus":q("12","Pa"),"poissonRatio":"0.25","densityLossFactor":"0"}],
            "physics":{"id":"wave","type":"elasticity","formulation":"planeStrain","dirichlet":{
                "id":"fixed","type":"dirichlet","selectionId":"outer","value":q("0","m"),
                "fieldValues":[q("0","m"),q("0","m"),q("0","m")]}}
        }});
        let parameters = crate::model_expression::resolve_parameters(
            model
                .pointer("/model/parameters")
                .unwrap()
                .as_array()
                .unwrap(),
            &model,
        )
        .unwrap();
        let case = harmonic_case("elastic-2d", "planeStrain", frequency, kz);
        let mesh = square_mesh(4);
        let mut mesh_map = map();
        mesh_map["domainCellSelections"]["domain"] =
            json!((0..mesh.triangles.len()).collect::<Vec<_>>());
        let solve = |model: &Value, mesh: &crate::mesh2d::Mesh2D, map: &Value| {
            crate::prepared_vector_model::solve(
                &case,
                &package,
                model,
                map,
                crate::prepared_vector_model::Mesh::Triangle(mesh),
                &parameters,
                &mut crate::SolverCache::default(),
            )
            .unwrap()
        };
        let baseline = solve(&model, &mesh, &mesh_map);
        assert!(baseline.solution.residual < 1e-7);
        assert!(
            baseline
                .solution
                .real
                .iter()
                .all(|value| value.abs() < 1e-12)
        );
        assert!(
            baseline
                .solution
                .imaginary
                .iter()
                .all(|value| value.abs() < 1e-12)
        );

        let response_changes = |response: &crate::prepared_vector::Result| {
            response
                .solution
                .real
                .iter()
                .zip(&response.solution.imaginary)
                .zip(
                    baseline
                        .solution
                        .real
                        .iter()
                        .zip(&baseline.solution.imaginary),
                )
                .map(|((real, imaginary), (base_real, base_imaginary))| {
                    (real - base_real).hypot(imaginary - base_imaginary)
                })
                .fold(0.0, f64::max)
        };

        let mut body_forced = model.clone();
        body_forced["model"]["physics"]["body-force"] = json!({
            "id":"body-force","type":"domainSource","selectionId":"domain",
            "values":[q("0","N/m^3"),q("1","N/m^3"),q("0","N/m^3")]
        });
        let body_response = solve(&body_forced, &mesh, &mesh_map);
        assert!(body_response.solution.residual < 1e-7);
        assert!(
            response_changes(&body_response) > 1e-9,
            "finite-kz body-force route did not change the solution"
        );

        let mut eigenstress_model = model.clone();
        eigenstress_model["model"]["physics"]["eigenstress"] = json!({
            "id":"eigenstress","type":"additionalDomainSource","selectionId":"domain",
            "values":[q("10*x/L","Pa"),q("0","Pa"),q("0","Pa"),
                q("0","Pa"),q("0","Pa"),q("0","Pa"),
                q("0","Pa"),q("0","Pa"),q("0","Pa")]
        });
        let eigenstress_response = solve(&eigenstress_model, &mesh, &mesh_map);
        assert!(eigenstress_response.solution.residual < 1e-7);
        assert!(
            response_changes(&eigenstress_response) > 1e-9,
            "finite-kz eigenstress route did not change the solution"
        );

        let mut traction_model = model.clone();
        traction_model["model"]["physics"]["traction"] = json!({
            "id":"traction","type":"neumann","selectionId":"top","value":q("0","Pa"),
            "fieldValues":[q("0","Pa"),q("1","Pa"),q("0","Pa")]
        });
        let mut traction_mesh = mesh.clone();
        for (_, tag) in traction_mesh.boundary_edges.iter_mut().skip(4).take(4) {
            *tag = 3;
        }
        let traction_map = json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2},
                {"selectionId":"top","physicalTag":3}],"domainCellSelections":{}});
        let traction_response = solve(&traction_model, &traction_mesh, &traction_map);
        assert!(traction_response.solution.residual < 1e-7);
        assert!(
            response_changes(&traction_response) > 1e-9,
            "finite-kz natural traction route did not change the solution"
        );
    }

    #[test]
    fn em2d_finite_kz_full_polarization_error_decreases_on_refinement() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let kz: f64 = 1.1;
        let length: f64 = 1.0;
        let kx = std::f64::consts::PI / length;
        let ky = 1.3 * std::f64::consts::PI / length;
        let omega = 299_792_458.0 * (kx * kx + ky * ky + kz * kz).sqrt();
        let model = json!({"scenario":{"profileId":"em-2d"},"model":{
            "parameters":[{"id":"length","name":"Length","symbol":"L","expression":"1","unit":"m"},
                {"id":"amplitude","name":"Electric-field amplitude","symbol":"E0","expression":"1","unit":"V/m"},
                {"id":"kz","name":"Longitudinal wavenumber","symbol":"kz0","expression":"1.1","unit":"rad/m"}],
            "variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":q("kz0","rad/m"),"imaginary":q("0","rad/m")}}],
            "materials":[{"id":"vacuum","selectionId":"domain","relativePermittivity":"1",
                "relativePermeability":"1","conductivity":q("0","S/m")}],
            "physics":{"id":"wave","formulation":"Full","pec":{
                "id":"pec","type":"dirichlet","selectionId":"outer","value":q("0","V/m"),
                "fieldValues":[q("1.3*E0*sin(pi*x/L)*cos(1.3*pi*y/L)","V/m"),
                    q("-E0*cos(pi*x/L)*sin(1.3*pi*y/L)","V/m"),q("0","V/m")]}}
        }});
        let parameters = crate::model_expression::resolve_parameters(
            model
                .pointer("/model/parameters")
                .unwrap()
                .as_array()
                .unwrap(),
            &model,
        )
        .unwrap();
        let errors = [8, 16].map(|n| {
            let mesh = square_mesh(n);
            let case = harmonic_case("em-2d", "Full", omega / std::f64::consts::TAU, kz);
            let result = crate::prepared_hcurl2d_model::solve(
                &case,
                &package,
                &model,
                &map(),
                &mesh,
                &parameters,
                &mut crate::SolverCache::default(),
            )
            .unwrap();
            assert!(result.values["residual"].as_f64().unwrap() < 1e-7);
            let mut squared = 0.0;
            let mut count = 0;
            for j in 1..n {
                for i in 1..n {
                    let node = j * (n + 1) + i;
                    let x = i as f64 / n as f64;
                    let y = j as f64 / n as f64;
                    let expected = [
                        C {
                            re: 1.3 * (kx * x).sin() * (ky * y).cos(),
                            im: 0.,
                        },
                        C {
                            re: -(kx * x).cos() * (ky * y).sin(),
                            im: 0.,
                        },
                        C::default(),
                    ];
                    for (component, name) in ["Ex", "Ey", "Ez"].into_iter().enumerate() {
                        let actual = result.series[name][node];
                        squared += (actual.re - expected[component].re).powi(2)
                            + (actual.im - expected[component].im).powi(2);
                    }
                    count += 3;
                }
            }
            (squared / count as f64).sqrt()
        });
        assert!(
            errors[1] < 0.9 * errors[0],
            "full-polarization electric-field errors did not decrease: {errors:?}"
        );
    }
}
