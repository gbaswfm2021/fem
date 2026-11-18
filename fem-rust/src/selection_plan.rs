//! Declarative selection-to-physical-tag binding for prepared FEM inputs.
//!
//! Collection shape, disabled/duplicate policies and coverage are chosen by
//! the caller's contract. Geometry/owner checks remain in simplex_topology.
use std::collections::{BTreeSet, HashMap, HashSet};
use serde_json::Value;
use crate::model_bindings::{self, BoundSelection, DisabledPolicy, DuplicatePolicy};

enum Collection { Array, Identified { types: HashSet<String> } }
enum Missing { Error, Empty }
enum DeclaredTag { Ignore, IfPresent, Required }

struct Plan {
    name: String,
    path: String,
    map_section: String,
    cell_map_section: Option<String>,
    collection: Collection,
    missing: Missing,
    disabled: DisabledPolicy,
    duplicates: DuplicatePolicy,
    declared_tag: DeclaredTag,
    assigned_in_mesh: bool,
    mesh_assigned: bool,
    all_map_assigned: bool,
    all_map_selections_assigned: bool,
    positive_mesh_tags: bool,
    reject_duplicate_map_tags: bool,
    map_id_key: Option<String>,
    unique_id_group: Option<String>,
    unique_tag_group: Option<String>,
}

/// Compiled once per selected scenario contract, independent of profile ID.
pub(crate) struct SelectionPlan { plans: Vec<Plan> }

fn text<'a>(value: &'a Value, name: &str, path: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{path}.{name} must be a nonempty string."))
}

fn valid_pointer(pointer: &str) -> bool {
    if !pointer.starts_with('/') { return false; }
    let mut chars = pointer.chars();
    while let Some(ch) = chars.next() {
        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) { return false; }
        if ch.is_control() { return false; }
    }
    true
}

impl SelectionPlan {
    /// Whether the declared collection is currently bound by cell rather than
    /// by physical tag. The scenario declaration owns the map section name.
    pub(crate) fn uses_cell_map(&self, collection: &str, map: &Value) -> bool {
        self.plans.iter().find(|plan| plan.name == collection)
            .and_then(|plan| plan.cell_map_section.as_ref())
            .is_some_and(|section| map.get(section).is_some())
    }

    /// Input shape is `{ "collections": [{ "name", "path", "mapSection",
    /// "kind", "missing", "disabled", "duplicates", "declaredTag",
    /// "coverage", "positiveMeshTags", ... }] }`. `identifiedEntries`
    /// requires a `types` list, and selects only object entries whose ID equals
    /// their key and whose type is in that list.
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let rows = value.get("collections").and_then(Value::as_array)
            .filter(|rows| !rows.is_empty()).ok_or("Selection plan requires a nonempty collections array.")?;
        let mut names = HashSet::new();
        let mut plans = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let label = format!("selectionPlan.collections[{i}]");
            let object = row.as_object().ok_or_else(|| format!("{label} must be an object."))?;
            for key in object.keys() {
                if !["name", "path", "mapSection", "cellMapSection", "kind", "types", "missing", "disabled", "duplicates", "declaredTag", "coverage", "positiveMeshTags", "mapTagDuplicates", "mapIdKey", "uniqueIdGroup", "uniqueTagGroup"].contains(&key.as_str()) {
                    return Err(format!("{label} has unsupported field {key:?}."));
                }
            }
            let name = text(row, "name", &label)?.to_string();
            if !names.insert(name.clone()) { return Err(format!("{label} duplicates collection {name:?}.")); }
            let path = text(row, "path", &label)?.to_string();
            if !valid_pointer(&path) { return Err(format!("{label}.path must be a valid JSON pointer.")); }
            let map_section = text(row, "mapSection", &label)?.to_string();
            let cell_map_section = row.get("cellMapSection").map(|v| v.as_str().filter(|s| !s.is_empty() && !s.contains('/')).map(str::to_owned)
                .ok_or_else(|| format!("{label}.cellMapSection must name a top-level map object."))).transpose()?;
            if map_section.contains('/') { return Err(format!("{label}.mapSection must name a top-level map array.")); }
            let collection = match text(row, "kind", &label)? {
                "array" => {
                    if row.get("types").is_some() { return Err(format!("{label}.types is only valid for identifiedEntries.")); }
                    Collection::Array
                }
                "identifiedEntries" => {
                    let types = row.get("types").and_then(Value::as_array).filter(|v| !v.is_empty()).ok_or_else(|| format!("{label}.types must be a nonempty array."))?;
                    let mut set = HashSet::new();
                    for ty in types {
                        let ty = ty.as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("{label}.types must contain nonempty strings."))?;
                        if !set.insert(ty.to_string()) { return Err(format!("{label}.types contains duplicate {ty:?}.")); }
                    }
                    Collection::Identified { types: set }
                }
                other => return Err(format!("{label}.kind {other:?} is unsupported.")),
            };
            let missing = match text(row, "missing", &label)? {
                "error" => Missing::Error, "empty" => Missing::Empty,
                other => return Err(format!("{label}.missing {other:?} is unsupported.")),
            };
            let disabled = match text(row, "disabled", &label)? {
                "include" => DisabledPolicy::Include, "skip" => DisabledPolicy::Skip,
                other => return Err(format!("{label}.disabled {other:?} is unsupported.")),
            };
            let duplicates = match text(row, "duplicates", &label)? {
                "reject" => DuplicatePolicy::Reject, "keepAll" => DuplicatePolicy::KeepAll, "lastWins" => DuplicatePolicy::LastWins,
                other => return Err(format!("{label}.duplicates {other:?} is unsupported.")),
            };
            let declared_tag = match text(row, "declaredTag", &label)? {
                "ignore" => DeclaredTag::Ignore, "ifPresentMatch" => DeclaredTag::IfPresent, "requireMatch" => DeclaredTag::Required,
                other => return Err(format!("{label}.declaredTag {other:?} is unsupported.")),
            };
            let mut coverage = HashSet::new();
            for rule in row.get("coverage").and_then(Value::as_array).ok_or_else(|| format!("{label}.coverage must be an array."))? {
                let rule = rule.as_str().ok_or_else(|| format!("{label}.coverage entries must be strings."))?;
                if !matches!(rule, "assignedInMesh" | "meshAssigned" | "allMapAssigned" | "allMapSelectionsAssigned") || !coverage.insert(rule) {
                    return Err(format!("{label}.coverage has an unsupported or repeated rule {rule:?}."));
                }
            }
            let positive_mesh_tags = row.get("positiveMeshTags").and_then(Value::as_bool)
                .ok_or_else(|| format!("{label}.positiveMeshTags must be boolean."))?;
            let reject_duplicate_map_tags = match row.get("mapTagDuplicates") {
                None => true,
                Some(value) => match value.as_str() {
                    Some("reject") => true, Some("allow") => false,
                    _ => return Err(format!("{label}.mapTagDuplicates must be 'reject' or 'allow'.")),
                }
            };
            let map_id_key = match row.get("mapIdKey") {
                None => None,
                Some(value) => match value.as_str() {
                    Some("id") => Some("id".to_string()), Some("selectionId") => Some("selectionId".to_string()),
                    Some("either") => None,
                    _ => return Err(format!("{label}.mapIdKey must be 'id', 'selectionId', or 'either'.")),
                }
            };
            let group = |key: &str| -> Result<Option<String>, String> {
                row.get(key).map(|value| value.as_str().filter(|v| !v.is_empty()).map(str::to_string)
                    .ok_or_else(|| format!("{label}.{key} must be a nonempty string."))).transpose()
            };
            let unique_id_group = group("uniqueIdGroup")?;
            let unique_tag_group = group("uniqueTagGroup")?;
            plans.push(Plan { name, path, map_section, cell_map_section, collection, missing, disabled, duplicates, declared_tag,
                assigned_in_mesh: coverage.contains("assignedInMesh"), mesh_assigned: coverage.contains("meshAssigned"),
                all_map_assigned: coverage.contains("allMapAssigned"), all_map_selections_assigned: coverage.contains("allMapSelectionsAssigned"),
                positive_mesh_tags, reject_duplicate_map_tags, map_id_key, unique_id_group, unique_tag_group });
        }
        Ok(Self { plans })
    }

    /// Mesh tag sets are keyed by collection name, since cell and facet tag
    /// universes differ. A missing set is an error whenever a mesh rule is
    /// declared; an explicitly supplied empty set remains meaningful.
    pub(crate) fn bind<'a>(&self, model: &'a Value, map: &Value,
        mesh_tags: &HashMap<String, Vec<usize>>) -> Result<HashMap<String, Vec<BoundSelection<'a>>>, String> {
        let mut output = HashMap::new();
        let mut ids_by_group = HashMap::<String, HashSet<String>>::new();
        let mut tags_by_group = HashMap::<String, HashSet<usize>>::new();
        for plan in &self.plans {
            let tags = model_bindings::selection_tags_with_keys(map, &plan.map_section, plan.reject_duplicate_map_tags, plan.map_id_key.as_deref())?;
            let rows: Vec<&Value> = match model.pointer(&plan.path) {
                None if matches!(plan.missing, Missing::Empty) => Vec::new(),
                None => return Err(format!("{} collection {} is missing.", plan.name, plan.path)),
                Some(value) => match &plan.collection {
                    Collection::Array => value.as_array().ok_or_else(|| format!("{} collection must be an array.", plan.name))?.iter().collect(),
                    Collection::Identified { types } => value.as_object().ok_or_else(|| format!("{} collection must be an object.", plan.name))?.iter().filter_map(|(key, value)| {
                        (value.get("id").and_then(Value::as_str) == Some(key.as_str())
                            && value.get("type").and_then(Value::as_str).is_some_and(|ty| types.contains(ty))).then_some(value)
                    }).collect(),
                }
            };
            if let Some(section) = plan.cell_map_section.as_ref().and_then(|key| map.get(key)) {
                if !matches!(plan.duplicates, DuplicatePolicy::KeepAll) || !matches!(plan.declared_tag, DeclaredTag::Ignore) {
                    return Err(format!("{} cell mapping requires keepAll and ignore declaredTag.", plan.name));
                }
                let raw = section.as_object().ok_or_else(|| format!("{} cell map must be an object.", plan.name))?;
                let mut membership = HashMap::new();
                for (id, cells) in raw {
                    let cells = cells.as_array().ok_or_else(|| format!("{} selection {id:?} cells must be an array.", plan.name))?
                        .iter().map(|cell| cell.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| format!("{} selection {id:?} has an invalid cell index.", plan.name)))
                        .collect::<Result<Vec<_>, _>>()?;
                    membership.insert(id.clone(), cells);
                }
                let count = mesh_tags.get(&plan.name).ok_or_else(|| format!("{} mesh cells were not supplied.", plan.name))?.len();
                let cells = model_bindings::bind_cell_selections(rows, &membership, count, &plan.name, plan.disabled)?;
                let bound = cells.into_iter().enumerate().flat_map(|(tag, rows)| rows.into_iter().map(move |row| BoundSelection {
                    tag, selection_id: row["selectionId"].as_str().expect("validated selection ID"), row
                })).collect();
                output.insert(plan.name.clone(), bound);
                continue;
            }
            // Validate every enabled source row before duplicate policies can
            // discard it, so malformed assignments are never silently hidden.
            for row in &rows {
                if matches!(plan.disabled, DisabledPolicy::Skip) && row.get("enabled").and_then(Value::as_bool) == Some(false) { continue; }
                if matches!(plan.declared_tag, DeclaredTag::Ignore) { continue; }
                let declared = row.get("physicalTag");
                if declared.is_none() && matches!(plan.declared_tag, DeclaredTag::IfPresent) { continue; }
                let id = row.get("selectionId").and_then(Value::as_str).ok_or_else(|| format!("{} row needs selectionId.", plan.name))?;
                let expected = tags.get(id).ok_or_else(|| format!("{} selection {id:?} is absent from the mesh map.", plan.name))?;
                if declared.and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()) != Some(*expected) {
                    return Err(format!("{} selection {id:?} physicalTag does not match its mesh map.", plan.name));
                }
            }
            let bound = model_bindings::bind_selections(rows, &tags, &plan.name, plan.disabled, plan.duplicates)?;
            for row in &bound {
                if let Some(group) = &plan.unique_id_group {
                    let id = row.row.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())
                        .ok_or_else(|| format!("{} row needs a nonempty id for uniqueness group {group:?}.", plan.name))?;
                    if !ids_by_group.entry(group.clone()).or_default().insert(id.to_owned()) {
                        return Err(format!("{} duplicates id {id:?} in group {group:?}.", plan.name));
                    }
                }
                if let Some(group) = &plan.unique_tag_group {
                    if !tags_by_group.entry(group.clone()).or_default().insert(row.tag) {
                        return Err(format!("{} duplicates tag {} in group {group:?}.", plan.name, row.tag));
                    }
                }
            }
            let assigned = bound.iter().map(|row| row.tag).collect::<BTreeSet<_>>();
            if plan.all_map_selections_assigned {
                let selections = bound.iter().map(|row| row.selection_id).collect::<HashSet<_>>();
                let mut missing = tags.keys().filter(|id| !selections.contains(id.as_str())).collect::<Vec<_>>();
                missing.sort_unstable();
                if let Some(id) = missing.first() { return Err(format!("{} map selection {id:?} has no assignment.", plan.name)); }
            }
            if plan.all_map_assigned {
                for tag in tags.values().copied().collect::<BTreeSet<_>>() {
                    if !assigned.contains(&tag) { return Err(format!("{} map tag {tag} has no assignment.", plan.name)); }
                }
            }
            if plan.assigned_in_mesh || plan.mesh_assigned || plan.positive_mesh_tags {
                let mesh = mesh_tags.get(&plan.name).ok_or_else(|| format!("{} mesh tags were not supplied.", plan.name))?.iter().copied().collect::<BTreeSet<_>>();
                if plan.positive_mesh_tags && mesh.contains(&0) { return Err(format!("{} mesh tags must be positive.", plan.name)); }
                if plan.assigned_in_mesh {
                    if let Some(tag) = assigned.difference(&mesh).next() { return Err(format!("{} assigned tag {tag} is absent from the mesh.", plan.name)); }
                }
                if plan.mesh_assigned {
                    if let Some(tag) = mesh.difference(&assigned).next() { return Err(format!("{} mesh tag {tag} has no assignment.", plan.name)); }
                }
            }
            output.insert(plan.name.clone(), bound);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn contract() -> Value { json!({"collections":[
        {"name":"materials","path":"/model/materials","mapSection":"physicalGroups","kind":"array",
         "missing":"error","disabled":"include","duplicates":"reject","declaredTag":"ifPresentMatch",
         "coverage":["meshAssigned"],"positiveMeshTags":true},
        {"name":"boundaries","path":"/model/physics","mapSection":"boundarySelections","kind":"identifiedEntries",
         "types":["dirichlet","neumann"],"missing":"empty","disabled":"skip","duplicates":"reject",
         "declaredTag":"ignore","coverage":["assignedInMesh"],"positiveMeshTags":true}
    ]}) }
    fn model() -> Value { json!({"model":{
        "materials":[{"selectionId":"domain","physicalTag":7}],
        "physics":{"fixed":{"id":"fixed","type":"dirichlet","selectionId":"wall"},
            "disabled":{"id":"disabled","type":"neumann","selectionId":"unknown","enabled":false},
            "settings":{"type":"dirichlet","selectionId":"unknown"},
            "periodic":{"id":"periodic","type":"periodic","selectionId":"unknown"}}
    }}) }
    fn map() -> Value { json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
        "boundarySelections":[{"id":"wall","physicalTag":3}]}) }
    fn mesh() -> HashMap<String, Vec<usize>> { HashMap::from([("materials".into(), vec![7]), ("boundaries".into(), vec![3,4]), ("domainSources".into(), vec![7])]) }

    #[test]
    fn cell_lookup_follows_declared_section_not_a_fixed_name() {
        let mut value = contract();
        value["collections"][1]["cellMapSection"] = json!("customCells");
        let plan = SelectionPlan::from_json(&value).unwrap();
        assert!(!plan.uses_cell_map("boundaries", &json!({"domainCellSelections":{}})));
        assert!(plan.uses_cell_map("boundaries", &json!({"customCells":{}})));
        assert!(!plan.uses_cell_map("materials", &json!({"customCells":{}})));
    }

    #[test]
    fn binds_arrays_and_keyed_physics_with_distinct_coverage() {
        let plan = SelectionPlan::from_json(&contract()).unwrap();
        let model = model();
        let bound = plan.bind(&model, &map(), &mesh()).unwrap();
        assert_eq!(bound["materials"].len(), 1);
        assert_eq!(bound["materials"][0].tag, 7);
        assert_eq!(bound["boundaries"].len(), 1);
        assert_eq!(bound["boundaries"][0].selection_id, "wall");
    }

    #[test]
    fn validates_duplicates_declared_tags_and_independent_coverage() {
        let plan = SelectionPlan::from_json(&contract()).unwrap();
        let mut value = model();
        value["model"]["materials"][0]["physicalTag"] = json!(8);
        assert!(plan.bind(&value, &map(), &mesh()).unwrap_err().contains("does not match"));
        let mut value = model();
        value["model"]["materials"].as_array_mut().unwrap().push(json!({"selectionId":"domain"}));
        assert!(plan.bind(&value, &map(), &mesh()).unwrap_err().contains("More than one"));
        let mut tags = mesh(); tags.insert("materials".into(), vec![7,8]);
        assert!(plan.bind(&model(), &map(), &tags).unwrap_err().contains("mesh tag 8"));
        let mut tags = mesh(); tags.insert("boundaries".into(), vec![4]);
        assert!(plan.bind(&model(), &map(), &tags).unwrap_err().contains("assigned tag 3"));
        let mut tags = mesh(); tags.remove("boundaries");
        assert!(plan.bind(&model(), &map(), &tags).unwrap_err().contains("not supplied"));
        let mut tags = mesh(); tags.insert("materials".into(), vec![0,7]);
        assert!(plan.bind(&model(), &map(), &tags).unwrap_err().contains("positive"));
    }

    #[test]
    fn missing_collections_map_errors_and_all_map_coverage_are_checked() {
        let mut c = contract(); c["collections"][0]["coverage"] = json!(["allMapAssigned"]);
        let plan = SelectionPlan::from_json(&c).unwrap();
        let mut groups = map(); groups["physicalGroups"].as_array_mut().unwrap().push(json!({"selectionId":"other","physicalTag":8}));
        assert!(plan.bind(&model(), &groups, &mesh()).unwrap_err().contains("map tag 8"));
        let mut value = model(); value["model"].as_object_mut().unwrap().remove("materials");
        assert!(plan.bind(&value, &map(), &mesh()).unwrap_err().contains("missing"));
        let mut duplicate_map = map(); duplicate_map["physicalGroups"].as_array_mut().unwrap().push(json!({"selectionId":"other","physicalTag":7}));
        assert!(plan.bind(&model(), &duplicate_map, &mesh()).unwrap_err().contains("duplicates"));
    }

    #[test]
    fn rejects_invalid_declarations_and_supports_last_wins() {
        let mut invalid = contract(); invalid["collections"][0]["coverage"] = json!(["mystery"]);
        assert!(SelectionPlan::from_json(&invalid).is_err());
        let mut invalid = contract(); invalid["collections"][0]["path"] = json!("/bad~2path");
        assert!(SelectionPlan::from_json(&invalid).is_err());
        let mut invalid = contract(); invalid["collections"][0]["unrecognised"] = json!(true);
        assert!(SelectionPlan::from_json(&invalid).is_err());
        let mut c = contract(); c["collections"][0]["duplicates"] = json!("lastWins");
        let mut value = model(); value["model"]["materials"].as_array_mut().unwrap().push(json!({"selectionId":"domain","v":2}));
        let plan = SelectionPlan::from_json(&c).unwrap();
        let bound = plan.bind(&value, &map(), &mesh()).unwrap();
        assert_eq!(bound["materials"][0].row["v"], 2);
    }

    #[test]
    fn map_id_policy_and_selection_coverage_distinguish_aliases() {
        let mut c = contract();
        c["collections"][0]["mapTagDuplicates"] = json!("allow");
        c["collections"][0]["mapIdKey"] = json!("selectionId");
        c["collections"][0]["coverage"] = json!(["allMapSelectionsAssigned"]);
        let plan = SelectionPlan::from_json(&c).unwrap();
        let mut map = map();
        map["physicalGroups"].as_array_mut().unwrap().push(json!({"selectionId":"other","physicalTag":7}));
        assert!(plan.bind(&model(), &map, &mesh()).unwrap_err().contains("map selection"));
        map["physicalGroups"][1] = json!({"id":"other","physicalTag":7});
        assert!(plan.bind(&model(), &map, &mesh()).unwrap_err().contains("selection ID"));
    }

    #[test]
    fn uniqueness_groups_check_ids_and_tags_across_collections() {
        let entry = |name, path| json!({"name":name,"path":path,"mapSection":"boundarySelections","kind":"array",
            "missing":"error","disabled":"include","duplicates":"reject","declaredTag":"ignore",
            "coverage":[],"positiveMeshTags":false,"mapTagDuplicates":"allow","mapIdKey":"id",
            "uniqueIdGroup":"boundary","uniqueTagGroup":"boundary"});
        let plan = SelectionPlan::from_json(&json!({"collections":[entry("a","/a"),entry("b","/b")]})).unwrap();
        let map = json!({"boundarySelections":[{"id":"wall","physicalTag":2},{"id":"other","physicalTag":2},{"id":"third","physicalTag":3}]});
        let duplicate_tag = json!({"a":[{"id":"one","selectionId":"wall"}],"b":[{"id":"two","selectionId":"other"}]});
        assert!(plan.bind(&duplicate_tag, &map, &HashMap::new()).unwrap_err().contains("duplicates tag"));
        let duplicate_id = json!({"a":[{"id":"same","selectionId":"wall"}],"b":[{"id":"same","selectionId":"third"}]});
        assert!(plan.bind(&duplicate_id, &map, &HashMap::new()).unwrap_err().contains("duplicates id"));
        let valid = json!({"a":[{"id":"one","selectionId":"wall"}],"b":[{"id":"two","selectionId":"third"}]});
        assert!(plan.bind(&valid, &map, &HashMap::new()).is_ok());
    }

    

    
}
