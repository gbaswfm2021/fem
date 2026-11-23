//! Scenario-neutral field projection and result preparation for scalar P1 frequency cases.
//! Field-family component mappings and output labels are declared by the package.
use std::collections::HashMap;
use serde_json::{Value, json};
use crate::{C, field_output};
use crate::prepared_case::{CaseKind, PreparedCase};
use crate::prepared_frequency_scalar::FrequencyResult;
use crate::result_contract::ResultData;

pub(crate) enum Mesh<'a> {
    Triangle(&'a crate::mesh2d::Mesh2D),
    Tetrahedron(&'a crate::mesh3d::IndexedMesh3D),
}

fn project(package: &Value, formulation: &str, declarations: &[crate::model_schema::FieldVariable], primary: &[C], recovered: &[[C; 3]]) -> Result<HashMap<String, Vec<C>>, String> {
    if primary.len() != recovered.len() { return Err("Frequency primary and recovered fields have different node counts.".into()); }
    let family = package.get("preparedFrequencyScalar").and_then(|v| v.get(formulation)).and_then(|v| v.get("fieldFamilies"))
        .ok_or("Frequency fieldFamilies declaration is required.")?;
    let zeros = vec![C::default(); primary.len()];
    let recovered_components: [Vec<C>;3] = std::array::from_fn(|axis| recovered.iter().map(|v| v[axis]).collect());
    let source = |name: &str| -> Result<&[C], String> {
        match name {
            "primary" => Ok(primary), "zero" => Ok(&zeros),
            "recovered0" => Ok(&recovered_components[0]), "recovered1" => Ok(&recovered_components[1]),
            "recovered2" => Ok(&recovered_components[2]), _ => Err(format!("Unsupported field-family component {name:?}.")),
        }
    };
    let scalars = family.get("scalars").and_then(Value::as_object).ok_or("fieldFamilies.scalars must be an object.")?;
    let vectors = family.get("vectors").and_then(Value::as_object).ok_or("fieldFamilies.vectors must be an object.")?;
    let mut scalar_families = HashMap::new();
    for (name, mapping) in scalars { scalar_families.insert(name.as_str(), source(mapping.as_str().ok_or("Scalar family mapping must be a string.")?)?); }
    let mut vector_families = HashMap::new();
    for (name, mapping) in vectors {
        let values = mapping.as_array().filter(|v| v.len()==3).ok_or("Vector family mapping must contain three components.")?;
        vector_families.insert(name.as_str(), [source(values[0].as_str().ok_or("Field component must be a string.")?)?,
            source(values[1].as_str().ok_or("Field component must be a string.")?)?,
            source(values[2].as_str().ok_or("Field component must be a string.")?)?]);
    }
    field_output::project_nodal_fields(&field_output::field_projection(declarations)?, &scalar_families, &vector_families)
}

fn simplex(nodes: Vec<usize>, tag: usize, material: usize) -> crate::integration::Simplex {
    crate::integration::Simplex { nodes, physical_tag: tag, material_tag: Some(material),
        operator_id: None, element_index: None, adjacent_element_index: None }
}

fn sample_mesh<const N: usize>(nodes: &[[f64; 3]], cells: &[([usize; N], usize)], facets: Vec<(Vec<usize>, usize)>,
    model: &Value, map: &Value, frequency: f64) -> Result<(Value, Value, Vec<crate::integration::Simplex>, Vec<crate::integration::Simplex>), String> {
    let material_fields = crate::material::fields_by_cell(model, map, frequency, nodes, cells, [0,1,2], [0.;3])?;
    let domain = cells.iter().map(|(cell, tag)| simplex(cell.to_vec(), *tag, *tag)).collect();
    let mut boundary = Vec::new();
    for (facet, tag) in facets {
        let material = cells.iter().find(|(cell, _)| facet.iter().all(|node| cell.contains(node)))
            .map(|(_, material)| *material).ok_or("Selected integration entity has no adjacent cell.")?;
        boundary.push(simplex(facet, tag, material));
    }
    let cells = json!(cells.iter().map(|(nodes, tag)| json!({"nodes":nodes.as_slice(),"materialTag":tag})).collect::<Vec<_>>());
    Ok((cells, Value::Object(material_fields), domain, boundary))
}

pub(crate) fn build(case: &PreparedCase, package: &Value, model: &Value, map: &Value, mesh: Mesh<'_>, result: &FrequencyResult) -> Result<ResultData, String> {
    let frequency = match case.kind { CaseKind::Frequency { frequency_hz, .. } => frequency_hz, _ => return Err("Frequency output requires a frequency case.".into()) };
    let (dimension, nodes, cells, material_fields, domain, boundary) = match mesh {
        Mesh::Triangle(mesh) => {
            let facets = mesh.boundary_edges.iter().chain(&mesh.integration_edges).map(|(nodes,tag)| (nodes.to_vec(),*tag)).collect();
            let (cells, materials, domain, boundary) = sample_mesh(&mesh.nodes, &mesh.triangles, facets, model, map, frequency)?;
            (2, &mesh.nodes, cells, materials, domain, boundary)
        },
        Mesh::Tetrahedron(mesh) => {
            let facets = mesh.boundary_faces.iter().map(|(nodes,tag)| (nodes.to_vec(),*tag))
                .chain(mesh.edge_selections.iter().map(|(nodes,tag)| (nodes.to_vec(),*tag))).collect();
            let (cells, materials, domain, boundary) = sample_mesh(&mesh.nodes, &mesh.tetrahedra, facets, model, map, frequency)?;
            (3, &mesh.nodes, cells, materials, domain, boundary)
        },
    };
    if result.solution.real.len() != nodes.len() || result.solution.imaginary.len() != nodes.len() {
        return Err("Frequency solution size does not match mesh nodes.".into());
    }
    let primary: Vec<C> = result.solution.real.iter().zip(&result.solution.imaginary).map(|(&re,&im)| C { re, im }).collect();
    let declarations = crate::model_schema::field_variables_for_model(model, false)?;
    let mut fields = project(package, &case.settings.formulation, &declarations, &primary, &result.recovered_nodes)?;
    let component = model.pointer("/model").ok_or("Model component is required.")?;
    let global = crate::integration::evaluate_model_variables(model, component, map, dimension, nodes, &domain, &boundary, &fields, frequency)?;
    let mut complex = HashMap::new();
    for (name, value) in &global {
        let re = value.get("re").and_then(Value::as_f64).ok_or("Global variable has no real value.")?;
        let im = value.get("im").and_then(Value::as_f64).ok_or("Global variable has no imaginary value.")?;
        complex.insert(name.clone(), C { re, im });
    }
    let maximum = primary.iter().map(|v| v.re.hypot(v.im)).fold(0.0, f64::max);
    fields.insert("primary".into(), primary);
    Ok(ResultData { values: HashMap::from([
        ("frequency".into(),json!(frequency)), ("formulation".into(),json!(case.settings.formulation)),
        ("nodes".into(),json!(nodes)), ("cells".into(),cells), ("materialFields".into(),material_fields),
        ("globalVariables".into(),Value::Object(global)), ("backend".into(),json!(case.settings.backend)),
        ("ordering".into(),json!(case.settings.ordering)), ("threads".into(),json!(case.settings.threads)),
        ("quadrature".into(),json!(case.settings.quadrature)), ("residual".into(),json!(result.solution.residual)),
        ("unknowns".into(),json!(result.solution.field_dofs)), ("maximum".into(),json!(maximum)),
        ("longitudinalWavenumberReal".into(),json!(result.longitudinal_wavenumber.re)),
        ("longitudinalWavenumberImaginary".into(),json!(result.longitudinal_wavenumber.im)),
    ]), complex, series: fields, matrices: HashMap::new() })
}


