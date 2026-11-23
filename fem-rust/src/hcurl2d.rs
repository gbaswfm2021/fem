//! Compatible lowest-order 2D H(curl) × H1 direct-sum field space.
//!
//! The first three local basis functions are oriented Whitney edge forms for
//! the in-plane field. The last three are nodal P1 functions in the
//! longitudinal component. All six are presented to the generic weak-form
//! engine as three-component fields on a 2D mesh, so its declared operator
//! dimension and longitudinal derivative substitution remain shared.

use std::collections::{BTreeMap,HashMap};
use crate::{C,SolverCache};
use crate::assembler::SparseMatrixSink;
use crate::weak_form::{BasisSample,Coefficient,FormPlan,VolumeLoadSamples};
use crate::boundary_form::BoundaryFormPlan;

#[derive(Clone,Copy)]
pub(crate) struct Point {
    pub(crate) barycentric:[f64;3],
    pub(crate) weight:f64,
}
pub(crate) enum CoefficientSample {
    Scalar(C),
    Matrix { rows:usize, columns:usize, values:Vec<C> },
}
impl CoefficientSample {
    fn as_ref(&self)->Coefficient<'_>{match self{
        Self::Scalar(value)=>Coefficient::Scalar(*value),
        Self::Matrix{rows,columns,values}=>Coefficient::Matrix{rows:*rows,columns:*columns,values},
    }}
}
pub(crate) struct Cell {
    pub(crate) nodes:[usize;3],
    pub(crate) points:Vec<(Point,HashMap<String,CoefficientSample>)>,
}
pub(crate) struct BoundaryPoint<'a> {
    pub(crate) element:usize,
    pub(crate) barycentric:[f64;3],
    pub(crate) normal:[f64;3],
    pub(crate) weight:f64,
    pub(crate) plan:&'a BoundaryFormPlan,
    pub(crate) coefficients:HashMap<String,C>,
    pub(crate) sources:HashMap<String,Vec<C>>,
}
pub(crate) struct DofTopology {
    pub(crate) elements:Vec<[usize;6]>,
    pub(crate) count:usize,
    pub(crate) edge_count:usize,
    edges:BTreeMap<[usize;2],usize>,
}
impl DofTopology {
    pub(crate) fn triangles(node_count:usize,cells:&[[usize;3]])->Result<Self,String>{
        let mut edges=BTreeMap::<[usize;2],usize>::new();
        for (index,cell) in cells.iter().enumerate(){
            if cell.iter().any(|&node|node>=node_count)||cell[0]==cell[1]||cell[1]==cell[2]||cell[2]==cell[0]{
                return Err(format!("H(curl) triangle {index} has repeated or out-of-range nodes."));
            }
            for [a,b] in [[cell[0],cell[1]],[cell[1],cell[2]],[cell[2],cell[0]]]{edges.entry([a.min(b),a.max(b)]).or_insert(0);}
        }
        for (index,value) in edges.values_mut().enumerate(){*value=index;}
        let edge_count=edges.len();
        let elements=cells.iter().map(|cell|{
            let edge_dofs=[[cell[0],cell[1]],[cell[1],cell[2]],[cell[2],cell[0]]]
                .map(|[a,b]|edges[&[a.min(b),a.max(b)]]);
            [edge_dofs[0],edge_dofs[1],edge_dofs[2],edge_count+cell[0],edge_count+cell[1],edge_count+cell[2]]
        }).collect::<Vec<_>>();
        let count=edge_count.checked_add(node_count).ok_or("H(curl) direct-sum DOF count overflow.")?;
        Ok(Self{elements,count,edge_count,edges})
    }
    pub(crate) fn edge(&self,mut nodes:[usize;2])->Option<usize>{nodes.sort_unstable();self.edges.get(&nodes).copied()}
}

/// Return oriented Whitney edge and longitudinal nodal basis samples for one
/// triangle. Gradients use row-major component-by-(x,y) ordering.
pub(crate) fn basis(nodes:&[[f64;2]],cell:[usize;3],barycentric:[f64;3],
    gradients:[[f64;2];3])->Result<(Vec<Vec<f64>>,Vec<Vec<f64>>),String>{
    if cell.iter().any(|&node|node>=nodes.len())||barycentric.iter().any(|value|!value.is_finite())
        ||barycentric.iter().any(|value|*value< -1e-12)|| (barycentric.iter().sum::<f64>()-1.).abs()>1e-9 {
        return Err("H(curl) triangle basis sample is invalid.".into());
    }
    let mut values=Vec::with_capacity(6);let mut derivatives=Vec::with_capacity(6);
    for [a,b] in [[0usize,1usize],[1,2],[2,0]]{
        let orientation=if cell[a]<cell[b]{1.}else{-1.};
        let mut value=vec![0.;3];let mut gradient=vec![0.;6];
        for component in 0..2{
            value[component]=orientation*(barycentric[a]*gradients[b][component]-barycentric[b]*gradients[a][component]);
            for axis in 0..2{gradient[component*2+axis]=orientation*(gradients[a][axis]*gradients[b][component]-gradients[b][axis]*gradients[a][component]);}
        }
        values.push(value);derivatives.push(gradient);
    }
    for a in 0..3{
        let mut value=vec![0.;3];value[2]=barycentric[a];
        let mut gradient=vec![0.;6];gradient[4]=gradients[a][0];gradient[5]=gradients[a][1];
        values.push(value);derivatives.push(gradient);
    }
    Ok((values,derivatives))
}

pub(crate) struct LinearSystem {pub(crate) matrix:Vec<BTreeMap<usize,C>>,pub(crate) rhs_re:Vec<f64>,pub(crate) rhs_im:Vec<f64>}
pub(crate) struct DofMap {pub(crate) representatives:Vec<usize>,pub(crate) weights:Vec<C>,pub(crate) count:usize}
impl DofMap {
    pub(crate) fn identity(count:usize)->Self{Self{representatives:(0..count).collect(),weights:vec![C{re:1.,im:0.};count],count}}
    pub(crate) fn validate(&self,count:usize)->Result<(),String>{
        if self.representatives.len()!=count||self.weights.len()!=count||self.count==0
            ||self.representatives.iter().any(|&index|index>=self.count)
            ||self.weights.iter().any(|weight|!weight.re.is_finite()||!weight.im.is_finite()||(weight.re==0.&&weight.im==0.)){
            return Err("H(curl) weighted DOF map does not match the direct-sum space.".into());
        }
        Ok(())
    }
}
pub(crate) struct Solution {pub(crate) real:Vec<f64>,pub(crate) imaginary:Vec<f64>,pub(crate) residual:f64,pub(crate) unknowns:usize}

pub(crate) fn assemble(nodes:&[[f64;2]],cells:&[Cell],form:&FormPlan,
    boundaries:&[BoundaryPoint<'_>],essential:&HashMap<usize,C>,loads:&[VolumeLoadSamples<'_>],
    dof_map:&DofMap,backend:&str,ordering:&str,threads:usize,tolerance:f64,cache:&mut SolverCache)
    ->Result<Solution,String>{
    let connectivity=cells.iter().map(|cell|cell.nodes).collect::<Vec<_>>();
    let topology=DofTopology::triangles(nodes.len(),&connectivity)?;
    let system=assemble_system(nodes,cells,form,boundaries,essential,loads,dof_map,cache)?;
    let (real,imaginary,residual)=crate::solver_common::solve_linear_system(&system.matrix,&system.rhs_re,&system.rhs_im,
        tolerance,backend,ordering,threads,cache)?;
    let mut expanded_real=vec![0.;topology.count];let mut expanded_imaginary=vec![0.;topology.count];
    for dof in 0..topology.count{
        let value=crate::c_mul(dof_map.weights[dof],C{re:real[dof_map.representatives[dof]],im:imaginary[dof_map.representatives[dof]]});
        expanded_real[dof]=value.re;expanded_imaginary[dof]=value.im;
    }
    Ok(Solution{real:expanded_real,imaginary:expanded_imaginary,residual,unknowns:dof_map.count})
}

/// Assemble the declared direct-sum operator and its routed loads without
/// solving it. The returned sparse master rows can be reused by generalized
/// and polynomial eigenvalue adapters without duplicating element assembly.
pub(crate) fn assemble_system(nodes:&[[f64;2]],cells:&[Cell],form:&FormPlan,
    boundaries:&[BoundaryPoint<'_>],essential:&HashMap<usize,C>,loads:&[VolumeLoadSamples<'_>],
    dof_map:&DofMap,cache:&mut SolverCache)->Result<LinearSystem,String>{
    if nodes.is_empty()||cells.is_empty(){return Err("H(curl) direct-sum problem requires a nonempty triangle mesh.".into());}
    let connectivity=cells.iter().map(|cell|cell.nodes).collect::<Vec<_>>();
    let topology=DofTopology::triangles(nodes.len(),&connectivity)?;
    dof_map.validate(topology.count)?;
    form.require_shape(2,3)?;
    form.require_discretization(crate::discretization::Cell::Triangle,crate::discretization::Family::NedelecH1DirectSum,3,1)?;
    for (_,samples) in loads {if samples.len()!=cells.len()||samples.iter().zip(cells).any(|(row,cell)|row.len()!=cell.points.len()){
        return Err("H(curl) volume-load samples do not match cell quadrature.".into());}}
    let mut workspace=std::mem::take(&mut cache.assembly);
    let assembled=(||->Result<LinearSystem,String>{
    let dofs=topology.elements.iter().map(|row|row.to_vec()).collect::<Vec<_>>();
    workspace.prepare(topology.count,&dofs)?;
    let mut rhs_re=vec![0.;topology.count];let mut rhs_im=vec![0.;topology.count];
    for (cell_index,cell) in cells.iter().enumerate(){
        let coords=cell.nodes.map(|node|nodes[node]);
        let (area,gradients)=crate::mesh2d::triangle_geometry(coords.map(|point|[point[0],point[1],0.]))?;
        if !area.is_finite()||area<=0. {return Err(format!("H(curl) triangle {cell_index} has nonpositive area."));}
        let element_dofs=&topology.elements[cell_index];
        for (point_index,(point,coefficients)) in cell.points.iter().enumerate(){
            let (values,derivatives)=basis(nodes,cell.nodes,point.barycentric,gradients)?;
            let samples=(0..6).map(|index|BasisSample{value:&values[index],gradient:&derivatives[index],curl:&[]}).collect::<Vec<_>>();
            let mut local=vec![vec![C::default();6];6];
            let coefficients=coefficients.iter().map(|(name,value)|(name.clone(),value.as_ref())).collect::<HashMap<_,_>>();
            form.accumulate(&samples,&coefficients,area*point.weight,&mut local)?;
            workspace.scatter_element(cell_index,element_dofs,&local)?;
            for (plan,rows) in loads{
                let mut load=vec![C::default();6];
                plan.accumulate(&samples,&rows[cell_index][point_index],area*point.weight,&mut load)?;
                for (&dof,value) in element_dofs.iter().zip(load){rhs_re[dof]+=value.re;rhs_im[dof]+=value.im;}
            }
        }
    }
    for boundary in boundaries {
        let cell=cells.get(boundary.element).ok_or("H(curl) boundary owner is out of range.")?;
        let coords=cell.nodes.map(|node|nodes[node]);
        let (area,gradients)=crate::mesh2d::triangle_geometry(coords.map(|point|[point[0],point[1],0.]))?;
        if area<=0. {return Err("H(curl) boundary owner has nonpositive area.".into());}
        let (values,_)=basis(nodes,cell.nodes,boundary.barycentric,gradients)?;
        let traces=values.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let dofs=&topology.elements[boundary.element];
        if boundary.plan.has_matrix_terms(){
            let mut local=vec![vec![C::default();6];6];
            boundary.plan.accumulate_matrix(&traces,&boundary.normal,&boundary.coefficients,boundary.weight,&mut local)?;
            workspace.scatter_element(boundary.element,dofs,&local)?;
        }
        let mut load=vec![C::default();6];
        boundary.plan.accumulate_load(&traces,&boundary.normal,&boundary.coefficients,&boundary.sources,boundary.weight,&mut load)?;
        for (&dof,value) in dofs.iter().zip(load){rhs_re[dof]+=value.re;rhs_im[dof]+=value.im;}
    }
    let (mut matrix,mut rhs_re,mut rhs_im)=workspace.reduce_weighted(
        &rhs_re,&rhs_im,&dof_map.representatives,&dof_map.weights,dof_map.count)?;
    for (&dof,&value) in essential{
        if dof>=dof_map.count||!value.re.is_finite()||!value.im.is_finite(){return Err("H(curl) essential constraint is invalid.".into());}
    }
    for (row_index,row) in matrix.iter_mut().enumerate(){
        if let Some(value)=essential.get(&row_index){
            row.clear();row.insert(row_index,C{re:1.,im:0.});rhs_re[row_index]=value.re;rhs_im[row_index]=value.im;
        } else {
            row.retain(|column,coefficient|{
                if let Some(value)=essential.get(column){
                    let correction=crate::c_mul(*coefficient,*value);rhs_re[row_index]-=correction.re;rhs_im[row_index]-=correction.im;false
                } else { true }
            });
        }
    }
    Ok(LinearSystem{matrix,rhs_re,rhs_im})
    })();
    cache.assembly=workspace;
    assembled
}

#[cfg(test)]
mod tests{
    use super::*;use serde_json::json;
    #[test]
    fn direct_sum_uses_shared_edges_and_three_component_analytic_curl(){
        let nodes=[[0.,0.],[1.,0.],[0.,1.],[1.,1.]];
        let topo=DofTopology::triangles(nodes.len(),&[[0,1,2],[1,3,2]]).unwrap();
        assert_eq!(topo.edge_count,5);assert_eq!(topo.count,9);assert_eq!(topo.edge([1,2]),topo.edge([2,1]));
        let grad=[[-1.,-1.],[1.,0.],[0.,1.]];
        let (values,derivatives)=basis(&nodes,[0,1,2],[1./3.;3],grad).unwrap();
        let samples=(0..6).map(|index|BasisSample{value:&values[index],gradient:&derivatives[index],curl:&[]}).collect::<Vec<_>>();
        let declaration=json!({"dimension":2,"operatorDimension":3,"fieldComponents":3,
            "terms":[{"test":"curl","trial":"curl","coefficient":"one"}]});
        let mut form=FormPlan::from_json(&declaration).unwrap();
        let mut at_zero=vec![vec![C::default();6];6];
        form.accumulate(&samples,&HashMap::from([("one".to_string(),Coefficient::Scalar(C{re:1.,im:0.}))]),1.,&mut at_zero).unwrap();
        form.set_longitudinal_wavenumber(C{re:2.,im:0.}).unwrap();
        let mut at_finite=vec![vec![C::default();6];6];
        form.accumulate(&samples,&HashMap::from([("one".to_string(),Coefficient::Scalar(C{re:1.,im:0.}))]),1.,&mut at_finite).unwrap();
        assert!(at_zero[0][3].re==0.&&at_zero[0][3].im==0.);
        assert!(at_finite[0][3].re.hypot(at_finite[0][3].im)>0.);
    }

    #[test]
    fn zero_kz_splits_isotropic_polarizations_and_real_kz_operator_is_hermitian(){
        let nodes=[[0.,0.],[1.,0.],[0.,1.]];
        let cell=[0,1,2];
        let (_,gradients)=crate::mesh2d::triangle_geometry([[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]]).unwrap();
        let (values,derivatives)=basis(&nodes,cell,[1./3.;3],gradients).unwrap();
        let samples=(0..6).map(|index|BasisSample{value:&values[index],gradient:&derivatives[index],curl:&[]}).collect::<Vec<_>>();
        let declaration=json!({"dimension":2,"operatorDimension":3,"fieldComponents":3,
            "terms":[{"test":"curl","trial":"curl","coefficient":"one"}]});
        let mut form=FormPlan::from_json(&declaration).unwrap();
        let coefficients=HashMap::from([("one".to_string(),Coefficient::Scalar(C{re:1.,im:0.}))]);
        let mut zero=vec![vec![C::default();6];6];
        form.accumulate(&samples,&coefficients,0.5,&mut zero).unwrap();
        for i in 0..3 { for j in 3..6 {
            assert_eq!(zero[i][j],C::default());
            assert_eq!(zero[j][i],C::default());
        }}
        form.set_longitudinal_wavenumber(C{re:2.5,im:0.}).unwrap();
        let mut finite=vec![vec![C::default();6];6];
        form.accumulate(&samples,&coefficients,0.5,&mut finite).unwrap();
        assert!(finite[0][3].re.hypot(finite[0][3].im)>0.);
        for i in 0..6 { for j in 0..6 {
            assert!((finite[i][j].re-finite[j][i].re).abs()<1e-12);
            assert!((finite[i][j].im+finite[j][i].im).abs()<1e-12);
        }}
    }
}
