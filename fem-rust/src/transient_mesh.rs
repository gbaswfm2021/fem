//! Generic nodal P1 simplex adapter for internal transient forms.
//! Field coefficients are supplied by the caller at each physical quadrature
//! point and time; this module does not know scenario or material identities.
use crate::solver_common::C;
use crate::transient_plan::{
    SampleLocation, SampledCoefficient, StateLayout, TransientAssemblyWorkspace, TransientPlan,
    TransientSource,
};
use crate::transient_step::TransientOperators;
use crate::weak_form::NodalP1Samples;
use std::collections::HashMap;

pub(crate) struct NodalCell {
    pub(crate) nodes: Vec<usize>,
    pub(crate) tag: usize,
}
pub(crate) struct NodalMesh<'a> {
    pub(crate) dimension: usize,
    pub(crate) nodes: &'a [[f64; 3]],
    pub(crate) cells: &'a [NodalCell],
}

/// Lift one mesh-node constraint map to the declared mixed nodal state.
/// This only constructs an algebraic map; selecting and matching periodic
/// mesh entities remains the responsibility of the common model adapter.
pub(crate) fn nodal_constraint_map(
    plan: &TransientPlan,
    layout: &StateLayout,
    representative: &[usize],
    weights: &[C],
    count: usize,
) -> Result<(Vec<usize>, Vec<C>, usize), String> {
    let node_map = crate::constraints::DofBlock {
        representative,
        weights,
        count,
    };
    let mut fields = Vec::with_capacity(plan.fields.len());
    for (index, field) in plan.fields.iter().enumerate() {
        let expected = representative
            .len()
            .checked_mul(field.components)
            .ok_or("Nodal constraint layout overflow.")?;
        if layout.field_count(index) != Some(expected) {
            return Err(format!(
                "Constraint map differs from nodal field {:?}.",
                field.id
            ));
        }
        fields.push(crate::constraints::interleaved_field_map(
            &node_map,
            field.components,
        )?);
    }
    let blocks = fields
        .iter()
        .map(
            |(representative, weights, count)| crate::constraints::DofBlock {
                representative,
                weights,
                count: *count,
            },
        )
        .collect::<Vec<_>>();
    crate::constraints::join_field_maps(&blocks)
}

/// Recover components of a nodal state in mesh-node order. Output labels and
/// physical units are supplied by the scenario's result declaration.
pub(crate) fn nodal_field_components(
    plan: &TransientPlan,
    layout: &StateLayout,
    mesh: &NodalMesh<'_>,
    state: &[C],
    field: usize,
) -> Result<Vec<Vec<C>>, String> {
    let definition = plan
        .fields
        .get(field)
        .ok_or("Transient result field index is out of range.")?;
    let values = layout.field_state(field, state)?;
    if values.len()
        != mesh
            .nodes
            .len()
            .checked_mul(definition.components)
            .ok_or("Transient field result size overflow.")?
    {
        return Err(format!(
            "Nodal result layout differs from field {:?}.",
            definition.id
        ));
    }
    let mut components = vec![Vec::with_capacity(mesh.nodes.len()); definition.components];
    for node in values.chunks_exact(definition.components) {
        for (component, value) in components.iter_mut().zip(node) {
            component.push(*value);
        }
    }
    Ok(components)
}

/// Interpolate a declared nodal field at an integration point. The entity
/// may be a cell, face or edge; its barycentric weights and node ordering are
/// supplied by the integration engine.
pub(crate) fn nodal_field_at_weights(
    plan:&TransientPlan, layout:&StateLayout, mesh:&NodalMesh<'_>, state:&[C],
    field:usize, nodes:&[usize], weights:&[f64],
) -> Result<Vec<C>,String> {
    let definition=plan.fields.get(field).ok_or("Transient result field index is out of range.")?;
    let values=layout.field_state(field,state)?;
    if layout.field_count(field)!=mesh.nodes.len().checked_mul(definition.components) {
        return Err(format!("Nodal result layout differs from field {:?}.",definition.id));
    }
    if nodes.is_empty() || nodes.len()!=weights.len() || nodes.len()>mesh.dimension+1
        || nodes.iter().any(|&node|node>=mesh.nodes.len())
        || weights.iter().any(|&weight|!weight.is_finite()||!(-1e-9..=1.+1e-9).contains(&weight))
        || (weights.iter().sum::<f64>()-1.).abs()>1e-9 {
        return Err("Transient interpolation entity or barycentric weights are invalid.".into());
    }
    let mut output=vec![C::default();definition.components];
    for (&node,&weight) in nodes.iter().zip(weights) {
        for component in 0..definition.components {
            let value=values[node*definition.components+component];
            if !value.re.is_finite()||!value.im.is_finite() {return Err("Transient result state contains a non-finite coefficient.".into());}
            output[component].re+=weight*value.re;output[component].im+=weight*value.im;
        }
    }
    Ok(output)
}

/// Assemble all declared bilinear terms, keeping global operators sparse.
/// Every field in this adapter uses the same mesh's nodal P1 basis. Boundary
/// integration and constraints are added by separate generic paths.
pub(crate) fn assemble_nodal_p1(
    plan: &TransientPlan,
    layout: &StateLayout,
    mesh: &NodalMesh<'_>,
    time: f64,
    quadrature: &str,
    workspace: &mut TransientAssemblyWorkspace,
    mut coefficient: impl FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
    sources: &[TransientSource<'_>],
    mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
) -> Result<TransientOperators, String> {
    let dim = mesh.dimension;
    if plan.dimension != dim
        || !(2..=3).contains(&dim)
        || !time.is_finite()
        || mesh.nodes.is_empty()
        || mesh.cells.is_empty()
        || mesh.nodes.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("Transient nodal mesh dimension, geometry or time is invalid.".into());
    }
    if dim == 2 && mesh.nodes.iter().any(|point| point[2] != mesh.nodes[0][2]) {
        return Err("Transient 2D nodal mesh must lie in one constant-z plane.".into());
    }
    for (field, definition) in plan.fields.iter().enumerate() {
        let count = mesh
            .nodes
            .len()
            .checked_mul(definition.components)
            .ok_or("Transient field DOF count overflow.")?;
        if layout.field_count(field) != Some(count) {
            return Err(format!(
                "Transient nodal layout differs from field {:?}.",
                definition.id
            ));
        }
    }
    let mut elements = Vec::with_capacity(mesh.cells.len());
    for cell in mesh.cells {
        if cell.nodes.len() != dim + 1 || cell.nodes.iter().any(|&node| node >= mesh.nodes.len()) {
            return Err("Transient simplex cell has invalid node connectivity.".into());
        }
        let mut element = Vec::new();
        for (field, definition) in plan.fields.iter().enumerate() {
            let local: Vec<_> = cell
                .nodes
                .iter()
                .flat_map(|&node| {
                    (0..definition.components).map(move |c| node * definition.components + c)
                })
                .collect();
            element.extend(layout.global_dofs(field, &local)?);
        }
        elements.push(element);
    }
    workspace.prepare(plan, layout, &elements)?;
    let mut load = if sources.is_empty() {
        None
    } else {
        Some(TransientOperators {
            derivatives: vec![],
            operator: vec![],
            load: vec![C::default(); layout.dimension],
            essential: vec![],
            essential_rates: vec![],
        })
    };
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let (measure, gradients, points): (f64, Vec<Vec<f64>>, Vec<(Vec<f64>, f64)>) = if dim == 2 {
            let (area, gradient) = crate::mesh2d::triangle_geometry([
                mesh.nodes[cell.nodes[0]],
                mesh.nodes[cell.nodes[1]],
                mesh.nodes[cell.nodes[2]],
            ])?;
            let q = crate::quadrature::triangle(quadrature)?
                .into_iter()
                .map(|v| (v[..3].to_vec(), v[3]))
                .collect();
            (area, gradient.iter().map(|v| v.to_vec()).collect(), q)
        } else {
            let (volume, gradient) = crate::simplex::tetra_p1(
                [
                    mesh.nodes[cell.nodes[0]],
                    mesh.nodes[cell.nodes[1]],
                    mesh.nodes[cell.nodes[2]],
                    mesh.nodes[cell.nodes[3]],
                ],
                crate::simplex::DeterminantTolerance::Absolute(0.0),
            )?;
            let q = crate::quadrature::tetrahedron(quadrature)?
                .into_iter()
                .map(|v| (v[..4].to_vec(), v[4]))
                .collect();
            (volume, gradient.iter().map(|v| v.to_vec()).collect(), q)
        };
        for (bary, normalized_weight) in points {
            let mut point = [0.; 3];
            for (i, &node) in cell.nodes.iter().enumerate() {
                for axis in 0..3 {
                    point[axis] += bary[i] * mesh.nodes[node][axis];
                }
            }
            let bases = plan
                .fields
                .iter()
                .map(|field| NodalP1Samples::new(dim, field.components, &bary, &gradients))
                .collect::<Result<Vec<_>, _>>()?;
            let basis_views: Vec<_> = bases.iter().map(|basis| basis.samples()).collect();
            let locals: Vec<Vec<usize>> = plan
                .fields
                .iter()
                .map(|field| {
                    cell.nodes
                        .iter()
                        .flat_map(|&node| {
                            (0..field.components).map(move |c| node * field.components + c)
                        })
                        .collect()
                })
                .collect();
            let location = SampleLocation {
                cell: cell_index,
                material_tag: cell.tag,
                point,
                time,
            };
            let coefficients = coefficient(location)?;
            plan.assemble_quadrature(
                layout,
                workspace,
                &basis_views,
                &locals,
                measure * normalized_weight,
                &coefficients,
            )?;
            if let Some(accumulator) = load.as_mut() {
                plan.assemble_sources(
                    layout,
                    &basis_views,
                    &locals,
                    location,
                    measure * normalized_weight,
                    sources,
                    &mut source_value,
                    accumulator,
                )?;
            }
        }
    }
    let mut result = workspace.rows()?;
    if let Some(accumulator) = load {
        result.load = accumulator.load;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weak_form::VolumeLoadPlan;
    use serde_json::json;
    #[test]
    fn result_interpolation_uses_entity_weights_and_declared_components() {
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"scalar","components":1},{"id":"vector","components":2}],
            "terms":[{"testField":"scalar","trialField":"scalar","test":"value","trial":"value","coefficient":"one","timeDerivative":"state"},
                     {"testField":"vector","trialField":"vector","test":"value","trial":"value","coefficient":"one","timeDerivative":"state"}]})).unwrap();
        let nodes=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]];
        let cells=[NodalCell{nodes:vec![0,1,2],tag:1}];
        let mesh=NodalMesh{dimension:2,nodes:&nodes,cells:&cells};
        let layout=StateLayout::new(&plan,&[3,6]).unwrap();
        let state=[1.,2.,4.,10.,20.,30.,40.,50.,60.].map(|re|C{re,im:2.*re});
        let value=nodal_field_at_weights(&plan,&layout,&mesh,&state,1,&[1,2],&[0.25,0.75]).unwrap();
        assert_eq!(value[0].re,45.);assert_eq!(value[1].re,55.);assert_eq!(value[0].im,90.);
        let scalar=nodal_field_at_weights(&plan,&layout,&mesh,&state,0,&[0,1,2],&[0.2,0.3,0.5]).unwrap();
        assert!((scalar[0].re-2.8).abs()<1e-12);
        assert!(nodal_field_at_weights(&plan,&layout,&mesh,&state,0,&[0,1],&[0.2,0.2]).is_err());
        assert!(nodal_field_at_weights(&plan,&layout,&mesh,&state,0,&[0,3],&[0.5,0.5]).is_err());
    }
    #[test]
    fn mixed_nodal_constraint_map_uses_declared_components() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let plan = TransientPlan::from_json(&package["transientForms"]["pressure"]).unwrap();
        let layout = StateLayout::new(&plan, &[3, 6]).unwrap();
        let unit = C { re: 1., im: 0. };
        let (map, weights, count) =
            nodal_constraint_map(&plan, &layout, &[0, 1, 0], &[unit; 3], 2).unwrap();
        assert_eq!(map, vec![0, 1, 0, 2, 3, 4, 5, 2, 3]);
        assert_eq!(weights.len(), layout.dimension);
        assert_eq!(count, 6);
    }
    #[test]
    fn scalar_triangle_samples_spatial_and_temporal_coefficients_in_sparse_pattern() {
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":1}],"terms":[{"testField":"u","trialField":"u",
                "test":"value","trial":"value","coefficient":"c","timeDerivative":"conservedProduct"}]})).unwrap();
        let layout = StateLayout::new(&plan, &[3]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let cells = [NodalCell {
            nodes: vec![0, 1, 2],
            tag: 7,
        }];
        let mesh = NodalMesh {
            dimension: 2,
            nodes: &nodes,
            cells: &cells,
        };
        let mut workspace = TransientAssemblyWorkspace::default();
        let mut sample = |location: SampleLocation| {
            let SampleLocation {
                cell,
                material_tag: tag,
                point,
                time,
            } = location;
            assert_eq!((cell, tag), (0, 7));
            Ok(HashMap::from([(
                "c".into(),
                SampledCoefficient::Scalar(C {
                    re: 1. + point[0] + time,
                    im: 0.,
                }),
            )]))
        };
        let a = assemble_nodal_p1(
            &plan,
            &layout,
            &mesh,
            0.,
            "standard",
            &mut workspace,
            &mut sample,
            &[],
            |_, _| unreachable!(),
        )
        .unwrap();
        let b = assemble_nodal_p1(
            &plan,
            &layout,
            &mesh,
            1.,
            "standard",
            &mut workspace,
            &mut sample,
            &[],
            |_, _| unreachable!(),
        )
        .unwrap();
        let total = |operator: &TransientOperators| {
            operator.derivatives[0]
                .1
                .iter()
                .flat_map(|r| r.values())
                .map(|v| v.re)
                .sum::<f64>()
        };
        assert!((total(&a) - 2. / 3.).abs() < 1e-12);
        assert!((total(&b) - 7. / 6.).abs() < 1e-12);
        let form =
            VolumeLoadPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"test":"value"}))
                .unwrap();
        let sources = [TransientSource {
            id: "source",
            field: 0,
            form: &form,
        }];
        let loaded = assemble_nodal_p1(
            &plan,
            &layout,
            &mesh,
            1.,
            "standard",
            &mut workspace,
            &mut sample,
            &sources,
            |name, location: SampleLocation| {
                let SampleLocation {
                    material_tag: tag,
                    point,
                    time,
                    ..
                } = location;
                assert_eq!((name, tag), ("source", 7));
                Ok(vec![C {
                    re: 1. + point[0] + time,
                    im: 0.,
                }])
            },
        )
        .unwrap();
        assert!((loaded.load.iter().map(|v| v.re).sum::<f64>() - 7. / 6.).abs() < 1e-12);
    }

    #[test]
    fn tetrahedron_p1_assembles_volume_without_dense_global_storage() {
        let plan =
            TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":3,
            "fields":[{"id":"u","components":1}],"terms":[{"testField":"u","trialField":"u",
                "test":"value","trial":"value","coefficient":"c","timeDerivative":"state"}]}))
            .unwrap();
        let layout = StateLayout::new(&plan, &[4]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let cells = [NodalCell {
            nodes: vec![0, 1, 2, 3],
            tag: 1,
        }];
        let mesh = NodalMesh {
            dimension: 3,
            nodes: &nodes,
            cells: &cells,
        };
        let mut workspace = TransientAssemblyWorkspace::default();
        let result = assemble_nodal_p1(
            &plan,
            &layout,
            &mesh,
            0.,
            "standard",
            &mut workspace,
            |_| {
                Ok(HashMap::from([(
                    "c".into(),
                    SampledCoefficient::Scalar(C { re: 1., im: 0. }),
                )]))
            },
            &[],
            |_, _| unreachable!(),
        )
        .unwrap();
        let total = result.derivatives[0]
            .1
            .iter()
            .flat_map(|row| row.values())
            .map(|v| v.re)
            .sum::<f64>();
        assert!((total - 1. / 6.).abs() < 1e-12);
    }

    #[test]
    fn vector_triangle_tensor_form_matches_existing_generic_kernel() {
        use crate::weak_form::{Coefficient, FormPlan};
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":2}],"terms":[{"testField":"u","trialField":"u",
                "test":"symmetricGradient","trial":"symmetricGradient","coefficient":"C","timeDerivative":"none"}]})).unwrap();
        let layout = StateLayout::new(&plan, &[6]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let cells = [NodalCell {
            nodes: vec![0, 1, 2],
            tag: 1,
        }];
        let mesh = NodalMesh {
            dimension: 2,
            nodes: &nodes,
            cells: &cells,
        };
        let mut tensor = vec![C::default(); 16];
        for i in 0..4 {
            tensor[4 * i + i] = C { re: 1., im: 0. };
        }
        let mut workspace = TransientAssemblyWorkspace::default();
        let assembled = assemble_nodal_p1(
            &plan,
            &layout,
            &mesh,
            0.,
            "standard",
            &mut workspace,
            |_| {
                Ok(HashMap::from([(
                    "C".into(),
                    SampledCoefficient::Tensor {
                        rows: 4,
                        columns: 4,
                        values: tensor.clone(),
                    },
                )]))
            },
            &[],
            |_, _| unreachable!(),
        )
        .unwrap();
        let reference = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,
            "terms":[{"test":"symmetricGradient","trial":"symmetricGradient","coefficient":"C"}]}))
        .unwrap();
        let coefficients = HashMap::from([(
            "C".to_string(),
            Coefficient::Matrix {
                rows: 4,
                columns: 4,
                values: &tensor,
            },
        )]);
        let mut rows = vec![std::collections::BTreeMap::new(); 6];
        for q in crate::quadrature::triangle("standard").unwrap() {
            crate::vector_form::add_declared_p1_quadrature(
                &reference,
                &mut rows,
                [0, 1, 2],
                [[-1., -1.], [1., 0.], [0., 1.]],
                [q[0], q[1], q[2]],
                0.5 * q[3],
                &coefficients,
            )
            .unwrap();
        }
        for i in 0..6 {
            for j in 0..6 {
                assert!((assembled.operator[i][&j].re - rows[i][&j].re).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn enabled_em2d_form_matches_uniform_conductive_decay() {
        use crate::transient_plan::TransientCoefficients;
        use crate::transient_step::{LinearOptions, integrate_backward_euler};
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let declaration = &package["transientForms"]["Ez"];
        let plan = TransientPlan::from_json(declaration).unwrap();
        let coefficients =
            TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
        let layout = StateLayout::new(&plan, &[3, 6]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let cells = [NodalCell {
            nodes: vec![0, 1, 2],
            tag: 1,
        }];
        let mesh = NodalMesh {
            dimension: 2,
            nodes: &nodes,
            cells: &cells,
        };
        let mut workspace = TransientAssemblyWorkspace::default();
        let inputs = HashMap::from([
            (
                "eps_r".into(),
                C {
                    re: 1. / 8.8541878128e-12,
                    im: 0.,
                },
            ),
            (
                "mu_r".into(),
                C {
                    re: 1. / 1.25663706212e-6,
                    im: 0.,
                },
            ),
            ("sigma".into(), C { re: 1., im: 0. }),
        ]);
        let mut initial = vec![C { re: 0., im: 0. }; 9];
        for value in &mut initial[..3] {
            value.re = 1.;
        }
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = integrate_backward_euler(
            0.,
            1.,
            0.25,
            10,
            &initial,
            &[1.],
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |time| {
                assemble_nodal_p1(
                    &plan,
                    &layout,
                    &mesh,
                    time,
                    "standard",
                    &mut workspace,
                    |_| coefficients.evaluate(&inputs),
                    &[],
                    |_, _| unreachable!(),
                )
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        let expected = 1.0_f64 / 1.25_f64.powi(4);
        for value in &result[..3] {
            assert!((value.re - expected).abs() < 1e-9);
        }
        for value in &result[3..] {
            assert!(value.re.abs() < 1e-9);
        }
    }

    #[test]
    fn acoustic_forms_drive_uniform_pressure_with_time_varying_material() {
        use crate::transient_plan::TransientCoefficients;
        use crate::transient_step::{LinearOptions, integrate_backward_euler};
        let files = [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
        ];
        for (index, text) in files.iter().enumerate() {
            let dim = index + 2;
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let declaration = &package["transientForms"]["pressure"];
            let source_declaration = declaration.get("domainSources").and_then(serde_json::Value::as_array)
                .and_then(|routes| routes.iter().find(|route| route["featureType"] == "domainSource"))
                .unwrap_or(&declaration["domainSource"]);
            let plan = TransientPlan::from_json(declaration).unwrap();
            let coefficients =
                TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
            let nodes = if dim == 2 {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            } else {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            };
            let cells = [NodalCell {
                nodes: (0..nodes.len()).collect(),
                tag: 1,
            }];
            let mesh = NodalMesh {
                dimension: dim,
                nodes: &nodes,
                cells: &cells,
            };
            let layout = StateLayout::new(&plan, &[nodes.len(), nodes.len() * dim]).unwrap();
            let source_plan = crate::domain_source::DomainSourcePlan::from_json(
                source_declaration,
                dim,
                1,
            )
            .unwrap();
            let row = json!({"id":"source","values":[{"expression":"1","unit":"Hz"}]});
            let sources_bound = source_plan
                .bind(&[crate::model_bindings::BoundSelection {
                    tag: 1,
                    selection_id: "domain",
                    row: &row,
                }])
                .unwrap();
            let sources = [TransientSource {
                id: "injection",
                field: 0,
                form: &source_plan.form,
            }];
            let options = LinearOptions {
                tolerance: 1e-10,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut workspace = TransientAssemblyWorkspace::default();
            let result = integrate_backward_euler(
                0.,
                1.,
                0.25,
                10,
                &vec![C::default(); layout.dimension],
                &[1.],
                &options,
                &mut crate::solver_common::SolverCache::default(),
                |time| {
                    assemble_nodal_p1(
                        &plan,
                        &layout,
                        &mesh,
                        time,
                        "standard",
                        &mut workspace,
                        |location| {
                            coefficients.evaluate(&HashMap::from([
                                (
                                    "rho".into(),
                                    C {
                                        re: 1. + location.time,
                                        im: 0.,
                                    },
                                ),
                                ("c".into(), C { re: 1., im: 0. }),
                            ]))
                        },
                        &sources,
                        |_, location| {
                            crate::domain_source::sample_load_at_time(
                                &source_plan,
                                &sources_bound,
                                location.material_tag,
                                location.point,
                                &HashMap::new(),
                                location.time,
                                &HashMap::new(),
                            )
                        },
                    )
                },
                |_, _, _| Ok(()),
            )
            .unwrap();
            for value in &result[..nodes.len()] {
                assert!((value.re - 2.).abs() < 1e-9);
            }
            for value in &result[nodes.len()..] {
                assert!(value.re.abs() < 1e-9);
            }
        }
    }

    #[test]
    fn elastic_forms_drive_uniform_acceleration_with_variable_density() {
        use crate::transient_plan::TransientCoefficients;
        use crate::transient_step::{LinearOptions, integrate_backward_euler};
        let cases = [
            (include_str!("scenarios/elastic2d.json"), "planeStress", 2),
            (include_str!("scenarios/elastic2d.json"), "planeStrain", 2),
            (include_str!("scenarios/elastic3d.json"), "displacement", 3),
        ];
        for (text, formulation, dim) in cases {
            let package: serde_json::Value = serde_json::from_str(text).unwrap();
            let declaration = &package["transientForms"][formulation];
            let source_declaration = declaration.get("domainSources").and_then(serde_json::Value::as_array)
                .and_then(|routes| routes.iter().find(|route| route["featureType"] == "domainSource"))
                .unwrap_or(&declaration["domainSource"]);
            let plan = TransientPlan::from_json(declaration).unwrap();
            let coefficients =
                TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
            let nodes = if dim == 2 {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            } else {
                vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            };
            let cells = [NodalCell {
                nodes: (0..nodes.len()).collect(),
                tag: 1,
            }];
            let mesh = NodalMesh {
                dimension: dim,
                nodes: &nodes,
                cells: &cells,
            };
            let field_dofs = nodes.len() * dim;
            let layout = StateLayout::new(&plan, &[field_dofs, field_dofs]).unwrap();
            let source_plan = crate::domain_source::DomainSourcePlan::from_json(
                source_declaration,
                dim,
                dim,
            )
            .unwrap();
            let values: Vec<_> = (0..dim)
                .map(|i| json!({"expression":if i==0 {"1"} else {"0"},"unit":"N/m^3"}))
                .collect();
            let row = json!({"id":"body-force","values":values});
            let bound = source_plan
                .bind(&[crate::model_bindings::BoundSelection {
                    tag: 1,
                    selection_id: "domain",
                    row: &row,
                }])
                .unwrap();
            let sources = [TransientSource {
                id: "force",
                field: 1,
                form: &source_plan.form,
            }];
            let options = LinearOptions {
                tolerance: 1e-10,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut workspace = TransientAssemblyWorkspace::default();
            let result = integrate_backward_euler(
                0.,
                1.,
                0.25,
                10,
                &vec![C::default(); layout.dimension],
                &[1.],
                &options,
                &mut crate::solver_common::SolverCache::default(),
                |time| {
                    assemble_nodal_p1(
                        &plan,
                        &layout,
                        &mesh,
                        time,
                        "standard",
                        &mut workspace,
                        |location| {
                            coefficients.evaluate(&HashMap::from([
                                (
                                    "rho".into(),
                                    C {
                                        re: 1. + location.time,
                                        im: 0.,
                                    },
                                ),
                                ("young".into(), C { re: 1., im: 0. }),
                                ("nu".into(), C { re: 0.25, im: 0. }),
                            ]))
                        },
                        &sources,
                        |_, location| {
                            crate::domain_source::sample_load_at_time(
                                &source_plan,
                                &bound,
                                location.material_tag,
                                location.point,
                                &HashMap::new(),
                                location.time,
                                &HashMap::new(),
                            )
                        },
                    )
                },
                |_, _, _| Ok(()),
            )
            .unwrap();
            let expected_u = 0.25 * (0.2 + 1. / 3. + 3. / 7. + 0.5);
            for node in 0..nodes.len() {
                for component in 0..dim {
                    let expected = if component == 0 { expected_u } else { 0. };
                    assert!((result[node * dim + component].re - expected).abs() < 1e-9);
                    let expected_v = if component == 0 { 0.5 } else { 0. };
                    assert!(
                        (result[field_dofs + node * dim + component].re - expected_v).abs() < 1e-9
                    );
                }
            }
        }
    }
}
