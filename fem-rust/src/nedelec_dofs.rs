//! Topological degree-of-freedom numbering for oriented tetrahedral elements.
//!
//! Entity keys use sorted node IDs; physical basis orientations are supplied
//! by `nedelec`. This module assigns no PDE coefficients, geometric matching,
//! boundary conditions, or scenario-specific meaning to an entity.
use std::collections::BTreeMap;
use crate::nedelec::{EDGES, FACES};
use crate::solver_common::{C, PhaseUnionFind};

pub(crate) struct DofTopology {
    pub(crate) element_dofs: Vec<Vec<usize>>,
    pub(crate) count: usize,
    edges: BTreeMap<[usize; 2], Vec<usize>>,
    faces: BTreeMap<[usize; 3], Vec<usize>>,
}

impl DofTopology {
    /// Number the shared edge moments (order 1 or 2) and, for order 2, the
    /// two shared face moments. Numbering follows first cell encounter and
    /// the finite-element library's local entity ordering.
    pub(crate) fn tetrahedra(node_count: usize, cells: &[[usize; 4]], order: u8) -> Result<Self, String> {
        if !matches!(order,1 | 2) { return Err(format!("Unsupported oriented tetrahedron order {order}.")); }
        let modes = usize::from(order);
        let mut count = 0usize;
        let mut edges = BTreeMap::<[usize;2],Vec<usize>>::new();
        let mut faces = BTreeMap::<[usize;3],Vec<usize>>::new();
        let mut element_dofs = Vec::with_capacity(cells.len());
        for (index, cell) in cells.iter().enumerate() {
            if cell.iter().any(|&node| node >= node_count) { return Err(format!("Cell {index} references an out-of-range node.")); }
            let mut key = *cell; key.sort_unstable();
            if key.windows(2).any(|pair| pair[0] == pair[1]) { return Err(format!("Cell {index} repeats a vertex.")); }
            let mut ids = Vec::with_capacity(if order == 1 {6} else {20});
            for edge in EDGES {
                let mut key = [cell[edge[0]],cell[edge[1]]]; key.sort_unstable();
                let entry = edges.entry(key).or_insert_with(|| {
                    let values = (count..count+modes).collect(); count += modes; values
                });
                ids.extend(entry.iter().copied());
            }
            if order == 2 {
                for face in FACES {
                    let mut key = face.map(|i| cell[i]); key.sort_unstable();
                    let entry = faces.entry(key).or_insert_with(|| {
                        let values = vec![count,count+1]; count += 2; values
                    });
                    ids.extend(entry.iter().copied());
                }
            }
            element_dofs.push(ids);
        }
        Ok(Self { element_dofs,count,edges,faces })
    }

    pub(crate) fn edge(&self, mut nodes: [usize;2]) -> Option<&[usize]> {
        nodes.sort_unstable(); self.edges.get(&nodes).map(Vec::as_slice)
    }

    pub(crate) fn face(&self, mut nodes: [usize;3]) -> Option<&[usize]> {
        nodes.sort_unstable(); self.faces.get(&nodes).map(Vec::as_slice)
    }

    /// Apply already-derived oriented complex relations in either dimension.
    /// The caller determines matching and orientation; relation `upper` is
    /// `factor * lower`. Contradictory cycles are rejected by PhaseUnionFind.
    pub(crate) fn constrain(&self, relations: &[Relation]) -> Result<ConstrainedDofs, String> {
        let mut classes = PhaseUnionFind::new(self.count);
        for relation in relations { classes.constrain(relation.lower,relation.upper,relation.factor)?; }
        let (representative,weights,count) = classes.mapping();
        Ok(ConstrainedDofs { representative,weights,count })
    }
}

pub(crate) struct Relation { pub(crate) lower: usize, pub(crate) upper: usize, pub(crate) factor: C }
pub(crate) struct ConstrainedDofs { pub(crate) representative: Vec<usize>, pub(crate) weights: Vec<C>, pub(crate) count: usize }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighboring_tetrahedra_share_edges_and_order_two_face_moments() {
        let cells = [[0,1,2,3],[2,1,0,4]];
        let first = DofTopology::tetrahedra(5,&cells,1).unwrap();
        assert_eq!(first.count,9); assert_eq!(first.element_dofs[0].len(),6);
        assert_eq!(first.edge([0,1]),first.edge([1,0])); assert!(first.face([0,1,2]).is_none());
        let second = DofTopology::tetrahedra(5,&cells,2).unwrap();
        assert_eq!(second.count,32); assert_eq!(second.element_dofs[0].len(),20);
        let shared = second.face([2,0,1]).unwrap();
        assert_eq!(shared.len(),2);
        assert!(shared.iter().all(|id| second.element_dofs[0].contains(id) && second.element_dofs[1].contains(id)));
        assert!(second.edge([0,1]).unwrap().iter().all(|id| second.element_dofs[0].contains(id) && second.element_dofs[1].contains(id)));
    }

    #[test]
    fn weighted_relations_preserve_orientation_phase_and_reject_conflicts() {
        let topology = DofTopology::tetrahedra(4,&[[0,1,2,3]],1).unwrap();
        let phase = C { re:0., im:-1. };
        let map = topology.constrain(&[Relation { lower:0,upper:1,factor:phase }]).unwrap();
        assert_eq!(map.count,5); assert_eq!(map.representative[0],map.representative[1]);
        let product = crate::solver_common::c_mul(phase,map.weights[0]);
        assert!((product.re-map.weights[1].re).abs()<1e-12 && (product.im-map.weights[1].im).abs()<1e-12);
        let contradictory = [Relation { lower:0,upper:1,factor:phase }, Relation { lower:0,upper:1,factor:C {re:1.,im:0.} }];
        assert!(topology.constrain(&contradictory).is_err());
        assert!(topology.constrain(&[Relation { lower:0,upper:99,factor:phase }]).is_err());
        assert!(topology.constrain(&[Relation { lower:0,upper:1,factor:C::default() }]).is_err());
    }

    #[test]
    fn rejects_invalid_cells_and_element_orders() {
        assert!(DofTopology::tetrahedra(4,&[[0,1,2,3]],3).is_err());
        assert!(DofTopology::tetrahedra(4,&[[0,1,2,4]],1).is_err());
        assert!(DofTopology::tetrahedra(4,&[[0,1,1,3]],2).is_err());
    }
}
