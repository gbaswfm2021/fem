//! Model-frame recovery and field projection for oriented tetrahedral fields.
//!
//! The names of field families are supplied by the package. Values and curls
//! are sampled from the edge basis at the actual point, not interpolated from
//! the nodal display fields.
use std::collections::HashMap;

use serde_json::Value;

use crate::{C, c_add, c_scale, field_output};
use crate::nedelec_driver::{Problem, Solution};
use crate::prepared_edge::{CellMaterialSample, PreparedEdgeMesh};

pub(crate) struct EdgeOutput<'a, 'p> {
    mesh: &'a PreparedEdgeMesh,
    problem: &'a Problem<'p>,
    solution: &'a Solution,
    samples: &'a [CellMaterialSample],
    projection: Vec<field_output::FieldProjection>,
    families: HashMap<String, Family>,
    domain_sources: &'a [(String, &'a crate::domain_source::DomainSourcePlan, crate::domain_source::DomainSources)],
    parameters: &'a HashMap<String, crate::model_expression::Scalar>,
    frequency: f64,
    source_by_cell: bool,
}



#[derive(Clone, Copy)]
enum Family { Primary, Recovered }

impl<'a, 'p> EdgeOutput<'a, 'p> {
    /// `field_families.vectors` maps schema family names to `primary` or
    /// `recovered`. Symbols and model-axis components remain schema-driven.
    pub(crate) fn new(mesh: &'a PreparedEdgeMesh, problem: &'a Problem<'p>, solution: &'a Solution,
        samples: &'a [CellMaterialSample], field_families: &Value,
        declarations: &[crate::model_schema::FieldVariable],
        domain_sources: &'a [(String, &'a crate::domain_source::DomainSourcePlan, crate::domain_source::DomainSources)],
        parameters: &'a HashMap<String, crate::model_expression::Scalar>, frequency: f64, source_by_cell: bool) -> Result<Self, String> {
        problem.recovery.require_shape(3, 3)?;
        problem.recovery.require_policy("cellCentroid", "nodalArithmeticMeanAndPoint")?;
        if samples.len() != mesh.cells.len() || problem.tets != mesh.cells.as_slice() || problem.nodes != mesh.nodes.as_slice() {
            return Err("Edge output mesh, problem and centroid samples must match.".into());
        }
        if !frequency.is_finite() || frequency <= 0. {
            return Err("Edge output frequency must be positive and finite.".into());
        }
        if solution.real.len() != mesh.driver_topology.count || solution.imaginary.len() != mesh.driver_topology.count {
            return Err("Edge output solution does not match mesh degrees of freedom.".into());
        }
        let vectors = field_families.get("vectors").and_then(Value::as_object)
            .ok_or("Edge fieldFamilies.vectors must be an object.")?;
        let mut families = HashMap::new();
        for (name, source) in vectors {
            let family = match source.as_str() {
                Some("primary") => Family::Primary,
                Some("recovered") => Family::Recovered,
                _ => return Err(format!("Unsupported edge field-family source for {name:?}.")),
            };
            families.insert(name.clone(), family);
        }
        let projection = field_output::field_projection(declarations)?;
        // Check all declared family mappings even for an empty node set.
        let empty = [C::default(); 3];
        let check: HashMap<&str, [C; 3]> = families.keys().map(|name| (name.as_str(), empty)).collect();
        field_output::project_point_fields(&projection, &HashMap::new(), &check)?;
        Ok(Self { mesh, problem, solution, samples, projection, families, domain_sources, parameters, frequency, source_by_cell })
    }

    /// Recover at a model-frame point. For an interface the primary trace is
    /// taken from its first owner and the recovered vector is averaged across
    /// both owners, matching the declared point sampling policy.
    pub(crate) fn vectors_at(&self, element: usize, adjacent: Option<usize>, point_model: [f64; 3]) -> Result<([C; 3], [C; 3]), String> {
        if point_model.iter().any(|x| !x.is_finite()) { return Err("Edge output point must be finite.".into()); }
        let point = self.mesh.frame.point_to_internal(point_model);
        let sample = |cell: usize| -> Result<([C; 3], [C; 3]), String> {
            let symbols = &self.samples.get(cell).ok_or("Edge output references a missing cell sample.")?.symbols;
            let (value, curl) = self.problem.sample_raw_at(self.solution, cell, point)?;
            let value = self.mesh.frame.vector_to_model(value);
            // PreparedEdgeMesh accepts only orientation-preserving frames, so
            // polar and axial vectors use the same permutation here.
            let curl = self.mesh.frame.vector_to_model(curl);
            let mut source_values = HashMap::new();
            let source_key = if self.source_by_cell { cell } else { self.mesh.cell_tags[cell] };
            for feature_type in self.problem.recovery.additive_domain_source_types() {
                if let Some((_, _, sources)) = self.domain_sources.iter().find(|(kind, _, _)| kind.as_str() == feature_type) {
                    source_values.insert(feature_type.to_owned(), sources.sample(
                        source_key, point_model, self.parameters, Some(self.frequency))?);
                }
            }
            Ok((value, self.problem.recovery.apply_curl_with_domain_sources(curl, symbols, &source_values)?))
        };
        let (primary, mut recovered) = sample(element)?;
        if let Some(other) = adjacent {
            if other == element { return Err("Edge output interface repeats its owning cell.".into()); }
            let (_, other) = sample(other)?;
            for axis in 0..3 { recovered[axis] = c_scale(c_add(recovered[axis], other[axis]), 0.5); }
        }
        Ok((primary, recovered))
    }

    pub(crate) fn point_fields(&self, element: usize, adjacent: Option<usize>, point_model: [f64; 3]) -> Result<HashMap<String, C>, String> {
        let (primary, recovered) = self.vectors_at(element, adjacent, point_model)?;
        let vectors = self.families.iter().map(|(name, family)| (name.as_str(), match family {
            Family::Primary => primary, Family::Recovered => recovered,
        })).collect();
        field_output::project_point_fields(&self.projection, &HashMap::new(), &vectors)
    }

    /// Evaluate prepared expressions at physical quadrature points using the
    /// edge basis. Configuration parsing and material sampling stay shared
    /// with the caller; no interpolation of nodal display values is involved.
    pub(crate) fn evaluate_variables(
        &self,
        variables: &[(String, fullmeta_core::expression::AstNode)],
        pointwise: &[(String, fullmeta_core::expression::AstNode)],
        operators: &[crate::integration::IntegrationOperator],
        domain: &[crate::integration::Simplex],
        boundary: &[crate::integration::Simplex],
        environment: HashMap<String, fullmeta_core::pointwise::ComplexValue>,
        material_sampler: &dyn Fn(&crate::integration::Simplex, [f64; 3])
            -> Result<HashMap<String, fullmeta_core::pointwise::ComplexValue>, String>,
    ) -> Result<serde_json::Map<String, Value>, String> {
        let sampler = |cell: &crate::integration::Simplex, _: &[f64], point: [f64; 3]| {
            let element = cell.element_index.ok_or("Edge integration cell has no owning element.")?;
            Ok(self.point_fields(element, cell.adjacent_element_index, point)?.into_iter()
                .map(|(name, value)| (name, fullmeta_core::pointwise::ComplexValue { re:value.re, im:value.im }))
                .collect())
        };
        crate::integration::evaluate_variables_with_sampler(variables, pointwise, operators, 3,
            &self.mesh.model_nodes, domain, boundary, environment, &sampler, material_sampler,
            [0,1,2], [0.;3])
    }

    pub(crate) fn nodal_fields(&self) -> Result<HashMap<String, Vec<C>>, String> {
        let mut primary = vec![[C::default(); 3]; self.mesh.nodes.len()];
        let mut recovered = primary.clone();
        let mut count = vec![0usize; self.mesh.nodes.len()];
        for (element, cell) in self.mesh.cells.iter().enumerate() {
            for &node in cell {
                let (p, r) = self.vectors_at(element, None, self.mesh.model_nodes[node])?;
                for axis in 0..3 {
                    primary[node][axis] = c_add(primary[node][axis], p[axis]);
                    recovered[node][axis] = c_add(recovered[node][axis], r[axis]);
                }
                count[node] += 1;
            }
        }
        for node in 0..count.len() {
            if count[node] != 0 { for axis in 0..3 {
                primary[node][axis] = c_scale(primary[node][axis], 1. / count[node] as f64);
                recovered[node][axis] = c_scale(recovered[node][axis], 1. / count[node] as f64);
            }}
        }
        let p: [Vec<C>; 3] = std::array::from_fn(|axis| primary.iter().map(|v| v[axis]).collect());
        let r: [Vec<C>; 3] = std::array::from_fn(|axis| recovered.iter().map(|v| v[axis]).collect());
        let vectors = self.families.iter().map(|(name, family)| (name.as_str(), match family {
            Family::Primary => [p[0].as_slice(), p[1].as_slice(), p[2].as_slice()],
            Family::Recovered => [r[0].as_slice(), r[1].as_slice(), r[2].as_slice()],
        })).collect();
        field_output::project_nodal_fields(&self.projection, &HashMap::new(), &vectors)
    }
}
