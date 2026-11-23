//! Bind a prepared transient case to the generic model and element runners.
//!
//! This internal bridge does not enable a Study. A caller must provide the
//! complete boundary implementation, fixed constraint map and publication.
use std::collections::HashMap;

use crate::model_bindings::BoundSelection;
use crate::solver_common::{C, SolverCache};
use crate::transient_initial::InitialField;
use crate::transient_model::TransientModelData;
use crate::transient_plan::{StateLayout, TransientAssemblyWorkspace, TransientPlan};
use crate::transient_request::PreparedTransientCase;
use crate::transient_result::{StateOutputPlan, TensorOutputPlan};
use crate::transient_step::{FixedDofMap, LinearOptions, TransientOperators};
use serde_json::Value;

/// Produce the source-free modal view of a model from the scenario's declared
/// transient boundary inputs. Natural prescribed loads and optional incident
/// sources are omitted; homogeneous essential data and boundary operators stay.
pub(crate) fn eigenfrequency_model(package:&Value,formulation:&str,model:&Value)->Result<Value,String>{
    let laws=package.pointer(&format!("/transientForms/{formulation}/boundaryLaws")).and_then(Value::as_object)
        .ok_or_else(||format!("Scenario transient form {formulation:?} has no boundary laws for eigenfrequency preparation."))?;
    let mut source_boundary_types=std::collections::HashSet::new();
    let mut optional_source_fields=HashMap::<String,std::collections::HashSet<String>>::new();
    for (kind,law) in laws{
        let input=law.get("input").unwrap_or(&Value::Null);
        if law.get("essentialForm").is_none()&&input.pointer("/prescribed/source").is_some(){source_boundary_types.insert(kind.clone());}
        for source in input.get("sources").and_then(Value::as_array).into_iter().flatten(){
            if let Some(path)=source.pointer("/product/factorPath").and_then(Value::as_str){
                if let Some(field)=path.strip_prefix('/').filter(|value|!value.contains('/')){optional_source_fields.entry(kind.clone()).or_default().insert(field.to_string());}
            }
            if let Some(path)=source.pointer("/product/componentsPath").and_then(Value::as_str){
                if let Some(field)=path.strip_prefix('/').filter(|value|!value.contains('/')){optional_source_fields.entry(kind.clone()).or_default().insert(field.to_string());}
            }
        }
    }
    let physics=model.pointer("/model/physics").and_then(Value::as_object).ok_or("Physics is missing.")?;
    let source_ids=crate::model_schema::physics_domain_sources(&Value::Object(physics.clone()))
        .filter_map(|row|row.get("id").and_then(Value::as_str).map(str::to_owned)).collect::<std::collections::HashSet<_>>();
    let mut result=model.clone();let features=result.pointer_mut("/model/physics").and_then(Value::as_object_mut).ok_or("Physics is missing.")?;
    features.retain(|id,row|{
        if source_ids.contains(id){return false;}
        let kind=row.get("type").and_then(Value::as_str).unwrap_or("");
        !source_boundary_types.contains(kind)
    });
    if let Some(features)=result.pointer_mut("/model/physics").and_then(Value::as_object_mut){
        for row in features.values_mut(){
            let kind=row.get("type").and_then(Value::as_str).unwrap_or("");
            if let Some(fields)=optional_source_fields.get(kind){if let Some(object)=row.as_object_mut(){for field in fields{object.remove(field);}}}
        }
    }
    Ok(result)
}

/// The selected model and map outlive these bindings; expressions and the
/// scenario's mathematical declarations are compiled only once per case.
pub(crate) struct BoundTransientCase<'a> {
    pub(crate) model: &'a Value,
    pub(crate) map: &'a Value,
    pub(crate) plan: TransientPlan,
    pub(crate) data: TransientModelData,
    pub(crate) selections: HashMap<String, Vec<BoundSelection<'a>>>,
    pub(crate) initial: HashMap<String, InitialField>,
    outputs: StateOutputPlan,
    tensor_output: Option<TensorOutputPlan>,
    zero_outputs: Vec<String>,
    boundary_laws: Value,
    essential_forms: Value,
    boundary_forms: Value,
}

pub(crate) struct SnapshotIdentity<'a> {
    pub(crate) study_id: &'a str,
    pub(crate) case_index: usize,
    pub(crate) sample_index: usize,
    pub(crate) time: f64,
}

impl<'a> BoundTransientCase<'a> {
    /// Sample state fields in the element space and project them through the
    /// scenario's output contract. Derived tensors and materials are separate.
    pub(crate) fn nodal_state_symbols_at(
        &self, mesh:&crate::transient_mesh::NodalMesh<'_>, layout:&StateLayout,
        state:&[C], nodes:&[usize], weights:&[f64],
    ) -> Result<HashMap<String,C>,String> {
        let fields=(0..self.plan.fields.len()).map(|field|
            crate::transient_mesh::nodal_field_at_weights(&self.plan,layout,mesh,state,field,nodes,weights)
        ).collect::<Result<Vec<_>,_>>()?;
        self.outputs.evaluate_point(&fields)
    }

    /// The cell owner is explicit so interface samples retain their side and
    /// oriented basis; no scenario-specific field ordering is assumed.
    pub(crate) fn edge_state_symbols_at(
        &self, mesh:&crate::transient_edge::EdgeTransientMesh, layout:&StateLayout,
        state:&[C], cell:usize, point:[f64;3],
    ) -> Result<HashMap<String,C>,String> {
        let fields=(0..self.plan.fields.len()).map(|field|
            mesh.field_at_point(&self.plan,layout,state,field,cell,point).map(|v|v.to_vec())
        ).collect::<Result<Vec<_>,_>>()?;
        self.outputs.evaluate_point(&fields)
    }

    pub(crate) fn triangle_state_symbols_at(
        &self, mesh:&crate::transient_triangle::TriangleTransientMesh, layout:&StateLayout,
        state:&[C], cell:usize, point:[f64;3],
    )->Result<HashMap<String,C>,String>{
        let fields=(0..self.plan.fields.len()).map(|field|
            mesh.field_at_point(&self.plan,layout,state,field,cell,point).map(|value|value.to_vec())
        ).collect::<Result<Vec<_>,_>>()?;
        self.outputs.evaluate_point(&fields)
    }

    fn complete_point_symbols(&self,values:HashMap<String,C>,declarations:&[crate::model_schema::FieldVariable])->Result<HashMap<String,C>,String>{
        let mut rows=values.into_iter().map(|(name,value)|(name,vec![value])).collect();
        crate::transient_result::add_declared_zero_fields(declarations,&mut rows,1)?;
        crate::transient_result::add_transient_zero_fields(&self.zero_outputs,declarations,&mut rows,1)?;
        Ok(rows.into_iter().map(|(name,values)|(name,values[0])).collect())
    }

    pub(crate) fn nodal_symbols_at(&self,prepared:&PreparedTransientCase,mesh:&crate::transient_mesh::NodalMesh<'_>,
        layout:&StateLayout,state:&[C],cell:usize,nodes:&[usize],weights:&[f64],point:[f64;3],time:f64,
        declarations:&[crate::model_schema::FieldVariable])->Result<HashMap<String,C>,String>{
        let mut values=self.nodal_state_symbols_at(mesh,layout,state,nodes,weights)?;
        if let Some(tensor)=&self.tensor_output {
            let sampler=self.data.sampler(&prepared.parameters);
            for (name,value) in tensor.at_cell(&self.plan,layout,mesh,state,cell,point,time,|location|sampler.coefficients(location))? {
                if values.insert(name.clone(),value).is_some(){return Err(format!("Duplicate transient output {name:?}."));}
            }
        }
        self.complete_point_symbols(values,declarations)
    }

    pub(crate) fn edge_symbols_at(&self,mesh:&crate::transient_edge::EdgeTransientMesh,layout:&StateLayout,state:&[C],
        cell:usize,point:[f64;3],declarations:&[crate::model_schema::FieldVariable])->Result<HashMap<String,C>,String>{
        if self.tensor_output.is_some(){return Err("Tensor recovery for this edge space is not supported.".into());}
        self.complete_point_symbols(self.edge_state_symbols_at(mesh,layout,state,cell,point)?,declarations)
    }

    pub(crate) fn triangle_symbols_at(&self,mesh:&crate::transient_triangle::TriangleTransientMesh,layout:&StateLayout,state:&[C],
        cell:usize,point:[f64;3],declarations:&[crate::model_schema::FieldVariable])->Result<HashMap<String,C>,String>{
        if self.tensor_output.is_some(){return Err("Tensor recovery for this triangle space is not supported.".into());}
        self.complete_point_symbols(self.triangle_state_symbols_at(mesh,layout,state,cell,point)?,declarations)
    }

    pub(crate) fn prepare(
        package: &Value,
        profile: &str,
        formulation: &str,
        model: &'a Value,
        map: &'a Value,
        mesh_tags: &HashMap<String, Vec<usize>>,
        initial_fields: &Value,
    ) -> Result<Self, String> {
        Self::prepare_with_source_policy(package,profile,formulation,model,map,mesh_tags,initial_fields,true)
    }

    pub(crate) fn prepare_eigenfrequency(
        package:&Value, profile:&str, formulation:&str, model:&'a Value, map:&'a Value,
        mesh_tags:&HashMap<String,Vec<usize>>,
    )->Result<Self,String>{
        Self::prepare_with_source_policy(package,profile,formulation,model,map,mesh_tags,&serde_json::json!({}),false)
    }

    fn prepare_with_source_policy(
        package: &Value,
        profile: &str,
        formulation: &str,
        model: &'a Value,
        map: &'a Value,
        mesh_tags: &HashMap<String, Vec<usize>>,
        initial_fields: &Value,
        bind_sources:bool,
    ) -> Result<Self, String> {
        if package.get("id").and_then(Value::as_str) != Some(profile)
            || model.pointer("/scenario/profileId").and_then(Value::as_str) != Some(profile)
        {
            return Err("Transient model and package profile identities must agree.".into());
        }
        let declaration = package
            .get("transientForms")
            .and_then(|v| v.get(formulation))
            .ok_or("Scenario has no transient form for the selected formulation.")?;
        let plan = TransientPlan::from_json(declaration)?;
        let selection = crate::selection_plan::SelectionPlan::from_json(
            package
                .get("transientSelectionPlan")
                .ok_or("Transient selection plan is required.")?,
        )?;
        let bindings = package
            .get("materialBindings")
            .and_then(|v| v.get(formulation))
            .ok_or("Scenario material bindings are required.")?;
        let (data, selections) = if bind_sources {
            TransientModelData::bind_selections(profile,&plan,declaration,bindings,&selection,model,map,mesh_tags)?
        } else {
            TransientModelData::bind_selections_without_sources(profile,&plan,declaration,bindings,&selection,model,map,mesh_tags)?
        };
        let initial = crate::transient_initial::compile_fields(&plan, initial_fields)?;
        let outputs = StateOutputPlan::from_json(&plan, &declaration["stateOutputs"])?;
        let tensor_output = declaration
            .get("tensorRecovery")
            .map(|value| TensorOutputPlan::from_json(&plan, value))
            .transpose()?;
        let zero_outputs = declaration
            .get("zeroOutputs")
            .map(|value| {
                value
                    .as_array()
                    .ok_or("Transient zeroOutputs must be an array.")?
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .ok_or(
                                "Transient zeroOutputs must contain nonempty strings.".to_string(),
                            )
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            model,
            map,
            plan,
            data,
            selections,
            initial,
            outputs,
            tensor_output,
            zero_outputs,
            boundary_laws: declaration
                .get("boundaryLaws")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            essential_forms: package
                .get("essentialForms")
                .and_then(|v| v.get(formulation))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            boundary_forms: declaration
                .get("boundaryForms")
                .or_else(|| {
                    package
                        .get("boundaryForms")
                        .and_then(|v| v.get(formulation))
                })
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        })
    }

    /// Assemble the declared scenario-neutral transient blocks once at the
    /// static coefficient sample used by an eigenfrequency Study. Applied
    /// volume sources are intentionally absent from this source-free pencil.
    pub(crate) fn assemble_eigen_nodal(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&crate::transient_mesh::NodalMesh<'_>,quadrature:&str,
    )->Result<(StateLayout,TransientOperators),String>{
        let counts=self.plan.fields.iter().map(|field|mesh.nodes.len().checked_mul(field.components)
            .ok_or("Eigenfrequency field DOF count overflow.".to_string())).collect::<Result<Vec<_>,_>>()?;
        let layout=StateLayout::new(&self.plan,&counts)?;
        let sampler=self.data.sampler(parameters);
        let operators=crate::transient_mesh::assemble_nodal_p1(&self.plan,&layout,mesh,0.,quadrature,
            &mut TransientAssemblyWorkspace::default(),|location|sampler.coefficients(location),&[],|_,_|
                Err("Eigenfrequency assembly does not evaluate applied sources.".into()))?;
        Ok((layout,operators))
    }

    pub(crate) fn assemble_eigen_edge(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&crate::transient_edge::EdgeTransientMesh,quadrature:&str,
    )->Result<(StateLayout,TransientOperators),String>{
        let layout=StateLayout::new(&self.plan,&vec![mesh.dof_count();self.plan.fields.len()])?;
        let sampler=self.data.sampler(parameters);
        let operators=mesh.assemble(&self.plan,&layout,0.,quadrature,&mut TransientAssemblyWorkspace::default(),
            |location|sampler.coefficients(location),&[],|_,_|Err("Eigenfrequency assembly does not evaluate applied sources.".into()))?;
        Ok((layout,operators))
    }

    pub(crate) fn assemble_eigen_oriented(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&impl crate::transient_runner::OrientedTransientSpace,
        quadrature:&str,
    )->Result<(StateLayout,TransientOperators),String>{
        let layout=StateLayout::new(&self.plan,&vec![mesh.dof_count();self.plan.fields.len()])?;
        let sampler=self.data.sampler(parameters);
        let mut coefficient=|location|sampler.coefficients(location);
        let mut source_value=|_:&str,_:crate::transient_plan::SampleLocation|->Result<Vec<C>,String>{
            Err("Eigenfrequency assembly does not evaluate applied sources.".into())
        };
        let operators=mesh.assemble(&self.plan,&layout,0.,quadrature,&mut TransientAssemblyWorkspace::default(),
            &mut coefficient,&[],&mut source_value)?;
        Ok((layout,operators))
    }

    pub(crate) fn nodal_eigen_snapshot(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&crate::transient_mesh::NodalMesh<'_>,
        layout:&StateLayout,state:&[C],study_id:&str,case_index:usize,mode_index:usize,eigenvalue:C,residual:f64,
        declarations:&[crate::model_schema::FieldVariable],
    )->Result<Value,String>{
        let fields=(0..self.plan.fields.len()).map(|field|crate::transient_mesh::nodal_field_components(&self.plan,layout,mesh,state,field))
            .collect::<Result<Vec<_>,_>>()?;
        let mut values=self.outputs.evaluate(&fields)?;
        if let Some(recovery)=&self.tensor_output {
            let sampler=self.data.sampler(parameters);
            for (name,field) in recovery.nodal_p1(&self.plan,layout,mesh,state,0.,|location|sampler.coefficients(location))?.nodal_symbols()? {
                if values.insert(name.clone(),field).is_some(){return Err(format!("Duplicate eigenfrequency output {name:?}."));}
            }
        }
        crate::transient_result::add_declared_zero_fields(declarations,&mut values,mesh.nodes.len())?;
        crate::transient_result::add_transient_zero_fields(&self.zero_outputs,declarations,&mut values,mesh.nodes.len())?;
        let cells=mesh.cells.iter().map(|cell|(cell.nodes.clone(),cell.tag)).collect::<Vec<_>>();
        let materials=self.data.material_fields(mesh.nodes,&cells,parameters,0.)?;
            crate::transient_result::eigenfrequency_mode_snapshot(study_id,case_index,mode_index,eigenvalue,residual,mesh.dimension,
            mesh.nodes,&cells,declarations,&values,Some(&materials))
    }

    pub(crate) fn edge_eigen_snapshot(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&crate::transient_edge::EdgeTransientMesh,
        nodes:&[[f64;3]],cells:&[(Vec<usize>,usize)],layout:&StateLayout,state:&[C],study_id:&str,case_index:usize,
          mode_index:usize,eigenvalue:C,residual:f64,declarations:&[crate::model_schema::FieldVariable],
    )->Result<Value,String>{
        if !mesh.matches_geometry(nodes,cells){return Err("Eigenfrequency edge output geometry differs from the solved mesh.".into());}
        if self.tensor_output.is_some(){return Err("Tensor recovery for this edge eigenfrequency space is not supported.".into());}
        let mut fields=Vec::with_capacity(self.plan.fields.len());
        for field in 0..self.plan.fields.len(){
            let samples=mesh.nodal_field_values(&self.plan,layout,state,field)?;
            let mut components=vec![Vec::with_capacity(samples.len());3];
            for sample in samples{for (component,value) in components.iter_mut().zip(sample){component.push(value);}}
            fields.push(components);
        }
        let mut values=self.outputs.evaluate(&fields)?;
        crate::transient_result::add_declared_zero_fields(declarations,&mut values,nodes.len())?;
        crate::transient_result::add_transient_zero_fields(&self.zero_outputs,declarations,&mut values,nodes.len())?;
        let materials=self.data.material_fields(nodes,cells,parameters,0.)?;
          crate::transient_result::eigenfrequency_mode_snapshot(study_id,case_index,mode_index,eigenvalue,residual,3,
            nodes,cells,declarations,&values,Some(&materials))
    }

    pub(crate) fn triangle_eigen_snapshot(
        &self,parameters:&HashMap<String,crate::model_expression::Scalar>,mesh:&crate::transient_triangle::TriangleTransientMesh,
        nodes:&[[f64;3]],cells:&[(Vec<usize>,usize)],layout:&StateLayout,state:&[C],study_id:&str,case_index:usize,
        mode_index:usize,eigenvalue:C,residual:f64,declarations:&[crate::model_schema::FieldVariable],
    )->Result<Value,String>{
        if !mesh.matches_geometry(nodes,cells){return Err("Eigenfrequency triangle output geometry differs from the solved mesh.".into());}
        if self.tensor_output.is_some(){return Err("Tensor recovery for this triangle eigenfrequency space is not supported.".into());}
        let mut fields=Vec::with_capacity(self.plan.fields.len());
        for field in 0..self.plan.fields.len(){
            let samples=mesh.nodal_field_values(&self.plan,layout,state,field)?;
            let mut components=vec![Vec::with_capacity(samples.len());3];
            for sample in samples{for (component,value) in components.iter_mut().zip(sample){component.push(value);}}
            fields.push(components);
        }
        let mut values=self.outputs.evaluate(&fields)?;
        crate::transient_result::add_declared_zero_fields(declarations,&mut values,nodes.len())?;
        crate::transient_result::add_transient_zero_fields(&self.zero_outputs,declarations,&mut values,nodes.len())?;
        let materials=self.data.material_fields(nodes,cells,parameters,0.)?;
        crate::transient_result::eigenfrequency_mode_snapshot(study_id,case_index,mode_index,eigenvalue,residual,2,
            nodes,cells,declarations,&values,Some(&materials))
    }

    pub(crate) fn prepare_nodal_boundaries(
        &self,
        mesh: &crate::transient_mesh::NodalMesh<'_>,
        facets: &[(Vec<usize>, usize)],
    ) -> Result<crate::transient_boundary::NodalBoundaries, String> {
        let selected = self
            .selections
            .get("boundaries")
            .ok_or("Transient boundary selection is required.")?;
        crate::transient_boundary::NodalBoundaries::prepare(
            &self.plan,
            &self.boundary_laws,
            &self.essential_forms,
            &self.boundary_forms,
            selected,
            mesh,
            facets,
        )
    }

    pub(crate) fn prepare_edge_boundaries(
        &self,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        facets: &[([usize; 3], usize)],
    ) -> Result<crate::transient_boundary::EdgeBoundaries, String> {
        let selected = self
            .selections
            .get("boundaries")
            .ok_or("Transient boundary selection is required.")?;
        crate::transient_boundary::EdgeBoundaries::prepare(
            &self.plan,
            &self.boundary_laws,
            &self.essential_forms,
            &self.boundary_forms,
            selected,
            mesh,
            facets,
        )
    }

    pub(crate) fn prepare_triangle_boundaries(
        &self,mesh:&crate::transient_triangle::TriangleTransientMesh,edges:&[([usize;2],usize)],
    )->Result<crate::transient_boundary::TriangleBoundaries,String>{
        let selected=self.selections.get("boundaries").ok_or("Transient boundary selection is required.")?;
        crate::transient_boundary::TriangleBoundaries::prepare(&self.plan,&self.boundary_laws,&self.essential_forms,
            selected,mesh,edges)
    }

    /// Run with the declared oriented boundary adapter, validating the
    /// initial trace before any constrained projection can alter it.
    pub(crate) fn run_edge_with_boundaries(
        &self,
        prepared: &PreparedTransientCase,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        boundaries: &crate::transient_boundary::EdgeBoundaries,
        quadrature: &str,
        mapping: Option<&FixedDofMap<'_>>,
        options: &LinearOptions<'_>,
        cache: &mut SolverCache,
        observe: impl FnMut(&TransientPlan, &StateLayout, f64, &[C], bool) -> Result<(), String>,
    ) -> Result<Vec<C>, String> {
        boundaries.validate_initial(
            &self.plan,
            mesh,
            &self.initial,
            &prepared.parameters,
            prepared.grid.start,
        )?;
        self.run_edge(
            prepared,
            mesh,
            quadrature,
            mapping,
            options,
            cache,
            |_, plan, layout, time, operators| {
                boundaries.apply(plan, layout, mesh, &prepared.parameters, time, operators)
            },
            observe,
        )
    }

    pub(crate) fn run_triangle_with_boundaries(
        &self,prepared:&PreparedTransientCase,mesh:&crate::transient_triangle::TriangleTransientMesh,
        boundaries:&crate::transient_boundary::TriangleBoundaries,quadrature:&str,mapping:Option<&FixedDofMap<'_>>,
        options:&LinearOptions<'_>,cache:&mut SolverCache,
        observe:impl FnMut(&TransientPlan,&StateLayout,f64,&[C],bool)->Result<(),String>,
    )->Result<Vec<C>,String>{
        boundaries.validate_initial(&self.plan,mesh,&self.initial,&prepared.parameters,prepared.grid.start)?;
        self.run_oriented(prepared,mesh,quadrature,mapping,options,cache,
            |_,plan,layout,time,operators|boundaries.apply(plan,layout,mesh,&prepared.parameters,time,operators),observe)
    }

    /// Form a nodal snapshot directly from one observed solver state. The
    /// caller chooses when and where to publish the returned document.
    pub(crate) fn nodal_snapshot(
        &self,
        prepared: &PreparedTransientCase,
        mesh: &crate::transient_mesh::NodalMesh<'_>,
        layout: &StateLayout,
        state: &[C],
        identity: SnapshotIdentity<'_>,
        declarations: &[crate::model_schema::FieldVariable],
        include_materials: bool,
    ) -> Result<Value, String> {
        let fields = (0..self.plan.fields.len())
            .map(|field| {
                crate::transient_mesh::nodal_field_components(
                    &self.plan, layout, mesh, state, field,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut values = self.outputs.evaluate(&fields)?;
        if let Some(recovery) = &self.tensor_output {
            let sampler = self.data.sampler(&prepared.parameters);
            let recovered = recovery
                .nodal_p1(&self.plan, layout, mesh, state, identity.time, |location| {
                    sampler.coefficients(location)
                })?
                .nodal_symbols()?;
            for (name, field) in recovered {
                if values.insert(name.clone(), field).is_some() {
                    return Err(format!("Duplicate transient output {name:?}."));
                }
            }
        }
        crate::transient_result::add_declared_zero_fields(
            declarations,
            &mut values,
            mesh.nodes.len(),
        )?;
        crate::transient_result::add_transient_zero_fields(
            &self.zero_outputs,
            declarations,
            &mut values,
            mesh.nodes.len(),
        )?;
        let cells = mesh
            .cells
            .iter()
            .map(|cell| (cell.nodes.clone(), cell.tag))
            .collect::<Vec<_>>();
        let materials = if include_materials {
            Some(self.data.material_fields(
                mesh.nodes,
                &cells,
                &prepared.parameters,
                identity.time,
            )?)
        } else {
            None
        };
        crate::transient_result::field_snapshot(
            identity.study_id,
            identity.case_index,
            identity.sample_index,
            identity.time,
            mesh.dimension,
            mesh.nodes,
            &cells,
            declarations,
            &values,
            materials.as_ref(),
        )
    }

    /// Oriented fields are sampled at nodes for visualization; their state
    /// remains in the edge basis and is not interpreted as nodal DOFs.
    pub(crate) fn edge_snapshot(
        &self,
        prepared: &PreparedTransientCase,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        nodes: &[[f64; 3]],
        cells: &[(Vec<usize>, usize)],
        layout: &StateLayout,
        state: &[C],
        identity: SnapshotIdentity<'_>,
        declarations: &[crate::model_schema::FieldVariable],
        include_materials: bool,
    ) -> Result<Value, String> {
        if !mesh.matches_geometry(nodes, cells) {
            return Err("Transient edge snapshot geometry differs from the solved mesh.".into());
        }
        if self.tensor_output.is_some() {
            return Err("Tensor recovery for this edge space is not supported.".into());
        }
        let fields = (0..self.plan.fields.len())
            .map(|field| {
                let samples = mesh.nodal_field_values(&self.plan, layout, state, field)?;
                let mut components = vec![Vec::with_capacity(samples.len()); 3];
                for sample in samples {
                    for (column, value) in components.iter_mut().zip(sample) {
                        column.push(value);
                    }
                }
                Ok(components)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut values = self.outputs.evaluate(&fields)?;
        crate::transient_result::add_declared_zero_fields(declarations, &mut values, nodes.len())?;
        crate::transient_result::add_transient_zero_fields(
            &self.zero_outputs,
            declarations,
            &mut values,
            nodes.len(),
        )?;
        let materials = if include_materials {
            Some(
                self.data
                    .material_fields(nodes, cells, &prepared.parameters, identity.time)?,
            )
        } else {
            None
        };
        crate::transient_result::field_snapshot(
            identity.study_id,
            identity.case_index,
            identity.sample_index,
            identity.time,
            3,
            nodes,
            cells,
            declarations,
            &values,
            materials.as_ref(),
        )
    }

    pub(crate) fn triangle_snapshot(
        &self,prepared:&PreparedTransientCase,mesh:&crate::transient_triangle::TriangleTransientMesh,
        nodes:&[[f64;3]],cells:&[(Vec<usize>,usize)],layout:&StateLayout,state:&[C],identity:SnapshotIdentity<'_>,
        declarations:&[crate::model_schema::FieldVariable],include_materials:bool,
    )->Result<Value,String>{
        if !mesh.matches_geometry(nodes,cells){return Err("Transient triangle snapshot geometry differs from the solved mesh.".into());}
        if self.tensor_output.is_some(){return Err("Tensor recovery for this triangle space is not supported.".into());}
        let mut fields=Vec::with_capacity(self.plan.fields.len());
        for field in 0..self.plan.fields.len(){
            let samples=mesh.nodal_field_values(&self.plan,layout,state,field)?;
            let mut components=vec![Vec::with_capacity(samples.len());3];
            for sample in samples{for (component,value) in components.iter_mut().zip(sample){component.push(value);}}
            fields.push(components);
        }
        let mut values=self.outputs.evaluate(&fields)?;
        crate::transient_result::add_declared_zero_fields(declarations,&mut values,nodes.len())?;
        crate::transient_result::add_transient_zero_fields(&self.zero_outputs,declarations,&mut values,nodes.len())?;
        let materials=if include_materials{Some(self.data.material_fields(nodes,cells,&prepared.parameters,identity.time)?)}else{None};
        crate::transient_result::field_snapshot(identity.study_id,identity.case_index,identity.sample_index,identity.time,
            2,nodes,cells,declarations,&values,materials.as_ref())
    }

    pub(crate) fn run_nodal(
        &self,
        prepared: &PreparedTransientCase,
        mesh: &crate::transient_mesh::NodalMesh<'_>,
        quadrature: &str,
        mapping: Option<&FixedDofMap<'_>>,
        options: &LinearOptions<'_>,
        cache: &mut SolverCache,
        mut boundary: impl FnMut(
            &HashMap<String, Vec<BoundSelection<'a>>>,
            &TransientPlan,
            &StateLayout,
            f64,
            &mut TransientOperators,
        ) -> Result<(), String>,
        mut observe: impl FnMut(&TransientPlan, &StateLayout, f64, &[C], bool) -> Result<(), String>,
    ) -> Result<Vec<C>, String> {
        let counts = self
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
        let layout = StateLayout::new(&self.plan, &counts)?;
        let sampler = self.data.sampler(&prepared.parameters);
        let sources = self.data.sources();
        crate::transient_runner::run_nodal(
            &self.plan,
            &layout,
            mesh,
            &prepared.grid,
            quadrature,
            &prepared.parameters,
            &self.initial,
            mapping,
            &mut TransientAssemblyWorkspace::default(),
            options,
            cache,
            |location| sampler.coefficients(location),
            &sources,
            |feature_type, location| sampler.source_for(feature_type, location),
            |time, ops| boundary(&self.selections, &self.plan, &layout, time, ops),
            |time, state, output| observe(&self.plan, &layout, time, state, output),
        )
    }

    pub(crate) fn run_edge(
        &self,
        prepared: &PreparedTransientCase,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        quadrature: &str,
        mapping: Option<&FixedDofMap<'_>>,
        options: &LinearOptions<'_>,
        cache: &mut SolverCache,
        mut boundary: impl FnMut(
            &HashMap<String, Vec<BoundSelection<'a>>>,
            &TransientPlan,
            &StateLayout,
            f64,
            &mut TransientOperators,
        ) -> Result<(), String>,
        mut observe: impl FnMut(&TransientPlan, &StateLayout, f64, &[C], bool) -> Result<(), String>,
    ) -> Result<Vec<C>, String> {
        self.run_oriented(prepared,mesh,quadrature,mapping,options,cache,boundary,observe)
    }

    pub(crate) fn run_oriented(
        &self,
        prepared:&PreparedTransientCase,
        mesh:&impl crate::transient_runner::OrientedTransientSpace,
        quadrature:&str,
        mapping:Option<&FixedDofMap<'_>>,
        options:&LinearOptions<'_>,
        cache:&mut SolverCache,
        mut boundary:impl FnMut(&HashMap<String,Vec<BoundSelection<'a>>>,&TransientPlan,&StateLayout,f64,&mut TransientOperators)->Result<(),String>,
        mut observe:impl FnMut(&TransientPlan,&StateLayout,f64,&[C],bool)->Result<(),String>,
    )->Result<Vec<C>,String>{
        let layout = StateLayout::new(&self.plan, &vec![mesh.dof_count(); self.plan.fields.len()])?;
        let sampler = self.data.sampler(&prepared.parameters);
        let sources = self.data.sources();
        crate::transient_runner::run_oriented(
            &self.plan,
            &layout,
            mesh,
            &prepared.grid,
            quadrature,
            &prepared.parameters,
            &self.initial,
            mapping,
            &mut TransientAssemblyWorkspace::default(),
            options,
            cache,
            |location| sampler.coefficients(location),
            &sources,
            |feature_type, location| sampler.source_for(feature_type, location),
            |time, ops| boundary(&self.selections, &self.plan, &layout, time, ops),
            |time, state, output| observe(&self.plan, &layout, time, state, output),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transient_mesh::{NodalCell, NodalMesh};
    use serde_json::json;
    #[test]
    fn prepared_case_binds_scenario_material_source_and_observes_each_requested_time() {
        for (dimension, profile, text) in [
            (2, "thermal-2d", include_str!("scenarios/thermal2d.json")),
            (3, "thermal-3d", include_str!("scenarios/thermal3d.json")),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{
                "id":"material","selectionId":"domain",
                "volumetricHeatCapacity":{"expression":"C0*(1+t/tau)","unit":"W*s/(m^3*K)"}}],
                "physics":{"source":{"id":"source","type":"domainSource","selectionId":"domain",
                "values":[{"expression":"1","unit":"W/m^3"}]}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![]),
            ]);
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                "T",
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
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
            let params = HashMap::from([
                (
                    "C0".into(),
                    crate::model_expression::Scalar {
                        value: 2.,
                        dimension: crate::model_expression::unit("W*s/(m^3*K)").unwrap().1,
                    },
                ),
                (
                    "tau".into(),
                    crate::model_expression::Scalar {
                        value: 1.,
                        dimension: crate::model_expression::unit("s").unwrap().1,
                    },
                ),
            ]);
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 1.,
                    max_step: 0.25,
                    max_steps: 4,
                    output_times: vec![0.5, 1.],
                },
                parameters: params,
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
            let options = LinearOptions {
                tolerance: 1e-10,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut output = Vec::new();
            let declarations =
                crate::model_schema::field_variables_for_profile(profile, Some("T"), false)
                    .unwrap();
            let result = bound
                .run_nodal(
                    &prepared,
                    &mesh,
                    "standard",
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |selected, _, _, _, _| {
                        assert_eq!(selected["domainSources"].len(), 1);
                        Ok(())
                    },
                    |_, layout, time, state, save| {
                        if save {
                            let document = bound.nodal_snapshot(
                                &prepared,
                                &mesh,
                                layout,
                                state,
                                SnapshotIdentity {
                                    study_id: "thermal",
                                    case_index: 2,
                                    sample_index: output.len(),
                                    time,
                                },
                                &declarations,
                                true,
                            )?;
                            assert_eq!(document["caseIndex"], 2);
                            assert_eq!(document["sampleIndex"], output.len());
                            let weights=vec![1./nodes.len() as f64;nodes.len()];
                            let point=bound.nodal_state_symbols_at(&mesh,layout,state,&cells[0].nodes,&weights)?;
                            assert!((point["T"].re-state[0].re).abs()<1e-10);
                            let capacity = document["materialFieldsByCell"]["Cvol"]["real"][0][0]
                                .as_f64()
                                .unwrap();
                            assert!((capacity - 2. * (1. + time)).abs() < 1e-10);
                            let material=bound.data.material_symbols_at(&prepared.parameters,7,[0.2,0.2,0.],time)?;
                            assert!((material["Cvol"].re-capacity).abs()<1e-10);
                            output.push((time, state[0].re));
                        }
                        Ok(())
                    },
                )
                .unwrap();
            let expected = (1..=4)
                .map(|step| 0.25 / (2. * (1. + step as f64 * 0.25)))
                .sum::<f64>();
            assert!(result.iter().all(|v| (v.re - expected).abs() < 1e-10));
            assert_eq!(output.len(), 2);
            assert_eq!((output[0].0, output[1].0), (0.5, 1.));
            assert!((output[1].1 - expected).abs() < 1e-10);
            assert!(
                BoundTransientCase::prepare(
                    &package,
                    "wrong",
                    "T",
                    &model,
                    &map,
                    &tags,
                    &json!({})
                )
                .is_err()
            );
        }
    }

    #[test]
    fn all_declared_formulations_run_through_the_same_case_binding() {
        for text in [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),
            include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),
            include_str!("scenarios/thermal3d.json"),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let profile = package["id"].as_str().unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let mut material = json!({"id":"material","selectionId":"domain"});
            if profile.starts_with("thermal-") {
                material["volumetricHeatCapacity"] = json!({"expression":"2","unit":"W*s/(m^3*K)"});
            }
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[material],"physics":{}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![]),
            ]);
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
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 0.1,
                    max_step: 0.1,
                    max_steps: 1,
                    output_times: vec![0.1],
                },
                parameters: HashMap::new(),
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
            let options = LinearOptions {
                tolerance: 1e-10,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            for formulation in package["transientForms"].as_object().unwrap().keys() {
                let bound = BoundTransientCase::prepare(
                    &package,
                    profile,
                    formulation,
                    &model,
                    &map,
                    &tags,
                    &json!({}),
                )
                .unwrap_or_else(|error| panic!("{profile}/{formulation}: {error}"));
                let mut samples = 0;
                let declarations = crate::model_schema::field_variables_for_profile(
                    profile,
                    Some(formulation),
                    false,
                )
                .unwrap();
                let snapshot_cells = cells
                    .iter()
                    .map(|c| (c.nodes.clone(), c.tag))
                    .collect::<Vec<_>>();
                let result = if package["schemaDocument"]["x-fem-scenario"]["execution"]["kind"]
                    .as_str()
                    == Some("orientedTetrahedronNedelec")
                {
                    let edge = crate::transient_edge::EdgeTransientMesh::prepare(
                        &nodes,
                        &[([0, 1, 2, 3], 7)],
                        1,
                    )
                    .unwrap();
                    bound.run_edge(
                        &prepared,
                        &edge,
                        "standard",
                        None,
                        &options,
                        &mut SolverCache::default(),
                        |_, _, _, _, _| Ok(()),
                        |_, layout, time, state, save| {
                            if save {
                                let snapshot = bound.edge_snapshot(
                                    &prepared,
                                    &edge,
                                    &nodes,
                                    &snapshot_cells,
                                    layout,
                                    state,
                                    SnapshotIdentity {
                                        study_id: "study",
                                        case_index: 0,
                                        sample_index: 0,
                                        time,
                                    },
                                    &declarations,
                                    true,
                                )?;
                                assert_eq!(snapshot["schema"], "fullmeta-transient-field/1");
                                let point=bound.edge_state_symbols_at(&edge,layout,state,0,[0.1,0.2,0.3])?;
                                for (symbol,value) in point {assert_eq!(value.re,snapshot["fields"][&symbol]["real"][0].as_f64().unwrap());}
                                let mut wrong_cells = snapshot_cells.clone();
                                wrong_cells[0].1 += 1;
                                assert!(
                                    bound
                                        .edge_snapshot(
                                            &prepared,
                                            &edge,
                                            &nodes,
                                            &wrong_cells,
                                            layout,
                                            state,
                                            SnapshotIdentity {
                                                study_id: "study",
                                                case_index: 0,
                                                sample_index: 0,
                                                time
                                            },
                                            &declarations,
                                            false
                                        )
                                        .is_err()
                                );
                                samples += 1;
                            }
                            Ok(())
                        },
                    )
                } else {
                    bound.run_nodal(
                        &prepared,
                        &mesh,
                        "standard",
                        None,
                        &options,
                        &mut SolverCache::default(),
                        |_, _, _, _, _| Ok(()),
                        |_, layout, time, state, save| {
                            if save {
                                let snapshot = bound.nodal_snapshot(
                                    &prepared,
                                    &mesh,
                                    layout,
                                    state,
                                    SnapshotIdentity {
                                        study_id: "study",
                                        case_index: 0,
                                        sample_index: 0,
                                        time,
                                    },
                                    &declarations,
                                    true,
                                )?;
                                assert_eq!(snapshot["schema"], "fullmeta-transient-field/1");
                                let weights=vec![1./nodes.len() as f64;nodes.len()];
                                let point=bound.nodal_state_symbols_at(&mesh,layout,state,&cells[0].nodes,&weights)?;
                                for (symbol,value) in point {assert_eq!(value.re,snapshot["fields"][&symbol]["real"][0].as_f64().unwrap());}
                                samples += 1;
                            }
                            Ok(())
                        },
                    )
                }
                .unwrap_or_else(|error| panic!("{profile}/{formulation}: {error}"));
                assert!(
                    result.iter().all(|v| v.re == 0. && v.im == 0.),
                    "{profile}/{formulation}"
                );
                assert_eq!(samples, 1, "{profile}/{formulation}");
            }
        }
    }

    #[test]
    fn declared_nodal_boundaries_apply_spatial_and_time_dependent_values() {
        for (profile, formulation, unit, text) in [
            (
                "thermal-2d",
                "T",
                "K",
                include_str!("scenarios/thermal2d.json"),
            ),
            (
                "thermal-3d",
                "T",
                "K",
                include_str!("scenarios/thermal3d.json"),
            ),
            (
                "acoustic-2d",
                "pressure",
                "Pa",
                include_str!("scenarios/acoustic2d.json"),
            ),
            (
                "acoustic-3d",
                "pressure",
                "Pa",
                include_str!("scenarios/acoustic3d.json"),
            ),
            ("em-2d", "Ez", "V/m", include_str!("scenarios/em2d.json")),
            ("em-2d", "Hz", "A/m", include_str!("scenarios/em2d.json")),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let mut material = json!({"id":"material","selectionId":"domain"});
            if profile.starts_with("thermal-") {
                material["volumetricHeatCapacity"] = json!({"expression":"2","unit":"W*s/(m^3*K)"});
            }
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[material],"physics":{
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"(t/tau)*(1+x/L)","unit":unit}}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                formulation,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundaries = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let params = HashMap::from([
                (
                    "tau".into(),
                    crate::model_expression::Scalar {
                        value: 1.,
                        dimension: crate::model_expression::unit("s").unwrap().1,
                    },
                ),
                (
                    "L".into(),
                    crate::model_expression::Scalar {
                        value: 1.,
                        dimension: crate::model_expression::unit("m").unwrap().1,
                    },
                ),
            ]);
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 1.,
                    max_step: 0.5,
                    max_steps: 2,
                    output_times: vec![0.5, 1.],
                },
                parameters: params,
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
            let options = LinearOptions {
                tolerance: 1e-6,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut outputs = 0;
            bound
                .run_nodal(
                    &prepared,
                    &mesh,
                    "standard",
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, plan, layout, time, ops| {
                        boundaries.apply(plan, layout, &mesh, &prepared.parameters, time, ops)
                    },
                    |plan, layout, time, state, save| {
                        if save {
                            let values = layout.field_state(0, state)?;
                            assert!(
                                (values[0].re - time).abs() < 1e-9,
                                "{profile}/{formulation}"
                            );
                            assert!(
                                (values[1].re - 2. * time).abs() < 1e-9,
                                "{profile}/{formulation}"
                            );
                            assert_eq!(plan.fields[0].components, 1);
                            outputs += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap_or_else(|error| panic!("{profile}/{formulation}: {error}"));
            assert_eq!(outputs, 2);
            assert!(
                bound
                    .prepare_nodal_boundaries(&mesh, &[(vec![0], 8)])
                    .is_err()
            );
        }
    }

    #[test]
    fn thermal_flux_trace_conserves_the_time_integrated_heat() {
        for (profile, text) in [
            ("thermal-2d", include_str!("scenarios/thermal2d.json")),
            ("thermal-3d", include_str!("scenarios/thermal3d.json")),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{"id":"material","selectionId":"domain",
                "volumetricHeatCapacity":{"expression":"2","unit":"W*s/(m^3*K)"}}],"physics":{
                "wall":{"id":"wall","type":"neumann","selectionId":"wall","value":{"expression":"-t/tau","unit":"W/m^2"}}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                "T",
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundaries = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let params = HashMap::from([(
                "tau".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("s").unwrap().1,
                },
            )]);
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 1.,
                    max_step: 0.5,
                    max_steps: 2,
                    output_times: vec![1.],
                },
                parameters: params,
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
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
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, plan, layout, time, ops| {
                        boundaries.apply(plan, layout, &mesh, &prepared.parameters, time, ops)
                    },
                    |_, _, _, _, _| Ok(()),
                )
                .unwrap_or_else(|error| panic!("{profile}: {error}"));
            let mean = result.iter().map(|v| v.re).sum::<f64>() / result.len() as f64;
            let expected = if dimension == 2 { 0.75 } else { 1.125 };
            assert!(
                (mean - expected).abs() < 1e-9,
                "{profile}: {mean} != {expected}"
            );
            let mut vector_model = model.clone();
            vector_model["model"]["physics"]["wall"]["value"]["expression"] = json!("0");
            vector_model["model"]["physics"]["wall"]["fieldValues"]=json!((0..dimension).map(|axis|
                json!({"expression":if axis==dimension-1 {"t/tau"}else{"0"},"unit":"W/m^2"})).collect::<Vec<_>>());
            let vector_bound = BoundTransientCase::prepare(
                &package,
                profile,
                "T",
                &vector_model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let vector_facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let vector_bc = vector_bound
                .prepare_nodal_boundaries(&mesh, &[(vector_facet, 8)])
                .unwrap();
            let layout = StateLayout::new(&vector_bound.plan, &[nodes.len()]).unwrap();
            let mut operators = vector_bound.plan.empty_operators(&layout);
            vector_bc
                .apply(
                    &vector_bound.plan,
                    &layout,
                    &mesh,
                    &prepared.parameters,
                    1.,
                    &mut operators,
                )
                .unwrap();
            let total = operators.load.iter().map(|v| v.re).sum::<f64>();
            let expected_load = if dimension == 2 { 1. } else { 0.5 };
            assert!(
                (total - expected_load).abs() < 1e-10,
                "{profile} vector flux load: {total}"
            );
        }
    }

    #[test]
    fn elastic_traction_loads_the_declared_momentum_block() {
        for (profile, formulation, text) in [
            (
                "elastic-2d",
                "planeStrain",
                include_str!("scenarios/elastic2d.json"),
            ),
            (
                "elastic-3d",
                "displacement",
                include_str!("scenarios/elastic3d.json"),
            ),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let vector = (0..dimension)
                .map(|axis| json!({"expression":format!("{}*t/tau",axis+2),"unit":"Pa"}))
                .collect::<Vec<_>>();
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{
                "wall":{"id":"wall","type":"neumann","selectionId":"wall","value":{"expression":"0","unit":"Pa"},"fieldValues":vector}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                formulation,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundaries = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let counts = bound
                .plan
                .fields
                .iter()
                .map(|field| nodes.len() * field.components)
                .collect::<Vec<_>>();
            let layout = StateLayout::new(&bound.plan, &counts).unwrap();
            let mut operators = bound.plan.empty_operators(&layout);
            let params = HashMap::from([(
                "tau".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("s").unwrap().1,
                },
            )]);
            boundaries
                .apply(&bound.plan, &layout, &mesh, &params, 1., &mut operators)
                .unwrap();
            let displacement = layout.field_state(0, &operators.load).unwrap();
            assert!(displacement.iter().all(|v| v.re == 0.));
            let momentum = layout.field_state(1, &operators.load).unwrap();
            let measure = if dimension == 2 { 1. } else { 0.5 };
            for axis in 0..dimension {
                let load = momentum
                    .chunks_exact(dimension)
                    .map(|row| row[axis].re)
                    .sum::<f64>();
                assert!(
                    (load - (axis + 2) as f64 * measure).abs() < 1e-10,
                    "{profile} axis {axis}: {load}"
                );
            }
        }
    }

    #[test]
    fn elastic_prescribed_displacement_and_velocity_follow_the_same_time_step() {
        for (profile, formulation, text) in [
            (
                "elastic-2d",
                "planeStress",
                include_str!("scenarios/elastic2d.json"),
            ),
            (
                "elastic-2d",
                "planeStrain",
                include_str!("scenarios/elastic2d.json"),
            ),
            (
                "elastic-3d",
                "displacement",
                include_str!("scenarios/elastic3d.json"),
            ),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let vector = (0..dimension)
                .map(|axis| json!({"expression":if axis==0 {"(t/tau)^2"}else{"0"},"unit":"m"}))
                .collect::<Vec<_>>();
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"m"},"fieldValues":vector}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                formulation,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundaries = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let params = HashMap::from([(
                "tau".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("s").unwrap().1,
                },
            )]);
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 1.,
                    max_step: 0.5,
                    max_steps: 2,
                    output_times: vec![0.5, 1.],
                },
                parameters: params,
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
            let options = LinearOptions {
                tolerance: 1e-8,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut samples = 0;
            bound
                .run_nodal(
                    &prepared,
                    &mesh,
                    "standard",
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, plan, layout, time, ops| {
                        boundaries.apply(plan, layout, &mesh, &prepared.parameters, time, ops)
                    },
                    |_, layout, time, state, save| {
                        if save {
                            let u = layout.field_state(0, state)?;
                            let v = layout.field_state(1, state)?;
                            assert!(
                                (u[0].re - time * time).abs() < 1e-9,
                                "{profile}/{formulation}"
                            );
                            assert!(
                                (v[0].re - (2. * time - 0.5)).abs() < 1e-9,
                                "{profile}/{formulation}"
                            );
                            samples += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap_or_else(|error| panic!("{profile}/{formulation}: {error}"));
            assert_eq!(samples, 2);
        }
    }

    #[test]
    fn acoustic_normal_velocity_produces_the_declared_pressure_balance() {
        for (profile, text) in [
            ("acoustic-2d", include_str!("scenarios/acoustic2d.json")),
            ("acoustic-3d", include_str!("scenarios/acoustic3d.json")),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{"id":"material","selectionId":"domain",
                "density":{"expression":"1","unit":"kg/m^3"},"soundSpeed":{"expression":"1","unit":"m/s"}}],"physics":{
                "wall":{"id":"wall","type":"neumann","selectionId":"wall","value":{"expression":"t/tau","unit":"m/s"}}}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                "pressure",
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundaries = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let params = HashMap::from([(
                "tau".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("s").unwrap().1,
                },
            )]);
            let prepared = PreparedTransientCase {
                grid: crate::transient_request::TimeGrid {
                    start: 0.,
                    end: 1.,
                    max_step: 0.5,
                    max_steps: 2,
                    output_times: vec![1.],
                },
                parameters: params,
                overrides: vec![],
                longitudinal_wavenumber: C::default(),
            };
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
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, plan, layout, time, ops| {
                        boundaries.apply(plan, layout, &mesh, &prepared.parameters, time, ops)
                    },
                    |_, _, _, _, _| Ok(()),
                )
                .unwrap_or_else(|error| panic!("{profile}: {error}"));
            let mean = result[..nodes.len()].iter().map(|v| v.re).sum::<f64>() / nodes.len() as f64;
            let expected = if dimension == 2 { -1.5 } else { -2.25 };
            assert!(
                (mean - expected).abs() < 1e-9,
                "{profile}: {mean} != {expected}"
            );
            let mut vector_model = model.clone();
            vector_model["model"]["physics"]["wall"]["value"]["expression"] = json!("0");
            vector_model["model"]["physics"]["wall"]["fieldValues"]=json!((0..dimension).map(|axis|
                json!({"expression":if axis==dimension-1 {"-t/tau"}else{"0"},"unit":"m/s"})).collect::<Vec<_>>());
            let vector_bound = BoundTransientCase::prepare(
                &package,
                profile,
                "pressure",
                &vector_model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let vector_bc = vector_bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let counts = vector_bound
                .plan
                .fields
                .iter()
                .map(|field| nodes.len() * field.components)
                .collect::<Vec<_>>();
            let layout = StateLayout::new(&vector_bound.plan, &counts).unwrap();
            let mut operators = vector_bound.plan.empty_operators(&layout);
            vector_bc
                .apply(
                    &vector_bound.plan,
                    &layout,
                    &mesh,
                    &prepared.parameters,
                    1.,
                    &mut operators,
                )
                .unwrap();
            let total = layout
                .field_state(0, &operators.load)
                .unwrap()
                .iter()
                .map(|v| v.re)
                .sum::<f64>();
            let expected_load = if dimension == 2 { -1. } else { -0.5 };
            assert!(
                (total - expected_load).abs() < 1e-10,
                "{profile} vector normal velocity load: {total}"
            );
        }
    }

    #[test]
    fn declared_instantaneous_impedance_adds_the_correct_sparse_boundary_blocks() {
        for (profile, formulation, acoustic, text) in [
            (
                "acoustic-2d",
                "pressure",
                true,
                include_str!("scenarios/acoustic2d.json"),
            ),
            (
                "acoustic-3d",
                "pressure",
                true,
                include_str!("scenarios/acoustic3d.json"),
            ),
            (
                "elastic-2d",
                "planeStrain",
                false,
                include_str!("scenarios/elastic2d.json"),
            ),
            (
                "elastic-3d",
                "displacement",
                false,
                include_str!("scenarios/elastic3d.json"),
            ),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let dimension = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let mut wall = json!({"id":"wall","type":"impedance","selectionId":"wall","impedance":{"expression":"2*(1+t/tau)","unit":"Pa*s/m"}});
            if acoustic {
                wall["incidentField"] = json!({"expression":"t/tau","unit":"1"});
                wall["polarization"] = json!([{"expression":"1","unit":"Pa"}]);
            }
            let model = json!({"scenario":{"profileId":profile},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{"wall":wall}}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let tags = HashMap::from([
                ("materials".into(), vec![7]),
                ("domainSources".into(), vec![7]),
                ("boundaries".into(), vec![8]),
            ]);
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
            let facet = if dimension == 2 {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            let bound = BoundTransientCase::prepare(
                &package,
                profile,
                formulation,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundary = bound
                .prepare_nodal_boundaries(&mesh, &[(facet, 8)])
                .unwrap();
            let counts = bound
                .plan
                .fields
                .iter()
                .map(|field| nodes.len() * field.components)
                .collect::<Vec<_>>();
            let layout = StateLayout::new(&bound.plan, &counts).unwrap();
            let mut operators = bound.plan.empty_operators(&layout);
            let params = HashMap::from([(
                "tau".into(),
                crate::model_expression::Scalar {
                    value: 1.,
                    dimension: crate::model_expression::unit("s").unwrap().1,
                },
            )]);
            boundary
                .apply(&bound.plan, &layout, &mesh, &params, 1., &mut operators)
                .unwrap();
            // Positive real impedance must produce a nonnegative quadratic
            // boundary contribution in its declared state block.
            let probe = (0..layout.dimension)
                .map(|i| ((i % 7) as f64) - 3.)
                .collect::<Vec<_>>();
            let energy = operators
                .operator
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    row.iter()
                        .map(|(&j, value)| probe[i] * value.re * probe[j])
                        .sum::<f64>()
                })
                .sum::<f64>();
            assert!(energy >= -1e-10, "{profile} impedance energy {energy}");
            let measure = if dimension == 2 { 1. } else { 0.5 };
            if acoustic {
                let matrix = operators
                    .operator
                    .iter()
                    .flat_map(|row| row.values())
                    .map(|v| v.re)
                    .sum::<f64>();
                let load = operators.load.iter().map(|v| v.re).sum::<f64>();
                assert!(
                    (matrix - measure / 4.).abs() < 1e-10,
                    "{profile} matrix {matrix}"
                );
                assert!((load - measure / 2.).abs() < 1e-10, "{profile} load {load}");
            } else {
                let matrix = operators
                    .operator
                    .iter()
                    .flat_map(|row| row.values())
                    .map(|v| v.re)
                    .sum::<f64>();
                assert!(
                    (matrix - 4. * measure * dimension as f64).abs() < 1e-10,
                    "{profile} matrix {matrix}"
                );
                let displacement = layout.field_state(0, &operators.load).unwrap();
                assert!(displacement.iter().all(|v| v.re == 0.));
            }
        }
    }

    #[test]
    fn electromagnetic_2d_tangential_and_impedance_boundary_contracts_have_physical_signs() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let nodes = vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let cells = [NodalCell {
            nodes: vec![0, 1, 2],
            tag: 7,
        }];
        let mesh = NodalMesh {
            dimension: 2,
            nodes: &nodes,
            cells: &cells,
        };
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
        let tags = HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7]),
            ("boundaries".into(), vec![8]),
        ]);
        for (form, primary_unit, vector_unit, scalar_unit, sign) in [
            ("Ez", "V/m", "A/m", "V/m^2", 1.),
            ("Hz", "A/m", "V/m", "A/m^2", -1.),
        ] {
            let mut model = json!({"scenario":{"profileId":"em-2d"},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{
                "wall":{"id":"wall","type":"neumann","selectionId":"wall","value":{"expression":"0","unit":scalar_unit},
                    "fieldValues":[{"expression":"1","unit":vector_unit},{"expression":"0","unit":vector_unit}]}}}});
            let bound = BoundTransientCase::prepare(
                &package,
                "em-2d",
                form,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let boundary = bound
                .prepare_nodal_boundaries(&mesh, &[(vec![0, 1], 8)])
                .unwrap();
            let counts = bound
                .plan
                .fields
                .iter()
                .map(|field| nodes.len() * field.components)
                .collect::<Vec<_>>();
            let layout = StateLayout::new(&bound.plan, &counts).unwrap();
            let mut operators = bound.plan.empty_operators(&layout);
            boundary
                .apply(
                    &bound.plan,
                    &layout,
                    &mesh,
                    &HashMap::new(),
                    0.,
                    &mut operators,
                )
                .unwrap();
            let load = layout
                .field_state(0, &operators.load)
                .unwrap()
                .iter()
                .map(|v| v.re)
                .sum::<f64>();
            assert!((load - sign).abs() < 1e-10, "{form} tangential load {load}");
            let mut bad = model.clone();
            bad["model"]["physics"]["wall"]["value"]["expression"] = json!("1");
            let bad_bound =
                BoundTransientCase::prepare(&package, "em-2d", form, &bad, &map, &tags, &json!({}))
                    .unwrap();
            let bad_bc = bad_bound
                .prepare_nodal_boundaries(&mesh, &[(vec![0, 1], 8)])
                .unwrap();
            assert!(
                bad_bc
                    .apply(
                        &bad_bound.plan,
                        &layout,
                        &mesh,
                        &HashMap::new(),
                        0.,
                        &mut bad_bound.plan.empty_operators(&layout)
                    )
                    .is_err()
            );
            model["model"]["physics"]["wall"] = json!({"id":"wall","type":"impedance","selectionId":"wall",
                "impedance":{"expression":"2","unit":"ohm"},"incidentField":{"expression":"1","unit":"1"},
                "polarization":[{"expression":"1","unit":primary_unit}]});
            let imp = BoundTransientCase::prepare(
                &package,
                "em-2d",
                form,
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let bc = imp
                .prepare_nodal_boundaries(&mesh, &[(vec![0, 1], 8)])
                .unwrap();
            let mut operators = imp.plan.empty_operators(&layout);
            bc.apply(
                &imp.plan,
                &layout,
                &mesh,
                &HashMap::new(),
                0.,
                &mut operators,
            )
            .unwrap();
            let matrix = operators
                .operator
                .iter()
                .flat_map(|row| row.values())
                .map(|v| v.re)
                .sum::<f64>();
            let load = operators.load.iter().map(|v| v.re).sum::<f64>();
            let expected = if form == "Ez" { 0.5 } else { 2. };
            assert!(
                (matrix - expected).abs() < 1e-10,
                "{form} impedance matrix {matrix}"
            );
            assert!(
                (load - 2. * expected).abs() < 1e-10,
                "{form} incident load {load}"
            );
            let parameters = HashMap::new();
            let sampler = imp.data.sampler(&parameters);
            let bulk = crate::transient_mesh::assemble_nodal_p1(
                &imp.plan,
                &layout,
                &mesh,
                0.,
                "standard",
                &mut TransientAssemblyWorkspace::default(),
                |point| sampler.coefficients(point),
                &[],
                |_, _| Err("unexpected source".into()),
            )
            .unwrap();
            let x = (0..layout.dimension)
                .map(|i| ((i % 7) as f64) - 3.)
                .collect::<Vec<_>>();
            let energy = bulk
                .operator
                .iter()
                .enumerate()
                .map(|(i, row)| row.iter().map(|(&j, v)| x[i] * v.re * x[j]).sum::<f64>())
                .sum::<f64>();
            assert!(
                energy.abs() < 1e-10,
                "{form} lossless curl exchange {energy}"
            );
        }
    }

    #[test]
    fn oriented_boundaries_enforce_time_varying_moments_and_check_initial_data() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let model = json!({"scenario":{"profileId":"em-3d"},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{
            "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"V/m"},
                "fieldValues":[{"expression":"t/tau","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]}}}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
        let tags = HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7]),
            ("boundaries".into(), vec![8]),
        ]);
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let params = HashMap::from([(
            "tau".into(),
            crate::model_expression::Scalar {
                value: 1e-9,
                dimension: crate::model_expression::unit("s").unwrap().1,
            },
        )]);
        let prepared = PreparedTransientCase {
            grid: crate::transient_request::TimeGrid {
                start: 0.,
                end: 1e-9,
                max_step: 1e-9,
                max_steps: 1,
                output_times: vec![1e-9],
            },
            parameters: params,
            overrides: vec![],
            longitudinal_wavenumber: C::default(),
        };
        let options = LinearOptions {
            tolerance: 1e-6,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        for order in [1, 2] {
            let mesh = crate::transient_edge::EdgeTransientMesh::prepare(
                &nodes,
                &[([0, 1, 2, 3], 7)],
                order,
            )
            .unwrap();
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
            let boundary = bound
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8)])
                .unwrap();
            let expected = mesh
                .exterior_face_moments([0, 1, 2], |_| {
                    Ok([C { re: 1., im: 0. }, C::default(), C::default()])
                })
                .unwrap();
            let mut observed = 0;
            bound
                .run_edge_with_boundaries(
                    &prepared,
                    &mesh,
                    &boundary,
                    "standard",
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, layout, time, state, save| {
                        if save {
                            assert_eq!(time, 1e-9);
                            let values = layout.field_state(0, state)?;
                            for &(dof, value) in &expected {
                                assert!(
                                    (values[dof].re - value.re).abs() < 1e-8,
                                    "order {order} DOF {dof}"
                                );
                            }
                            observed += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap_or_else(|error| panic!("order {order}: {error}"));
            assert_eq!(observed, 1);
            let initial = json!({"electric":[{"expression":"1","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]});
            let bad = BoundTransientCase::prepare(
                &package, "em-3d", "electric", &model, &map, &tags, &initial,
            )
            .unwrap();
            let bad_boundary = bad
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8)])
                .unwrap();
            assert!(
                bad.run_edge_with_boundaries(
                    &prepared,
                    &mesh,
                    &bad_boundary,
                    "standard",
                    None,
                    &options,
                    &mut SolverCache::default(),
                    |_, _, _, _, _| Ok(())
                )
                .err()
                .unwrap()
                .contains("Initial field conflicts")
            );
            assert!(
                bound
                    .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8), ([2, 1, 0], 8)])
                    .is_err()
            );
        }
    }

    #[test]
    fn electromagnetic_3d_edge_trace_and_impedance_use_oriented_boundary_forms() {
        let package: Value = serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let mut model = json!({"scenario":{"profileId":"em-3d"},"model":{"materials":[{"id":"material","selectionId":"domain"}],"physics":{
            "wall":{"id":"wall","type":"neumann","selectionId":"wall","value":{"expression":"0","unit":"V/m^2"},
                "fieldValues":[{"expression":"0","unit":"A/m"},{"expression":"1","unit":"A/m"},{"expression":"0","unit":"A/m"}]}}}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
        let tags = HashMap::from([
            ("materials".into(), vec![7]),
            ("domainSources".into(), vec![7]),
            ("boundaries".into(), vec![8]),
        ]);
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        for order in [1, 2] {
            let mesh = crate::transient_edge::EdgeTransientMesh::prepare(
                &nodes,
                &[([0, 1, 2, 3], 7)],
                order,
            )
            .unwrap();
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
            let bc = bound
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8)])
                .unwrap();
            let layout = StateLayout::new(&bound.plan, &vec![mesh.dof_count(); 2]).unwrap();
            let mut operators = bound.plan.empty_operators(&layout);
            bc.apply(
                &bound.plan,
                &layout,
                &mesh,
                &HashMap::new(),
                0.,
                &mut operators,
            )
            .unwrap();
            let mut expected = vec![0.; mesh.dof_count()];
            for q in mesh.exterior_face_points([0, 1, 2]).unwrap() {
                for (i, &dof) in q.dofs.iter().enumerate() {
                    expected[dof] += q.weight * q.basis[i][0];
                }
            }
            let actual = layout.field_state(0, &operators.load).unwrap();
            for (a, e) in actual.iter().zip(&expected) {
                assert!((a.re - e).abs() < 1e-10, "order {order} vector boundary");
            }
            let mut bad = model.clone();
            bad["model"]["physics"]["wall"]["value"]["expression"] = json!("1");
            let bad_bound = BoundTransientCase::prepare(
                &package,
                "em-3d",
                "electric",
                &bad,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let bad_bc = bad_bound
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8)])
                .unwrap();
            assert!(
                bad_bc
                    .apply(
                        &bad_bound.plan,
                        &layout,
                        &mesh,
                        &HashMap::new(),
                        0.,
                        &mut bad_bound.plan.empty_operators(&layout)
                    )
                    .is_err()
            );
            model["model"]["physics"]["wall"] = json!({"id":"wall","type":"impedance","selectionId":"wall","impedance":{"expression":"2","unit":"ohm"},
                "incidentField":{"expression":"1","unit":"1"},"polarization":[{"expression":"1","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]});
            let imp = BoundTransientCase::prepare(
                &package,
                "em-3d",
                "electric",
                &model,
                &map,
                &tags,
                &json!({}),
            )
            .unwrap();
            let bc = imp
                .prepare_edge_boundaries(&mesh, &[([0, 1, 2], 8)])
                .unwrap();
            let mut operators = imp.plan.empty_operators(&layout);
            bc.apply(
                &imp.plan,
                &layout,
                &mesh,
                &HashMap::new(),
                0.,
                &mut operators,
            )
            .unwrap();
            let actual = layout.field_state(0, &operators.load).unwrap();
            for (a, e) in actual.iter().zip(&expected) {
                assert!(
                    (a.re - e).abs() < 1e-10,
                    "order {order} impedance incident load"
                );
            }
            let probe = (0..layout.dimension)
                .map(|i| ((i % 7) as f64) - 3.)
                .collect::<Vec<_>>();
            let energy = operators
                .operator
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    row.iter()
                        .map(|(&j, v)| probe[i] * v.re * probe[j])
                        .sum::<f64>()
                })
                .sum::<f64>();
            assert!(energy >= -1e-10, "order {order} impedance energy {energy}");
            let params = HashMap::new();
            let sampler = imp.data.sampler(&params);
            let bulk = mesh
                .assemble(
                    &imp.plan,
                    &layout,
                    0.,
                    "standard",
                    &mut TransientAssemblyWorkspace::default(),
                    |point| sampler.coefficients(point),
                    &[],
                    |_, _| Err("unexpected source".into()),
                )
                .unwrap();
            let exchange = bulk
                .operator
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    row.iter()
                        .map(|(&j, v)| probe[i] * v.re * probe[j])
                        .sum::<f64>()
                })
                .sum::<f64>();
            assert!(
                exchange.abs() < 1e-9,
                "order {order} lossless curl exchange {exchange}"
            );
            model["model"]["physics"]["wall"]["type"] = json!("neumann");
            model["model"]["physics"]["wall"]["value"] = json!({"expression":"0","unit":"V/m^2"});
            model["model"]["physics"]["wall"]["fieldValues"] = json!([{"expression":"0","unit":"A/m"},{"expression":"1","unit":"A/m"},{"expression":"0","unit":"A/m"}]);
        }
    }
}

#[cfg(test)]
mod eigenfrequency_model_tests{
    use super::*;
    #[test]
    fn every_scenario_filters_declared_modal_loads_and_keeps_boundary_operators(){
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")]{
            let package:Value=serde_json::from_str(text).unwrap();let profile=package["id"].as_str().unwrap();
            for formulation in package["transientForms"].as_object().unwrap().keys(){
                let laws=package.pointer(&format!("/transientForms/{formulation}/boundaryLaws")).unwrap().as_object().unwrap();
                let essential=laws.iter().find(|(_,law)|law.get("essentialForm").is_some()).map(|(kind,_)|kind.clone());
                let load=laws.iter().find(|(_,law)|law.get("essentialForm").is_none()&&law.pointer("/input/prescribed/source").is_some()).map(|(kind,_)|kind.clone());
                let impedance=laws.iter().find(|(_,law)|law.pointer("/input/sources").and_then(Value::as_array).is_some_and(|values|!values.is_empty())).map(|(kind,_)|kind.clone());
                let source_type=package.pointer("/schemaDocument/x-fem-physics/featureHandlingByType").and_then(Value::as_object).unwrap().iter()
                    .find(|(_,behavior)|behavior.as_str()==Some("domainSource")).map(|(kind,_)|kind.clone()).unwrap();
                let mut physics=serde_json::Map::new();
                if let Some(kind)=&essential{physics.insert("essential".into(),serde_json::json!({"id":"essential","type":kind,"value":{"expression":"0","unit":"1"}}));}
                if let Some(kind)=&load{physics.insert("load".into(),serde_json::json!({"id":"load","type":kind,"value":{"expression":"t+freq","unit":"1"}}));}
                if let Some(kind)=&impedance{physics.insert("impedance".into(),serde_json::json!({"id":"impedance","type":kind,"impedance":{"expression":"2","unit":"1"},
                    "incidentField":{"expression":"t+freq","unit":"1"},"polarization":[{"expression":"t+freq","unit":"1"}]}));}
                physics.insert("volumeSource".into(),serde_json::json!({"id":"volumeSource","type":source_type,"values":[{"expression":"t+freq","unit":"1"}]}));
                let model=serde_json::json!({"model":{"physics":Value::Object(physics)}});
                let filtered=eigenfrequency_model(&package,formulation,&model).unwrap_or_else(|error|panic!("{profile}/{formulation}: {error}"));
                let values=filtered.pointer("/model/physics").unwrap().as_object().unwrap();
                assert!(!values.contains_key("volumeSource"),"{profile}/{formulation}: volume source remained");
                if load.is_some(){assert!(!values.contains_key("load"),"{profile}/{formulation}: natural source remained");}
                if essential.is_some(){assert!(values.contains_key("essential"),"{profile}/{formulation}: essential constraint was removed");}
                if impedance.is_some(){let row=&values["impedance"];assert!(row.get("impedance").is_some(),"{profile}/{formulation}: boundary operator removed");
                    assert!(row.get("incidentField").is_none()&&row.get("polarization").is_none(),"{profile}/{formulation}: incident source remained");}
            }
        }
    }
}
