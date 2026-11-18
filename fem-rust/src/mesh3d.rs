//! Generic tetrahedral mesh data and FULLMETA_MESH/1 reader.
//!
//! Callers choose the record policy explicitly; no physics profile is encoded
//! here. The same parser supports ordered terminated files and indexed files
//! with either duplicate rejection or last-record-wins behavior.

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct Mesh3D {
    pub(crate) nodes: Vec<[f64; 3]>,
    pub(crate) tetrahedra: Vec<([usize; 4], usize)>,
    pub(crate) boundary_faces: Vec<([usize; 3], usize)>,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexedMesh3D {
    pub(crate) nodes: Vec<[f64; 3]>,
    pub(crate) tetrahedra: Vec<([usize; 4], usize)>,
    pub(crate) boundary_faces: Vec<([usize; 3], usize)>,
    pub(crate) edge_selections: Vec<([usize; 2], usize)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DuplicatePolicy { Reject, LastWins }

#[derive(Clone, Copy)]
enum RecordPolicy {
    OrderedTerminated,
    Indexed(DuplicatePolicy),
    /// Node/cell IDs index arrays; facet/edge records are an ordered stream
    /// whose record IDs carry no meaning. Short and unrelated rows are ignored.
    IndexedEntityStream,
}

struct Records {
    nodes: BTreeMap<usize, [f64; 3]>,
    cells: BTreeMap<usize, ([usize; 4], usize)>,
    faces: BTreeMap<usize, ([usize; 3], usize)>,
    edges: BTreeMap<usize, ([usize; 2], usize)>,
}

fn insert<T>(map: &mut BTreeMap<usize, T>, id: usize, value: T, kind: &str, policy: RecordPolicy) -> Result<(), String> {
    match policy {
        RecordPolicy::OrderedTerminated if id != map.len() => return Err(format!("Duplicate or noncontiguous mesh {kind} ID.")),
        RecordPolicy::Indexed(DuplicatePolicy::Reject) if map.contains_key(&id) => return Err(format!("Duplicate mesh {kind} {id}.")),
        _ => {},
    }
    map.insert(id, value);
    Ok(())
}

fn dense<T>(entries: BTreeMap<usize, T>, kind: &str) -> Result<Vec<T>, String> {
    let mut output = Vec::with_capacity(entries.len());
    for (expected, (id, value)) in entries.into_iter().enumerate() {
        if id != expected { return Err(format!("The tetrahedral mesh has incomplete {kind} records.")); }
        output.push(value);
    }
    Ok(output)
}

fn parse_v1(text: &str, policy: RecordPolicy) -> Result<IndexedMesh3D, String> {
    let ordered = matches!(policy, RecordPolicy::OrderedTerminated);
    let entity_stream = matches!(policy, RecordPolicy::IndexedEntityStream);
    let content = if ordered { text.trim() } else { text };
    let mut lines = content.lines().enumerate();
    let mut header = false;
    if ordered {
        if lines.next().map(|(_, line)| line.trim()) != Some("FULLMETA_MESH/1") {
            return Err("Expected FULLMETA_MESH/1 header.".into());
        }
        header = true;
    }
    let mut records = Records { nodes: BTreeMap::new(), cells: BTreeMap::new(), faces: BTreeMap::new(), edges: BTreeMap::new() };
    let mut ended = false;
    for (line_index, line) in lines {
        if ordered && ended { return Err("Unexpected data after mesh terminator.".into()); }
        let row: Vec<_> = line.trim().split(',').collect();
        let record = row.first().copied().unwrap_or("");
        if !ordered && record == "FULLMETA_MESH/1" { header = true; continue; }
        if entity_stream && row.len() < 5 { continue; }
        let index = |i: usize| -> Result<usize, String> {
            row.get(i).ok_or_else(|| format!("Incomplete mesh line {}.", line_index + 1))?
                .parse().map_err(|_| format!("Invalid integer in mesh line {}.", line_index + 1))
        };
        let valid_length = |needed: usize| if ordered { row.len() == needed } else { row.len() >= needed };
        match record {
            "V" if valid_length(5) => {
                let id = index(1)?;
                let mut point = [0.0; 3];
                for axis in 0..3 {
                    point[axis] = row[axis + 2].parse::<f64>().map_err(|e| e.to_string())?;
                    if !point[axis].is_finite() { return Err(format!("Mesh node {id} must be finite.")); }
                }
                insert(&mut records.nodes, id, point, "node", policy)?;
            }
            "T" if valid_length(7) => {
                insert(&mut records.cells, index(1)?, ([index(2)?, index(3)?, index(4)?, index(5)?], index(6)?), "tetrahedron", policy)?;
            }
            "F" if valid_length(6) => {
                let id = if entity_stream { records.faces.len() } else { index(1)? };
                insert(&mut records.faces, id, ([index(2)?, index(3)?, index(4)?], index(5)?), "boundary face", policy)?;
            }
            "L" if ordered => {},
            "L" if valid_length(5) => {
                let id = if entity_stream { records.edges.len() } else { index(1)? };
                insert(&mut records.edges, id, ([index(2)?, index(3)?], index(4)?), "edge", policy)?;
            }
            "I" | "P" if ordered => {},
            "FULLMETA_MESH_DATA_END" if ordered && row.len() == 1 => ended = true,
            "V" | "T" | "F" | "L" if entity_stream => {},
            "V" | "T" | "F" | "L" => return Err(format!("Unsupported or malformed mesh line {}.", line_index + 1)),
            _ if !ordered => {},
            _ => return Err(format!("Unsupported or malformed mesh line {}.", line_index + 1)),
        }
    }
    if !header && !entity_stream { return Err("Expected FULLMETA_MESH/1 header.".into()); }
    if ordered && !ended { return Err("Mesh terminator is missing; mesh may be truncated.".into()); }
    let nodes = dense(records.nodes, "node")?;
    let tetrahedra = dense(records.cells, "tetrahedron")?;
    let boundary_faces = dense(records.faces, "boundary face")?;
    let edge_selections = dense(records.edges, "edge")?;
    if !ordered {
        if nodes.is_empty() || tetrahedra.is_empty() {
            return Err("The tetrahedral mesh has incomplete node or cell records.".into());
        }
        if tetrahedra.iter().any(|(v, _)| v.iter().any(|&i| i >= nodes.len()))
            || boundary_faces.iter().any(|(v, _)| v.iter().any(|&i| i >= nodes.len()))
            || edge_selections.iter().any(|(v, _)| v.iter().any(|&i| i >= nodes.len())) {
            return Err("The tetrahedral mesh references an out-of-range node.".into());
        }
    }
    Ok(IndexedMesh3D { nodes, tetrahedra, boundary_faces, edge_selections })
}

/// Strict ordered, terminated mesh; L/I/P records are ignored metadata.
pub(crate) fn parse_contiguous_v1(text: &str) -> Result<Mesh3D, String> {
    let mesh = parse_v1(text, RecordPolicy::OrderedTerminated)?;
    Ok(Mesh3D { nodes: mesh.nodes, tetrahedra: mesh.tetrahedra, boundary_faces: mesh.boundary_faces })
}

/// Indexed mesh: unordered records and extra fields are accepted; a header is
/// required, the terminator and unrelated metadata are optional. Duplicate
/// handling is caller-declared. All four entity types must have contiguous IDs;
/// missing face/edge IDs return an error instead of panicking.
pub(crate) fn parse_indexed_v1(text: &str, duplicate: DuplicatePolicy) -> Result<IndexedMesh3D, String> {
    parse_v1(text, RecordPolicy::Indexed(duplicate))
}

/// Indexed nodes and cells plus sequential facet/edge records; entity record
/// IDs and short or unrelated rows are ignored. Validation of facet identity,
/// positive tags, and topology belongs to the caller's declared policy.
pub(crate) fn parse_indexed_entity_stream(text: &str) -> Result<IndexedMesh3D, String> {
    parse_v1(text, RecordPolicy::IndexedEntityStream)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MESH: &str = "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,4\nF,0,0,1,2,8\nL,0,0,1,9\nFULLMETA_MESH_DATA_END";
    #[test]
    fn entity_stream_uses_connectivity_not_face_or_edge_record_ids() {
        let text = "V,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,4\nF,ignored,0,1,2,8\nF,ignored,0,1,3,9\nL,ignored,0,1,10\nL,ignored,1,2,11\nT,short,row\n";
        let mesh = parse_indexed_entity_stream(text).unwrap();
        assert_eq!(mesh.boundary_faces, vec![([0,1,2],8),([0,1,3],9)]);
        assert_eq!(mesh.edge_selections, vec![([0,1],10),([1,2],11)]);
        assert!(parse_indexed_v1(text, DuplicatePolicy::LastWins).is_err());
        assert!(parse_indexed_entity_stream("V,0,0,0,0\nT,0,0,1,2,3,4").is_err());
    }
    #[test]
    fn parses_terminated_tetrahedron_and_ignores_declared_metadata() {
        let mesh = parse_contiguous_v1(MESH).unwrap();
        assert_eq!(mesh.tetrahedra, vec![([0, 1, 2, 3], 4)]);
        assert_eq!(mesh.boundary_faces, vec![([0, 1, 2], 8)]);
        assert_eq!(mesh.nodes.len(), 4);
    }
    #[test]
    fn strict_requires_first_header_ordered_records_and_final_terminator() {
        assert!(parse_contiguous_v1("FULLMETA_MESH/1\nV,0,0,0,0").is_err());
        assert!(parse_contiguous_v1("FULLMETA_MESH/1\nV,1,0,0,0\nFULLMETA_MESH_DATA_END").is_err());
        assert!(parse_contiguous_v1("V,0,0,0,0\nFULLMETA_MESH/1\nFULLMETA_MESH_DATA_END").is_err());
        assert!(parse_contiguous_v1(&format!("{MESH}\nV,4,1,1,1")).is_err());
        assert!(parse_contiguous_v1("FULLMETA_MESH/1\nV,0,0,0,0,extra\nFULLMETA_MESH_DATA_END").is_err());
    }
    #[test]
    fn indexed_allows_out_of_order_extra_fields_and_header_anywhere() {
        let text = "V,2,0,1,0,extra\nV,0,0,0,0\nFULLMETA_MESH/1,extra\nV,3,0,0,1\nV,1,1,0,0\nT,0,0,1,2,3,4\nF,0,0,1,2,8\nL,0,0,1,9\n";
        let mesh = parse_indexed_v1(text, DuplicatePolicy::Reject).unwrap();
        assert_eq!(mesh.nodes.len(), 4);
        assert_eq!(mesh.tetrahedra, vec![([0, 1, 2, 3], 4)]);
        assert_eq!(mesh.boundary_faces, vec![([0, 1, 2], 8)]);
        assert_eq!(mesh.edge_selections, vec![([0, 1], 9)]);
    }
    #[test]
    fn indexed_duplicate_handling_is_explicit() {
        let text = format!("{MESH}\nV,0,5,6,7\nF,0,0,2,3,12\nL,0,0,2,10\nT,0,0,1,2,3,5");
        assert!(parse_indexed_v1(&text, DuplicatePolicy::Reject).is_err());
        let mesh = parse_indexed_v1(&text, DuplicatePolicy::LastWins).unwrap();
        assert_eq!(mesh.nodes[0], [5., 6., 7.]);
        assert_eq!(mesh.tetrahedra[0].1, 5);
        assert_eq!(mesh.boundary_faces[0].1, 12);
        assert_eq!(mesh.edge_selections[0].1, 10);
    }
    #[test]
    fn indexed_rejects_missing_ids_invalid_references_and_malformed_records() {
        for text in [
            "FULLMETA_MESH/1\nV,0,0,0,0\nT,0,0,0,0,0,1\nF,1,0,0,0,1",
            "FULLMETA_MESH/1\nV,0,0,0,0\nT,0,0,0,0,0,1\nL,1,0,0,1",
            "FULLMETA_MESH/1\nV,0,0,0,0\nT,0,0,0,0,1,1",
            "FULLMETA_MESH/1\nV,0,0,0,0\nT,0,0,0",
            "V,0,0,0,0\nT,0,0,0,0,0,1",
        ] { assert!(parse_indexed_v1(text, DuplicatePolicy::LastWins).is_err(), "{text}"); }
        let huge_id = "FULLMETA_MESH/1\nV,18446744073709551615,0,0,0\nT,0,0,0,0,0,1";
        assert!(parse_indexed_v1(huge_id, DuplicatePolicy::LastWins).is_err());
    }
}
