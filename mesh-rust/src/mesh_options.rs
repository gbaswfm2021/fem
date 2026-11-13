use crate::expression::{Result, Scalar, evaluate_quantity};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct MeshOptions {
    pub min_size: f64,
    pub max_size: f64,
    pub algorithm_3d: i64,
    pub element_order: i64,
    pub optimize: bool,
    pub optimize_netgen: bool,
    pub optimize_threshold: f64,
    pub smoothing: i64,
}

pub fn parse(mesh: &Value, symbols: &HashMap<String, Scalar>) -> Result<MeshOptions> {
    let min_size = evaluate_quantity(
        mesh.get("minSize")
            .ok_or("model.mesh.minSize is required.")?,
        symbols,
        "model.mesh.minSize",
    )?;
    let max_size = evaluate_quantity(
        mesh.get("maxSize")
            .ok_or("model.mesh.maxSize is required.")?,
        symbols,
        "model.mesh.maxSize",
    )?;
    if !min_size.is_finite() || !max_size.is_finite() || min_size <= 0.0 || max_size < min_size {
        return Err(
            "model.mesh sizes must be finite, positive, and maxSize must be at least minSize."
                .into(),
        );
    }
    let gmsh = mesh.get("gmsh").ok_or("model.mesh.gmsh is required.")?;
    let integer = |key: &str| {
        gmsh.get(key)
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("model.mesh.gmsh.{key} must be an integer."))
    };
    let boolean = |key: &str| {
        gmsh.get(key)
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("model.mesh.gmsh.{key} must be a boolean."))
    };
    let element_order = integer("elementOrder")?;
    if element_order != 1 {
        return Err(format!(
            "The current Gmsh converter supports element order 1; order {element_order} is saved but not supported by this mesh path."
        ));
    }
    if integer("threads")? != 1 {
        return Err("Gmsh meshing is fixed to one thread for deterministic output.".into());
    }
    Ok(MeshOptions {
        min_size,
        max_size,
        algorithm_3d: integer("algorithm3D")?,
        element_order,
        optimize: boolean("optimize")?,
        optimize_netgen: boolean("optimizeNetgen")?,
        optimize_threshold: gmsh
            .get("optimizeThreshold")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or("model.mesh.gmsh.optimizeThreshold must be a finite number.")?,
        smoothing: integer("smoothing")?,
    })
}

fn fmt(value: f64) -> String {
    format!("{value:.15e}")
}

pub fn emit(
    geo: &mut Vec<String>,
    options: &MeshOptions,
    has_size_controls: bool,
    dimension: usize,
) {
    geo.push(format!("Mesh.MeshSizeMin = {};", fmt(options.min_size)));
    geo.push(format!("Mesh.MeshSizeMax = {};", fmt(options.max_size)));
    geo.push("Mesh.MeshSizeFromPoints=0;".to_string());
    geo.push("Mesh.MeshSizeFromCurvature=0;".to_string());
    geo.push(format!(
        "Mesh.MeshSizeExtendFromBoundary={};",
        if has_size_controls { 0 } else { 1 }
    ));
    if dimension == 3 {
        geo.push(format!("Mesh.Algorithm3D={};", options.algorithm_3d));
    } else {
        geo.push("Mesh.Algorithm=6;".to_string());
    }
    geo.push(format!(
        "Mesh.Optimize={};",
        if options.optimize { 1 } else { 0 }
    ));
    geo.push(format!(
        "Mesh.OptimizeNetgen={};",
        if options.optimize_netgen { 1 } else { 0 }
    ));
    geo.push(format!(
        "Mesh.OptimizeThreshold={};",
        fmt(options.optimize_threshold)
    ));
    geo.push(format!("Mesh.Smoothing={};", options.smoothing));
    geo.push("Mesh.MshFileVersion=2.2;".to_string());
    geo.push("Mesh.Binary=0;".to_string());
    geo.push("Mesh.SaveAll=0;".to_string());
    geo.push(format!("Mesh.ElementOrder={};", options.element_order));
    geo.push("Mesh.RandomFactor=1e-9;".to_string());
    geo.push("Mesh.RandomSeed=1;".to_string());
    geo.push("General.NumThreads=1;".to_string());
    if dimension == 3 {
        geo.push("Mesh.MaxNumThreads3D=1;".to_string());
    }
}
