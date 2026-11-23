//! Bind generic transient quadrature samples to declared model inputs.
//!
//! This adapter is internal infrastructure; it does not enable a Study.
use std::collections::HashMap;

use crate::domain_source::{DomainSourcePlan, DomainSources};
use crate::material::CompiledMaterialAssignments;
use crate::model_expression::{self, Scalar};
use crate::solver_common::C;
use crate::transient_plan::{
    SampleLocation, SampledCoefficient, TransientCoefficients, TransientDomainSourcePlan,
    TransientPlan, TransientSource,
};

/// Owned compiled data for one parameter case. Selection resolution is
/// performed by a declared SelectionPlan before constructing this adapter.
pub(crate) struct TransientModelData {
    materials: CompiledMaterialAssignments,
    bindings: crate::material_bindings::MaterialBindings,
    coefficients: TransientCoefficients,
    source: Vec<(TransientDomainSourcePlan, DomainSources)>,
    source_by_cell: bool,
}

impl TransientModelData {
    /// Resolve scenario-declared selection collections before compiling the
    /// reusable material and source expressions. Other selected collections
    /// are returned for the boundary adapter to interpret.
    pub(crate) fn bind_selections<'a>(
        profile: &str,
        plan: &TransientPlan,
        declaration: &serde_json::Value,
        binding_declaration: &serde_json::Value,
        selection_plan: &crate::selection_plan::SelectionPlan,
        model: &'a serde_json::Value,
        map: &serde_json::Value,
        mesh_tags: &HashMap<String, Vec<usize>>,
    ) -> Result<
        (
            Self,
            HashMap<String, Vec<crate::model_bindings::BoundSelection<'a>>>,
        ),
        String,
    > {
        Self::bind_selections_with_source_policy(profile,plan,declaration,binding_declaration,selection_plan,model,map,mesh_tags,true)
    }

    /// Bind a source-free linear Study without compiling applied-load
    /// expressions. Source rows remain present in the model and are still
    /// available to other Study routes in the same project.
    pub(crate) fn bind_selections_without_sources<'a>(
        profile:&str, plan:&TransientPlan, declaration:&serde_json::Value,
        binding_declaration:&serde_json::Value, selection_plan:&crate::selection_plan::SelectionPlan,
        model:&'a serde_json::Value, map:&serde_json::Value, mesh_tags:&HashMap<String,Vec<usize>>,
    )->Result<(Self,HashMap<String,Vec<crate::model_bindings::BoundSelection<'a>>>),String>{
        Self::bind_selections_with_source_policy(profile,plan,declaration,binding_declaration,selection_plan,model,map,mesh_tags,false)
    }

    fn bind_selections_with_source_policy<'a>(
        profile: &str,
        plan: &TransientPlan,
        declaration: &serde_json::Value,
        binding_declaration: &serde_json::Value,
        selection_plan: &crate::selection_plan::SelectionPlan,
        model: &'a serde_json::Value,
        map: &serde_json::Value,
        mesh_tags: &HashMap<String, Vec<usize>>,
        bind_sources:bool,
    ) -> Result<
        (
            Self,
            HashMap<String, Vec<crate::model_bindings::BoundSelection<'a>>>,
        ),
        String,
    > {
        let selected = selection_plan.bind(model, map, mesh_tags)?;
        let materials = selected
            .get("materials")
            .ok_or("Transient selection contract requires a materials collection.")?;
        let sources = if bind_sources {selected.get("domainSources").map(Vec::as_slice).unwrap_or(&[])} else {&[]};
        let result = Self::from_bound(
            profile,
            plan,
            declaration,
            binding_declaration,
            materials,
            sources,
            selection_plan.uses_cell_map("domainSources", map),
        )?;
        Ok((result, selected))
    }

    pub(crate) fn from_bound(
        profile: &str,
        plan: &TransientPlan,
        declaration: &serde_json::Value,
        binding_declaration: &serde_json::Value,
        material_rows: &[crate::model_bindings::BoundSelection<'_>],
        source_rows: &[crate::model_bindings::BoundSelection<'_>],
        source_by_cell: bool,
    ) -> Result<Self, String> {
        let mut assignments = HashMap::new();
        for row in material_rows {
            if assignments.insert(row.tag, row.row.clone()).is_some() {
                return Err(format!(
                    "Duplicate material assignment for tag {}.",
                    row.tag
                ));
            }
        }
        let materials = if let Some(properties) = declaration.get("materialProperties") {
            let properties = properties
                .as_array()
                .ok_or("Transient materialProperties must be an array.")?;
            CompiledMaterialAssignments::compile_with_definitions(
                profile,
                &assignments,
                properties,
            )?
        } else {
            CompiledMaterialAssignments::compile_for_study(profile, &assignments, "transient")?
        };
        let bindings = crate::material_bindings::MaterialBindings::from_json(
            declaration
                .get("materialBindings")
                .unwrap_or(binding_declaration),
        )?;
        let coefficients = TransientCoefficients::from_json(plan, &declaration["coefficients"])?;
        let mut route_declarations = match declaration.get("domainSources") {
            Some(value) => value.as_array().ok_or("Transient domainSources must be an array.")?.clone(),
            None => Vec::new(),
        };
        if route_declarations.is_empty() {
            if let Some(legacy) = declaration.get("domainSource") {
                route_declarations.push(legacy.clone());
            }
        }
        let mut routes = HashMap::<String, TransientDomainSourcePlan>::new();
        for route in &route_declarations {
            let contract = TransientDomainSourcePlan::from_json(plan, route)?;
            let kind = contract.feature_type.clone();
            if routes.insert(kind.clone(), contract).is_some() {
                return Err(format!("Transient source route repeats feature type {kind:?}."));
            }
        }
        let mut active_types = std::collections::BTreeSet::new();
        for row in source_rows {
            let kind = row.row.get("type").and_then(serde_json::Value::as_str)
                .filter(|v| !v.is_empty()).unwrap_or("domainSource");
            if !routes.contains_key(kind) {
                return Err(format!("Active transient domain source type {kind:?} has no declared route."));
            }
            active_types.insert(kind.to_owned());
        }
        let mut source = Vec::with_capacity(active_types.len());
        for kind in active_types {
            let contract = routes.remove(&kind).ok_or_else(|| format!("Transient source route {kind:?} disappeared."))?;
            let matching = source_rows.iter().filter(|row| row.row.get("type").and_then(serde_json::Value::as_str).unwrap_or("domainSource") == kind.as_str())
                .map(|row| crate::model_bindings::BoundSelection { tag: row.tag, selection_id: row.selection_id, row: row.row })
                .collect::<Vec<_>>();
            let values = contract.input.bind(&matching)?;
            source.push((contract, values));
        }
        Ok(Self {
            materials,
            bindings,
            coefficients,
            source,
            source_by_cell,
        })
    }

    pub(crate) fn sampler<'a>(
        &'a self,
        parameters: &'a HashMap<String, Scalar>,
    ) -> TransientModelSampler<'a> {
        TransientModelSampler {
            materials: &self.materials,
            bindings: &self.bindings,
            coefficients: &self.coefficients,
            parameters,
            source: self.source.iter().map(|(plan, values)|
                (plan.feature_type.clone(), (&plan.input, values))).collect(),
            source_by_cell: self.source_by_cell,
        }
    }

    pub(crate) fn sources(&self) -> Vec<TransientSource<'_>> {
        self.source.iter().map(|(plan, _)| plan.source(&plan.feature_type)).collect()
    }

    /// Material symbols for a physical integration point and time. The tag
    /// belongs to the owning cell, preserving jumps across interfaces.
    pub(crate) fn material_symbols_at(
        &self, parameters:&HashMap<String,Scalar>, tag:usize, point:[f64;3], time:f64,
    ) -> Result<HashMap<String,C>,String> {
        Ok(self.materials.evaluate_at_time(tag,parameters,time,point,"transient result material")?
            .into_iter().map(|(name,value)|(name,C{re:value.value,im:0.})).collect())
    }

    /// Sample material expressions at every cell-local vertex at an output
    /// time. Keeping cell-local values preserves discontinuities at material
    /// interfaces; no work is done unless a snapshot requests these fields.
    pub(crate) fn material_fields(
        &self,
        nodes: &[[f64; 3]],
        cells: &[(Vec<usize>, usize)],
        parameters: &HashMap<String, Scalar>,
        time: f64,
    ) -> Result<serde_json::Map<String, serde_json::Value>, String> {
        let mut rows = Vec::with_capacity(cells.len());
        let mut symbols: Option<std::collections::BTreeSet<String>> = None;
        for (index, (vertices, tag)) in cells.iter().enumerate() {
            let mut per_cell = std::collections::BTreeMap::<String, Vec<f64>>::new();
            for &node in vertices {
                let point = *nodes
                    .get(node)
                    .ok_or("Transient material result references a missing node.")?;
                let values = self.materials.evaluate_at_time(
                    *tag,
                    parameters,
                    time,
                    point,
                    &format!("cell {index}"),
                )?;
                let keys = values
                    .keys()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                if let Some(expected) = &symbols {
                    if &keys != expected {
                        return Err(
                            "Transient material fields are inconsistent across cells.".into()
                        );
                    }
                } else {
                    symbols = Some(keys);
                }
                for (name, value) in values {
                    per_cell.entry(name).or_default().push(value.value);
                }
            }
            if vertices.is_empty() {
                return Err("Transient material result cell has no vertices.".into());
            }
            rows.push(per_cell);
        }
        let mut output = serde_json::Map::new();
        for symbol in symbols.unwrap_or_default() {
            let real = rows
                .iter()
                .map(|row| {
                    row.get(&symbol)
                        .cloned()
                        .ok_or("Transient material field is missing from a cell.")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let imaginary = real
                .iter()
                .map(|row| vec![0.; row.len()])
                .collect::<Vec<_>>();
            output.insert(symbol, serde_json::json!({"real":real,"imag":imaginary}));
        }
        Ok(output)
    }
}

/// A selection can be keyed by physical tag or by mesh cell; material tags
/// and source membership are deliberately independent.
pub(crate) struct TransientModelSampler<'a> {
    pub(crate) materials: &'a CompiledMaterialAssignments,
    pub(crate) bindings: &'a crate::material_bindings::MaterialBindings,
    pub(crate) coefficients: &'a TransientCoefficients,
    pub(crate) parameters: &'a HashMap<String, Scalar>,
    pub(crate) source: HashMap<String, (&'a DomainSourcePlan, &'a DomainSources)>,
    pub(crate) source_by_cell: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_bindings::BoundSelection;
    use crate::transient_plan::TransientPlan;
    use serde_json::{Value, json};
    #[test]
    fn elastic_forms_reject_harmonic_density_loss_without_a_causal_time_law() {
        for text in [
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let profile = package["id"].as_str().unwrap();
            for (form, declaration) in package["transientForms"].as_object().unwrap() {
                let plan = TransientPlan::from_json(declaration).unwrap();
                let row = json!({"id":"material","densityLossFactor":"0.1"});
                let selected = [BoundSelection {
                    tag: 1,
                    selection_id: "domain",
                    row: &row,
                }];
                let data = TransientModelData::from_bound(
                    profile,
                    &plan,
                    declaration,
                    &package["materialBindings"][form],
                    &selected,
                    &[],
                    false,
                )
                .unwrap();
                let params = HashMap::new();
                let error = data
                    .sampler(&params)
                    .coefficients(SampleLocation {
                        cell: 0,
                        material_tag: 1,
                        point: [0.; 3],
                        time: 0.,
                    })
                    .err()
                    .expect("nonzero harmonic loss must not be silently ignored");
                assert!(
                    error.contains("loss") || error.contains("implemented"),
                    "{profile}/{form}: {error}"
                );
            }
        }
    }
    #[test]
    fn thermal_forms_use_capacity_times_temperature_rate_in_both_dimensions() {
        use crate::transient_mesh::{NodalCell, NodalMesh};
        use crate::transient_plan::{StateLayout, TransientAssemblyWorkspace};
        use crate::transient_step::LinearOptions;
        for (dimension, profile, text) in [
            (2, "thermal-2d", include_str!("scenarios/thermal2d.json")),
            (3, "thermal-3d", include_str!("scenarios/thermal3d.json")),
        ] {
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let declaration = &package["transientForms"]["T"];
            let plan = TransientPlan::from_json(declaration).unwrap();
            let selection = crate::selection_plan::SelectionPlan::from_json(
                &package["transientSelectionPlan"],
            )
            .unwrap();
            let model = json!({"model":{"materials":[{"id":"material","selectionId":"domain",
                "volumetricHeatCapacity":{"expression":"C0*(1+t/tau)","unit":"W*s/(m^3*K)"}}],"physics":{
                "source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1","unit":"W/m^3"}]}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![]),
            ]);
            let (data, _) = TransientModelData::bind_selections(
                profile,
                &plan,
                declaration,
                &package["materialBindings"]["T"],
                &selection,
                &model,
                &map,
                &tags,
            )
            .unwrap();
            let parameters = HashMap::from([
                (
                    "C0".into(),
                    Scalar {
                        value: 2.,
                        dimension: model_expression::unit("W*s/(m^3*K)").unwrap().1,
                    },
                ),
                (
                    "tau".into(),
                    Scalar {
                        value: 1.,
                        dimension: model_expression::unit("s").unwrap().1,
                    },
                ),
            ]);
            let sampler = data.sampler(&parameters);
            let nodes = if dimension == 2 {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            } else {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            };
            let cells = [NodalCell {
                nodes: (0..nodes.len()).collect(),
                tag: 7,
            }];
            let mesh = NodalMesh {
                dimension,
                nodes: &nodes,
                cells: &cells,
            };
            let layout = StateLayout::new(&plan, &[nodes.len()]).unwrap();
            let grid = crate::transient_request::TimeGrid {
                start: 0.,
                end: 1.,
                max_step: 0.25,
                max_steps: 4,
                output_times: vec![1.],
            };
            let mut workspace = TransientAssemblyWorkspace::default();
            let sources = data.sources();
            let result = crate::transient_runner::run_nodal(
                &plan,
                &layout,
                &mesh,
                &grid,
                "standard",
                &parameters,
                &HashMap::new(),
                None,
                &mut workspace,
                &LinearOptions {
                    tolerance: 1e-10,
                    backend: "faer",
                    ordering: "amd",
                    threads: 1,
                },
                &mut crate::solver_common::SolverCache::default(),
                |location| sampler.coefficients(location),
                &sources,
                |feature_type, location| sampler.source_for(feature_type, location),
                |_, _| Ok(()),
                |_, _, _| Ok(()),
            )
            .unwrap();
            let expected = (1..=4)
                .map(|step| 0.25 / (2. * (1. + step as f64 * 0.25)))
                .sum::<f64>();
            assert!(result.iter().all(|v| (v.re - expected).abs() < 1e-10));
            let mut missing = model.clone();
            missing["model"]["materials"][0]
                .as_object_mut()
                .unwrap()
                .remove("volumetricHeatCapacity");
            assert!(
                TransientModelData::bind_selections(
                    profile,
                    &plan,
                    declaration,
                    &package["materialBindings"]["T"],
                    &selection,
                    &missing,
                    &map,
                    &tags
                )
                .is_err()
            );
            let mut spatial = model.clone();
            spatial["model"]["materials"][0]["volumetricHeatCapacity"]["expression"] =
                json!("C0*(1+t/tau+x/L)");
            let (spatial_data, _) = TransientModelData::bind_selections(
                profile,
                &plan,
                declaration,
                &package["materialBindings"]["T"],
                &selection,
                &spatial,
                &map,
                &tags,
            )
            .unwrap();
            let mut spatial_parameters = parameters.clone();
            spatial_parameters.insert(
                "L".into(),
                Scalar {
                    value: 1.,
                    dimension: model_expression::unit("m").unwrap().1,
                },
            );
            let location = SampleLocation {
                cell: 0,
                material_tag: 7,
                point: [1., 0., 0.],
                time: 1.,
            };
            match &spatial_data
                .sampler(&spatial_parameters)
                .coefficients(location)
                .unwrap()["capacity"]
            {
                SampledCoefficient::Scalar(value) => assert!((value.re - 6.).abs() < 1e-12),
                _ => panic!("capacity must be scalar"),
            }
            spatial_parameters.get_mut("C0").unwrap().value = -2.;
            assert!(
                spatial_data
                    .sampler(&spatial_parameters)
                    .coefficients(location)
                    .is_err()
            );
        }
    }
    #[test]
    fn bound_model_data_compiles_once_and_keeps_no_source_path_empty() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let declaration = &package["transientForms"]["pressure"];
        let plan = TransientPlan::from_json(declaration).unwrap();
        let material = json!({"id":"air"});
        let material_rows = [BoundSelection {
            tag: 7,
            selection_id: "material",
            row: &material,
        }];
        let empty = TransientModelData::from_bound(
            "acoustic-2d",
            &plan,
            declaration,
            &package["materialBindings"]["pressure"],
            &material_rows,
            &[],
            true,
        )
        .unwrap();
        assert!(empty.sources().is_empty());
        let source = json!({"id":"source","values":[{"expression":"1","unit":"Hz"}]});
        let source_rows = [BoundSelection {
            tag: 0,
            selection_id: "domain",
            row: &source,
        }];
        let data = TransientModelData::from_bound(
            "acoustic-2d",
            &plan,
            declaration,
            &package["materialBindings"]["pressure"],
            &material_rows,
            &source_rows,
            true,
        )
        .unwrap();
        assert_eq!(data.sources()[0].field, 0);
        let parameters = HashMap::new();
        let sampler = data.sampler(&parameters);
        let location = SampleLocation {
            cell: 0,
            material_tag: 7,
            point: [0.; 3],
            time: 0.,
        };
        assert_eq!(sampler.source(location).unwrap()[0].re, 1.);
        sampler.coefficients(location).unwrap();
    }

    #[test]
    fn declared_selection_plan_binds_model_material_and_cell_source() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let declaration = &package["transientForms"]["pressure"];
        let plan = TransientPlan::from_json(declaration).unwrap();
        let selections =
            crate::selection_plan::SelectionPlan::from_json(&package["selectionPlan"]).unwrap();
        let model = json!({"model":{"materials":[{"id":"air","selectionId":"domain"}],"physics":{
            "source":{"id":"source","type":"domainSource","selectionId":"domain",
                "values":[{"expression":"1","unit":"Hz"}]}}}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[],"domainCellSelections":{"domain":[0]}});
        let tags = HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7]),
            ("boundaries".into(), vec![]),
        ]);
        let (data, selected) = TransientModelData::bind_selections(
            "acoustic-2d",
            &plan,
            declaration,
            &package["materialBindings"]["pressure"],
            &selections,
            &model,
            &map,
            &tags,
        )
        .unwrap();
        assert_eq!(selected["domainSources"][0].tag, 0);
        let params = HashMap::new();
        let value = data
            .sampler(&params)
            .source(SampleLocation {
                cell: 0,
                material_tag: 7,
                point: [0.; 3],
                time: 0.,
            })
            .unwrap();
        assert_eq!(value[0].re, 1.);
    }

    #[test]
    fn every_wave_form_binds_project_materials_through_its_declared_selection_plan() {
        for text in [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),
            include_str!("scenarios/em3d.json"),
        ] {
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let profile = package["id"].as_str().unwrap();
            let selections = crate::selection_plan::SelectionPlan::from_json(
                &package["transientSelectionPlan"],
            )
            .unwrap();
            let model = json!({"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![]),
            ]);
            for (formulation, declaration) in package["transientForms"].as_object().unwrap() {
                let plan = TransientPlan::from_json(declaration).unwrap();
                let (data, _) = TransientModelData::bind_selections(
                    profile,
                    &plan,
                    declaration,
                    &package["materialBindings"][formulation],
                    &selections,
                    &model,
                    &map,
                    &tags,
                )
                .unwrap_or_else(|e| panic!("{profile}/{formulation}: {e}"));
                let params = HashMap::new();
                data.sampler(&params)
                    .coefficients(SampleLocation {
                        cell: 0,
                        material_tag: 7,
                        point: [0.; 3],
                        time: 0.,
                    })
                    .unwrap();
                assert!(data.sources().is_empty());
            }
        }
    }

    #[test]
    fn every_enabled_form_evaluates_declared_default_materials() {
        for text in [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),
            include_str!("scenarios/em3d.json"),
        ] {
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let profile = package["id"].as_str().unwrap();
            let materials = CompiledMaterialAssignments::compile(
                profile,
                &HashMap::from([(1, json!({"id":"default"}))]),
            )
            .unwrap();
            for (formulation, declaration) in package["transientForms"].as_object().unwrap() {
                let plan = TransientPlan::from_json(declaration).unwrap();
                let coefficients =
                    TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
                let bindings = crate::material_bindings::MaterialBindings::from_json(
                    &package["materialBindings"][formulation],
                )
                .unwrap();
                let params = HashMap::new();
                let sampler = TransientModelSampler {
                    materials: &materials,
                    bindings: &bindings,
                    coefficients: &coefficients,
                    parameters: &params,
                    source: HashMap::new(),
                    source_by_cell: false,
                };
                sampler
                    .coefficients(SampleLocation {
                        cell: 0,
                        material_tag: 1,
                        point: [0.; 3],
                        time: 0.,
                    })
                    .unwrap_or_else(|error| panic!("{profile}/{formulation}: {error}"));
            }
        }
    }

    #[test]
    fn sampled_model_drives_sparse_transient_assembly_in_both_dimensions() {
        use crate::transient_mesh::{NodalCell, NodalMesh};
        use crate::transient_plan::{StateLayout, TransientAssemblyWorkspace};
        use crate::transient_step::LinearOptions;
        for (dimension, profile, text) in [
            (2, "acoustic-2d", include_str!("scenarios/acoustic2d.json")),
            (3, "acoustic-3d", include_str!("scenarios/acoustic3d.json")),
        ] {
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let declaration = &package["transientForms"]["pressure"];
            let plan = TransientPlan::from_json(declaration).unwrap();
            let parameters = HashMap::from([
                (
                    "rho0".into(),
                    Scalar {
                        value: 2.,
                        dimension: model_expression::unit("kg/m^3").unwrap().1,
                    },
                ),
                (
                    "tau".into(),
                    Scalar {
                        value: 2.,
                        dimension: model_expression::unit("s").unwrap().1,
                    },
                ),
            ]);
            let selections = crate::selection_plan::SelectionPlan::from_json(
                &package["transientSelectionPlan"],
            )
            .unwrap();
            let model = json!({"model":{"materials":[{"id":"air","selectionId":"domain",
                "density":{"expression":"rho0*(1+t/tau)","unit":"kg/m^3"}}],"physics":{
                "source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1","unit":"Hz"}]}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![]),
            ]);
            let (data, _) = TransientModelData::bind_selections(
                profile,
                &plan,
                declaration,
                &package["materialBindings"]["pressure"],
                &selections,
                &model,
                &map,
                &tags,
            )
            .unwrap();
            let sampler = data.sampler(&parameters);
            let nodes = if dimension == 2 {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            } else {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            };
            let cells = [NodalCell {
                nodes: (0..nodes.len()).collect(),
                tag: 7,
            }];
            let mesh = NodalMesh {
                dimension,
                nodes: &nodes,
                cells: &cells,
            };
            let layout = StateLayout::new(&plan, &[nodes.len(), dimension * nodes.len()]).unwrap();
            let sources = data.sources();
            let mut workspace = TransientAssemblyWorkspace::default();
            let grid = crate::transient_request::TimeGrid {
                start: 0.,
                end: 1.,
                max_step: 0.25,
                max_steps: 8,
                output_times: vec![1.],
            };
            let solution = crate::transient_runner::run_nodal(
                &plan,
                &layout,
                &mesh,
                &grid,
                "standard",
                &parameters,
                &HashMap::new(),
                None,
                &mut workspace,
                &LinearOptions {
                    tolerance: 1e-10,
                    backend: "faer",
                    ordering: "amd",
                    threads: 1,
                },
                &mut crate::solver_common::SolverCache::default(),
                |location| sampler.coefficients(location),
                &sources,
                |feature_type, location| sampler.source_for(feature_type, location),
                |_, _| Ok(()),
                |_, _, _| Ok(()),
            )
            .unwrap();
            let expected = 3. * 343_f64.powi(2);
            for value in &solution[..nodes.len()] {
                assert!(
                    (value.re - expected).abs() < 1e-9 * expected,
                    "dimension {dimension}: {} vs {expected}",
                    value.re
                );
            }
            let node_map = (0..nodes.len())
                .map(|node| if node == 0 { 0 } else { node - 1 })
                .collect::<Vec<_>>();
            let weights = vec![C { re: 1., im: 0. }; nodes.len()];
            let (map, weights, count) = crate::transient_mesh::nodal_constraint_map(
                &plan,
                &layout,
                &node_map,
                &weights,
                nodes.len() - 1,
            )
            .unwrap();
            let mapping = crate::transient_step::FixedDofMap {
                representative: &map,
                weights: &weights,
                count,
            };
            let constrained = crate::transient_runner::run_nodal(
                &plan,
                &layout,
                &mesh,
                &grid,
                "standard",
                &parameters,
                &HashMap::new(),
                Some(&mapping),
                &mut workspace,
                &LinearOptions {
                    tolerance: 1e-10,
                    backend: "faer",
                    ordering: "amd",
                    threads: 1,
                },
                &mut crate::solver_common::SolverCache::default(),
                |location| sampler.coefficients(location),
                &sources,
                |feature_type, location| sampler.source_for(feature_type, location),
                |_, _| Ok(()),
                |_, _, _| Ok(()),
            )
            .unwrap();
            for value in &constrained[..nodes.len()] {
                assert!((value.re - expected).abs() < 1e-9 * expected);
            }
            let pressure = crate::transient_mesh::nodal_field_components(
                &plan,
                &layout,
                &mesh,
                &constrained,
                0,
            )
            .unwrap();
            assert_eq!(pressure.len(), 1);
            assert_eq!(pressure[0].len(), nodes.len());
            assert!((pressure[0][0].re - expected).abs() < 1e-9 * expected);
            let fields = (0..plan.fields.len())
                .map(|field| {
                    crate::transient_mesh::nodal_field_components(
                        &plan,
                        &layout,
                        &mesh,
                        &constrained,
                        field,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let result_plan = crate::transient_result::StateOutputPlan::from_json(
                &plan,
                &declaration["stateOutputs"],
            )
            .unwrap();
            let mut outputs = result_plan.evaluate(&fields).unwrap();
            assert!((outputs["p"][0].re - expected).abs() < 1e-9 * expected);
            assert!(outputs["vx"][0].re.abs() < 1e-9);
            let declarations =
                crate::model_schema::field_variables_for_profile(profile, Some("pressure"), false)
                    .unwrap();
            crate::transient_result::add_declared_zero_fields(
                &declarations,
                &mut outputs,
                nodes.len(),
            )
            .unwrap();
            let material_cells = vec![(cells[0].nodes.clone(), cells[0].tag)];
            let materials = data
                .material_fields(&nodes, &material_cells, &parameters, 1.)
                .unwrap();
            assert_eq!(materials["rho"]["real"][0][0], 3.);
            let snapshot = crate::transient_result::field_snapshot(
                "study",
                0,
                0,
                1.,
                dimension,
                &nodes,
                &material_cells,
                &declarations,
                &outputs,
                Some(&materials),
            )
            .unwrap();
            assert_eq!(snapshot["materialFieldsByCell"]["rho"]["real"][0][0], 3.);
        }
    }

    #[test]
    fn declared_material_and_cell_selected_source_follow_space_and_time() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let declaration = &package["transientForms"]["pressure"];
        let source_declaration = declaration.get("domainSources").and_then(serde_json::Value::as_array)
            .and_then(|routes| routes.iter().find(|route| route["featureType"] == "domainSource"))
            .unwrap_or(&declaration["domainSource"]);
        let plan = TransientPlan::from_json(declaration).unwrap();
        let coefficients =
            TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
        let bindings = crate::material_bindings::MaterialBindings::from_json(
            &package["materialBindings"]["pressure"],
        )
        .unwrap();
        let materials = CompiledMaterialAssignments::compile(
            "acoustic-2d",
            &HashMap::from([(
                7,
                json!({
                    "id":"air", "density":{"expression":"rho0*(1+t/tau+x/L)","unit":"kg/m^3"}
                }),
            )]),
        )
        .unwrap();
        let params = HashMap::from([
            (
                "rho0".into(),
                Scalar {
                    value: 2.,
                    dimension: model_expression::unit("kg/m^3").unwrap().1,
                },
            ),
            (
                "tau".into(),
                Scalar {
                    value: 2.,
                    dimension: model_expression::unit("s").unwrap().1,
                },
            ),
            (
                "L".into(),
                Scalar {
                    value: 2.,
                    dimension: model_expression::unit("m").unwrap().1,
                },
            ),
        ]);
        let source_plan = DomainSourcePlan::from_json(source_declaration, 2, 1).unwrap();
        let row = json!({"id":"source","values":[{"expression":"t/tau","unit":"Hz"}]});
        let sources = source_plan
            .bind(&[BoundSelection {
                tag: 1,
                selection_id: "selected",
                row: &row,
            }])
            .unwrap();
        let sampler = TransientModelSampler {
            materials: &materials,
            bindings: &bindings,
            coefficients: &coefficients,
            parameters: &params,
            source: HashMap::from([("domainSource".to_owned(), (&source_plan, &sources))]),
            source_by_cell: true,
        };
        let location = SampleLocation {
            cell: 1,
            material_tag: 7,
            point: [2., 0., 0.],
            time: 2.,
        };
        match sampler
            .coefficients(location)
            .unwrap()
            .get("density")
            .unwrap()
        {
            SampledCoefficient::Scalar(value) => assert!((value.re - 6.).abs() < 1e-12),
            _ => panic!("density must be scalar"),
        }
        assert!((sampler.source(location).unwrap()[0].re - 1.).abs() < 1e-12);
        assert_eq!(
            sampler
                .source(SampleLocation {
                    cell: 0,
                    ..location
                })
                .unwrap()[0]
                .re,
            0.
        );
    }
}

impl TransientModelSampler<'_> {
    fn symbols(&self, location: SampleLocation) -> Result<HashMap<String, C>, String> {
        let raw = self.materials.evaluate_at_time(
            location.material_tag,
            self.parameters,
            location.time,
            location.point,
            &format!("cell {}", location.cell),
        )?;
        let mut context = self.parameters.clone();
        if context.contains_key("t") {
            return Err("Parameter t conflicts with transient time.".into());
        }
        context.insert(
            "t".into(),
            Scalar {
                value: location.time,
                dimension: model_expression::unit("s")?.1,
            },
        );
        for (name, value) in ["x", "y", "z"].into_iter().zip(location.point) {
            context.insert(
                name.into(),
                Scalar {
                    value,
                    dimension: model_expression::unit("m")?.1,
                },
            );
        }
        let mut quantities = context.clone();
        quantities.extend(raw.iter().map(|(name, value)| (name.clone(), *value)));
        quantities.extend(
            self.bindings
                .evaluate(&raw, &context, "transient material")?,
        );
        Ok(quantities
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    C {
                        re: value.value,
                        im: 0.,
                    },
                )
            })
            .collect())
    }

    pub(crate) fn coefficients(
        &self,
        location: SampleLocation,
    ) -> Result<HashMap<String, SampledCoefficient>, String> {
        self.coefficients.evaluate(&self.symbols(location)?)
    }

    pub(crate) fn source(&self, location: SampleLocation) -> Result<Vec<C>, String> {
        self.source_for("domainSource", location)
    }

    pub(crate) fn source_for(&self, feature_type: &str, location: SampleLocation) -> Result<Vec<C>, String> {
        let (plan, sources) = self.source.get(feature_type)
            .ok_or_else(|| format!("Transient source route {feature_type:?} was not bound."))?;
        let key = if self.source_by_cell {
            location.cell
        } else {
            location.material_tag
        };
        crate::domain_source::sample_load_at_time(
            plan,
            sources,
            key,
            location.point,
            self.parameters,
            location.time,
            &self.symbols(location)?,
        )
    }
}
