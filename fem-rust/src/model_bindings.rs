//! Generic mappings from declared model selections to mesh physical tags.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn selection_tags(
    map: &Value,
    property: &str,
) -> Result<HashMap<String, usize>, String> {
    selection_tags_with_policy(map, property, true)
}

/// Some prepared input formats allow different selection IDs to refer to the
/// same positive tag. ID duplication remains invalid under either policy.
pub(crate) fn selection_tags_with_policy(
    map: &Value,
    property: &str,
    reject_duplicate_tags: bool,
) -> Result<HashMap<String, usize>, String> {
    selection_tags_with_keys(map, property, reject_duplicate_tags, None)
}

/// Restrict the selection ID field when a prepared request contract requires
/// an exact map representation. `None` retains the established fallback.
pub(crate) fn selection_tags_with_keys(
    map: &Value,
    property: &str,
    reject_duplicate_tags: bool,
    required_key: Option<&str>,
) -> Result<HashMap<String, usize>, String> {
    let rows = map
        .get(property)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("mesh map.{property} must be an array."))?;
    let mut tags = HashMap::new();
    let mut seen_tags = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        let path = format!("mesh map.{property}[{index}]");
        let id = required_key.map_or_else(
            || row.get("selectionId").or_else(|| row.get("id")),
            |key| row.get(key),
        )
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| format!("{path} needs a selection ID."))?;
        let tag = row
            .get("physicalTag")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&tag| tag > 0)
            .ok_or_else(|| format!("{path}.physicalTag must be a positive integer."))?;
        if tags.insert(id.to_string(), tag).is_some() || !seen_tags.insert(tag) && reject_duplicate_tags {
            return Err(format!("{path} duplicates a selection ID or physical tag."));
        }
    }
    Ok(tags)
}

/// Whether explicitly disabled model entries participate in tag binding.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DisabledPolicy {
    Include,
    Skip,
}

/// Duplicate model rows can have different legacy meanings. Keep their
/// treatment explicit instead of embedding a scenario-name dispatch here.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DuplicatePolicy {
    Reject,
    KeepAll,
    LastWins,
}

#[derive(Debug)]
pub(crate) struct BoundSelection<'a> {
    pub(crate) tag: usize,
    pub(crate) selection_id: &'a str,
    pub(crate) row: &'a Value,
}

/// Resolve selections to mesh cell indices. Unlike physical tags, cells may
/// belong to several selections and a selection may contain many cells.
/// The caller supplies membership for the current mesh; row order is retained
/// within each cell so additive features are evaluated deterministically.
pub(crate) fn bind_cell_selections<'a>(
    rows: impl IntoIterator<Item = &'a Value>,
    membership: &HashMap<String, Vec<usize>>,
    cell_count: usize,
    description: &str,
    disabled: DisabledPolicy,
) -> Result<Vec<Vec<&'a Value>>, String> {
    let mut cells = vec![Vec::new(); cell_count];
    for (index, row) in rows.into_iter().enumerate() {
        if matches!(disabled, DisabledPolicy::Skip)
            && row.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let id = row.get("selectionId").and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| format!("{description} row {index} needs selectionId."))?;
        let selected = membership.get(id).ok_or_else(||
            format!("{description} selection {id:?} is absent from the mesh selection map."))?;
        if selected.is_empty() {
            return Err(format!("{description} selection {id:?} contains no mesh cells."));
        }
        let mut seen = HashSet::new();
        for &cell in selected {
            if cell >= cell_count {
                return Err(format!("{description} selection {id:?} references missing mesh cell {cell}."));
            }
            if !seen.insert(cell) {
                return Err(format!("{description} selection {id:?} repeats mesh cell {cell}."));
            }
            cells[cell].push(row);
        }
    }
    Ok(cells)
}

/// Resolve model rows through the already validated mesh selection map.
/// `description` appears only in diagnostics. Input order is preserved;
/// LastWins replaces the earlier entry at its existing position.
pub(crate) fn bind_selections<'a>(
    rows: impl IntoIterator<Item = &'a Value>,
    tags: &HashMap<String, usize>,
    description: &str,
    disabled: DisabledPolicy,
    duplicates: DuplicatePolicy,
) -> Result<Vec<BoundSelection<'a>>, String> {
    let mut output: Vec<BoundSelection<'a>> = Vec::new();
    let mut locations = HashMap::<usize, usize>::new();
    for (index, row) in rows.into_iter().enumerate() {
        if matches!(disabled, DisabledPolicy::Skip)
            && row.get("enabled").and_then(Value::as_bool) == Some(false)
        {
            continue;
        }
        let selection_id = row.get("selectionId").and_then(Value::as_str)
            .ok_or_else(|| format!("{description} row {index} needs selectionId."))?;
        let tag = *tags.get(selection_id).ok_or_else(||
            format!("{description} selection {selection_id:?} is absent from the mesh map."))?;
        let value = BoundSelection { tag, selection_id, row };
        if let Some(&location) = locations.get(&tag) {
            match duplicates {
                DuplicatePolicy::Reject => return Err(format!("More than one {description} assignment targets physical tag {tag}.")),
                DuplicatePolicy::LastWins => output[location] = value,
                DuplicatePolicy::KeepAll => output.push(value),
            }
        } else {
            locations.insert(tag, output.len());
            output.push(value);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn binds_overlapping_domain_selections_independently_of_material_tags() {
        let rows = json!([
            {"id":"first","selectionId":"whole"},
            {"id":"second","selectionId":"part"},
            {"id":"disabled","selectionId":"missing","enabled":false}
        ]);
        let membership = HashMap::from([("whole".into(), vec![0, 1, 2]), ("part".into(), vec![1, 2])]);
        let cells = bind_cell_selections(rows.as_array().unwrap(), &membership, 3, "Source", DisabledPolicy::Skip).unwrap();
        let ids = cells.iter().map(|cell| cell.iter().map(|row| row["id"].as_str().unwrap()).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(ids, vec![vec!["first"], vec!["first", "second"], vec!["first", "second"]]);
        assert!(bind_cell_selections(rows.as_array().unwrap(), &membership, 3, "Source", DisabledPolicy::Include).is_err());
        let invalid = HashMap::from([("whole".into(), vec![0, 3])]);
        assert!(bind_cell_selections(&rows.as_array().unwrap()[..1], &invalid, 3, "Source", DisabledPolicy::Skip).is_err());
    }
    #[test]
    fn maps_selection_ids_and_rejects_duplicate_tags() {
        let map = json!({"physicalGroups":[{"selectionId":"a","physicalTag":2},{"id":"b","physicalTag":3}]});
        assert_eq!(selection_tags(&map, "physicalGroups").unwrap()["b"], 3);
        let duplicate =
            json!({"physicalGroups":[{"id":"a","physicalTag":2},{"id":"b","physicalTag":2}]});
        assert!(selection_tags(&duplicate, "physicalGroups").is_err());
    }

    #[test]
    fn resolves_model_rows_with_explicit_disabled_and_duplicate_policies() {
        let tags = HashMap::from([("wall".to_string(), 7), ("other".to_string(), 8)]);
        let entries = json!([
            {"selectionId":"wall", "value":1},
            {"selectionId":"missing", "enabled":false},
            {"selectionId":"wall", "value":2},
            {"selectionId":"other", "value":3}
        ]);
        let rows = entries.as_array().unwrap();
        assert!(bind_selections(rows, &tags, "Boundary", DisabledPolicy::Include, DuplicatePolicy::KeepAll).is_err());
        assert!(bind_selections(rows, &tags, "Boundary", DisabledPolicy::Skip, DuplicatePolicy::Reject).is_err());
        let all = bind_selections(rows, &tags, "Boundary", DisabledPolicy::Skip, DuplicatePolicy::KeepAll).unwrap();
        assert_eq!(all.iter().map(|entry| entry.tag).collect::<Vec<_>>(), [7,7,8]);
        let last = bind_selections(rows, &tags, "Boundary", DisabledPolicy::Skip, DuplicatePolicy::LastWins).unwrap();
        assert_eq!(last.len(), 2);
        assert_eq!(last[0].selection_id, "wall");
        assert_eq!(last[0].row["value"], 2);
        assert_eq!(last[1].tag, 8);
    }

    #[test]
    fn diagnoses_unknown_or_missing_selection_before_binding() {
        let tags = HashMap::from([("known".to_string(), 1)]);
        let missing = json!([{}]);
        assert!(bind_selections(missing.as_array().unwrap(), &tags, "Material", DisabledPolicy::Include, DuplicatePolicy::Reject).unwrap_err().contains("selectionId"));
        let unknown = json!([{"selectionId":"unknown"}]);
        assert!(bind_selections(unknown.as_array().unwrap(), &tags, "Material", DisabledPolicy::Include, DuplicatePolicy::Reject).unwrap_err().contains("absent from the mesh map"));
    }
}
