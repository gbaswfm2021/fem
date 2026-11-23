//! Fixed, real periodic identifications for all transient field blocks.
//! Mesh correspondence is rebuilt for each prepared sweep case.
use crate::solver_common::C;
use crate::transient_case::BoundTransientCase;
use crate::transient_plan::StateLayout;
use crate::transient_request::PreparedTransientCase;
use crate::transient_step::FixedDofMap;
use serde_json::Value;
use std::collections::HashSet;

fn verify_exterior(
    cells: &[Vec<usize>],
    facets: &[(Vec<usize>, usize)],
    tags: &HashSet<usize>,
    dimension: usize,
) -> Result<(), String> {
    let mut owners = std::collections::HashMap::<Vec<usize>, usize>::new();
    for cell in cells {
        if cell.len() != dimension + 1
            || cell.iter().copied().collect::<HashSet<_>>().len() != cell.len()
        {
            return Err("Transient periodic mesh has invalid cells.".into());
        }
        for excluded in 0..cell.len() {
            let mut face = cell
                .iter()
                .enumerate()
                .filter_map(|(i, &v)| (i != excluded).then_some(v))
                .collect::<Vec<_>>();
            face.sort_unstable();
            *owners.entry(face).or_default() += 1;
        }
    }
    for (vertices, _) in facets.iter().filter(|(_, tag)| tags.contains(tag)) {
        let mut key = vertices.clone();
        key.sort_unstable();
        if key.len() != dimension || owners.get(&key) != Some(&1) {
            return Err("Transient periodic facets must be exterior mesh facets.".into());
        }
    }
    Ok(())
}

pub(crate) struct StateConstraintMap {
    representative: Vec<usize>,
    weights: Vec<C>,
    count: usize,
}
impl StateConstraintMap {
    pub(crate) fn is_identity(&self) -> bool {
        self.count == self.representative.len()
            && self
                .representative
                .iter()
                .enumerate()
                .all(|(i, &value)| value == i)
            && self
                .weights
                .iter()
                .all(|weight| weight.re == 1. && weight.im == 0.)
    }
    pub(crate) fn as_fixed(&self) -> FixedDofMap<'_> {
        FixedDofMap {
            representative: &self.representative,
            weights: &self.weights,
            count: self.count,
        }
    }
}

fn matching(
    case: &BoundTransientCase<'_>,
    parameters: &std::collections::HashMap<String,crate::model_expression::Scalar>,
    model: &Value,
    map: &Value,
    nodes: &[[f64; 3]],
    facets: &[(Vec<usize>, usize)],
    dimension: usize,
) -> Result<crate::scalar2d::PeriodicDofMap, String> {
    if !std::ptr::eq(case.model, model) || !std::ptr::eq(case.map, map) {
        return Err("Transient periodic model or mesh map differs from the bound case.".into());
    }
    let result = crate::scalar2d::periodic_dof_map_for_entities_with_symbols(
        model,
        map,
        nodes,
        facets,
        dimension,
        Some(parameters),
        true,
    )?;
    for pair in &result.pairs {
        let mut lower = HashSet::new();
        let mut upper = HashSet::new();
        for (vertices, tag) in facets
            .iter()
            .filter(|(_, tag)| *tag == pair.lower_tag || *tag == pair.upper_tag)
        {
            if vertices.len() != dimension {
                return Err("Transient periodic facet has the wrong dimension.".into());
            }
            let mut key = if *tag == pair.lower_tag {
                vertices
                    .iter()
                    .map(|node| {
                        pair.node_map
                            .get(node)
                            .copied()
                            .ok_or("Periodic facet has an unmatched node.")
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vertices.clone()
            };
            key.sort_unstable();
            if !(if *tag == pair.lower_tag {
                &mut lower
            } else {
                &mut upper
            })
            .insert(key)
            {
                return Err("Transient periodic side has duplicate facets.".into());
            }
        }
        if lower != upper {
            return Err("Transient periodic sides have incompatible facet connectivity.".into());
        }
    }
    let selected = case
        .selections
        .get("boundaries")
        .ok_or("Transient boundary selection is required.")?;
    let ordinary = selected.iter().map(|row| row.tag).collect::<HashSet<_>>();
    if ordinary
        .iter()
        .any(|tag| result.boundary_tags.contains(tag))
    {
        return Err(
            "A transient periodic boundary also has an ordinary boundary condition.".into(),
        );
    }
    let mut periodic_facets = HashSet::new();
    for (vertices, tag) in facets
        .iter()
        .filter(|(_, tag)| result.boundary_tags.contains(tag))
    {
        let mut key = vertices.clone();
        key.sort_unstable();
        if !periodic_facets.insert(key) {
            return Err(format!("Duplicate transient periodic facet for tag {tag}."));
        }
    }
    for (vertices, _) in facets.iter().filter(|(_, tag)| ordinary.contains(tag)) {
        let mut key = vertices.clone();
        key.sort_unstable();
        if periodic_facets.contains(&key) {
            return Err(
                "A transient periodic facet also has an ordinary boundary condition.".into(),
            );
        }
    }
    Ok(result)
}

pub(crate) fn nodal(
    case: &BoundTransientCase<'_>,
    prepared: &PreparedTransientCase,
    model: &Value,
    map: &Value,
    mesh: &crate::transient_mesh::NodalMesh<'_>,
    facets: &[(Vec<usize>, usize)],
) -> Result<StateConstraintMap, String> {
    nodal_with_parameters(case,&prepared.parameters,model,map,mesh,facets)
}

pub(crate) fn nodal_with_parameters(
    case:&BoundTransientCase<'_>,parameters:&std::collections::HashMap<String,crate::model_expression::Scalar>,
    model:&Value,map:&Value,mesh:&crate::transient_mesh::NodalMesh<'_>,facets:&[(Vec<usize>,usize)],
)->Result<StateConstraintMap,String>{
    let base = matching(
        case,
        parameters,
        model,
        map,
        mesh.nodes,
        facets,
        mesh.dimension,
    )?;
    let cells = mesh
        .cells
        .iter()
        .map(|cell| cell.nodes.clone())
        .collect::<Vec<_>>();
    verify_exterior(&cells, facets, &base.boundary_tags, mesh.dimension)?;
    let counts = case
        .plan
        .fields
        .iter()
        .map(|field| {
            mesh.nodes
                .len()
                .checked_mul(field.components)
                .ok_or("Transient field size overflow.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let layout = StateLayout::new(&case.plan, &counts)?;
    let (representative, weights, count) = crate::transient_mesh::nodal_constraint_map(
        &case.plan,
        &layout,
        &base.node_to_dof,
        &base.node_weights,
        base.dof_count,
    )?;
    Ok(StateConstraintMap {
        representative,
        weights,
        count,
    })
}

pub(crate) fn edge(
    case: &BoundTransientCase<'_>,
    prepared: &PreparedTransientCase,
    model: &Value,
    map: &Value,
    mesh: &crate::transient_edge::EdgeTransientMesh,
    nodes: &[[f64; 3]],
    cells: &[(Vec<usize>, usize)],
    facets: &[(Vec<usize>, usize)],
) -> Result<StateConstraintMap, String> {
    edge_with_parameters(case,&prepared.parameters,model,map,mesh,nodes,cells,facets)
}

pub(crate) fn edge_with_parameters(
    case:&BoundTransientCase<'_>,parameters:&std::collections::HashMap<String,crate::model_expression::Scalar>,
    model:&Value,map:&Value,mesh:&crate::transient_edge::EdgeTransientMesh,nodes:&[[f64;3]],
    cells:&[(Vec<usize>,usize)],facets:&[(Vec<usize>,usize)],
)->Result<StateConstraintMap,String>{
    if !mesh.matches_geometry(nodes, cells) {
        return Err("Transient periodic edge mesh geometry differs from the solved mesh.".into());
    }
    let base = matching(case, parameters, model, map, nodes, facets, 3)?;
    verify_exterior(
        &cells
            .iter()
            .map(|(nodes, _)| nodes.clone())
            .collect::<Vec<_>>(),
        facets,
        &base.boundary_tags,
        3,
    )?;
    let layout = StateLayout::new(&case.plan, &vec![mesh.dof_count(); case.plan.fields.len()])?;
    let (representative, weights, count) =
        mesh.periodic_constraint_map(&case.plan, &layout, &base.pairs, facets)?;
    Ok(StateConstraintMap {
        representative,
        weights,
        count,
    })
}

/// Match periodic triangle traces for the direct-sum Whitney/H1 space, then
/// lift the same oriented map to every contract-declared field block.
pub(crate) fn triangle(
    case:&BoundTransientCase<'_>,parameters:&std::collections::HashMap<String,crate::model_expression::Scalar>,model:&Value,map:&Value,
    mesh:&crate::transient_triangle::TriangleTransientMesh,nodes:&[[f64;3]],
    cells:&[([usize;3],usize)],facets:&[(Vec<usize>,usize)],
)->Result<StateConstraintMap,String>{
    let output_cells=cells.iter().map(|(nodes,tag)|(nodes.to_vec(),*tag)).collect::<Vec<_>>();
    if !mesh.matches_geometry(nodes,&output_cells){return Err("Transient periodic triangle mesh geometry differs from the solved mesh.".into());}
    let base=matching(case,parameters,model,map,nodes,facets,2)?;
    let generic_cells=cells.iter().map(|(nodes,_)|nodes.to_vec()).collect::<Vec<_>>();
    verify_exterior(&generic_cells,facets,&base.boundary_tags,2)?;
    let layout=StateLayout::new(&case.plan,&vec![mesh.dof_count();case.plan.fields.len()])?;
    let mut classes=crate::solver_common::PhaseUnionFind::new(mesh.dof_count());
    for pair in &base.pairs{
        let mut phase:Option<C>=None;
        for (&lower,&upper) in &pair.node_map{
            let lower_weight=*base.node_weights.get(lower).ok_or("Periodic lower triangle node has no phase weight.")?;
            let upper_weight=*base.node_weights.get(upper).ok_or("Periodic upper triangle node has no phase weight.")?;
            let relation=upper_weight.div(lower_weight);
            if let Some(previous)=phase{
                if (previous.re-relation.re).hypot(previous.im-relation.im)>1e-9*previous.re.hypot(previous.im).max(relation.re.hypot(relation.im)).max(1.){
                    return Err("Periodic triangle pair has inconsistent phase factors across its nodes.".into());
                }
            }else{phase=Some(relation);}
            let low=mesh.longitudinal_node_dof(lower).ok_or("Periodic lower longitudinal triangle DOF is missing.")?;
            let high=mesh.longitudinal_node_dof(upper).ok_or("Periodic upper longitudinal triangle DOF is missing.")?;
            classes.constrain(low,high,relation)?;
        }
        let lower_edges=facets.iter().filter(|(_,tag)|*tag==pair.lower_tag).map(|(edge,_)|{
            edge.as_slice().try_into().map_err(|_|"Periodic lower triangle facet must be an edge.".to_string())
        }).collect::<Result<Vec<[usize;2]>,String>>()?;
        let upper_edges=facets.iter().filter(|(_,tag)|*tag==pair.upper_tag).map(|(edge,_)|{
            edge.as_slice().try_into().map_err(|_|"Periodic upper triangle facet must be an edge.".to_string())
        }).collect::<Result<Vec<[usize;2]>,String>>()?.into_iter().map(|mut edge|{edge.sort_unstable();edge}).collect::<HashSet<_>>();
        let mut matched=HashSet::new();
        for mut edge in lower_edges{
            edge.sort_unstable();
            let mapped=[*pair.node_map.get(&edge[0]).ok_or("Periodic lower triangle edge has an unmatched endpoint.")?,
                *pair.node_map.get(&edge[1]).ok_or("Periodic lower triangle edge has an unmatched endpoint.")?];
            let mut target=mapped;target.sort_unstable();
            if !upper_edges.contains(&target){return Err("Periodic triangle edge traces do not match under the node correspondence.".into());}
            if !matched.insert(target){return Err("Periodic correspondence maps multiple lower triangle edges to one upper edge.".into());}
            let low=mesh.edge_dof(edge).ok_or("Periodic lower triangle edge has no Whitney DOF.")?;
            let high=mesh.edge_dof(mapped).ok_or("Periodic upper triangle edge has no Whitney DOF.")?;
            let orientation=if mapped[0]<mapped[1]{1.}else{-1.};
            classes.constrain(low,high,crate::c_scale(phase.unwrap_or(C{re:1.,im:0.}),orientation))?;
        }
        if matched.len()!=upper_edges.len(){return Err("Periodic triangle pair does not match every upper edge.".into());}
    }
    let (representative,weights,count)=classes.mapping();
    let (representative,weights,count)=mesh.constraint_map(&case.plan,&layout,&representative,&weights,count)?;
    Ok(StateConstraintMap{representative,weights,count})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transient_mesh::{NodalCell, NodalMesh};
    use crate::transient_step::LinearOptions;
    use serde_json::json;
    fn prepared() -> PreparedTransientCase {
        PreparedTransientCase {
            grid: crate::transient_request::TimeGrid {
                start: 0.,
                end: 1.,
                max_step: 0.5,
                max_steps: 2,
                output_times: vec![1.],
            },
            parameters: std::collections::HashMap::from([(
                "L".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("m").unwrap().1,
                },
            )]),
            overrides: vec![],
            longitudinal_wavenumber: crate::solver_common::C::default(),
        }
    }
    #[test]
    fn nodal_periodicity_uses_case_parameters_and_rejects_phase_and_overlap() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/thermal2d.json")).unwrap();
        let pair = json!({"id":"p","dimension":2,"lowerSelectionId":"lo","upperSelectionId":"hi",
            "translation":[{"expression":"L","unit":"m"},{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]});
        let mut model = json!({"scenario":{"profileId":"thermal-2d"},"model":{"parameters":[],"mesh":{"periodicPairIds":["p"]},
            "materials":[{"id":"material","selectionId":"domain","volumetricHeatCapacity":{"expression":"2","unit":"W*s/(m^3*K)"}}],
            "physics":{"periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"p"},
                "source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1","unit":"W/m^3"}]}}}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"lo","physicalTag":1},{"id":"hi","physicalTag":2}],
            "periodicPairs":[pair],"domainCellSelections":{"domain":[0,1]}});
        let tags = std::collections::HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7, 7]),
            ("boundaries".into(), vec![1, 2]),
        ]);
        let nodes = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
        let cells = [
            NodalCell {
                nodes: vec![0, 1, 2],
                tag: 7,
            },
            NodalCell {
                nodes: vec![0, 2, 3],
                tag: 7,
            },
        ];
        let mesh = NodalMesh {
            dimension: 2,
            nodes: &nodes,
            cells: &cells,
        };
        let facets = vec![(vec![0, 3], 1), (vec![1, 2], 2)];
        let prepared = prepared();
        let bound = BoundTransientCase::prepare(
            &package,
            "thermal-2d",
            "T",
            &model,
            &map,
            &tags,
            &json!({}),
        )
        .unwrap();
        let constraints = nodal(&bound, &prepared, &model, &map, &mesh, &facets).unwrap();
        let fixed = constraints.as_fixed();
        assert_eq!(fixed.representative[0], fixed.representative[1]);
        assert_eq!(fixed.representative[2], fixed.representative[3]);
        let options = LinearOptions {
            tolerance: 1e-9,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = bound
            .run_nodal(
                &prepared,
                &mesh,
                "standard",
                Some(&fixed),
                &options,
                &mut crate::solver_common::SolverCache::default(),
                |_, _, _, _, _| Ok(()),
                |_, _, _, _, _| Ok(()),
            )
            .unwrap();
        assert!(result.iter().all(|v| (v.re - 0.5).abs() < 1e-9));
        let mut changed = self::prepared();
        changed.parameters.get_mut("L").unwrap().value = 2.;
        assert!(nodal(&bound, &changed, &model, &map, &mesh, &facets).is_err());
        let moved_nodes = [[0., 0., 0.], [2., 0., 0.], [2., 1., 0.], [0., 1., 0.]];
        let moved_mesh = NodalMesh {
            dimension: 2,
            nodes: &moved_nodes,
            cells: &cells,
        };
        let rematched = nodal(&bound, &changed, &model, &map, &moved_mesh, &facets).unwrap();
        assert_eq!(
            rematched.as_fixed().representative[0],
            rematched.as_fixed().representative[1]
        );
        let mut phase_model = model.clone();
        phase_model["model"]["physics"]["periodic"]["phaseVector"] = json!([{"expression":"0","unit":"rad/m"},{"expression":"1","unit":"rad/m"},{"expression":"0","unit":"rad/m"}]);
        let phase_bound = BoundTransientCase::prepare(
            &package,
            "thermal-2d",
            "T",
            &phase_model,
            &map,
            &tags,
            &json!({}),
        )
        .unwrap();
        assert!(
            nodal(&phase_bound, &prepared, &phase_model, &map, &mesh, &facets)
                .err()
                .unwrap()
                .contains("zero phase")
        );
        let mut overlap_model = model.clone();
        overlap_model["model"]["physics"]["wall"] = json!({"id":"wall","type":"dirichlet","selectionId":"lo","value":{"expression":"0","unit":"K"}});
        let overlap = BoundTransientCase::prepare(
            &package,
            "thermal-2d",
            "T",
            &overlap_model,
            &map,
            &tags,
            &json!({}),
        )
        .unwrap();
        assert!(
            nodal(&overlap, &prepared, &overlap_model, &map, &mesh, &facets)
                .err()
                .unwrap()
                .contains("ordinary")
        );
    }

    #[test]
    fn oriented_periodicity_matches_first_and_second_order_face_moments() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let pair = json!({"id":"p","dimension":3,"lowerSelectionId":"lo","upperSelectionId":"hi",
            "translation":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"},{"expression":"2*L","unit":"m"}]});
        let model = json!({"scenario":{"profileId":"em-3d"},"model":{"parameters":[],"mesh":{"periodicPairIds":["p"]},
            "materials":[{"id":"material","selectionId":"domain"}],"physics":{"periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"p"}}}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"lo","physicalTag":1},{"id":"hi","physicalTag":2}],"periodicPairs":[pair],"domainCellSelections":{}});
        let tags = std::collections::HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7]),
            ("boundaries".into(), vec![1, 2]),
        ]);
        let nodes = [
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 2.],
            [1., 0., 2.],
            [0., 1., 2.],
            [0., 0., -1.],
            [0., 0., 3.],
        ];
        let cells = vec![(vec![0, 1, 2, 6], 7), (vec![5, 4, 3, 7], 7)];
        let facets = vec![(vec![0, 1, 2], 1), (vec![3, 4, 5], 2)];
        let mut prepared = prepared();
        prepared.grid.end = 1e-9;
        prepared.grid.max_step = 1e-9;
        prepared.grid.max_steps = 1;
        prepared.grid.output_times = vec![1e-9];
        let bound = BoundTransientCase::prepare(
            &package,
            "em-3d",
            "electric",
            &model,
            &map,
            &tags,
            &json!({}),
        )
        .unwrap();
        for order in [1, 2] {
            let mesh = crate::transient_edge::EdgeTransientMesh::prepare(
                &nodes,
                &[([0, 1, 2, 6], 7), ([5, 4, 3, 7], 7)],
                order,
            )
            .unwrap();
            let constraints = edge(
                &bound, &prepared, &model, &map, &mesh, &nodes, &cells, &facets,
            )
            .unwrap();
            let fixed = constraints.as_fixed();
            assert!(fixed.count < 2 * mesh.dof_count());
            let lower = mesh
                .exterior_face_moments([0, 1, 2], |_| {
                    Ok([C { re: 1., im: 0. }, C { re: 2., im: 0. }, C::default()])
                })
                .unwrap();
            let upper = mesh
                .exterior_face_moments([3, 4, 5], |_| {
                    Ok([C { re: 1., im: 0. }, C { re: 2., im: 0. }, C::default()])
                })
                .unwrap();
            let mut reduced = vec![None::<f64>; fixed.count];
            for (dof, value) in lower.into_iter().chain(upper) {
                let rep = fixed.representative[dof];
                let normalized = value.re / fixed.weights[dof].re;
                if let Some(old) = reduced[rep] {
                    assert!((old - normalized).abs() < 1e-9, "order {order}");
                } else {
                    reduced[rep] = Some(normalized);
                }
            }
            let boundary = bound
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 1), ([3, 4, 5], 2)])
                .unwrap();
            let options = LinearOptions {
                tolerance: 1e-6,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let state = bound
                .run_edge_with_boundaries(
                    &prepared,
                    &mesh,
                    &boundary,
                    "standard",
                    Some(&fixed),
                    &options,
                    &mut crate::solver_common::SolverCache::default(),
                    |_, _, _, _, _| Ok(()),
                )
                .unwrap();
            assert!(
                state.iter().all(|value| value.re == 0. && value.im == 0.),
                "order {order}"
            );
        }
    }
}
