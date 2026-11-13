use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type V3 = [f64; 3];

struct Mesh {
    dimension: usize,
    nodes: Vec<V3>,
    tetrahedra: Vec<[usize; 4]>,
    materials: Vec<usize>,
    boundary_triangles: Vec<SurfaceTriangle>,
    material_interface_triangles: Vec<MaterialInterfaceTriangle>,
    triangles: Vec<[usize; 3]>,
    triangle_materials: Vec<usize>,
    boundary_edges: Vec<BoundaryEdge>,
    integration_edges: Vec<BoundaryEdge>,
    material_interface_edges: Vec<MaterialInterfaceEdge>,
    point_selection_nodes: Vec<(usize, usize)>,
}

struct SurfaceTriangle {
    nodes: [usize; 3],
    physical_tag: usize,
}

struct MaterialInterfaceTriangle {
    nodes: [usize; 3],
    first_material: usize,
    second_material: usize,
}

struct BoundaryEdge {
    nodes: [usize; 2],
    physical_tag: usize,
}

struct MaterialInterfaceEdge {
    nodes: [usize; 2],
    first_material: usize,
    second_material: usize,
}

fn configured_gmsh_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("FULLMETA_GMSH_PATH").map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "FULLMETA_GMSH_PATH does not name a file: {}",
            path.display()
        ));
    }
    if !crate::schema_metadata::model_schema_selected() {
        // A project dispatcher starts the standalone `mesh` command in a new
        // process, so the model-selected schema must be recovered from its env.
        // Keep conventional PATH lookup only for direct, schema-free CLI use.
        if env::var_os("FULLMETA_MODEL_SCHEMA_FILE").is_some() {
            crate::schema_metadata::select_model_schema_from_environment()?;
        } else {
            return Ok(PathBuf::from("gmsh"));
        }
    }
    crate::schema_metadata::runtime_tool_path("gmsh")
}

/// Temporary geometry files live beside the original so relative Gmsh paths
/// resolve as before. They are removed on both success and failure.
struct TemporaryGeometry(PathBuf);
impl Drop for TemporaryGeometry {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn prepare_zero_expression_geometry(
    source: &str,
    map_path: &Path,
    gmsh: &Path,
    parent: &Path,
    stem: &str,
) -> Result<Option<TemporaryGeometry>, String> {
    let map: Value = match fs::read(map_path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid model map: {e}"))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return if source.contains("// FullMeta deferred zero filter ") {
                Err("The model map containing zero-expression filters is missing.".into())
            } else {
                Ok(None)
            };
        }
        Err(error) => return Err(format!("Could not read model map: {error}")),
    };
    let model_dimension = map.get("dimension").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut filters = Vec::new();
    for (key, entity_dimension) in [
        ("boundarySelections", model_dimension.saturating_sub(1)),
        ("edgeSelections", 1),
        ("pointSelections", 0),
    ] {
        if let Some(rows) = map.get(key).and_then(Value::as_array) {
            for row in rows {
                if let Some(filter) = row.get("zeroExpressionFilter") {
                    filters.push((entity_dimension, row, filter));
                }
            }
        }
    }
    if source.matches("// FullMeta deferred zero filter ").count() != filters.len() {
        return Err("Geometry and model map disagree about zero-expression filters.".into());
    }
    if filters.is_empty() {
        return Ok(None);
    }
    let tolerance = map
        .get("toleranceM")
        .and_then(Value::as_f64)
        .ok_or("Missing geometry tolerance.")?;
    let dll = if let Some(path) = env::var_os("FULLMETA_GMSH_API_PATH") {
        PathBuf::from(path)
    } else {
        let vendor = gmsh
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("Cannot locate the Gmsh SDK; set FULLMETA_GMSH_API_PATH.")?;
        vendor.join("gmsh-4.15.2-sdk").join("gmsh-4.15.dll")
    };
    let query =
        TemporaryGeometry(parent.join(format!("{stem}-zero-query-{}.geo", std::process::id())));
    let unmeshed = source
        .lines()
        .filter(|line| !matches!(line.trim(), "Mesh 2;" | "Mesh 3;"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&query.0, unmeshed)
        .map_err(|e| format!("Could not prepare Gmsh geometry query: {e}"))?;
    let api = crate::gmsh_api::GmshApi::load(&dll)?;
    api.set_number("General.Terminal", 0.0)?;
    api.open(&query.0)?;
    let mut prepared = source.to_owned();
    for (dimension, row, filter) in filters {
        let tag = row
            .get("physicalTag")
            .and_then(Value::as_i64)
            .ok_or("Missing selection physical tag.")? as i32;
        let expression = filter
            .get("expression")
            .and_then(Value::as_str)
            .ok_or("Missing zero expression.")?;
        let path = filter
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Missing zero expression path.")?;
        let variable = filter
            .get("variable")
            .and_then(Value::as_str)
            .ok_or("Missing selection variable.")?;
        if !variable
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("Invalid selection variable.".into());
        }
        let mut symbols = std::collections::HashMap::new();
        for (name, value) in filter
            .get("symbols")
            .and_then(Value::as_object)
            .ok_or("Missing zero expression symbols.")?
        {
            let number = value
                .get("value")
                .and_then(Value::as_f64)
                .ok_or("Invalid symbol value.")?;
            let dimension = serde_json::from_value(
                value
                    .get("dimension")
                    .cloned()
                    .ok_or("Missing symbol dimension.")?,
            )
            .map_err(|e| format!("Invalid symbol dimension: {e}"))?;
            symbols.insert(
                name.clone(),
                crate::expression::Scalar {
                    value: number,
                    dimension,
                },
            );
        }
        let entities = api.filter_physical_entities(
            dimension as i32,
            tag,
            model_dimension,
            expression,
            &symbols,
            tolerance,
            path,
        )?;
        let marker = format!("// FullMeta deferred zero filter {variable}");
        if prepared.matches(&marker).count() != 1 {
            return Err(format!(
                "Missing or duplicate filter marker for {variable}."
            ));
        }
        let ids = entities
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        prepared = prepared.replacen(&marker, &format!("{marker}\n{variable}[] = {{{ids}}};"), 1);
    }
    drop(api);
    let result =
        TemporaryGeometry(parent.join(format!("{stem}-zero-filtered-{}.geo", std::process::id())));
    fs::write(&result.0, prepared)
        .map_err(|e| format!("Could not prepare filtered Gmsh geometry: {e}"))?;
    Ok(Some(result))
}

#[cfg(not(windows))]
fn prepare_zero_expression_geometry(
    source: &str,
    _: &Path,
    _: &Path,
    _: &Path,
    _: &str,
) -> Result<Option<TemporaryGeometry>, String> {
    if source.contains("// FullMeta deferred zero filter ") {
        Err("The Gmsh API zero-expression filter is currently available on Windows only.".into())
    } else {
        Ok(None)
    }
}

const TETRAHEDRON_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];

fn find_material_interface_triangles(
    tetrahedra: &[[usize; 4]],
    materials: &[usize],
) -> Vec<MaterialInterfaceTriangle> {
    let mut faces = BTreeMap::<[usize; 3], ([usize; 3], Vec<usize>)>::new();
    for (tetrahedron, &material) in tetrahedra.iter().zip(materials) {
        for local_face in TETRAHEDRON_FACES {
            let nodes = local_face.map(|local_node| tetrahedron[local_node]);
            let mut key = nodes;
            key.sort_unstable();
            let adjacent_materials = &mut faces.entry(key).or_insert((nodes, Vec::new())).1;
            if !adjacent_materials.contains(&material) {
                adjacent_materials.push(material);
            }
        }
    }
    faces
        .into_iter()
        .filter_map(|(_, (nodes, adjacent_materials))| {
            let first_material = *adjacent_materials.first()?;
            let second_material = *adjacent_materials
                .iter()
                .find(|&&material| material != first_material)?;
            Some(MaterialInterfaceTriangle {
                nodes,
                first_material,
                second_material,
            })
        })
        .collect()
}

fn find_material_interface_edges(
    triangles: &[[usize; 3]],
    materials: &[usize],
) -> Vec<MaterialInterfaceEdge> {
    const TRIANGLE_EDGES: [[usize; 2]; 3] = [[0, 1], [1, 2], [2, 0]];
    let mut edges = BTreeMap::<[usize; 2], ([usize; 2], Vec<usize>)>::new();
    for (triangle, &material) in triangles.iter().zip(materials) {
        for local_edge in TRIANGLE_EDGES {
            let nodes = local_edge.map(|local_node| triangle[local_node]);
            let mut key = nodes;
            key.sort_unstable();
            let adjacent_materials = &mut edges.entry(key).or_insert((nodes, Vec::new())).1;
            if !adjacent_materials.contains(&material) {
                adjacent_materials.push(material);
            }
        }
    }
    edges
        .into_iter()
        .filter_map(|(_, (nodes, adjacent_materials))| {
            let first_material = *adjacent_materials.first()?;
            let second_material = *adjacent_materials
                .iter()
                .find(|&&material| material != first_material)?;
            Some(MaterialInterfaceEdge {
                nodes,
                first_material,
                second_material,
            })
        })
        .collect()
}

fn find_boundary_edges(triangles: &[[usize; 3]]) -> Vec<BoundaryEdge> {
    const TRIANGLE_EDGES: [[usize; 2]; 3] = [[0, 1], [1, 2], [2, 0]];
    let mut edges = BTreeMap::<[usize; 2], ([usize; 2], usize)>::new();
    for triangle in triangles {
        for local_edge in TRIANGLE_EDGES {
            let nodes = local_edge.map(|local_node| triangle[local_node]);
            let mut key = nodes;
            key.sort_unstable();
            edges
                .entry(key)
                .and_modify(|entry| entry.1 += 1)
                .or_insert((nodes, 1));
        }
    }
    edges
        .into_values()
        .filter_map(|(nodes, uses)| {
            (uses == 1).then_some(BoundaryEdge {
                nodes,
                physical_tag: 0,
            })
        })
        .collect()
}

fn export_fullmeta_mesh(mesh: &Mesh) -> String {
    if mesh.dimension == 2 {
        let mut output = String::from("FULLMETA_MESH/2\n");
        for (id, point) in mesh.nodes.iter().enumerate() {
            output.push_str(&format!(
                "V,{id},{:.15e},{:.15e},{:.15e}\n",
                point[0], point[1], point[2]
            ));
        }
        for (id, edge) in mesh.boundary_edges.iter().enumerate() {
            output.push_str(&format!(
                "E,{id},{},{},{}\n",
                edge.nodes[0], edge.nodes[1], edge.physical_tag
            ));
        }
        for (id, edge) in mesh.integration_edges.iter().enumerate() {
            output.push_str(&format!(
                "C,{id},{},{},{}\n",
                edge.nodes[0], edge.nodes[1], edge.physical_tag
            ));
        }
        for (id, edge) in mesh.material_interface_edges.iter().enumerate() {
            output.push_str(&format!(
                "J,{id},{},{},{},{}\n",
                edge.nodes[0], edge.nodes[1], edge.first_material, edge.second_material
            ));
        }
        for (id, (triangle, material)) in mesh
            .triangles
            .iter()
            .zip(&mesh.triangle_materials)
            .enumerate()
        {
            output.push_str(&format!(
                "Q,{id},{},{},{},{material}\n",
                triangle[0], triangle[1], triangle[2]
            ));
        }
        for (id, &(tag, node)) in mesh.point_selection_nodes.iter().enumerate() {
            output.push_str(&format!("P,{id},{node},{tag}\n"));
        }
        output.push_str("FULLMETA_MESH_DATA_END\n");
        return output;
    }
    let mut output = String::from("FULLMETA_MESH/1\n");
    for (id, point) in mesh.nodes.iter().enumerate() {
        output.push_str(&format!(
            "V,{id},{:.15e},{:.15e},{:.15e}\n",
            point[0], point[1], point[2]
        ));
    }
    for (id, triangle) in mesh.boundary_triangles.iter().enumerate() {
        output.push_str(&format!(
            "F,{id},{},{},{},{}\n",
            triangle.nodes[0], triangle.nodes[1], triangle.nodes[2], triangle.physical_tag
        ));
    }
    for (id, edge) in mesh
        .boundary_edges
        .iter()
        .filter(|edge| edge.physical_tag > 0)
        .enumerate()
    {
        output.push_str(&format!(
            "L,{id},{},{},{}\n",
            edge.nodes[0], edge.nodes[1], edge.physical_tag
        ));
    }
    for (id, triangle) in mesh.material_interface_triangles.iter().enumerate() {
        output.push_str(&format!(
            "I,{id},{},{},{},{},{}\n",
            triangle.nodes[0],
            triangle.nodes[1],
            triangle.nodes[2],
            triangle.first_material,
            triangle.second_material
        ));
    }
    for (id, (tetrahedron, material)) in mesh.tetrahedra.iter().zip(&mesh.materials).enumerate() {
        output.push_str(&format!(
            "T,{id},{},{},{},{},{material}\n",
            tetrahedron[0], tetrahedron[1], tetrahedron[2], tetrahedron[3]
        ));
    }
    for (id, &(tag, node)) in mesh.point_selection_nodes.iter().enumerate() {
        output.push_str(&format!("P,{id},{node},{tag}\n"));
    }
    output.push_str("FULLMETA_MESH_DATA_END\n");
    output
}

fn absolute_path(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn parse_msh2(path: &Path) -> Result<Mesh, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "Could not read {} as an ASCII MSH2 file: {error}",
            path.display()
        )
    })?;
    let mut lines = text.lines();
    let mut nodes = Vec::new();
    let mut node_map = HashMap::new();
    let mut tetrahedra = Vec::new();
    let mut materials = Vec::new();
    let mut boundary_triangles = Vec::new();
    let mut triangles = Vec::new();
    let mut triangle_materials = Vec::new();
    let mut boundary_edges = Vec::new();
    let mut integration_edges = Vec::new();
    let mut point_selection_nodes = Vec::new();

    while let Some(line) = lines.next() {
        match line.trim() {
            "$Nodes" => {
                let count: usize = lines
                    .next()
                    .ok_or("Missing node count in MSH2 file")?
                    .trim()
                    .parse()?;
                for _ in 0..count {
                    let row = lines.next().ok_or("Short MSH2 node section")?;
                    let fields: Vec<_> = row.split_whitespace().collect();
                    if fields.len() < 4 {
                        return Err("Malformed node row in MSH2 file".into());
                    }
                    let id: u64 = fields[0].parse()?;
                    let point: V3 = [
                        fields[1].parse::<f64>()?,
                        fields[2].parse::<f64>()?,
                        fields[3].parse::<f64>()?,
                    ];
                    if node_map.insert(id, nodes.len()).is_some() {
                        return Err(format!("Duplicate node id {id} in MSH2 file").into());
                    }
                    nodes.push(point);
                }
            }
            "$Elements" => {
                let count: usize = lines
                    .next()
                    .ok_or("Missing element count in MSH2 file")?
                    .trim()
                    .parse()?;
                for _ in 0..count {
                    let row = lines.next().ok_or("Short MSH2 element section")?;
                    let fields: Vec<_> = row.split_whitespace().collect();
                    if fields.len() < 3 {
                        return Err("Malformed element row in MSH2 file".into());
                    }
                    let element_type: u32 = fields[1].parse()?;
                    let tag_count: usize = fields[2].parse()?;
                    let node_start = 3 + tag_count;

                    if element_type == 15 {
                        if fields.len() < node_start + 1 {
                            return Err("Malformed point element in MSH2 file".into());
                        }
                        let physical_tag = if tag_count > 0 {
                            fields[3].parse::<usize>()?
                        } else {
                            0
                        };
                        let gmsh_id: u64 = fields[node_start].parse()?;
                        let node = *node_map.get(&gmsh_id).ok_or_else(|| {
                            format!("Point element references unknown node {gmsh_id}")
                        })?;
                        if physical_tag > 0 {
                            point_selection_nodes.push((physical_tag, node));
                        }
                    } else if element_type == 4 {
                        if fields.len() < node_start + 4 {
                            return Err("Malformed linear tetrahedron in MSH2 file".into());
                        }
                        let physical_tag = if tag_count > 0 {
                            fields[3].parse::<usize>()?
                        } else {
                            0
                        };
                        let mut tet = [0usize; 4];
                        for local in 0..4 {
                            let gmsh_id: u64 = fields[node_start + local].parse()?;
                            tet[local] = *node_map.get(&gmsh_id).ok_or_else(|| {
                                format!("Tetrahedron references unknown node {gmsh_id}")
                            })?;
                        }
                        tetrahedra.push(tet);
                        materials.push(physical_tag);
                    } else if element_type == 2 {
                        if fields.len() < node_start + 3 {
                            return Err("Malformed linear triangle in MSH2 file".into());
                        }
                        let physical_tag = if tag_count > 0 {
                            fields[3].parse::<usize>()?
                        } else {
                            0
                        };
                        let mut triangle = [0usize; 3];
                        for local in 0..3 {
                            let gmsh_id: u64 = fields[node_start + local].parse()?;
                            triangle[local] = *node_map.get(&gmsh_id).ok_or_else(|| {
                                format!("Triangle references unknown node {gmsh_id}")
                            })?;
                        }
                        triangles.push(triangle);
                        triangle_materials.push(physical_tag);
                    } else if element_type == 1 {
                        if fields.len() < node_start + 2 {
                            return Err("Malformed linear line in MSH2 file".into());
                        }
                        let physical_tag = if tag_count > 0 {
                            fields[3].parse::<usize>()?
                        } else {
                            0
                        };
                        if physical_tag > 0 {
                            let mut edge = [0usize; 2];
                            for local in 0..2 {
                                let gmsh_id: u64 = fields[node_start + local].parse()?;
                                edge[local] = *node_map.get(&gmsh_id).ok_or_else(|| {
                                    format!("Line references unknown node {gmsh_id}")
                                })?;
                            }
                            boundary_edges.push(BoundaryEdge {
                                nodes: edge,
                                physical_tag,
                            });
                        }
                    } else if matches!(element_type, 5..=7 | 11..=14 | 17..=19 | 29..=32) {
                        return Err(format!(
                            "Unsupported Gmsh 3D element type {element_type}; FullMeta mesh export currently requires linear tetrahedra (type 4)."
                        )
                        .into());
                    }
                }
            }
            _ => {}
        }
    }

    if nodes.is_empty() || (tetrahedra.is_empty() && triangles.is_empty()) {
        return Err(
            "MSH2 file contains no nodes or supported linear triangle/tetrahedron cells.".into(),
        );
    }
    let dimension = if tetrahedra.is_empty() { 2 } else { 3 };
    if dimension == 3 {
        for (triangle, physical_tag) in triangles
            .iter()
            .copied()
            .zip(triangle_materials.iter().copied())
        {
            if physical_tag > 0 {
                boundary_triangles.push(SurfaceTriangle {
                    nodes: triangle,
                    physical_tag,
                });
            }
        }
    } else {
        if nodes.iter().any(|node| node[2].abs() > 1e-10) {
            return Err("A 2D triangular mesh must lie in the x-y plane (z=0).".into());
        }
        let tagged_edges = std::mem::take(&mut boundary_edges);
        let mut gmsh_boundary_tags = BTreeMap::<[usize; 2], usize>::new();
        for edge in &tagged_edges {
            let mut key = edge.nodes;
            key.sort_unstable();
            gmsh_boundary_tags.insert(key, edge.physical_tag);
        }
        boundary_edges = find_boundary_edges(&triangles);
        let exterior_keys = boundary_edges
            .iter()
            .map(|edge| {
                let mut key = edge.nodes;
                key.sort_unstable();
                key
            })
            .collect::<HashSet<_>>();
        for edge in &mut boundary_edges {
            let mut key = edge.nodes;
            key.sort_unstable();
            edge.physical_tag = gmsh_boundary_tags.get(&key).copied().unwrap_or(0);
        }
        let mut internal_by_key = BTreeMap::<[usize; 2], usize>::new();
        for edge in tagged_edges {
            let mut key = edge.nodes;
            key.sort_unstable();
            if !exterior_keys.contains(&key) && edge.physical_tag > 0 {
                internal_by_key.insert(key, edge.physical_tag);
            }
        }
        integration_edges = internal_by_key
            .into_iter()
            .map(|(nodes, physical_tag)| BoundaryEdge {
                nodes,
                physical_tag,
            })
            .collect();
    }
    let material_interface_triangles = find_material_interface_triangles(&tetrahedra, &materials);
    let material_interface_edges = if dimension == 2 {
        find_material_interface_edges(&triangles, &triangle_materials)
    } else {
        Vec::new()
    };
    Ok(Mesh {
        dimension,
        nodes,
        tetrahedra,
        materials,
        boundary_triangles,
        material_interface_triangles,
        triangles,
        triangle_materials,
        boundary_edges,
        integration_edges,
        material_interface_edges,
        point_selection_nodes,
    })
}

fn enrich_point_selection_coordinates(mesh: &Mesh, map_path: &Path) -> Result<(), Box<dyn Error>> {
    if !map_path.is_file() {
        return Ok(());
    }
    let mut map: Value =
        serde_json::from_slice(&fs::read(map_path).map_err(|error| {
            format!("Could not read model map {}: {error}", map_path.display())
        })?)?;
    let Some(selections) = map.get_mut("pointSelections").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for selection in selections {
        let Some(tag) = selection
            .get("physicalTag")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
        else {
            continue;
        };
        let mut nodes = mesh
            .point_selection_nodes
            .iter()
            .filter_map(|(physical_tag, node)| (*physical_tag == tag).then_some(*node))
            .collect::<Vec<_>>();
        nodes.sort_unstable();
        nodes.dedup();
        selection["meshNodeCount"] = json!(nodes.len());
        if nodes.len() == 1 {
            selection["coordinateM"] = json!(mesh.nodes[nodes[0]]);
        } else if let Some(object) = selection.as_object_mut() {
            object.remove("coordinateM");
        }
    }
    fs::write(map_path, serde_json::to_vec_pretty(&map)?)?;
    Ok(())
}

fn selection_contains_point(
    selection_id: &str,
    point: V3,
    domain_feature_ids: &[String],
    primitives: &HashMap<String, &Value>,
    selections: &HashMap<String, &Value>,
    tolerance: f64,
    path: &str,
) -> Result<bool, String> {
    let selection = selections
        .get(selection_id)
        .ok_or_else(|| format!("{path} references unknown domain selection {selection_id}."))?;
    let addresses = crate::selection_address::selection_addresses(
        selection,
        domain_feature_ids,
        &format!("{path}.{selection_id}"),
    )?;
    crate::selection_address::matches_any_domain_address(
        point,
        &addresses,
        domain_feature_ids,
        primitives,
        tolerance,
        &format!("{path}.{selection_id}"),
    )
}

#[derive(Default)]
struct MaterialClassification {
    corrections: usize,
    names_by_tag: BTreeMap<usize, String>,
}

fn apply_model_material_regions(
    mesh: &mut Mesh,
    map_path: &Path,
    geo_name: &str,
) -> Result<MaterialClassification, Box<dyn Error>> {
    if mesh.dimension != 3 {
        return Err("3D material region classification was called for a 2D mesh.".into());
    }
    if !map_path.is_file() {
        return Ok(MaterialClassification::default());
    }
    let map: Value =
        serde_json::from_slice(&fs::read(map_path).map_err(|error| {
            format!("Could not read model map {}: {error}", map_path.display())
        })?)?;
    let map_schema = map.get("schema").and_then(Value::as_str).unwrap_or("");
    if map_schema != "fullmeta-mesh-map/6" {
        return Err(format!(
            "Model mesh map {} must use fullmeta-mesh-map/6.",
            map_path.display()
        )
        .into());
    }
    if map.get("dimension").and_then(Value::as_u64) != Some(3) {
        return Err("A 3D mesh requires a dimension-3 model map.".into());
    }
    if map.get("geometrySource").and_then(Value::as_str) != Some(geo_name) {
        return Err(format!(
            "Model map {} does not describe geometry source {geo_name}.",
            map_path.display()
        )
        .into());
    }
    let geometry = &map;
    let tolerance = geometry
        .get("toleranceM")
        .and_then(Value::as_f64)
        .ok_or("Model mesh map toleranceM must be numeric.")?;
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(
            "Model mesh map material geometry tolerance must be finite and nonnegative.".into(),
        );
    }
    let primitive_values = geometry
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or("Model mesh map primitives must be an array.")?;
    let mut primitives = HashMap::<String, &Value>::new();
    for primitive in primitive_values {
        let id = primitive
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Model mesh map contains a primitive without an ID.")?;
        if primitives.insert(id.to_string(), primitive).is_some() {
            return Err(format!("Model mesh map contains duplicate primitive ID {id}. ").into());
        }
    }
    let feature_ids = geometry
        .get("domainFeatureIds")
        .and_then(Value::as_array)
        .ok_or("Model mesh map domainFeatureIds must be an array.")?;
    let domain_feature_ids = feature_ids
        .iter()
        .map(|id| {
            id.as_str()
                .map(str::to_string)
                .ok_or("Model mesh map domainFeatureIds must contain strings.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let selection_values = geometry
        .get("domainSelections")
        .and_then(Value::as_array)
        .ok_or("Model mesh map domainSelections must be an array.")?;
    let mut selections = HashMap::<String, &Value>::new();
    for selection in selection_values {
        let id = selection
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Model mesh map contains a domain selection without an ID.")?;
        if selections.insert(id.to_string(), selection).is_some() {
            return Err(
                format!("Model mesh map contains duplicate domain selection ID {id}.").into(),
            );
        }
    }
    let groups = map
        .get("physicalGroups")
        .and_then(Value::as_array)
        .ok_or("Model mesh map physicalGroups must be an array.")?;
    if groups.is_empty() {
        return Ok(MaterialClassification::default());
    }
    let mut material_regions = Vec::<(String, String, usize)>::new();
    let mut names_by_tag = BTreeMap::<usize, String>::new();
    let mut tags = HashSet::new();
    for group in groups {
        let name = group
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Model mesh map physical group has no name.")?
            .to_string();
        let selection_id = group
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Model mesh map physical group has no selectionId.")?
            .to_string();
        let tag = group
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or("Model mesh map physical group has no numeric physicalTag.")?
            as usize;
        if tag == 0 || !tags.insert(tag) {
            return Err(format!(
                "Model mesh map material physical tag {tag} is zero or duplicated."
            )
            .into());
        }
        if !selections.contains_key(&selection_id) {
            return Err(format!(
                "Model mesh map material group {name} references missing selection {selection_id}."
            )
            .into());
        }
        names_by_tag.insert(tag, name.clone());
        material_regions.push((name, selection_id, tag));
    }

    let mut changed = 0;
    for (tet_index, (tetrahedron, physical_tag)) in
        mesh.tetrahedra.iter().zip(&mut mesh.materials).enumerate()
    {
        let center = std::array::from_fn(|axis| {
            tetrahedron
                .iter()
                .map(|&node| mesh.nodes[node][axis])
                .sum::<f64>()
                / 4.0
        });
        let mut matches = Vec::new();
        for (name, selection_id, tag) in &material_regions {
            if selection_contains_point(
                selection_id,
                center,
                &domain_feature_ids,
                &primitives,
                &selections,
                tolerance,
                "model mesh map",
            )? {
                matches.push((name.as_str(), *tag));
            }
        }
        if matches.len() != 1 {
            return Err(format!("Tetrahedron {tet_index} at ({:.9e}, {:.9e}, {:.9e}) matches {} material regions ({}). Check that material selections cover the simulation domain exactly once.", center[0], center[1], center[2], matches.len(), matches.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ")).into());
        }
        let expected = matches[0].1;
        if *physical_tag != expected {
            *physical_tag = expected;
            changed += 1;
        }
    }
    mesh.material_interface_triangles =
        find_material_interface_triangles(&mesh.tetrahedra, &mesh.materials);
    Ok(MaterialClassification {
        corrections: changed,
        names_by_tag,
    })
}

fn classify_2d_materials(
    mesh: &mut Mesh,
    map_path: &Path,
    geo_name: &str,
) -> Result<MaterialClassification, Box<dyn Error>> {
    if !map_path.is_file() {
        return Ok(MaterialClassification::default());
    }
    let map: Value =
        serde_json::from_slice(&fs::read(map_path).map_err(|error| {
            format!("Could not read model map {}: {error}", map_path.display())
        })?)?;
    let schema = map.get("schema").and_then(Value::as_str).unwrap_or("");
    if schema != "fullmeta-mesh-map/6" {
        return Err(format!(
            "2D model mesh map {} must use fullmeta-mesh-map/6.",
            map_path.display()
        )
        .into());
    }
    if map.get("dimension").and_then(Value::as_u64) != Some(2) {
        return Err("A 2D mesh requires a dimension-2 model map.".into());
    }
    if map.get("geometrySource").and_then(Value::as_str) != Some(geo_name) {
        return Err(format!(
            "Model map {} does not describe geometry source {geo_name}.",
            map_path.display()
        )
        .into());
    }
    let groups = map
        .get("physicalGroups")
        .and_then(Value::as_array)
        .ok_or("Model mesh map physicalGroups must be an array.")?;
    if groups.is_empty() {
        return Ok(MaterialClassification::default());
    }
    let mut classification = MaterialClassification::default();
    let selection_values = map
        .get("domainSelections")
        .and_then(Value::as_array)
        .ok_or("2D model map domainSelections must be an array.")?;
    let selections = selection_values
        .iter()
        .filter_map(|selection| Some((selection.get("id")?.as_str()?.to_string(), selection)))
        .collect::<HashMap<_, _>>();
    let primitive_values = map
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or("2D model map primitives must be an array.")?;
    let primitives = primitive_values
        .iter()
        .filter_map(|primitive| Some((primitive.get("id")?.as_str()?.to_string(), primitive)))
        .collect::<HashMap<_, _>>();
    let feature_ids = map
        .get("domainFeatureIds")
        .and_then(Value::as_array)
        .ok_or("2D model map domainFeatureIds must be an array.")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or("2D model map domainFeatureIds must contain strings.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tolerance = map
        .get("toleranceM")
        .and_then(Value::as_f64)
        .ok_or("2D model map toleranceM must be numeric.")?
        .max(1e-10);
    let mut material_regions = Vec::<(String, String, usize)>::new();
    for group in groups {
        let name = group
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Model map physical group has no name.")?;
        let selection_id = group
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Model map physical group has no selectionId.")?;
        let tag = group
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or("Model map physical group has no numeric physicalTag.")?
            as usize;
        if tag == 0
            || classification
                .names_by_tag
                .insert(tag, name.to_string())
                .is_some()
        {
            return Err(
                format!("2D model map has zero or duplicate material physical tag {tag}.").into(),
            );
        }
        if !selections.contains_key(selection_id) {
            return Err(format!(
                "2D material group {name:?} references missing selection {selection_id:?}."
            )
            .into());
        }
        material_regions.push((name.to_string(), selection_id.to_string(), tag));
    }
    let mut changed = 0;
    for (triangle_index, (triangle, tag)) in mesh
        .triangles
        .iter()
        .zip(&mut mesh.triangle_materials)
        .enumerate()
    {
        let center = std::array::from_fn(|axis| {
            triangle
                .iter()
                .map(|&node| mesh.nodes[node][axis])
                .sum::<f64>()
                / 3.0
        });
        let mut matches = Vec::new();
        for (name, selection_id, material_tag) in &material_regions {
            let selection = selections.get(selection_id).ok_or_else(|| {
                format!("2D material group references missing selection {selection_id:?}.")
            })?;
            let addresses = crate::selection_address::selection_addresses(
                selection,
                &feature_ids,
                &format!("2D mesh map selection {selection_id}"),
            )?;
            if crate::selection_address::matches_any_domain_address(
                center,
                &addresses,
                &feature_ids,
                &primitives,
                tolerance,
                &format!("2D mesh map selection {selection_id}"),
            )? {
                matches.push((name.as_str(), *material_tag));
            }
        }
        if matches.len() != 1 {
            return Err(format!("Triangle {triangle_index} at ({:.9e}, {:.9e}) matches {} material regions. Assign every partition exactly one material.",center[0],center[1],matches.len()).into());
        }
        if *tag != matches[0].1 {
            *tag = matches[0].1;
            changed += 1;
        }
    }
    mesh.material_interface_edges =
        find_material_interface_edges(&mesh.triangles, &mesh.triangle_materials);
    let boundary_selections = map
        .get("boundarySelections")
        .and_then(Value::as_array)
        .ok_or("2D model map boundarySelections must be an array.")?;
    let selections = boundary_selections
        .iter()
        .filter(|selection| selection.get("plane").is_some())
        .map(|selection| {
            let plane = selection
                .get("plane")
                .ok_or("Boundary selection plane is required.")?;
            let point = plane
                .get("pointM")
                .and_then(Value::as_array)
                .ok_or("Boundary selection plane.pointM must contain three numbers.")?;
            let normal = plane
                .get("normal")
                .and_then(Value::as_array)
                .ok_or("Boundary selection plane.normal must contain three numbers.")?;
            if point.len() != 3 || normal.len() != 3 {
                return Err(
                    "Boundary selection plane point and normal must contain three numbers.".into(),
                );
            }
            let point = [
                point[0]
                    .as_f64()
                    .ok_or("Boundary selection plane point must be numeric.")?,
                point[1]
                    .as_f64()
                    .ok_or("Boundary selection plane point must be numeric.")?,
                point[2]
                    .as_f64()
                    .ok_or("Boundary selection plane point must be numeric.")?,
            ];
            let normal = [
                normal[0]
                    .as_f64()
                    .ok_or("Boundary selection plane normal must be numeric.")?,
                normal[1]
                    .as_f64()
                    .ok_or("Boundary selection plane normal must be numeric.")?,
                normal[2]
                    .as_f64()
                    .ok_or("Boundary selection plane normal must be numeric.")?,
            ];
            let tag = selection
                .get("physicalTag")
                .and_then(Value::as_u64)
                .ok_or("Boundary selection physicalTag must be numeric.")?
                as usize;
            Ok((point, normal, tag))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    for edge in &mut mesh.boundary_edges {
        if edge.physical_tag != 0 {
            continue;
        }
        for &(point, normal, tag) in &selections {
            let on_selection = edge.nodes.iter().all(|&node| {
                (0..3)
                    .map(|axis| normal[axis] * (mesh.nodes[node][axis] - point[axis]))
                    .sum::<f64>()
                    .abs()
                    <= tolerance
            });
            if on_selection {
                edge.physical_tag = tag;
                break;
            }
        }
    }
    classification.corrections = changed;
    Ok(classification)
}

fn triangle_area_and_quality(points: [V3; 3]) -> (f64, f64) {
    let ab = sub(points[1], points[0]);
    let ac = sub(points[2], points[0]);
    let bc = sub(points[2], points[1]);
    let area = 0.5 * (ab[0] * ac[1] - ab[1] * ac[0]).abs();
    let sum_squared_edges = dot(ab, ab) + dot(ac, ac) + dot(bc, bc);
    let normalized_mean_ratio = if sum_squared_edges > 0.0 {
        4.0 * 3.0f64.sqrt() * area / sum_squared_edges
    } else {
        0.0
    };
    (area, normalized_mean_ratio)
}

fn report_2d_mesh(
    mesh: &Mesh,
    classification: &MaterialClassification,
    geo_path: &Path,
    msh_path: &Path,
    mesh_path: &Path,
    log_path: &Path,
) -> Value {
    let mut quality = Vec::with_capacity(mesh.triangles.len());
    let mut materials = BTreeMap::<usize, usize>::new();
    let mut material_metrics = BTreeMap::<usize, (f64, Vec<f64>)>::new();
    for (triangle, &material) in mesh.triangles.iter().zip(&mesh.triangle_materials) {
        let points = triangle.map(|node| mesh.nodes[node]);
        let (area, ratio) = triangle_area_and_quality(points);
        quality.push(ratio);
        *materials.entry(material).or_default() += 1;
        let metrics = material_metrics.entry(material).or_default();
        metrics.0 += area;
        let longest_edge = [(0, 1), (1, 2), (2, 0)]
            .iter()
            .map(|&(a, b)| {
                let edge = sub(points[a], points[b]);
                dot(edge, edge).sqrt()
            })
            .fold(0.0, f64::max);
        metrics.1.push(longest_edge);
    }
    let material_diagnostics: Vec<Value> = materials.iter().map(|(&tag, &count)| {
        let (area, sizes) = material_metrics.get_mut(&tag).expect("each material count has metrics");
        json!({
            "physicalMaterialId": tag,
            "name": classification.names_by_tag.get(&tag).cloned().unwrap_or_else(|| format!("Physical material {tag}")),
            "triangles": count,
            "areaM2": area,
            "longestEdgeM": quality_stats(sizes)
        })
    }).collect();
    let mut boundary_groups = BTreeMap::<usize, usize>::new();
    for edge in &mesh.boundary_edges {
        *boundary_groups.entry(edge.physical_tag).or_default() += 1;
    }
    json!({
        "tool": "FullMeta generic Gmsh mesh preparation",
        "geometry": geo_path,
        "msh": msh_path,
        "mesh": mesh_path,
        "gmshLog": log_path,
        "dimension": 2,
        "elementFamily": "triangle",
        "nodes": mesh.nodes.len(),
        "triangles": mesh.triangles.len(),
        "trianglesByPhysicalMaterialId": materials,
        "materialDiagnostics": material_diagnostics,
        "semanticMaterialTagCorrections": classification.corrections,
        "boundaryEdges": mesh.boundary_edges.len(),
        "materialInterfaceEdges": mesh.material_interface_edges.len(),
        "boundaryEdgesByPhysicalSelectionId": boundary_groups,
        "quality": {"normalizedMeanRatio": quality_stats(&mut quality)}
    })
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn determinant(a: V3, b: V3, c: V3) -> f64 {
    dot(a, cross(b, c))
}

fn tetrahedron_quality(points: [V3; 4]) -> (f64, f64, f64) {
    let mut edge_lengths_squared = 0.0;
    for i in 0..4 {
        for j in i + 1..4 {
            edge_lengths_squared += dot(sub(points[i], points[j]), sub(points[i], points[j]));
        }
    }
    let volume = determinant(
        sub(points[1], points[0]),
        sub(points[2], points[0]),
        sub(points[3], points[0]),
    )
    .abs()
        / 6.0;
    let mean_ratio = if edge_lengths_squared > 0.0 {
        12.0 * (3.0 * volume).powf(2.0 / 3.0) / edge_lengths_squared
    } else {
        0.0
    };

    let ideal_angle = (1.0f64 / 3.0).acos();
    let mut normals = Vec::new();
    for opposite in 0..4 {
        let face: Vec<_> = (0..4).filter(|&index| index != opposite).collect();
        let mut normal = cross(
            sub(points[face[1]], points[face[0]]),
            sub(points[face[2]], points[face[0]]),
        );
        if dot(normal, sub(points[opposite], points[face[0]])) > 0.0 {
            normal = normal.map(|value| -value);
        }
        let length = dot(normal, normal).sqrt();
        if length == 0.0 {
            return (0.0, 0.0, volume);
        }
        normals.push(normal.map(|value| value / length));
    }

    let mut min_angle = f64::INFINITY;
    let mut max_angle = f64::NEG_INFINITY;
    for i in 0..4 {
        for j in i + 1..4 {
            let angle = (-dot(normals[i], normals[j])).clamp(-1.0, 1.0).acos();
            min_angle = min_angle.min(angle);
            max_angle = max_angle.max(angle);
        }
    }
    let skew = ((max_angle - ideal_angle) / (std::f64::consts::PI - ideal_angle))
        .max((ideal_angle - min_angle) / ideal_angle)
        .clamp(0.0, 1.0);
    (mean_ratio, 1.0 - skew, volume)
}

fn quality_stats(values: &mut [f64]) -> Value {
    values.sort_by(f64::total_cmp);
    let count = values.len();
    json!({
        "min": values[0],
        "mean": values.iter().sum::<f64>() / count as f64,
        "p05": values[(count - 1) * 5 / 100],
        "median": values[(count - 1) / 2],
        "p95": values[(count - 1) * 95 / 100],
        "max": values[count - 1]
    })
}

fn gmsh_process_log(
    executable: &Path,
    geometry: &Path,
    dimension: u8,
    status: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> String {
    // A successful Gmsh process may emit no warnings. Preserve a real invocation
    // record as well as both captured streams, even for a completely quiet run.
    format!(
        "FullMeta Gmsh invocation\nExecutable: {}\nGeometry: {}\nDimension: {dimension}\nExit status: {status}\n--- stdout ({} bytes) ---\n{}\n--- stderr ({} bytes) ---\n{}\n",
        executable.display(),
        geometry.display(),
        stdout.len(),
        String::from_utf8_lossy(stdout),
        stderr.len(),
        String::from_utf8_lossy(stderr)
    )
}

#[cfg(test)]
mod gmsh_log_tests {
    use super::gmsh_process_log;
    use std::path::Path;

    #[test]
    fn quiet_success_retains_the_actual_invocation_and_status() {
        let log = gmsh_process_log(
            Path::new("gmsh.exe"),
            Path::new("case.geo"),
            3,
            "exit code: 0",
            b"",
            b"",
        );
        assert!(log.contains("Geometry: case.geo"));
        assert!(log.contains("Dimension: 3\nExit status: exit code: 0"));
        assert!(log.contains("stdout (0 bytes)"));
        assert!(log.contains("stderr (0 bytes)"));
    }

    #[test]
    fn failed_run_preserves_both_captured_streams_and_failure_status() {
        let log = gmsh_process_log(
            Path::new("gmsh.exe"),
            Path::new("case.geo"),
            2,
            "exit code: 1",
            b"mesh output",
            b"geometry error",
        );
        assert!(log.contains("Exit status: exit code: 1"));
        assert!(log.contains("mesh output"));
        assert!(log.contains("geometry error"));
    }
}

pub fn run(geometry_input: &str, gmsh_override: Option<&str>) -> Result<Value, Box<dyn Error>> {
    let geo_path = absolute_path(Path::new(geometry_input))?;
    if !geo_path.is_file() {
        return Err(format!("Geometry file not found: {}", geo_path.display()).into());
    }
    if !geo_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("geo"))
    {
        return Err("Input must be a Gmsh .geo file.".into());
    }
    let parent = geo_path.parent().unwrap_or(Path::new("."));
    let geo_name = geo_path
        .file_name()
        .ok_or("Geometry path has no filename")?;
    let stem = geo_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("Geometry filename is not valid UTF-8")?;
    let msh_name = format!("{stem}.msh");
    let mesh_name = format!("{stem}-mesh.mesh");
    let log_name = format!("{stem}-gmsh.log");
    let report_name = format!("{stem}-mesh-report.json");

    let gmsh = if let Some(executable) = gmsh_override {
        let path = PathBuf::from(executable);
        if path.is_absolute() || path.components().count() > 1 {
            absolute_path(&path)?
        } else {
            path
        }
    } else {
        configured_gmsh_path().map_err(|error| -> Box<dyn Error> { error.into() })?
    };

    let geometry_source = fs::read_to_string(&geo_path)?;
    let map_name = format!("{stem}-model-map.json");
    let prepared_geometry = prepare_zero_expression_geometry(
        &geometry_source,
        &parent.join(&map_name),
        &gmsh,
        parent,
        stem,
    )?;
    let gmsh_geometry_name = prepared_geometry
        .as_ref()
        .map(|file| file.0.file_name().expect("temporary geometry has a name"))
        .unwrap_or(geo_name);
    let mesh_dimension = geometry_source
        .lines()
        .any(|line| line.split("//").next().unwrap_or("").trim() == "Mesh 2;");
    let run = Command::new(&gmsh)
        .current_dir(parent)
        .arg(if mesh_dimension { "-2" } else { "-3" })
        .arg("-format")
        .arg("msh2")
        .arg("-o")
        .arg(&msh_name)
        .arg("-v")
        .arg("4")
        .arg(gmsh_geometry_name)
        .output()
        .map_err(|error| {
            format!(
                "Could not launch Gmsh at {}: {error}. Check x-fullmeta-runtime.tools.gmsh in the selected application schema or pass an executable path.",
                gmsh.display()
            )
        })?;
    let log = gmsh_process_log(
        &gmsh,
        &geo_path,
        if mesh_dimension { 2 } else { 3 },
        &run.status.to_string(),
        &run.stdout,
        &run.stderr,
    );
    fs::write(parent.join(&log_name), &log)?;
    if !run.status.success() {
        let tail = log.lines().rev().take(35).collect::<Vec<_>>();
        return Err(format!(
            "Gmsh failed ({}). Log: {}\n{}",
            run.status,
            parent.join(&log_name).display(),
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        )
        .into());
    }

    let msh_path = parent.join(&msh_name);
    let mut mesh = parse_msh2(&msh_path)?;
    let geo_name = geo_name
        .to_str()
        .ok_or("Geometry filename is not valid UTF-8")?;
    enrich_point_selection_coordinates(&mesh, &parent.join(&map_name))?;
    let material_classification = if mesh.dimension == 2 {
        classify_2d_materials(&mut mesh, &parent.join(map_name), geo_name)?
    } else {
        apply_model_material_regions(&mut mesh, &parent.join(map_name), geo_name)?
    };
    let mesh_path = parent.join(&mesh_name);
    fs::write(&mesh_path, export_fullmeta_mesh(&mesh))?;
    if mesh.dimension == 2 {
        let report = report_2d_mesh(
            &mesh,
            &material_classification,
            &geo_path,
            &msh_path,
            &mesh_path,
            &parent.join(&log_name),
        );
        let report_path = parent.join(&report_name);
        fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
        return Ok(report);
    }
    let mut mean_ratio = Vec::with_capacity(mesh.tetrahedra.len());
    let mut dihedral = Vec::with_capacity(mesh.tetrahedra.len());
    let mut materials = BTreeMap::<usize, usize>::new();
    let mut material_metrics = BTreeMap::<usize, (f64, Vec<f64>)>::new();
    for (tet, &material) in mesh.tetrahedra.iter().zip(&mesh.materials) {
        let points = tet.map(|node| mesh.nodes[node]);
        let (ratio, score, volume) = tetrahedron_quality(points);
        if volume <= 1e-22 {
            return Err("Generated mesh contains a degenerate tetrahedron.".into());
        }
        mean_ratio.push(ratio);
        dihedral.push(score);
        *materials.entry(material).or_default() += 1;
        let metrics = material_metrics.entry(material).or_default();
        metrics.0 += volume;
        let mut longest_edge = 0.0f64;
        for i in 0..4 {
            for j in i + 1..4 {
                let edge = sub(points[i], points[j]);
                longest_edge = longest_edge.max(dot(edge, edge).sqrt());
            }
        }
        metrics.1.push(longest_edge);
    }
    let material_diagnostics: Vec<Value> = materials.iter().map(|(&tag, &count)| {
        let (volume, characteristic_sizes) = material_metrics.get_mut(&tag).expect("each material count has metrics");
        json!({
            "physicalMaterialId": tag,
            "name": material_classification.names_by_tag.get(&tag).cloned().unwrap_or_else(|| format!("Physical material {tag}")),
            "tetrahedra": count,
            "volumeM3": volume,
            "longestEdgeM": quality_stats(characteristic_sizes)
        })
    }).collect();
    let mut boundary_groups = BTreeMap::<usize, usize>::new();
    for triangle in &mesh.boundary_triangles {
        *boundary_groups.entry(triangle.physical_tag).or_default() += 1;
    }

    let report = json!({
        "tool": "FullMeta generic Gmsh mesh preparation",
        "geometry": geo_path,
        "msh": msh_path,
        "mesh": mesh_path,
        "gmshLog": parent.join(&log_name),
        "nodes": mesh.nodes.len(),
        "tetrahedra": mesh.tetrahedra.len(),
        "tetrahedraByPhysicalMaterialId": materials,
        "materialDiagnostics": material_diagnostics,
        "semanticMaterialTagCorrections": material_classification.corrections,
        "boundaryTriangles": mesh.boundary_triangles.len(),
        "materialInterfaceTriangles": mesh.material_interface_triangles.len(),
        "boundaryTrianglesByPhysicalSelectionId": boundary_groups,
        "quality": {
            "normalizedMeanRatio": quality_stats(&mut mean_ratio),
            "normalizedDihedralScore": quality_stats(&mut dihedral)
        }
    });
    let report_path = parent.join(&report_name);
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}
