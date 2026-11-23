//! Scenario-declared projection of transient state samples to result symbols.
//! This is an internal result adapter, not a time-series file format.
use crate::solver_common::C;
use crate::transient_mesh::NodalMesh;
use crate::transient_plan::TransientPlan;
use crate::transient_plan::{SampleLocation, SampledCoefficient, StateLayout};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

struct StateOutput {
    symbol: String,
    field: usize,
    component: usize,
}
pub(crate) struct StateOutputPlan {
    outputs: Vec<StateOutput>,
}

pub(crate) struct TensorOutputPlan {
    field: usize,
    form: crate::tensor_recovery::TensorRecoveryPlan,
}

pub(crate) struct TensorSamples {
    pub(crate) components: Vec<String>,
    pub(crate) cells: Vec<Vec<C>>,
    pub(crate) nodes: Vec<Vec<C>>,
}

impl TensorSamples {
    pub(crate) fn nodal_symbols(&self) -> Result<HashMap<String, Vec<C>>, String> {
        let mut result = HashMap::with_capacity(self.components.len());
        for (index, name) in self.components.iter().enumerate() {
            let mut values = Vec::with_capacity(self.nodes.len());
            for node in &self.nodes {
                values.push(
                    *node
                        .get(index)
                        .ok_or("Transient tensor sample has the wrong component count.")?,
                );
            }
            if result.insert(name.clone(), values).is_some() {
                return Err("Transient tensor result contains duplicate symbols.".into());
            }
        }
        Ok(result)
    }
}

impl TensorOutputPlan {
    /// Evaluate the declared tensor from the owning P1 cell at a physical
    /// point and time, retaining material jumps and time-dependent moduli.
    pub(crate) fn at_cell(&self,plan:&TransientPlan,layout:&StateLayout,mesh:&NodalMesh<'_>,state:&[C],
        cell_index:usize,point:[f64;3],time:f64,
        mut coefficients:impl FnMut(SampleLocation)->Result<HashMap<String,SampledCoefficient>,String>,
    )->Result<HashMap<String,C>,String> {
        let dim=plan.dimension;
        let cell=mesh.cells.get(cell_index).ok_or("Transient tensor result cell is missing.")?;
        if mesh.dimension!=dim||cell.nodes.len()!=dim+1||cell.nodes.iter().any(|&n|n>=mesh.nodes.len())||!time.is_finite() {
            return Err("Transient tensor result cell or time is invalid.".into());
        }
        let gradients:Vec<Vec<f64>>=if dim==2 {
            let (_,g)=crate::mesh2d::triangle_geometry([mesh.nodes[cell.nodes[0]],mesh.nodes[cell.nodes[1]],mesh.nodes[cell.nodes[2]]])?;
            g.iter().map(|v|v.to_vec()).collect()
        }else{
            let (_,g)=crate::simplex::tetra_p1([mesh.nodes[cell.nodes[0]],mesh.nodes[cell.nodes[1]],mesh.nodes[cell.nodes[2]],mesh.nodes[cell.nodes[3]]],
                crate::simplex::DeterminantTolerance::Absolute(0.))?;
            g.iter().map(|v|v.to_vec()).collect()
        };
        let values=layout.field_state(self.field,state)?;
        if values.len()!=mesh.nodes.len()*dim{return Err("Transient tensor result layout is invalid.".into());}
        let mut gradient=vec![C::default();dim*dim];
        for (local,&node) in cell.nodes.iter().enumerate(){for component in 0..dim {for axis in 0..dim {
            let value=values[node*dim+component];gradient[component*dim+axis].re+=value.re*gradients[local][axis];
            gradient[component*dim+axis].im+=value.im*gradients[local][axis];
        }}}
        let scalars=coefficients(SampleLocation{cell:cell_index,material_tag:cell.tag,point,time})?.into_iter()
            .filter_map(|(name,value)|match value{SampledCoefficient::Scalar(v)=>Some((name,v)),_=>None}).collect();
        let result=self.form.apply(&gradient,&scalars)?;
        Ok(self.form.components().iter().cloned().zip(result).collect())
    }
    pub(crate) fn from_json(plan: &TransientPlan, value: &Value) -> Result<Self, String> {
        let id = value
            .get("field")
            .and_then(Value::as_str)
            .ok_or("Transient tensor recovery needs a field.")?;
        let field = plan
            .fields
            .iter()
            .position(|v| v.id == id)
            .ok_or("Transient tensor recovery field is not declared.")?;
        let form = crate::tensor_recovery::TensorRecoveryPlan::from_json(
            value
                .get("form")
                .ok_or("Transient tensor recovery form is required.")?,
        )?;
        form.require_shape(plan.dimension, plan.fields[field].components)?;
        form.require_policy("cellCentroid", "cellAndNodalArithmeticMean")?;
        Ok(Self { field, form })
    }

    /// Recover declared tensor components at centroids and average incident
    /// cell samples for nodal visualization, using material values at time.
    pub(crate) fn nodal_p1(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        mesh: &NodalMesh<'_>,
        state: &[C],
        time: f64,
        mut coefficients: impl FnMut(
            SampleLocation,
        ) -> Result<HashMap<String, SampledCoefficient>, String>,
    ) -> Result<TensorSamples, String> {
        let dim = plan.dimension;
        let expected = mesh
            .nodes
            .len()
            .checked_mul(dim)
            .ok_or("Transient tensor result size overflow.")?;
        if mesh.dimension != dim
            || !time.is_finite()
            || layout.field_count(self.field) != Some(expected)
        {
            return Err("Transient tensor recovery mesh, state or time is invalid.".into());
        }
        let values = layout.field_state(self.field, state)?;
        let mut cells = Vec::with_capacity(mesh.cells.len());
        let mut sums = vec![vec![C::default(); self.form.components().len()]; mesh.nodes.len()];
        let mut counts = vec![0usize; mesh.nodes.len()];
        for (index, cell) in mesh.cells.iter().enumerate() {
            if cell.nodes.len() != dim + 1 || cell.nodes.iter().any(|&n| n >= mesh.nodes.len()) {
                return Err("Transient tensor cell connectivity is invalid.".into());
            }
            let gradients: Vec<Vec<f64>> = if dim == 2 {
                let (_, g) = crate::mesh2d::triangle_geometry([
                    mesh.nodes[cell.nodes[0]],
                    mesh.nodes[cell.nodes[1]],
                    mesh.nodes[cell.nodes[2]],
                ])?;
                g.iter().map(|v| v.to_vec()).collect()
            } else {
                let (_, g) = crate::simplex::tetra_p1(
                    [
                        mesh.nodes[cell.nodes[0]],
                        mesh.nodes[cell.nodes[1]],
                        mesh.nodes[cell.nodes[2]],
                        mesh.nodes[cell.nodes[3]],
                    ],
                    crate::simplex::DeterminantTolerance::Absolute(0.),
                )?;
                g.iter().map(|v| v.to_vec()).collect()
            };
            let point = std::array::from_fn(|axis| {
                cell.nodes
                    .iter()
                    .map(|&node| mesh.nodes[node][axis])
                    .sum::<f64>()
                    / (dim + 1) as f64
            });
            let mut gradient = vec![C::default(); dim * dim];
            for (local, &node) in cell.nodes.iter().enumerate() {
                for component in 0..dim {
                    for axis in 0..dim {
                        let value = values[node * dim + component];
                        gradient[component * dim + axis].re += value.re * gradients[local][axis];
                        gradient[component * dim + axis].im += value.im * gradients[local][axis];
                    }
                }
            }
            let evaluated = coefficients(SampleLocation {
                cell: index,
                material_tag: cell.tag,
                point,
                time,
            })?;
            let scalars = evaluated
                .into_iter()
                .filter_map(|(name, value)| match value {
                    SampledCoefficient::Scalar(v) => Some((name, v)),
                    _ => None,
                })
                .collect();
            let result = self.form.apply(&gradient, &scalars)?;
            for &node in &cell.nodes {
                for (sum, value) in sums[node].iter_mut().zip(&result) {
                    sum.re += value.re;
                    sum.im += value.im;
                }
                counts[node] += 1;
            }
            cells.push(result);
        }
        for (node, &count) in counts.iter().enumerate() {
            if count == 0 {
                return Err(format!("Transient tensor result node {node} is unused."));
            }
            for value in &mut sums[node] {
                value.re /= count as f64;
                value.im /= count as f64;
            }
        }
        Ok(TensorSamples {
            components: self.form.components().to_vec(),
            cells,
            nodes: sums,
        })
    }
}

impl StateOutputPlan {
    pub(crate) fn from_json(plan: &TransientPlan, value: &Value) -> Result<Self, String> {
        let entries = value
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or("Transient state outputs must be a nonempty array.")?;
        let mut outputs = Vec::with_capacity(entries.len());
        let mut symbols = HashSet::new();
        for entry in entries {
            let symbol = entry
                .get("symbol")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or("Transient state output needs a symbol.")?;
            if !symbols.insert(symbol.to_string()) {
                return Err(format!("Duplicate transient output symbol {symbol:?}."));
            }
            let id = entry
                .get("field")
                .and_then(Value::as_str)
                .ok_or("Transient state output needs a field.")?;
            let field = plan.fields.iter().position(|v| v.id == id).ok_or_else(|| {
                format!("Transient output {symbol:?} references an undeclared field.")
            })?;
            let component = entry
                .get("component")
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v < plan.fields[field].components)
                .ok_or_else(|| format!("Transient output {symbol:?} has an invalid component."))?;
            if plan.fields[field].unit.is_none() {
                return Err(format!(
                    "Transient output field {id:?} has no declared unit."
                ));
            }
            outputs.push(StateOutput {
                symbol: symbol.into(),
                field,
                component,
            });
        }
        Ok(Self { outputs })
    }
    /// Project one directly sampled state into the same scenario-declared
    /// symbols used by field snapshots, without assuming a field family.
    pub(crate) fn evaluate_point(&self, fields:&[Vec<C>]) -> Result<HashMap<String,C>,String> {
        let mut output=HashMap::with_capacity(self.outputs.len());
        for mapping in &self.outputs {
            let value=*fields.get(mapping.field).and_then(|field|field.get(mapping.component))
                .ok_or_else(||format!("Transient output {:?} has no sampled component.",mapping.symbol))?;
            if !value.re.is_finite()||!value.im.is_finite(){return Err(format!("Transient output {:?} is non-finite.",mapping.symbol));}
            output.insert(mapping.symbol.clone(),value);
        }
        Ok(output)
    }
    /// Each input is `[field][component][sample]`, in a caller-declared
    /// common location such as nodes. Derived quantities need their own
    /// scenario-declared recovery and are not invented here.
    pub(crate) fn evaluate(
        &self,
        fields: &[Vec<Vec<C>>],
    ) -> Result<HashMap<String, Vec<C>>, String> {
        let mut output = HashMap::with_capacity(self.outputs.len());
        let mut count = None;
        for mapping in &self.outputs {
            let values = fields
                .get(mapping.field)
                .and_then(|field| field.get(mapping.component))
                .ok_or_else(|| {
                    format!(
                        "Transient output {:?} has no sampled component.",
                        mapping.symbol
                    )
                })?;
            if values
                .iter()
                .any(|v| !v.re.is_finite() || !v.im.is_finite())
            {
                return Err(format!(
                    "Transient output {:?} is non-finite.",
                    mapping.symbol
                ));
            }
            if count.is_some_and(|n| n != values.len()) {
                return Err("Transient output fields have incompatible sample counts.".into());
            }
            count = Some(values.len());
            output.insert(mapping.symbol.clone(), values.clone());
        }
        Ok(output)
    }
}

/// Include fields declared identically zero, or zero in the transient study,
/// for the selected scenario and formulation. No other derived values are inferred.
pub(crate) fn add_declared_zero_fields(
    declarations: &[crate::model_schema::FieldVariable],
    values: &mut HashMap<String, Vec<C>>,
    sample_count: usize,
) -> Result<(), String> {
    for declaration in declarations.iter().filter(|field| {
        field.identically_zero || field.zero_in_studies.iter().any(|study| study == "transient")
    }) {
        if let Some(existing) = values.get(&declaration.symbol) {
            if existing.len() != sample_count || existing.iter().any(|v| v.re != 0. || v.im != 0.) {
                return Err(format!(
                    "Transient output {:?} conflicts with a declared zero field.",
                    declaration.symbol
                ));
            }
        } else {
            values.insert(declaration.symbol.clone(), vec![C::default(); sample_count]);
        }
    }
    Ok(())
}

/// A transient reduction can make a field zero even when the common field
/// catalog does not mark it identically zero for every Study kind. The
/// scenario's transient contract must explicitly name each such output.
pub(crate) fn add_transient_zero_fields(
    names: &[String],
    declarations: &[crate::model_schema::FieldVariable],
    values: &mut HashMap<String, Vec<C>>,
    sample_count: usize,
) -> Result<(), String> {
    let mut unique = HashSet::new();
    for name in names {
        if !unique.insert(name) || !declarations.iter().any(|field| field.symbol == *name) {
            return Err(format!(
                "Transient zero output {name:?} is duplicated or undeclared."
            ));
        }
        if let Some(existing) = values.get(name) {
            if existing.len() != sample_count || existing.iter().any(|v| v.re != 0. || v.im != 0.) {
                return Err(format!(
                    "Transient zero output {name:?} conflicts with sampled values."
                ));
            }
        } else {
            values.insert(name.clone(), vec![C::default(); sample_count]);
        }
    }
    Ok(())
}

/// Serialize a time snapshot using the existing field-symbol declarations.
/// The application must separately register and expose this new format.
pub(crate) fn field_snapshot(
    study_id: &str,
    case_index: usize,
    sample_index: usize,
    time: f64,
    dimension: usize,
    nodes: &[[f64; 3]],
    cells: &[(Vec<usize>, usize)],
    declarations: &[crate::model_schema::FieldVariable],
    values: &HashMap<String, Vec<C>>,
    material_fields: Option<&serde_json::Map<String, Value>>,
) -> Result<Value, String> {
    if study_id.is_empty()
        || case_index as u128 > 9_007_199_254_740_991_u128
        || sample_index as u128 > 9_007_199_254_740_991_u128
        || !time.is_finite()
        || !(2..=3).contains(&dimension)
        || nodes.is_empty()
        || nodes.iter().flatten().any(|v| !v.is_finite())
        || dimension == 2 && nodes.iter().any(|point| point[2] != 0.)
        || cells.is_empty()
    {
        return Err("Transient field snapshot has invalid study, time or mesh geometry.".into());
    }
    for (vertices, tag) in cells {
        if *tag as u128 > 9_007_199_254_740_991_u128
            || vertices.len() != dimension + 1
            || vertices.iter().any(|&node| node >= nodes.len())
            || vertices.iter().copied().collect::<HashSet<_>>().len() != vertices.len()
        {
            return Err("Transient field snapshot has invalid simplex connectivity.".into());
        }
    }
    for field in declarations {
        let data = values
            .get(&field.symbol)
            .ok_or_else(|| format!("Missing transient field {:?}.", field.symbol))?;
        if data.len() != nodes.len()
            || data.iter().any(|value| {
                !value.re.is_finite()
                    || !value.im.is_finite()
                    || field.identically_zero && (value.re != 0. || value.im != 0.)
            })
        {
            return Err(format!(
                "Transient field {:?} must have one finite value per node.",
                field.symbol
            ));
        }
    }
    let fields =
        crate::field_output::declared_component_fields_for(declarations, dimension, values)?;
    let mut document = serde_json::json!({"schema":"fullmeta-transient-field/1","studyId":study_id,
        "caseIndex":case_index,"sampleIndex":sample_index,"timeSeconds":time,"dimension":dimension,
        "fieldComponentCoordinateSystem":"model","nodes":nodes,
        "fields":fields});
    let topology = cells
        .iter()
        .map(|(vertices, tag)| serde_json::json!({"nodes":vertices,"materialTag":tag}))
        .collect::<Vec<_>>();
    document[if dimension == 2 {
        "triangles"
    } else {
        "tetrahedra"
    }] = serde_json::json!(topology);
    if let Some(material_fields) = material_fields {
        for (name, field) in material_fields {
            for part in ["real", "imag"] {
                let rows = field
                    .get(part)
                    .and_then(Value::as_array)
                    .filter(|rows| rows.len() == cells.len())
                    .ok_or_else(|| {
                        format!(
                            "Transient material field {name:?} has an invalid {part} cell count."
                        )
                    })?;
                if rows.iter().any(|row| {
                    row.as_array().is_none_or(|row| {
                        row.len() != dimension + 1
                            || row
                                .iter()
                                .any(|v| v.as_f64().is_none_or(|x| !x.is_finite()))
                    })
                }) {
                    return Err(format!(
                        "Transient material field {name:?} has invalid {part} samples."
                    ));
                }
            }
        }
        document["materialFieldsByCell"] = Value::Object(material_fields.clone());
    }
    Ok(document)
}

/// Serialize one eigenmode as a mode-indexed field state. The temporal
/// eigenvalue remains the primary result coordinate; frequency is a derived
/// complex value and no artificial time/frequency sample is introduced.
pub(crate) fn eigenfrequency_mode_snapshot(
    study_id:&str,case_index:usize,mode_index:usize,temporal_eigenvalue:C,relative_residual:f64,dimension:usize,
    nodes:&[[f64;3]],cells:&[(Vec<usize>,usize)],declarations:&[crate::model_schema::FieldVariable],
    values:&HashMap<String,Vec<C>>,materials:Option<&serde_json::Map<String,Value>>,
)->Result<Value,String>{
    if study_id.is_empty()||!matches!(dimension,2|3)||nodes.is_empty()||cells.is_empty()
          ||!temporal_eigenvalue.re.is_finite()||!temporal_eigenvalue.im.is_finite()||!relative_residual.is_finite()||relative_residual<0.
        ||nodes.iter().flatten().any(|value|!value.is_finite()){
        return Err("Eigenfrequency mode snapshot identity, eigenvalue or mesh is invalid.".into());
    }
    let mut fields=serde_json::Map::new();
    for (symbol,samples) in values {
        let definition=declarations.iter().find(|field|field.symbol==*symbol)
            .ok_or_else(||format!("Eigenfrequency output {symbol:?} is not declared."))?;
        if samples.len()!=nodes.len()||samples.iter().any(|value|!value.re.is_finite()||!value.im.is_finite()){
            return Err(format!("Eigenfrequency output {symbol:?} has invalid nodal samples."));
        }
        fields.insert(symbol.clone(),serde_json::json!({"unit":definition.unit,"real":samples.iter().map(|value|value.re).collect::<Vec<_>>(),
            "imag":samples.iter().map(|value|value.im).collect::<Vec<_>>() }));
    }
    let node_count=nodes.len();
    let topology=cells.iter().map(|(cell_nodes,tag)|{
        if cell_nodes.len()!=dimension+1||cell_nodes.iter().any(|&node|node>=node_count){
            return Err("Eigenfrequency output topology has invalid cell connectivity.".to_string());
        }
        Ok(serde_json::json!({"nodes":cell_nodes,"materialTag":tag}))
    }).collect::<Result<Vec<_>,_>>()?;
    let frequency=serde_json::json!({"realHz":temporal_eigenvalue.im/(2.*std::f64::consts::PI),
        "imaginaryHz":-temporal_eigenvalue.re/(2.*std::f64::consts::PI)});
    let mut result=serde_json::json!({"schema":"fullmeta-eigenfrequency-mode/1","studyId":study_id,
        "caseIndex":case_index,"modeIndex":mode_index,"temporalEigenvalue":{"realPerSecond":temporal_eigenvalue.re,"imaginaryPerSecond":temporal_eigenvalue.im},
          "complexFrequencyHz":frequency,"relativeResidual":relative_residual,"dimension":dimension,"nodes":nodes,"fields":fields,
        "normalization":"unit Euclidean norm in the constraint-reduced state space"});
    if dimension==2{result["triangles"]=serde_json::json!(topology);}else{result["tetrahedra"]=serde_json::json!(topology);}
    if let Some(materials)=materials{result["materialFieldsByCell"]=Value::Object(materials.clone());}
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_tensor_recovery_matches_uniform_strain_in_2d_and_3d() {
        use crate::transient_mesh::{NodalCell, NodalMesh};
        use crate::transient_plan::TransientCoefficients;
        for (text, formulation, dim, normal, lateral, out_of_plane) in [
            (
                include_str!("scenarios/elastic2d.json"),
                "planeStress",
                2,
                10. / 0.9375,
                2.5 / 0.9375,
                0.,
            ),
            (
                include_str!("scenarios/elastic2d.json"),
                "planeStrain",
                2,
                12.,
                4.,
                4.,
            ),
            (
                include_str!("scenarios/elastic3d.json"),
                "displacement",
                3,
                12.,
                4.,
                4.,
            ),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let declaration = &package["transientForms"][formulation];
            let plan = TransientPlan::from_json(declaration).unwrap();
            let recovery =
                TensorOutputPlan::from_json(&plan, &declaration["tensorRecovery"]).unwrap();
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
            let count = nodes.len() * dim;
            let layout = StateLayout::new(&plan, &[count, count]).unwrap();
            let mut state = vec![C::default(); layout.dimension];
            for (index, node) in nodes.iter().enumerate() {
                state[index * dim].re = node[0];
            }
            let sampled = recovery
                .nodal_p1(&plan, &layout, &mesh, &state, 1., |_| {
                    coefficients.evaluate(&HashMap::from([
                        ("rho".into(), C { re: 1., im: 0. }),
                        ("young".into(), C { re: 10., im: 0. }),
                        ("nu".into(), C { re: 0.25, im: 0. }),
                    ]))
                })
                .unwrap();
            for (label, expected) in [
                ("sigma_xx", normal),
                ("sigma_yy", lateral),
                ("sigma_zz", out_of_plane),
            ] {
                let index = sampled.components.iter().position(|v| v == label).unwrap();
                assert!(
                    (sampled.cells[0][index].re - expected).abs() < 1e-10,
                    "{formulation}/{label}"
                );
                assert!(
                    sampled
                        .nodes
                        .iter()
                        .all(|node| (node[index].re - expected).abs() < 1e-10)
                );
            }
            let by_symbol = sampled.nodal_symbols().unwrap();
            assert!((by_symbol["sigma_xx"][0].re - normal).abs() < 1e-10);
            let at_point=recovery.at_cell(&plan,&layout,&mesh,&state,0,[0.2,0.1,0.],2.,|location|{
                coefficients.evaluate(&HashMap::from([("rho".into(),C{re:1.,im:0.}),
                    ("young".into(),C{re:10.*(1.+location.point[0]+location.time),im:0.}),
                    ("nu".into(),C{re:0.25,im:0.})]))
            }).unwrap();
            assert!((at_point["sigma_xx"].re-3.2*normal).abs()<1e-9,"{formulation}");
            let later = recovery
                .nodal_p1(&plan, &layout, &mesh, &state, 2., |location| {
                    coefficients.evaluate(&HashMap::from([
                        ("rho".into(), C { re: 1., im: 0. }),
                        (
                            "young".into(),
                            C {
                                re: 10. * location.time,
                                im: 0.,
                            },
                        ),
                        ("nu".into(), C { re: 0.25, im: 0. }),
                    ]))
                })
                .unwrap();
            assert!((later.nodal_symbols().unwrap()["sigma_xx"][0].re - 2. * normal).abs() < 1e-10);
        }
    }
    #[test]
    fn zero_outputs_follow_existing_scenario_field_contract() {
        let model: Value = serde_json::from_str(include_str!(
            "../../examples/acoustic_room_2d/fem_model.json"
        ))
        .unwrap();
        let fields = crate::model_schema::field_variables_for_model(&model, false).unwrap();
        let mut values = HashMap::new();
        add_declared_zero_fields(&fields, &mut values, 3).unwrap();
        assert_eq!(values["vz"].len(), 3);
        assert!(values["vz"].iter().all(|v| v.re == 0. && v.im == 0.));
        values.insert("vz".into(), vec![C { re: 1., im: 0. }; 3]);
        assert!(add_declared_zero_fields(&fields, &mut values, 3).is_err());
    }

    #[test]
    fn snapshot_serializes_declared_fields_with_study_and_time_identity() {
        let model: Value = serde_json::from_str(include_str!(
            "../../examples/acoustic_room_2d/fem_model.json"
        ))
        .unwrap();
        let fields = crate::model_schema::field_variables_for_model(&model, false).unwrap();
        let mut values = HashMap::from([
            ("p".into(), vec![C { re: 2., im: 0. }; 3]),
            ("vx".into(), vec![C::default(); 3]),
            ("vy".into(), vec![C::default(); 3]),
        ]);
        add_declared_zero_fields(&fields, &mut values, 3).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let cells = [(vec![0, 1, 2], 7)];
        let snapshot = field_snapshot(
            "study", 2, 3, 0.25, 2, &nodes, &cells, &fields, &values, None,
        )
        .unwrap();
        assert_eq!(snapshot["schema"], "fullmeta-transient-field/1");
        assert_eq!(snapshot["studyId"], "study");
        assert_eq!(snapshot["caseIndex"], 2);
        assert_eq!(snapshot["sampleIndex"], 3);
        assert_eq!(snapshot["timeSeconds"], 0.25);
        assert_eq!(snapshot["fields"]["p"]["unit"], "Pa");
        assert_eq!(snapshot["fields"]["p"]["real"][0], 2.);
        assert!(
            field_snapshot(
                "study",
                2,
                3,
                f64::NAN,
                2,
                &nodes,
                &cells,
                &fields,
                &values,
                None
            )
            .is_err()
        );
    }
    #[test]
    fn every_scenario_form_declares_valid_state_output_mappings() {
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
            for (formulation, declaration) in package["transientForms"].as_object().unwrap() {
                let plan = TransientPlan::from_json(declaration).unwrap();
                let outputs =
                    StateOutputPlan::from_json(&plan, &declaration["stateOutputs"]).unwrap();
                let declared_fields = &package["schemaDocument"]["x-fem-scenario"]["fields"]["formulations"]
                    [formulation];
                for mapping in declaration["stateOutputs"].as_array().unwrap() {
                    let symbol = mapping["symbol"].as_str().unwrap();
                    let field = plan
                        .fields
                        .iter()
                        .find(|field| field.id == mapping["field"].as_str().unwrap())
                        .unwrap();
                    let declared = [
                        "fieldVariables",
                        "derivedFieldVariables",
                        "zeroFieldVariables",
                    ]
                    .iter()
                    .filter_map(|key| declared_fields[*key].as_array())
                    .flatten()
                    .find(|entry| entry["symbol"] == symbol)
                    .unwrap();
                    assert_eq!(
                        declared["unit"].as_str(),
                        field.unit.as_deref(),
                        "{formulation}/{symbol}"
                    );
                }
                let fields = plan
                    .fields
                    .iter()
                    .map(|field| {
                        (0..field.components)
                            .map(|component| {
                                vec![C {
                                    re: component as f64,
                                    im: 0.,
                                }]
                            })
                            .collect()
                    })
                    .collect::<Vec<Vec<Vec<C>>>>();
                let snapshot=outputs.evaluate(&fields).unwrap();
                let point_fields=fields.iter().map(|field|field.iter().map(|component|component[0]).collect()).collect::<Vec<Vec<C>>>();
                let point=outputs.evaluate_point(&point_fields).unwrap();
                assert_eq!(point.len(),declaration["stateOutputs"].as_array().unwrap().len());
                for (symbol,value) in point {assert_eq!(value.re,snapshot[&symbol][0].re);assert_eq!(value.im,snapshot[&symbol][0].im);}
                assert!(outputs.evaluate_point(&[]).is_err());
            }
        }
    }
}
