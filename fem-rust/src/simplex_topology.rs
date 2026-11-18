//! Physics-neutral connectivity and facet geometry for simplex meshes.
//!
//! Facet keys are sorted vertex IDs. Each facet records one owner per incident
//! cell and the opposite vertex, allowing an outward normal to be reconstructed
//! independently of input cell/facet vertex order. No boundary condition,
//! material or scenario policy is inferred here.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TopologyPolicy {
    pub(crate) reject_duplicate_cells: bool,
    pub(crate) reject_non_manifold_facets: bool,
    pub(crate) require_all_nodes_used: bool,
    pub(crate) require_nonempty_cells: bool,
    /// The determinant is twice triangle area or six times tetrahedron volume.
    pub(crate) determinant_tolerance: Option<DeterminantTolerance>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DeterminantTolerance {
    /// `reject_equal` selects `<` versus `<=` at the declared threshold.
    Absolute { minimum: f64, reject_equal: bool },
    /// The reference scale is the longest edge from local vertex zero,
    /// raised to the spatial dimension. The scale is independent of units.
    Relative { fraction: f64, reject_equal: bool },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SelectedFacetOwnership { ExteriorOnly, InteriorOnly, AnyExisting }

#[derive(Clone, Copy, Debug)]
pub(crate) enum SelectedFacetDuplicates { Reject, KeepAll, LastWins }

#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectedFacetPolicy {
    pub(crate) ownership: SelectedFacetOwnership,
    pub(crate) duplicates: SelectedFacetDuplicates,
    pub(crate) require_positive_tag: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectedFacet {
    pub(crate) facet_index: usize,
    pub(crate) tag: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FacetOwner {
    pub(crate) cell: usize,
    pub(crate) opposite_vertex: usize,
    pub(crate) local_facet: usize,
}

#[derive(Debug)]
pub(crate) struct Facet<const F: usize> {
    pub(crate) vertices: [usize; F],
    pub(crate) owners: Vec<FacetOwner>,
}

#[derive(Debug)]
pub(crate) struct SimplexTopology<const F: usize> {
    pub(crate) facets: Vec<Facet<F>>,
    /// Component IDs are compact and deterministic; unused nodes are None.
    pub(crate) node_components: Vec<Option<usize>>,
    pub(crate) cell_components: Vec<usize>,
    pub(crate) component_count: usize,
}

impl<const F: usize> SimplexTopology<F> {
    pub(crate) fn facet(&self, mut vertices: [usize; F]) -> Option<&Facet<F>> {
        vertices.sort_unstable();
        self.facets.binary_search_by_key(&vertices, |facet| facet.vertices)
            .ok().map(|index| &self.facets[index])
    }

    /// Resolve selected facets to adjacency entries. `required_tags` lists
    /// enabled selections; missing tags fail even if no input record used them.
    /// Duplicate detection canonicalizes vertex order and is independent of tag.
    pub(crate) fn bind_selected_facets(
        &self,
        selections: &[([usize; F], usize)],
        required_tags: &[usize],
        policy: SelectedFacetPolicy,
    ) -> Result<Vec<SelectedFacet>, String> {
        let mut output: Vec<SelectedFacet> = Vec::new();
        let mut positions = BTreeMap::<usize, usize>::new();
        let mut used_tags = BTreeSet::new();
        for &(mut vertices, tag) in selections {
            if policy.require_positive_tag && tag == 0 {
                return Err("Selected facet requires a positive physical tag.".into());
            }
            vertices.sort_unstable();
            if vertices.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(format!("Selected facet {vertices:?} repeats a vertex."));
            }
            let index = self.facets.binary_search_by_key(&vertices, |facet| facet.vertices)
                .map_err(|_| format!("Selected facet {vertices:?} has no adjacent simplex."))?;
            let owner_count = self.facets[index].owners.len();
            match policy.ownership {
                SelectedFacetOwnership::ExteriorOnly if owner_count != 1 =>
                    return Err(format!("Selected facet {vertices:?} must be exterior.")),
                SelectedFacetOwnership::InteriorOnly if owner_count != 2 =>
                    return Err(format!("Selected facet {vertices:?} must have two adjacent cells.")),
                _ => {},
            }
            let selected = SelectedFacet { facet_index: index, tag };
            if let Some(&position) = positions.get(&index) {
                match policy.duplicates {
                    SelectedFacetDuplicates::Reject => return Err(format!("Selected facet {vertices:?} is duplicated.")),
                    SelectedFacetDuplicates::KeepAll => output.push(selected),
                    SelectedFacetDuplicates::LastWins => output[position] = selected,
                }
            } else {
                positions.insert(index, output.len());
                output.push(selected);
            }
        }
        for facet in &output { used_tags.insert(facet.tag); }
        for &tag in required_tags {
            if !used_tags.contains(&tag) {
                return Err(format!("Selected physical tag {tag} has no matching facet."));
            }
        }
        Ok(output)
    }
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    let mut root = node;
    while parent[root] != root { root = parent[root]; }
    while parent[node] != node {
        let next = parent[node];
        parent[node] = root;
        node = next;
    }
    root
}

fn join(parent: &mut [usize], a: usize, b: usize) {
    let a = find(parent, a);
    let b = find(parent, b);
    if a != b {
        let (small, large) = if a < b { (a, b) } else { (b, a) };
        parent[large] = small;
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn build<const N: usize, const F: usize>(
    dimension: usize,
    points: &[[f64; 3]],
    cells: &[[usize; N]],
    identified_nodes: &[(usize, usize)],
    policy: TopologyPolicy,
) -> Result<SimplexTopology<F>, String> {
    if !matches!((dimension, N, F), (2, 3, 2) | (3, 4, 3)) {
        return Err("Unsupported simplex dimension or facet size.".into());
    }
    if points.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Mesh coordinates must be finite.".into());
    }
    if policy.require_nonempty_cells && cells.is_empty() {
        return Err("Mesh must contain at least one simplex cell.".into());
    }
    if policy.determinant_tolerance.is_some_and(|tolerance| {
        let value = match tolerance {
            DeterminantTolerance::Absolute { minimum, .. } => minimum,
            DeterminantTolerance::Relative { fraction, .. } => fraction,
        };
        !value.is_finite() || value < 0.0
    }) {
        return Err("Simplex determinant tolerance must be finite and non-negative.".into());
    }
    let mut parent: Vec<_> = (0..points.len()).collect();
    let mut used = vec![false; points.len()];
    let mut seen_cells = BTreeSet::new();
    let mut facets: BTreeMap<[usize; F], Vec<FacetOwner>> = BTreeMap::new();
    for (cell_index, cell) in cells.iter().enumerate() {
        if cell.iter().any(|&i| i >= points.len()) {
            return Err(format!("Cell {cell_index} references an out-of-range node."));
        }
        let mut key = *cell;
        key.sort_unstable();
        if key.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(format!("Cell {cell_index} repeats a vertex."));
        }
        if !seen_cells.insert(key) && policy.reject_duplicate_cells {
            return Err(format!("Cell {cell_index} duplicates another cell."));
        }
        if let Some(tolerance) = policy.determinant_tolerance {
            let e1 = sub(points[cell[1]], points[cell[0]]);
            let e2 = sub(points[cell[2]], points[cell[0]]);
            let det = if dimension == 2 {
                e1[0] * e2[1] - e1[1] * e2[0]
            } else {
                dot(e1, cross(e2, sub(points[cell[3]], points[cell[0]])))
            };
            let (threshold, reject_equal) = match tolerance {
                DeterminantTolerance::Absolute { minimum, reject_equal } => (minimum, reject_equal),
                DeterminantTolerance::Relative { fraction, reject_equal } => {
                    let scale = (1..N).map(|i| {
                        let edge = sub(points[cell[i]], points[cell[0]]);
                        if dimension == 2 { edge[0].hypot(edge[1]) } else { dot(edge, edge).sqrt() }
                    }).fold(0.0_f64, f64::max);
                    (fraction * scale.powi(dimension as i32), reject_equal)
                }
            };
            if !det.is_finite() || !threshold.is_finite() || det == 0.0
                || if reject_equal { det.abs() <= threshold } else { det.abs() < threshold } {
                return Err(format!("Cell {cell_index} is degenerate under the declared tolerance."));
            }
        }
        for &node in cell { used[node] = true; join(&mut parent, cell[0], node); }
        for opposite in 0..N {
            let mut source = 0;
            let mut vertices = std::array::from_fn(|_| {
                if source == opposite { source += 1; }
                let node = cell[source];
                source += 1;
                node
            });
            vertices.sort_unstable();
            let owners = facets.entry(vertices).or_default();
            owners.push(FacetOwner { cell: cell_index, opposite_vertex: cell[opposite], local_facet: opposite });
            if policy.reject_non_manifold_facets && owners.len() > 2 {
                return Err(format!("Facet {vertices:?} has more than two adjacent cells."));
            }
        }
    }
    if policy.require_all_nodes_used && used.iter().any(|&value| !value) {
        return Err("Mesh contains an unused node.".into());
    }
    for &(a, b) in identified_nodes {
        if a >= points.len() || b >= points.len() { return Err("Identified node is outside the mesh.".into()); }
        join(&mut parent, a, b);
    }
    let mut component_ids = BTreeMap::new();
    let mut node_components = vec![None; points.len()];
    for node in 0..points.len() {
        if used[node] {
            let root = find(&mut parent, node);
            let next = component_ids.len();
            node_components[node] = Some(*component_ids.entry(root).or_insert(next));
        }
    }
    let cell_components = cells.iter().map(|cell| node_components[cell[0]].unwrap()).collect();
    Ok(SimplexTopology {
        facets: facets.into_iter().map(|(vertices, owners)| Facet { vertices, owners }).collect(),
        node_components,
        cell_components,
        component_count: component_ids.len(),
    })
}

pub(crate) fn triangles(points: &[[f64; 3]], cells: &[[usize; 3]],
    identified_nodes: &[(usize, usize)], policy: TopologyPolicy) -> Result<SimplexTopology<2>, String> {
    build(2, points, cells, identified_nodes, policy)
}

pub(crate) fn tetrahedra(points: &[[f64; 3]], cells: &[[usize; 4]],
    identified_nodes: &[(usize, usize)], policy: TopologyPolicy) -> Result<SimplexTopology<3>, String> {
    build(3, points, cells, identified_nodes, policy)
}

/// Edge outward normal in the XY plane and physical edge length.
pub(crate) fn edge_normal(points: &[[f64; 3]], facet: &Facet<2>, owner: usize) -> Result<([f64; 3], f64), String> {
    let own = facet.owners.get(owner).ok_or("Facet owner is outside the adjacency table.")?;
    let p = *points.get(facet.vertices[0]).ok_or("Facet vertex is outside the mesh.")?;
    let q = *points.get(facet.vertices[1]).ok_or("Facet vertex is outside the mesh.")?;
    let opposite = *points.get(own.opposite_vertex).ok_or("Facet opposite vertex is outside the mesh.")?;
    let delta = sub(q, p);
    let length = delta[0].hypot(delta[1]);
    if !length.is_finite() || length == 0.0 { return Err("Facet edge is degenerate.".into()); }
    let mut normal = [delta[1] / length, -delta[0] / length, 0.0];
    let inward = dot(normal, sub(opposite, p));
    if !inward.is_finite() || inward == 0.0 { return Err("Facet owner has degenerate geometry.".into()); }
    if inward > 0.0 { normal = normal.map(|v| -v); }
    Ok((normal, length))
}

/// Triangle outward normal in 3D and physical face area.
pub(crate) fn face_normal(points: &[[f64; 3]], facet: &Facet<3>, owner: usize) -> Result<([f64; 3], f64), String> {
    let own = facet.owners.get(owner).ok_or("Facet owner is outside the adjacency table.")?;
    let p = *points.get(facet.vertices[0]).ok_or("Facet vertex is outside the mesh.")?;
    let q = *points.get(facet.vertices[1]).ok_or("Facet vertex is outside the mesh.")?;
    let r = *points.get(facet.vertices[2]).ok_or("Facet vertex is outside the mesh.")?;
    let opposite = *points.get(own.opposite_vertex).ok_or("Facet opposite vertex is outside the mesh.")?;
    let mut normal = cross(sub(q, p), sub(r, p));
    let length = dot(normal, normal).sqrt();
    if !length.is_finite() || length == 0.0 { return Err("Facet face is degenerate.".into()); }
    let inward = dot(normal, sub(opposite, p));
    if !inward.is_finite() || inward == 0.0 { return Err("Facet owner has degenerate geometry.".into()); }
    if inward > 0.0 { normal = normal.map(|v| -v); }
    Ok((normal.map(|v| v / length), length / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn strict() -> TopologyPolicy { TopologyPolicy { reject_duplicate_cells: true,
        reject_non_manifold_facets: true, require_all_nodes_used: false, require_nonempty_cells: true,
        determinant_tolerance: Some(DeterminantTolerance::Absolute { minimum: 0.0, reject_equal: true }) } }

    #[test]
    fn triangle_owners_have_opposite_outward_normals_despite_vertex_order() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [1.,1.,0.]];
        let topology = triangles(&points, &[[2,1,0], [1,2,3]], &[], strict()).unwrap();
        let edge = topology.facet([2,1]).unwrap();
        assert_eq!(edge.owners.len(), 2);
        let (a, length) = edge_normal(&points, edge, 0).unwrap();
        let (b, other_length) = edge_normal(&points, edge, 1).unwrap();
        assert!((length - 2.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(length, other_length);
        for axis in 0..3 { assert!((a[axis]+b[axis]).abs() < 1e-12); }
        assert_eq!(topology.component_count, 1);
    }

    #[test]
    fn tetra_face_normals_are_outward_for_both_cells() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [0.,0.,1.], [0.,0.,-1.]];
        let topology = tetrahedra(&points, &[[0,1,2,3], [2,1,0,4]], &[], strict()).unwrap();
        let face = topology.facet([2,0,1]).unwrap();
        assert_eq!(face.owners.len(), 2);
        let (a, area) = face_normal(&points, face, 0).unwrap();
        let (b, _) = face_normal(&points, face, 1).unwrap();
        assert_eq!(area, 0.5);
        assert_eq!(a, [0., 0., -1.]);
        assert_eq!(b, [0., 0., 1.]);
    }

    #[test]
    fn components_include_node_identifications_and_unused_node_policy() {
        let points = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.], [3.,0.,0.],[4.,0.,0.],[3.,1.,0.], [8.,8.,0.]];
        let cells = [[0,1,2], [3,4,5]];
        let independent = triangles(&points, &cells, &[], strict()).unwrap();
        assert_eq!(independent.cell_components, vec![0,1]);
        assert_eq!(independent.node_components[6], None);
        assert_eq!(triangles(&points, &cells, &[(1,3)], strict()).unwrap().component_count, 1);
        assert!(triangles(&points, &cells, &[(1,8)], strict()).is_err());
        assert!(triangles(&points, &cells, &[], TopologyPolicy { require_all_nodes_used: true, ..strict() }).is_err());
    }

    #[test]
    fn duplicate_nonmanifold_and_degenerate_policies_are_explicit() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [0.,-1.,0.], [0.,2.,0.]];
        let duplicate = [[0,1,2], [2,1,0]];
        assert!(triangles(&points, &duplicate, &[], strict()).is_err());
        assert!(triangles(&points, &duplicate, &[], TopologyPolicy { reject_duplicate_cells: false, ..strict() }).is_ok());
        let nonmanifold = [[0,1,2], [0,3,1], [0,1,4]];
        assert!(triangles(&points, &nonmanifold, &[], strict()).is_err());
        let permissive = triangles(&points, &nonmanifold, &[], TopologyPolicy { reject_non_manifold_facets: false, ..strict() }).unwrap();
        assert_eq!(permissive.facet([0,1]).unwrap().owners.len(), 3);
        let flat = [[0.,0.,0.], [1.,0.,0.], [2.,0.,0.]];
        assert!(triangles(&flat, &[[0,1,2]], &[], strict()).is_err());
        let permissive = triangles(&flat, &[[0,1,2]], &[], TopologyPolicy { determinant_tolerance: None, ..strict() }).unwrap();
        assert!(edge_normal(&flat, permissive.facet([0,1]).unwrap(), 0).is_err());
        assert!(triangles(&points, &[[0,1,7]], &[], strict()).is_err());
        assert!(triangles(&points, &[[0,0,1]], &[], strict()).is_err());
    }

    #[test]
    fn selected_facets_enforce_ownership_coverage_and_duplicate_policy() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [1.,1.,0.]];
        let topology = triangles(&points, &[[0,1,2], [1,3,2]], &[], strict()).unwrap();
        let exterior = SelectedFacetPolicy { ownership: SelectedFacetOwnership::ExteriorOnly,
            duplicates: SelectedFacetDuplicates::Reject, require_positive_tag: true };
        let bound = topology.bind_selected_facets(&[([1,0], 10), ([1,3], 11)], &[10,11], exterior).unwrap();
        assert_eq!(bound.len(), 2);
        assert_eq!(topology.facets[bound[0].facet_index].vertices, [0,1]);
        assert!(topology.bind_selected_facets(&[([1,2],10)], &[10], exterior).is_err());
        assert!(topology.bind_selected_facets(&[([0,3],10)], &[10], exterior).is_err());
        assert!(topology.bind_selected_facets(&[([0,0],10)], &[10], exterior).is_err());
        assert!(topology.bind_selected_facets(&[([0,1],0)], &[], exterior).is_err());
        assert!(topology.bind_selected_facets(&[([0,1],10)], &[11], exterior).is_err());
        assert!(topology.bind_selected_facets(&[([0,1],10), ([1,0],11)], &[], exterior).is_err());
        let all = topology.bind_selected_facets(&[([0,1],10),([1,0],11)], &[10,11],
            SelectedFacetPolicy { duplicates: SelectedFacetDuplicates::KeepAll, ..exterior }).unwrap();
        assert_eq!(all.len(), 2);
        let last = topology.bind_selected_facets(&[([0,1],10),([1,0],11)], &[11],
            SelectedFacetPolicy { duplicates: SelectedFacetDuplicates::LastWins, ..exterior }).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].tag, 11);
        assert!(topology.bind_selected_facets(&[([0,1],10),([1,0],11)], &[10],
            SelectedFacetPolicy { duplicates: SelectedFacetDuplicates::LastWins, ..exterior }).is_err());
        let interior = SelectedFacetPolicy { ownership: SelectedFacetOwnership::InteriorOnly, ..exterior };
        assert!(topology.bind_selected_facets(&[([2,1],3)], &[3], interior).is_ok());
        assert!(topology.bind_selected_facets(&[([0,1],3)], &[3], interior).is_err());
        let any = SelectedFacetPolicy { ownership: SelectedFacetOwnership::AnyExisting, ..exterior };
        assert!(topology.bind_selected_facets(&[([2,1],3),([0,1],4)], &[3,4], any).is_ok());
    }

    #[test]
    fn absolute_and_relative_cell_tolerances_match_declared_comparison() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,0.25,0.]];
        let cells = [[0,1,2]];
        let absolute = |reject_equal| TopologyPolicy { determinant_tolerance:
            Some(DeterminantTolerance::Absolute { minimum: 0.25, reject_equal }), ..strict() };
        assert!(triangles(&points, &cells, &[], absolute(true)).is_err());
        assert!(triangles(&points, &cells, &[], absolute(false)).is_ok());
        let relative = |fraction| TopologyPolicy { determinant_tolerance:
            Some(DeterminantTolerance::Relative { fraction, reject_equal: true }), ..strict() };
        assert!(triangles(&points, &cells, &[], relative(0.3)).is_err());
        assert!(triangles(&points, &cells, &[], relative(0.2)).is_ok());
        assert!(triangles(&points, &cells, &[], relative(f64::NAN)).is_err());
        assert!(triangles(&points, &[], &[], strict()).is_err());
        assert!(triangles(&[], &[], &[], TopologyPolicy { require_nonempty_cells: false, ..strict() }).is_ok());
    }
}
