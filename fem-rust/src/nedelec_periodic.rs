//! Oriented edge/face moment relations for matched tetrahedral surfaces.
//!
//! This converts a validated vertex correspondence and a complex phase into
//! weighted DOF relations. It does not decide how surfaces are selected or
//! how vertices are matched. Order-two face modes are supported when their
//! canonical tangent directions are parallel mode by mode; a transform that
//! mixes the two face moments is rejected instead of silently approximated.
use std::collections::{HashMap, HashSet};
use crate::nedelec::canonical_points;
use crate::nedelec_dofs::{DofTopology, Relation};
use crate::simplex::{dot,norm,sub};
use crate::solver_common::{C,c_scale};

fn key(mut face:[usize;3]) -> Result<[usize;3],String> {
    face.sort_unstable();
    if face.windows(2).any(|pair| pair[0] == pair[1]) { return Err("Matched face repeats a node.".into()); }
    Ok(face)
}

fn orientation(lower:[usize;2],upper:[usize;2],nodes:&[[f64;3]],tolerance:f64) -> Result<f64,String> {
    let a = sub(nodes[lower[1]],nodes[lower[0]]);
    let b = sub(nodes[upper[1]],nodes[upper[0]]);
    let lengths = norm(a)*norm(b);
    if !lengths.is_finite() || lengths <= 0. { return Err("Matched moment direction is degenerate or non-finite.".into()); }
    let cosine = dot(a,b)/lengths;
    if !cosine.is_finite() || (cosine.abs()-1.).abs() > tolerance {
        return Err("Matched moment directions are not parallel within the declared tolerance.".into());
    }
    Ok(cosine.signum())
}

fn edge_factor(factor:C, sign:f64, mode:usize) -> C {
    c_scale(factor,if mode == 0 { sign } else { 1. })
}

/// Build relations whose upper moment equals factor times the corresponding
/// lower moment. `node_map` maps every selected lower vertex to a unique upper
/// vertex. Both face lists must cover one another exactly.
pub(crate) fn matched_surface_relations(topology:&DofTopology,nodes:&[[f64;3]],order:u8,
    lower_faces:&[[usize;3]],upper_faces:&[[usize;3]],node_map:&HashMap<usize,usize>,
    factor:C,orientation_tolerance:f64) -> Result<Vec<Relation>,String> {
    if !matches!(order,1 | 2) { return Err("Matched moment relations support orders 1 and 2.".into()); }
    if !orientation_tolerance.is_finite() || orientation_tolerance < 0. { return Err("Orientation tolerance must be finite and nonnegative.".into()); }
    if !factor.re.is_finite() || !factor.im.is_finite() || factor.re.hypot(factor.im) < 1e-150 { return Err("Matched moment factor must be finite and nonzero.".into()); }
    if nodes.iter().flatten().any(|v| !v.is_finite()) { return Err("Matched mesh coordinates must be finite.".into()); }
    if lower_faces.is_empty() || lower_faces.len()!=upper_faces.len() { return Err("Matched surfaces must contain equal nonzero face counts.".into()); }
    let mut upper_keys = HashSet::new();
    for &face in upper_faces {
        if face.iter().any(|&node| node >= nodes.len()) { return Err("Matched face references an out-of-range node.".into()); }
        if !upper_keys.insert(key(face)?) { return Err("Matched upper surface has duplicate faces.".into()); }
    }
    let mut lower_keys = HashSet::new();
    let mut matched = HashSet::new();
    let mut mapped_nodes = HashMap::new();
    let mut upper_nodes = HashMap::<usize,usize>::new();
    let mut relations = Vec::new();
    for &face in lower_faces {
        if face.iter().any(|&node| node >= nodes.len()) { return Err("Matched face references an out-of-range node.".into()); }
        let lower_key = key(face)?;
        if !lower_keys.insert(lower_key) { return Err("Matched lower surface has duplicate faces.".into()); }
        let mut mapped = [0usize;3];
        for i in 0..3 {
            let lower = face[i];
            let upper = *node_map.get(&lower).ok_or("Matched lower face has no vertex correspondence.")?;
            if upper >= nodes.len() { return Err("Matched vertex correspondence is out of range.".into()); }
            if let Some(previous) = upper_nodes.insert(upper,lower) { if previous != lower { return Err("Matched vertex correspondence is not one-to-one.".into()); } }
            mapped_nodes.insert(lower,upper); mapped[i] = upper;
        }
        let upper_key = key(mapped)?;
        if !upper_keys.contains(&upper_key) || !matched.insert(upper_key) { return Err("A matched face has no unique corresponding upper face.".into()); }
        if order == 2 {
            let lower_ids = topology.face(lower_key).ok_or("Lower face moments are absent from the DOF topology.")?;
            let upper_ids = topology.face(upper_key).ok_or("Upper face moments are absent from the DOF topology.")?;
            if lower_ids.len()!=2 || upper_ids.len()!=2 { return Err("Second-order face moment count must be two.".into()); }
            let a = canonical_points(lower_key.to_vec(),nodes);
            let b = canonical_points(upper_key.to_vec(),nodes);
            for mode in 0..2 {
                let sign = orientation([a[0],a[mode+1]],[b[0],b[mode+1]],nodes,orientation_tolerance)?;
                relations.push(Relation { lower:lower_ids[mode], upper:upper_ids[mode], factor:c_scale(factor,sign) });
            }
        }
        for [a,b] in [[face[0],face[1]],[face[1],face[2]],[face[2],face[0]]] {
            let lower_ids = topology.edge([a,b]).ok_or("Lower edge moments are absent from the DOF topology.")?;
            let upper_ids = topology.edge([mapped_nodes[&a],mapped_nodes[&b]]).ok_or("Upper edge moments are absent from the DOF topology.")?;
            if lower_ids.len()!=order as usize || upper_ids.len()!=order as usize { return Err("Edge moment count does not match the declared order.".into()); }
            let lower = canonical_points(vec![a,b],nodes);
            let upper = canonical_points(vec![mapped_nodes[&a],mapped_nodes[&b]],nodes);
            let sign = orientation([lower[0],lower[1]],[upper[0],upper[1]],nodes,orientation_tolerance)?;
            for mode in 0..order as usize {
                // Reversing the edge reverses the constant tangent moment;
                // the odd scalar moment changes sign as well and is invariant.
                relations.push(Relation { lower:lower_ids[mode], upper:upper_ids[mode], factor:edge_factor(factor,sign,mode) });
            }
        }
    }
    if matched.len()!=upper_keys.len() { return Err("Matched surfaces do not cover the same faces.".into()); }
    Ok(relations)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translation_preserves_all_order_one_and_two_moments() {
        let nodes = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,2.],[1.,0.,2.],[0.,1.,2.],[0.,0.,-1.],[0.,0.,3.]];
        let map = HashMap::from([(0,3),(1,4),(2,5)]);
        for order in [1,2] {
            let topology = DofTopology::tetrahedra(nodes.len(),&[[0,1,2,6],[5,4,3,7]],order).unwrap();
            let phase = C {re:0.,im:-1.};
            let relations = matched_surface_relations(&topology,&nodes,order,&[[2,0,1]],&[[4,5,3]],&map,phase,1e-8).unwrap();
            assert_eq!(relations.len(),if order==1 {3} else {8});
            assert!(relations.iter().all(|r| (r.factor.re-phase.re).abs()<1e-12 && (r.factor.im-phase.im).abs()<1e-12));
            let constrained = topology.constrain(&relations).unwrap();
            for r in relations { assert_eq!(constrained.representative[r.lower],constrained.representative[r.upper]); }
        }
    }

    #[test]
    fn reversed_edge_orientation_changes_only_constant_moment() {
        let nodes = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[5.,0.,2.],[4.,0.,2.],[5.,1.,2.],[0.,0.,-1.],[5.,0.,3.]];
        let map = HashMap::from([(0,3),(1,4),(2,5)]);
        let topology = DofTopology::tetrahedra(nodes.len(),&[[0,1,2,6],[3,4,5,7]],2).unwrap();
        // Reflection changes the canonical face tangent basis; the current
        // two-mode relation cannot represent its general transformation.
        assert!(matched_surface_relations(&topology,&nodes,2,&[[0,1,2]],&[[3,4,5]],&map,C {re:1.,im:0.},1e-8).is_err());
        let sign = orientation([0,1],[3,4],&nodes,1e-8).unwrap();
        assert_eq!(sign,-1.);
        assert_eq!(edge_factor(C {re:1.,im:0.},sign,0).re,-1.);
        assert_eq!(edge_factor(C {re:1.,im:0.},sign,1).re,1.);
    }

    #[test]
    fn rejects_malformed_faces_missing_or_nonbijective_matches_and_bad_phase() {
        let nodes = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
        let topology = DofTopology::tetrahedra(4,&[[0,1,2,3]],1).unwrap();
        assert!(matched_surface_relations(&topology,&nodes,1,&[[0,1,1]],&[[0,1,2]],&HashMap::new(),C {re:1.,im:0.},1e-8).is_err());
        let map = HashMap::from([(0,0),(1,0),(2,2)]);
        assert!(matched_surface_relations(&topology,&nodes,1,&[[0,1,2]],&[[0,1,2]],&map,C {re:1.,im:0.},1e-8).is_err());
        assert!(matched_surface_relations(&topology,&nodes,1,&[[0,1,2]],&[[0,1,2]],&HashMap::new(),C::default(),1e-8).is_err());
    }
}
