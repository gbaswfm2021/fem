type Point = [f64; 3];

use crate::{C, c_add};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Mesh2D {
    pub(super) nodes: Vec<Point>,
    pub(super) triangles: Vec<([usize; 3], usize)>,
    pub(super) boundary_edges: Vec<([usize; 2], usize)>,
    pub(super) integration_edges: Vec<([usize; 2], usize)>,
}

pub(super) fn parse_mesh_v2(text: &str) -> Result<Mesh2D, String> {
    let mut nodes = Vec::<Option<Point>>::new();
    let mut triangles = Vec::<Option<([usize; 3], usize)>>::new();
    let mut edges = Vec::<Option<([usize; 2], usize)>>::new();
    let mut integration_edges = Vec::<Option<([usize; 2], usize)>>::new();
    let mut header = false;
    for (line_index, line) in text.lines().enumerate() {
        let row: Vec<_> = line.trim().split(',').collect();
        match row.first().copied().unwrap_or("") {
            "FULLMETA_MESH/2" => header = true,
            "V" if row.len() >= 5 => {
                let id = row[1]
                    .parse::<usize>()
                    .map_err(|error| format!("mesh line {} node ID: {error}", line_index + 1))?;
                if nodes.len() <= id {
                    nodes.resize(id + 1, None);
                }
                let point = [
                    row[2].parse::<f64>().map_err(|error| error.to_string())?,
                    row[3].parse::<f64>().map_err(|error| error.to_string())?,
                    row[4].parse::<f64>().map_err(|error| error.to_string())?,
                ];
                if point.iter().any(|value| !value.is_finite()) || point[2].abs() > 1e-10 {
                    return Err(format!("2D mesh node {id} must be finite and lie in z=0."));
                }
                nodes[id] = Some(point);
            }
            "E" if row.len() >= 5 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if edges.len() <= id {
                    edges.resize(id + 1, None);
                }
                edges[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[4].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            "C" if row.len() >= 5 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if integration_edges.len() <= id {
                    integration_edges.resize(id + 1, None);
                }
                integration_edges[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[4].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            "Q" if row.len() >= 6 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if triangles.len() <= id {
                    triangles.resize(id + 1, None);
                }
                triangles[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                        row[4].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[5].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            _ => {}
        }
    }
    if !header {
        return Err("The 2D solver requires a FULLMETA_MESH/2 file.".into());
    }
    if nodes.is_empty()
        || nodes.iter().any(Option::is_none)
        || triangles.is_empty()
        || triangles.iter().any(Option::is_none)
    {
        return Err("The triangle mesh has incomplete node or cell records.".into());
    }
    let nodes: Vec<Point> = nodes.into_iter().map(Option::unwrap).collect();
    let triangles: Vec<([usize; 3], usize)> = triangles.into_iter().map(Option::unwrap).collect();
    let mut boundary_edges: Vec<([usize; 2], usize)> =
        edges.into_iter().map(Option::unwrap).collect();
    let mut integration_edges: Vec<([usize; 2], usize)> =
        integration_edges.into_iter().map(Option::unwrap).collect();
    if boundary_edges
        .iter()
        .chain(&integration_edges)
        .any(|(edge, _)| edge.iter().any(|node| *node >= nodes.len()))
        || triangles
            .iter()
            .any(|(triangle, _)| triangle.iter().any(|node| *node >= nodes.len()))
    {
        return Err("The triangle mesh references an out-of-range node.".into());
    }
    boundary_edges.sort_by_key(|(edge, _)| {
        let mut edge = *edge;
        edge.sort_unstable();
        edge
    });
    integration_edges.sort_by_key(|(edge, _)| {
        let mut edge = *edge;
        edge.sort_unstable();
        edge
    });
    Ok(Mesh2D {
        nodes,
        triangles,
        boundary_edges,
        integration_edges,
    })
}

pub(super) fn triangle_geometry(points: [Point; 3]) -> Result<(f64, [[f64; 2]; 3]), String> {
    let [a, b, c] = points;
    let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if !det.is_finite() || det == 0.0 {
        return Err(
            "The triangular mesh contains a degenerate cell or a non-finite determinant.".into(),
        );
    }
    let gradients = [
        [(b[1] - c[1]) / det, (c[0] - b[0]) / det],
        [(c[1] - a[1]) / det, (a[0] - c[0]) / det],
        [(a[1] - b[1]) / det, (b[0] - a[0]) / det],
    ];
    if gradients.iter().flatten().any(|value| !value.is_finite()) {
        return Err(
            "The triangular mesh cell gradients are non-finite at the supplied coordinate scale."
                .into(),
        );
    }
    let area = 0.5 * det.abs();
    if area == 0.0 {
        return Err(
            "The triangular mesh cell area underflows at the supplied coordinate scale.".into(),
        );
    }
    Ok((area, gradients))
}

pub(super) fn add_scalar_p1_helmholtz_triangle(
    matrix: &mut [BTreeMap<usize, C>],
    triangle: [usize; 3],
    area: f64,
    gradients: [[f64; 2]; 3],
    diffusion: C,
    reaction: C,
) {
    for i in 0..3 {
        for j in 0..3 {
            let gradient_product =
                gradients[i][0] * gradients[j][0] + gradients[i][1] * gradients[j][1];
            let mass_integral = if i == j { 2.0 } else { 1.0 };
            let scale = area;
            let value = C {
                re: scale * (diffusion.re * gradient_product + reaction.re * mass_integral / 12.0),
                im: scale * (diffusion.im * gradient_product + reaction.im * mass_integral / 12.0),
            };
            let entry = matrix[triangle[i]].entry(triangle[j]).or_default();
            *entry = c_add(*entry, value);
        }
    }
}

pub(super) fn adjacent_material_tag(mesh: &Mesh2D, edge: [usize; 2]) -> Result<usize, String> {
    mesh.triangles
        .iter()
        .find_map(|(triangle, tag)| {
            let has_first = triangle.contains(&edge[0]);
            let has_second = triangle.contains(&edge[1]);
            (has_first && has_second).then_some(*tag)
        })
        .ok_or_else(|| format!("Boundary edge {:?} has no adjacent triangle.", edge))
}
