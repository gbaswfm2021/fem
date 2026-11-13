use crate::expression::Scalar;
use serde_json::Value;
use std::collections::HashMap;

/// Collect addressable geometry in preorder while retaining the complete
/// primitive for each group. Nested feature IDs travel with their geometry.
pub fn nested_address_primitives(roots: &[Value]) -> Result<Vec<Value>, String> {
    fn collect(
        value: &Value,
        output: &mut Vec<Value>,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Addressable geometry is missing its ID.")?;
        if !seen.insert(id.to_string()) {
            return Err(format!(
                "Geometry feature id {id:?} is used more than once."
            ));
        }
        output.push(value.clone());
        if let Some(children) = value.get("features").and_then(Value::as_array) {
            for child in children {
                collect(child, output, seen)?;
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        collect(root, &mut output, &mut seen)?;
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug)]
pub struct PlaneFilter {
    pub point: [f64; 3],
    pub normal: [f64; 3],
}

/// Evaluate the zero expression at a point supplied by the CAD entity.
/// Address membership is resolved separately before this predicate is used.
pub fn evaluate_coordinate_expression(
    expression: &str,
    symbols: &HashMap<String, Scalar>,
    dimension: usize,
    point: [f64; 3],
    path: &str,
) -> Result<f64, String> {
    let mut values = symbols
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                Scalar {
                    value: value.value,
                    dimension: [0.0, 0.0, 0.0, 0.0, value.dimension[4]],
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for axis in 0..3 {
        values.insert(
            ["x", "y", "z"][axis].into(),
            Scalar {
                value: if dimension == 2 && axis == 2 {
                    0.0
                } else {
                    point[axis]
                },
                dimension: fullmeta_core::quantity::ZERO_DIMENSION,
            },
        );
    }
    let result = crate::expression::evaluate_expression(expression, &values, path)?;
    if result.dimension[4].abs() > 1e-10 {
        return Err(format!(
            "{path}: a geometric coordinate expression must not have temperature units."
        ));
    }
    Ok(result.value)
}

pub fn coordinate_expression(selection: &Value, path: &str) -> Result<Option<String>, String> {
    let Some(value) = selection.get("coordinateExpression") else {
        return Ok(None);
    };
    let expression = value
        .as_str()
        .ok_or_else(|| format!("{path}.coordinateExpression must be a string."))?
        .trim();
    if expression.is_empty() {
        return Err(format!(
            "{path}.coordinateExpression must not be empty; remove the field to clear the filter."
        ));
    }
    Ok(Some(expression.to_string()))
}

pub fn plane_filter(
    selection: &Value,
    symbols: &HashMap<String, Scalar>,
    dimension: usize,
    path: &str,
) -> Result<Option<PlaneFilter>, String> {
    let Some(expression) = coordinate_expression(selection, path)? else {
        return Ok(None);
    };
    coordinate_plane(
        &expression,
        symbols,
        dimension,
        &format!("{path}.coordinateExpression"),
    )
}

/// Keep the established exact plane path, deferring other valid zero sets until
/// the address-selected CAD entities are available from Gmsh.
pub fn selection_filter(
    selection: &Value,
    symbols: &HashMap<String, Scalar>,
    dimension: usize,
    path: &str,
) -> Result<(Option<PlaneFilter>, Option<Value>), String> {
    // Sampling alone cannot prove an expression affine: for example x*y*z
    // vanishes on every coordinate axis. Classify the expression structurally.
    fn degree(node: &fullmeta_core::expression::AstNode, dimension: usize) -> usize {
        use fullmeta_core::expression::AstNode;
        match node {
            AstNode::Number { .. } => 0,
            AstNode::Symbol { name } => {
                usize::from(name == "x" || name == "y" || (name == "z" && dimension == 3))
            }
            AstNode::Unary { value, .. } => degree(value, dimension),
            AstNode::Binary { op, left, right } => {
                let a = degree(left, dimension);
                let b = degree(right, dimension);
                match op.as_str() {
                    "+" | "-" => a.max(b),
                    "*" => a.saturating_add(b).min(2),
                    "/" if b == 0 => a,
                    _ if a == 0 && b == 0 => 0,
                    _ => 2,
                }
            }
            AstNode::Call { args, .. } => {
                if args.iter().all(|arg| degree(arg, dimension) == 0) {
                    0
                } else {
                    2
                }
            }
        }
    }
    let Some(expression) = coordinate_expression(selection, path)? else {
        return Ok((None, None));
    };
    let expression_path = format!("{path}.coordinateExpression");
    let inspected = fullmeta_core::expression::inspect_expression(&expression)
        .map_err(|error| format!("{expression_path}: {error}"))?;
    crate::expression::validate_expression(&expression, &expression_path)?;
    for name in &inspected.symbols {
        if !matches!(name.as_str(), "x" | "y" | "z" | "pi" | "e") && !symbols.contains_key(name) {
            return Err(format!("{expression_path}: unknown symbol {name:?}."));
        }
    }
    if degree(&inspected.tree, dimension) > 1 {
        let values: std::collections::BTreeMap<_, _> = symbols
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    serde_json::json!({"value":value.value,"dimension":value.dimension}),
                )
            })
            .collect();
        return Ok((
            None,
            Some(
                serde_json::json!({"expression":expression,"symbols":values,"path":expression_path}),
            ),
        ));
    }
    match plane_filter(selection, symbols, dimension, path) {
        Ok(plane) => Ok((plane, None)),
        Err(error) if error.contains("the zero set must be an affine plane") => {
            let expression =
                coordinate_expression(selection, path)?.ok_or("Missing coordinate expression.")?;
            let values: std::collections::BTreeMap<_, _> = symbols
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        serde_json::json!({"value":value.value,"dimension":value.dimension}),
                    )
                })
                .collect();
            Ok((
                None,
                Some(
                    serde_json::json!({"expression":expression,"symbols":values,"path":format!("{path}.coordinateExpression")}),
                ),
            ))
        }
        Err(error) => Err(error),
    }
}

pub fn coordinate_plane(
    expression: &str,
    symbols: &HashMap<String, Scalar>,
    dimension: usize,
    path: &str,
) -> Result<Option<PlaneFilter>, String> {
    let evaluate_at = |point: [f64; 3]| {
        evaluate_coordinate_expression(expression, symbols, dimension, point, path)
    };
    let origin = evaluate_at([0.0; 3])?;
    let mut gradient = [0.0; 3];
    for axis in 0..dimension {
        let mut point = [0.0; 3];
        point[axis] = 1.0;
        gradient[axis] = evaluate_at(point)? - origin;
    }
    let gradient_length = gradient
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if !gradient_length.is_finite() || gradient_length <= 1e-14 {
        if [0.0, 0.23, -0.31, 0.72]
            .into_iter()
            .all(|x| evaluate_at([x, 0.0, 0.0]).is_ok_and(|value| value.abs() <= 1e-12))
            && [0.0, 0.47, -0.29, 0.81]
                .into_iter()
                .all(|y| evaluate_at([0.0, y, 0.0]).is_ok_and(|value| value.abs() <= 1e-12))
            && [0.0, 0.54, -0.18, 1.13]
                .into_iter()
                .all(|z| evaluate_at([0.0, 0.0, z]).is_ok_and(|value| value.abs() <= 1e-12))
            && [[0.23, 0.47, 0.0], [-0.31, 0.72, 0.0], [1.13, -0.29, 0.0]]
                .into_iter()
                .all(|point| evaluate_at(point).is_ok_and(|value| value.abs() <= 1e-12))
        {
            return Ok(None);
        }
        return Err(format!(
            "{path}: the zero set must be an affine plane for exact CAD selection; enter 0 to keep all entities matching the membership address."
        ));
    }
    let normal = gradient.map(|value| value / gradient_length);
    let point = gradient.map(|value| -origin * value / (gradient_length * gradient_length));
    if !point.iter().all(|value| value.is_finite()) {
        return Err(format!(
            "{path}: the zero plane has non-finite coordinates."
        ));
    }
    for point in [[0.23, 0.47, 0.81], [-0.31, 0.72, 0.18], [1.13, -0.29, 0.54]] {
        let actual = evaluate_at(point)?;
        let expected = origin
            + gradient
                .iter()
                .zip(point)
                .map(|(coefficient, coordinate)| coefficient * coordinate)
                .sum::<f64>();
        if (actual - expected).abs() > 1e-10 * actual.abs().max(expected.abs()).max(1e-12) {
            return Err(format!(
                "{path}: the zero set must be an affine plane for exact CAD selection; enter 0 to keep all entities matching the membership address."
            ));
        }
    }
    Ok(Some(PlaneFilter { point, normal }))
}

pub fn selection_addresses(
    selection: &Value,
    feature_ids: &[String],
    path: &str,
) -> Result<Vec<String>, String> {
    let addresses = selection
        .get("addresses")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{path}.addresses must be an array."))?;
    let known_ids = feature_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    addresses
        .iter()
        .enumerate()
        .map(|(index, address)| {
            let address_path = format!("{path}.addresses[{index}]");
            if let Some(bits) = address.as_str() {
                if bits.chars().count() != feature_ids.len() {
                    return Err(format!(
                        "{address_path} has {} bits but the geometry has {} domain features.",
                        bits.chars().count(),
                        feature_ids.len()
                    ));
                }
                if !bits.chars().all(|bit| matches!(bit, '0' | '1' | '*')) {
                    return Err(format!(
                        "{address_path} may contain only 0, 1, and * characters."
                    ));
                }
                return Ok(bits.to_string());
            }
            let membership = address
                .get("membership")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    format!(
                        "{address_path}.membership must be an object keyed by geometry feature ID."
                    )
                })?;
            for id in membership.keys() {
                if !known_ids.contains(id.as_str()) {
                    return Err(format!(
                        "{address_path}.membership references unknown geometry feature {id:?}."
                    ));
                }
            }
            feature_ids
                .iter()
                .map(|id| match membership.get(id) {
                    None => Ok('0'),
                    Some(raw_state) => {
                        let state = raw_state.as_str().ok_or_else(|| {
                            format!(
                                "{address_path}.membership.{id} must be a membership-state string."
                            )
                        })?;
                        match state {
                            "outside" | "0" => Ok('0'),
                            "inside" | "1" => Ok('1'),
                            "boundary" | "*" => Ok('*'),
                            other => Err(format!(
                                "{address_path}.membership.{id} has unsupported state {other:?}."
                            )),
                        }
                    }
                })
                .collect::<Result<String, String>>()
        })
        .collect()
}

pub fn contains_primitive(
    point: [f64; 3],
    primitive: &Value,
    tolerance: f64,
    path: &str,
) -> Result<bool, String> {
    match primitive.get("type").and_then(Value::as_str) {
        Some("rectangle") => {
            let min = primitive
                .get("min")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.min is required."))?;
            let size = primitive
                .get("size")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.size is required."))?;
            if min.len() != 2 || size.len() != 2 {
                return Err(format!(
                    "{path} rectangle requires two-dimensional min and size."
                ));
            }
            let x = min[0]
                .as_f64()
                .ok_or_else(|| format!("{path}.min[0] must be numeric."))?;
            let y = min[1]
                .as_f64()
                .ok_or_else(|| format!("{path}.min[1] must be numeric."))?;
            let w = size[0]
                .as_f64()
                .ok_or_else(|| format!("{path}.size[0] must be numeric."))?;
            let h = size[1]
                .as_f64()
                .ok_or_else(|| format!("{path}.size[1] must be numeric."))?;
            Ok(point[0] >= x - tolerance
                && point[0] <= x + w + tolerance
                && point[1] >= y - tolerance
                && point[1] <= y + h + tolerance)
        }
        Some("circle") => {
            let center = primitive
                .get("center")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.center is required."))?;
            if center.len() != 2 {
                return Err(format!("{path}.center must contain two coordinates."));
            }
            let cx = center[0]
                .as_f64()
                .ok_or_else(|| format!("{path}.center[0] must be numeric."))?;
            let cy = center[1]
                .as_f64()
                .ok_or_else(|| format!("{path}.center[1] must be numeric."))?;
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{path}.radius is required."))?;
            Ok((point[0] - cx).powi(2) + (point[1] - cy).powi(2) <= (radius + tolerance).powi(2))
        }
        Some("box") => {
            let min = vector3(
                primitive
                    .get("min")
                    .ok_or_else(|| format!("{path}.min is required."))?,
                &format!("{path}.min"),
            )?;
            let size = vector3(
                primitive
                    .get("size")
                    .ok_or_else(|| format!("{path}.size is required."))?,
                &format!("{path}.size"),
            )?;
            Ok((0..3).all(|axis| {
                point[axis] >= min[axis] - tolerance
                    && point[axis] <= min[axis] + size[axis] + tolerance
            }))
        }
        Some("cylinder") => {
            let base = vector3(
                primitive
                    .get("base")
                    .ok_or_else(|| format!("{path}.base is required."))?,
                &format!("{path}.base"),
            )?;
            let axis = vector3(
                primitive
                    .get("axis")
                    .ok_or_else(|| format!("{path}.axis is required."))?,
                &format!("{path}.axis"),
            )?;
            let length = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{path}.radius is required."))?;
            let height = primitive
                .get("height")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{path}.height is required."))?;
            if !length.is_finite()
                || length <= 0.0
                || !radius.is_finite()
                || radius <= 0.0
                || !height.is_finite()
                || height <= 0.0
            {
                return Err(format!("{path} has invalid cylinder dimensions."));
            }
            let direction = axis.map(|value| value / length);
            let relative = std::array::from_fn::<_, 3, _>(|index| point[index] - base[index]);
            let axial = (0..3)
                .map(|index| relative[index] * direction[index])
                .sum::<f64>();
            let radial_squared = (0..3)
                .map(|index| {
                    let radial = relative[index] - axial * direction[index];
                    radial * radial
                })
                .sum::<f64>();
            Ok(axial >= -tolerance
                && axial <= height + tolerance
                && radial_squared <= (radius + tolerance).powi(2))
        }
        Some("union") => {
            let features = primitive
                .get("features")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.features is required for a union."))?;
            for (index, child) in features.iter().enumerate() {
                if contains_primitive(
                    point,
                    child,
                    tolerance,
                    &format!("{path}.features[{index}]"),
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Some("intersection") => {
            let features = primitive
                .get("features")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.features is required for an intersection."))?;
            for (index, child) in features.iter().enumerate() {
                if !contains_primitive(
                    point,
                    child,
                    tolerance,
                    &format!("{path}.features[{index}]"),
                )? {
                    return Ok(false);
                }
            }
            Ok(!features.is_empty())
        }
        Some(kind) => Err(format!(
            "{path}.type {kind:?} is not a supported domain primitive."
        )),
        None => Err(format!("{path}.type is required.")),
    }
}

pub fn matches_domain_address(
    point: [f64; 3],
    address: &str,
    feature_ids: &[String],
    primitives: &std::collections::HashMap<String, &Value>,
    tolerance: f64,
    path: &str,
) -> Result<bool, String> {
    let bits = address.chars().collect::<Vec<_>>();
    if bits.len() != feature_ids.len() {
        return Err(format!(
            "{path}.address has {} bits but the geometry has {} domain features.",
            bits.len(),
            feature_ids.len()
        ));
    }
    for (bit, id) in bits.iter().zip(feature_ids) {
        let primitive = primitives
            .get(id)
            .ok_or_else(|| format!("{path} references missing primitive {id:?}."))?;
        let inside = contains_primitive(point, primitive, tolerance, &format!("{path}.{id}"))?;
        if (*bit == '1' && !inside) || (*bit == '0' && inside) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn emit_domain_address_entities(
    geo: &mut Vec<String>,
    variable: &str,
    entity_dimension: usize,
    address: &str,
    primitives: &[Value],
    entity_list: &str,
    tolerance: f64,
) -> Result<(), String> {
    emit_domain_address_entities_with_membership(
        geo,
        variable,
        entity_dimension,
        address,
        primitives,
        entity_list,
        tolerance,
        None,
    )
}

/// Emit a domain query with optional per-entity membership arrays for features
/// whose center of mass can fall outside their geometry (such as hollow unions).
pub fn emit_domain_address_entities_with_membership(
    geo: &mut Vec<String>,
    variable: &str,
    entity_dimension: usize,
    address: &str,
    primitives: &[Value],
    entity_list: &str,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    if !matches!(entity_dimension, 2 | 3) {
        return Err("Address-based domain entity queries require a 2D or 3D model.".into());
    }
    if address.chars().count() != primitives.len()
        || !address.chars().all(|bit| matches!(bit, '0' | '1'))
    {
        return Err(format!(
            "Domain address {address:?} must contain one 0/1 bit for each of the {} domain geometry features.",
            primitives.len()
        ));
    }
    if let Some(membership) = feature_membership {
        if membership.len() != primitives.len() {
            return Err(format!(
                "Domain address membership map has {} entries for {} geometry features.",
                membership.len(),
                primitives.len()
            ));
        }
    }
    let geometry_word = if entity_dimension == 2 {
        "Surface"
    } else {
        "Volume"
    };
    let entity_array = if entity_dimension == 2 {
        "allSurfaces"
    } else {
        "allVolumes"
    };
    geo.push(format!("{variable}[] = {{}};"));
    geo.push(format!("For {variable}Index In {{0:#{entity_array}[]-1}}"));
    geo.push(format!(
        "  {variable}Center[] = CenterOfMass {geometry_word} {{{entity_array}[{variable}Index]}};"
    ));
    let axes = if entity_dimension == 2 { 3 } else { 3 };
    for axis in 0..axes {
        geo.push(format!(
            "  {variable}Coordinate{axis} = {variable}Center[{axis}];"
        ));
    }
    let mut predicates = Vec::with_capacity(primitives.len());
    for (index, (bit, primitive)) in address.chars().zip(primitives).enumerate() {
        let inside = if let Some(Some(array)) =
            feature_membership.and_then(|membership| membership.get(index))
        {
            format!("{array}[{variable}Index] == 1")
        } else {
            primitive_predicate(primitive, variable, index, tolerance)?
        };
        predicates.push(if bit == '1' {
            inside
        } else {
            format!("!({inside})")
        });
    }
    let condition = if predicates.is_empty() {
        "1".to_string()
    } else {
        predicates.join(" && ")
    };
    geo.push(format!("  If ({condition})"));
    geo.push(format!(
        "    {variable}[] += {{{entity_array}[{variable}Index]}};"
    ));
    geo.push("  EndIf".to_string());
    geo.push("EndFor".to_string());
    let _ = entity_list;
    Ok(())
}

pub fn emit_domain_address_union(
    geo: &mut Vec<String>,
    variable: &str,
    entity_dimension: usize,
    addresses: &[String],
    primitives: &[Value],
    entity_list: &str,
    tolerance: f64,
) -> Result<(), String> {
    emit_domain_address_union_with_membership(
        geo,
        variable,
        entity_dimension,
        addresses,
        primitives,
        entity_list,
        tolerance,
        None,
    )
}

pub fn emit_domain_address_union_with_membership(
    geo: &mut Vec<String>,
    variable: &str,
    entity_dimension: usize,
    addresses: &[String],
    primitives: &[Value],
    entity_list: &str,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    if addresses.is_empty() {
        return Err("A domain selection needs at least one membership address.".into());
    }
    geo.push(format!("{variable}[] = {{}};"));
    for (index, address) in addresses.iter().enumerate() {
        let part = format!("{variable}Address{index}");
        emit_domain_address_entities_with_membership(
            geo,
            &part,
            entity_dimension,
            address,
            primitives,
            entity_list,
            tolerance,
            feature_membership,
        )?;
        geo.push(format!("{variable}[] += {{{part}[]}};"));
    }
    geo.push(format!("{variable}[] = Unique[{variable}[]];"));
    Ok(())
}

fn append_global_exterior_entities(
    geo: &mut Vec<String>,
    target: &str,
    region: &str,
    domain_keyword: &str,
    global_exterior: &str,
    suffix: &str,
) {
    let boundary = format!("{target}OuterBoundary{suffix}");
    let outer_index = format!("{target}OuterIndex{suffix}");
    let global_index = format!("{target}GlobalIndex{suffix}");
    geo.push(format!(
        "{boundary}[] = CombinedBoundary{{ {domain_keyword}{{{region}[]}}; }};"
    ));
    geo.push(format!("For {outer_index} In {{0:#{boundary}[]-1}}"));
    geo.push(format!(
        "  For {global_index} In {{0:#{global_exterior}[]-1}}"
    ));
    geo.push(format!(
        "    If (Abs({boundary}({outer_index})) == Abs({global_exterior}({global_index})))"
    ));
    geo.push(format!(
        "      {target}[] += {{{boundary}({outer_index})}};"
    ));
    geo.push("    EndIf".to_string());
    geo.push("  EndFor".to_string());
    geo.push("EndFor".to_string());
}

fn plane_distance_expr(coordinate: &str, plane: PlaneFilter) -> String {
    // Parenthesize the point coordinate because affine plane derivation can
    // produce negative values (including negative zero). Without the inner
    // parentheses, Gmsh receives expressions such as `center[1]--0`, which
    // its parser rejects.
    (0..3)
        .map(|axis| {
            format!(
                "({}*({coordinate}[{axis}]-({})))",
                fmt(plane.normal[axis]),
                fmt(plane.point[axis])
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn apply_plane_filter(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    entity: &str,
    filter: Option<PlaneFilter>,
    tolerance: f64,
) -> Result<(), String> {
    let Some(plane) = filter else {
        return Ok(());
    };
    if geometry_dimension == 2
        && (plane.point[2].abs() > tolerance || plane.normal[2].abs() > 1e-12)
    {
        return Err(
            "A 2D plane filter must lie in the model plane and have a normal in that plane.".into(),
        );
    }
    let gmsh_entity = match entity {
        "boundary" if geometry_dimension == 2 => "Curve",
        "boundary" => "Surface",
        "edge" => "Curve",
        "point" => "Point",
        _ => return Err(format!("Entity {entity:?} cannot be filtered.")),
    };
    let index = format!("{variable}FilterIndex");
    let raw = format!("{variable}Unfiltered");
    let tol = fmt(tolerance.max(2e-7));
    geo.push(format!("{raw}[] = {variable}[]; {variable}[] = {{}};"));
    geo.push(format!("For {index} In {{0:#{raw}[]-1}}"));
    match gmsh_entity {
        "Surface" => {
            geo.push(format!(
                "  {variable}Center[] = CenterOfMass Surface {{{raw}({index})}};"
            ));
            geo.push(format!(
                "  {variable}Distance = {};",
                plane_distance_expr(&format!("{variable}Center"), plane)
            ));
            geo.push(format!("  If (Fabs({variable}Distance) < {tol})"));
            geo.push(format!("    {variable}ParametricBounds[] = Parametric BoundingBox Surface {{{raw}({index})}};"));
            let mut normal_checks = Vec::new();
            for (sample, (u_fraction, v_fraction)) in [
                (0.2, 0.2),
                (0.5, 0.2),
                (0.8, 0.2),
                (0.2, 0.5),
                (0.5, 0.5),
                (0.8, 0.5),
                (0.2, 0.8),
                (0.5, 0.8),
                (0.8, 0.8),
            ]
            .into_iter()
            .enumerate()
            {
                let u = format!(
                    "{variable}ParametricBounds(0)+({}-{})*{}",
                    format!("{variable}ParametricBounds(2)"),
                    format!("{variable}ParametricBounds(0)"),
                    fmt(u_fraction)
                );
                let v = format!(
                    "{variable}ParametricBounds(1)+({}-{})*{}",
                    format!("{variable}ParametricBounds(3)"),
                    format!("{variable}ParametricBounds(1)"),
                    fmt(v_fraction)
                );
                let normal = format!("{variable}Normal{sample}");
                geo.push(format!(
                    "    {normal}[] = Normal Surface {{{raw}({index})}} Parametric {{{u}, {v}}};"
                ));
                let dot = (0..3)
                    .map(|axis| format!("({}*{normal}[{axis}])", fmt(plane.normal[axis])))
                    .collect::<Vec<_>>()
                    .join(" + ");
                let length = format!("{normal}Length");
                geo.push(format!(
                    "    {length} = Sqrt({normal}[0]^2+{normal}[1]^2+{normal}[2]^2);"
                ));
                normal_checks.push(format!(
                    "{length} > 1e-12 && Fabs(Fabs({dot}/{length})-1) < 1e-6"
                ));
            }
            geo.push(format!("    If ({})", normal_checks.join(" && ")));
            geo.push(format!("      {variable}[] += {{{raw}({index})}};"));
            geo.push("    EndIf".to_string());
            geo.push("  EndIf".to_string());
        }
        "Curve" => {
            geo.push(format!(
                "  {variable}Center[] = CenterOfMass Curve {{{raw}({index})}};"
            ));
            geo.push(format!(
                "  {variable}Distance = {};",
                plane_distance_expr(&format!("{variable}Center"), plane)
            ));
            geo.push(format!("  If (Fabs({variable}Distance) < {tol})"));
            geo.push(format!(
                "    {variable}Endpoints[] = Boundary {{ Curve {{{raw}({index})}}; }};"
            ));
            geo.push(format!("    If (#{variable}Endpoints[] == 2)"));
            geo.push(format!(
                "      {variable}Endpoint0[] = Point {{ Abs({variable}Endpoints(0)) }};"
            ));
            geo.push(format!(
                "      {variable}Endpoint1[] = Point {{ Abs({variable}Endpoints(1)) }};"
            ));
            let endpoint_checks = (0..2)
                .map(|endpoint| {
                    format!(
                        "Fabs({}) < {tol}",
                        plane_distance_expr(&format!("{variable}Endpoint{endpoint}"), plane)
                    )
                })
                .collect::<Vec<_>>()
                .join(" && ");
            geo.push(format!("      If ({endpoint_checks})"));
            geo.push(format!("        {variable}[] += {{{raw}({index})}};"));
            geo.push("      EndIf".to_string());
            geo.push("    EndIf".to_string());
            geo.push("  EndIf".to_string());
        }
        "Point" => {
            geo.push(format!("  {variable}Point[] = Point {{{raw}({index})}};"));
            geo.push(format!(
                "  If (Fabs({}) < {tol})",
                plane_distance_expr(&format!("{variable}Point"), plane)
            ));
            geo.push(format!("    {variable}[] += {{{raw}({index})}};"));
            geo.push("  EndIf".to_string());
        }
        _ => unreachable!(),
    }
    geo.push("EndFor".to_string());
    Ok(())
}

fn emit_boundary_address(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    address: &str,
    bits: &[char],
    stars: &[usize],
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    let domain_keyword = if geometry_dimension == 2 {
        "Surface"
    } else {
        "Volume"
    };
    let global_exterior = format!("{variable}GlobalExterior");
    if geometry_dimension == 2 {
        geo.push(format!(
            "{global_exterior}[] = CombinedBoundary{{ Surface{{allSurfaces[]}}; }};"
        ));
    } else {
        geo.push(format!(
            "{global_exterior}[] = CombinedBoundary{{ Volume{{allVolumes[]}}; }};"
        ));
    }
    geo.push(format!("{variable}[] = {{}};"));
    let combinations = 1usize
        .checked_shl((stars.len() - 1) as u32)
        .ok_or_else(|| {
            format!("Selection address {address:?} has too many transition bits to compile.")
        })?;
    for combination in 0..combinations {
        let mut lower = bits.to_vec();
        let mut upper = bits.to_vec();
        lower[stars[0]] = '0';
        upper[stars[0]] = '1';
        for (selector, index) in stars.iter().copied().skip(1).enumerate() {
            let side = if combination & (1 << selector) == 0 {
                '0'
            } else {
                '1'
            };
            lower[index] = side;
            upper[index] = if side == '0' { '1' } else { '0' };
        }
        let low_address = lower.iter().collect::<String>();
        let high_address = upper.iter().collect::<String>();
        let low = format!("{variable}RegionLow{combination}");
        let high = format!("{variable}RegionHigh{combination}");
        emit_domain_address_entities_with_membership(
            geo,
            &low,
            geometry_dimension,
            &low_address,
            primitives,
            "",
            tolerance,
            feature_membership,
        )?;
        emit_domain_address_entities_with_membership(
            geo,
            &high,
            geometry_dimension,
            &high_address,
            primitives,
            "",
            tolerance,
            feature_membership,
        )?;
        let candidates = format!("{variable}SharedCandidates{combination}");
        let exterior = format!("{variable}SharedExterior{combination}");
        geo.push(format!(
            "{candidates}[] = Boundary{{ {domain_keyword}{{{low}[],{high}[]}}; }};"
        ));
        geo.push(format!("{candidates}[] = Unique[Abs[{candidates}[]]];"));
        geo.push(format!(
            "{exterior}[] = CombinedBoundary{{ {domain_keyword}{{{low}[],{high}[]}}; }};"
        ));
        geo.push(format!("{exterior}[] = Unique[Abs[{exterior}[]]];"));
        geo.push(format!("{candidates}[] -= {{{exterior}[]}};"));
        geo.push(format!("{variable}[] += {{{candidates}[]}};"));
        if low_address.chars().all(|bit| bit == '0') || high_address.chars().all(|bit| bit == '0') {
            let interior = if low_address.chars().all(|bit| bit == '0') {
                &high
            } else {
                &low
            };
            append_global_exterior_entities(
                geo,
                variable,
                interior,
                domain_keyword,
                &global_exterior,
                &combination.to_string(),
            );
        }
    }
    geo.push(format!("{variable}[] = Unique[Abs[{variable}[]]];"));
    apply_plane_filter(
        geo,
        variable,
        geometry_dimension,
        "boundary",
        plane_filter,
        tolerance,
    )
}

fn emit_edge_address(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    address: &str,
    bits: &[char],
    stars: &[usize],
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    let global_exterior = format!("{variable}GlobalExterior");
    geo.push(format!(
        "{global_exterior}[] = CombinedBoundary{{ Volume{{allVolumes[]}}; }};"
    ));
    let combinations = 1usize
        .checked_shl((stars.len() - 1) as u32)
        .ok_or_else(|| {
            format!("Selection address {address:?} has too many transition bits to compile.")
        })?;
    let fixed_bits_are_exterior = bits.iter().all(|bit| *bit == '*' || *bit == '0');
    let mut transition_sets = Vec::with_capacity(stars.len());
    for (star_ordinal, star_index) in stars.iter().copied().enumerate() {
        let transition = format!("{variable}Transition{star_ordinal}");
        let regions_zero = format!("{transition}RegionsZero");
        let regions_one = format!("{transition}RegionsOne");
        geo.push(format!("{regions_zero}[] = {{}}; {regions_one}[] = {{}};"));
        for combination in 0..combinations {
            let mut zero = bits.to_vec();
            let mut one = bits.to_vec();
            zero[star_index] = '0';
            one[star_index] = '1';
            for (selector, index) in stars
                .iter()
                .copied()
                .filter(|index| *index != star_index)
                .enumerate()
            {
                let side = if combination & (1 << selector) == 0 {
                    '0'
                } else {
                    '1'
                };
                zero[index] = side;
                one[index] = side;
            }
            let zero_address = zero.iter().collect::<String>();
            let one_address = one.iter().collect::<String>();
            let zero_region = format!("{transition}Zero{combination}");
            let one_region = format!("{transition}One{combination}");
            emit_domain_address_entities_with_membership(
                geo,
                &zero_region,
                geometry_dimension,
                &zero_address,
                primitives,
                "",
                tolerance,
                feature_membership,
            )?;
            emit_domain_address_entities_with_membership(
                geo,
                &one_region,
                geometry_dimension,
                &one_address,
                primitives,
                "",
                tolerance,
                feature_membership,
            )?;
            geo.push(format!(
                "{regions_zero}[] += {{{zero_region}[]}}; {regions_one}[] += {{{one_region}[]}};"
            ));
        }
        geo.push(format!("{regions_zero}[] = Unique[Abs[{regions_zero}[]]]; {regions_one}[] = Unique[Abs[{regions_one}[]]];"));
        let candidates = format!("{transition}Candidates");
        let exterior = format!("{transition}Exterior");
        geo.push(format!(
            "{candidates}[] = Boundary{{ Volume{{{regions_zero}[],{regions_one}[]}}; }};"
        ));
        geo.push(format!("{candidates}[] = Unique[Abs[{candidates}[]]];"));
        geo.push(format!(
            "{exterior}[] = CombinedBoundary{{ Volume{{{regions_zero}[],{regions_one}[]}}; }};"
        ));
        geo.push(format!("{exterior}[] = Unique[Abs[{exterior}[]]];"));
        geo.push(format!(
            "{transition}[] = {candidates}[]; {transition}[] -= {{{exterior}[]}};"
        ));
        if fixed_bits_are_exterior {
            append_global_exterior_entities(
                geo,
                &transition,
                &regions_one,
                "Volume",
                &global_exterior,
                &star_ordinal.to_string(),
            );
        }
        geo.push(format!("{transition}[] = Unique[Abs[{transition}[]]];"));
        transition_sets.push(transition);
    }
    let mut curve_sets = Vec::with_capacity(transition_sets.len());
    for (index, transition) in transition_sets.iter().enumerate() {
        let curves = format!("{variable}TransitionCurves{index}");
        geo.push(format!(
            "{curves}[] = Boundary{{ Surface{{{transition}[]}}; }};"
        ));
        geo.push(format!("{curves}[] = Unique[Abs[{curves}[]]];"));
        curve_sets.push(curves);
    }
    geo.push(format!("{variable}[] = {}[];", curve_sets[0]));
    for (index, next) in curve_sets.iter().enumerate().skip(1) {
        let previous = format!("{variable}IntersectionPrevious{index}");
        let intersection = format!("{variable}Intersection{index}");
        let outer = format!("{variable}IntersectionOuter{index}");
        let inner = format!("{variable}IntersectionInner{index}");
        geo.push(format!("{previous}[] = {variable}[]; {variable}[] = {{}};"));
        geo.push(format!("For {outer} In {{0:#{previous}[]-1}}"));
        geo.push(format!("  For {inner} In {{0:#{next}[]-1}}"));
        geo.push(format!(
            "    If (Abs({previous}({outer})) == Abs({next}({inner})))"
        ));
        geo.push(format!("      {variable}[] += {{{previous}({outer})}};"));
        geo.push("    EndIf".to_string());
        geo.push("  EndFor".to_string());
        geo.push("EndFor".to_string());
        geo.push(format!(
            "{intersection}[] = Unique[Abs[{variable}[]]]; {variable}[] = {intersection}[];"
        ));
    }
    apply_plane_filter(
        geo,
        variable,
        geometry_dimension,
        "edge",
        plane_filter,
        tolerance,
    )
}

/// Domain addresses name partitions directly. A `*` on a lower-dimensional
/// selection marks a geometry-feature boundary containing that entity. The
/// explicit entity chooses its topological dimension; extra stars account for
/// touching or coincident geometry features.
pub fn emit_address_selection(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    entity: &str,
    address: &str,
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
) -> Result<(), String> {
    emit_address_selection_with_membership(
        geo,
        variable,
        geometry_dimension,
        entity,
        address,
        primitives,
        plane_filter,
        tolerance,
        None,
    )
}

pub fn emit_address_selection_with_membership(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    entity: &str,
    address: &str,
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    if !matches!(geometry_dimension, 2 | 3) {
        return Err("Address selections require a 2D or 3D model.".into());
    }
    let bits = address.chars().collect::<Vec<_>>();
    if bits.len() != primitives.len() || bits.iter().any(|bit| !matches!(bit, '0' | '1' | '*')) {
        return Err(format!(
            "Selection address {address:?} must contain one membership bit for each of the {} domain geometry features.",
            primitives.len()
        ));
    }
    let stars = bits
        .iter()
        .enumerate()
        .filter_map(|(index, bit)| (*bit == '*').then_some(index))
        .collect::<Vec<_>>();
    let minimum_stars = match entity {
        "domain" => 0,
        "boundary" => 1,
        "edge" if geometry_dimension == 3 => 1,
        "point" => 1,
        _ => {
            return Err(format!(
                "Entity {entity:?} is not selectable in a {geometry_dimension}D model."
            ));
        }
    };
    if (entity == "domain" && stars.len() != 0)
        || (entity != "domain" && stars.len() < minimum_stars)
    {
        return Err(if entity == "domain" {
            format!("Domain selection address {address:?} cannot contain * bits.")
        } else {
            format!(
                "Selection {entity} address {address:?} needs at least one * membership bit for a geometry-feature boundary; the entity field selects its topological dimension."
            )
        });
    }
    if entity == "domain" {
        return emit_domain_address_entities_with_membership(
            geo,
            variable,
            geometry_dimension,
            address,
            primitives,
            if geometry_dimension == 2 {
                "SurfacesList"
            } else {
                "VolumesList"
            },
            tolerance,
            feature_membership,
        );
    }
    if entity == "point" {
        geo.push(format!("{variable}Candidates[] = Point{{:}};"));
        geo.push(format!("{variable}[] = {{}};"));
        geo.push(format!(
            "For {variable}Index In {{0:#{variable}Candidates[]-1}}"
        ));
        geo.push(format!(
            "  {variable}Center[] = Point {{{variable}Candidates[{variable}Index]}};"
        ));
        for axis in 0..geometry_dimension {
            geo.push(format!(
                "  {variable}Coordinate{axis} = {variable}Center[{axis}];"
            ));
        }
        let mut conditions = Vec::with_capacity(bits.len());
        for (index, (bit, primitive)) in bits.iter().zip(primitives).enumerate() {
            let (inside, on_boundary) =
                primitive_point_predicates(primitive, variable, index, tolerance)?;
            conditions.push(match bit {
                '*' => on_boundary,
                '1' => format!("({inside} && !({on_boundary}))"),
                '0' => format!("(!({inside}) && !({on_boundary}))"),
                _ => unreachable!(),
            });
        }
        if let Some(plane) = plane_filter {
            if geometry_dimension == 2
                && (plane.point[2].abs() > tolerance || plane.normal[2].abs() > 1e-12)
            {
                return Err("A 2D plane filter must lie in the model plane and have a normal in that plane.".into());
            }
            conditions.push(format!(
                "Fabs({}) < {}",
                plane_distance_expr(&format!("{variable}Center"), plane),
                fmt(tolerance.max(2e-7))
            ));
        }
        geo.push(format!("  If ({})", conditions.join(" && ")));
        geo.push(format!(
            "    {variable}[] += {{{variable}Candidates[{variable}Index]}};"
        ));
        geo.push("  EndIf".to_string());
        geo.push("EndFor".to_string());
        return Ok(());
    }
    if entity == "boundary" {
        return emit_boundary_address(
            geo,
            variable,
            geometry_dimension,
            address,
            &bits,
            &stars,
            primitives,
            plane_filter,
            tolerance,
            feature_membership,
        );
    }
    if entity == "edge" {
        return emit_edge_address(
            geo,
            variable,
            geometry_dimension,
            address,
            &bits,
            &stars,
            primitives,
            plane_filter,
            tolerance,
            feature_membership,
        );
    }
    Err(format!(
        "Entity {entity:?} is not supported for an address selection."
    ))
}

pub fn emit_address_selection_union(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    entity: &str,
    addresses: &[String],
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
) -> Result<(), String> {
    emit_address_selection_union_with_membership(
        geo,
        variable,
        geometry_dimension,
        entity,
        addresses,
        primitives,
        plane_filter,
        tolerance,
        None,
    )
}

pub fn emit_address_selection_union_with_membership(
    geo: &mut Vec<String>,
    variable: &str,
    geometry_dimension: usize,
    entity: &str,
    addresses: &[String],
    primitives: &[Value],
    plane_filter: Option<PlaneFilter>,
    tolerance: f64,
    feature_membership: Option<&[Option<String>]>,
) -> Result<(), String> {
    if addresses.is_empty() {
        return Err(format!(
            "A {entity} selection needs at least one membership address."
        ));
    }
    geo.push(format!("{variable}[] = {{}};"));
    for (index, address) in addresses.iter().enumerate() {
        let part = format!("{variable}Address{index}");
        emit_address_selection_with_membership(
            geo,
            &part,
            geometry_dimension,
            entity,
            address,
            primitives,
            plane_filter,
            tolerance,
            feature_membership,
        )?;
        geo.push(format!("{variable}[] += {{{part}[]}};"));
    }
    geo.push(format!("{variable}[] = Unique[{variable}[]];"));
    Ok(())
}

pub fn matches_any_domain_address(
    point: [f64; 3],
    addresses: &[String],
    feature_ids: &[String],
    primitives: &std::collections::HashMap<String, &Value>,
    tolerance: f64,
    path: &str,
) -> Result<bool, String> {
    if addresses.is_empty() {
        return Err(format!(
            "{path}.addresses must contain at least one membership address."
        ));
    }
    for (index, address) in addresses.iter().enumerate() {
        if matches_domain_address(
            point,
            address,
            feature_ids,
            primitives,
            tolerance,
            &format!("{path}.addresses[{index}]"),
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn primitive_predicate(
    primitive: &Value,
    prefix: &str,
    index: usize,
    tolerance: f64,
) -> Result<String, String> {
    let coord = |axis: usize| format!("{prefix}Coordinate{axis}");
    match primitive.get("type").and_then(Value::as_str) {
        Some("rectangle") => {
            let min = primitive
                .get("min")
                .and_then(Value::as_array)
                .ok_or("Rectangle primitive is missing min.")?;
            let size = primitive
                .get("size")
                .and_then(Value::as_array)
                .ok_or("Rectangle primitive is missing size.")?;
            let x = min
                .first()
                .and_then(Value::as_f64)
                .ok_or("Rectangle min x must be numeric.")?;
            let y = min
                .get(1)
                .and_then(Value::as_f64)
                .ok_or("Rectangle min y must be numeric.")?;
            let w = size
                .first()
                .and_then(Value::as_f64)
                .ok_or("Rectangle width must be numeric.")?;
            let h = size
                .get(1)
                .and_then(Value::as_f64)
                .ok_or("Rectangle height must be numeric.")?;
            Ok(format!(
                "({} >= {} && {} <= {} && {} >= {} && {} <= {})",
                coord(0),
                fmt(x - tolerance),
                coord(0),
                fmt(x + w + tolerance),
                coord(1),
                fmt(y - tolerance),
                coord(1),
                fmt(y + h + tolerance)
            ))
        }
        Some("circle") => {
            let center = primitive
                .get("center")
                .and_then(Value::as_array)
                .ok_or("Circle primitive is missing center.")?;
            let x = center
                .first()
                .and_then(Value::as_f64)
                .ok_or("Circle center x must be numeric.")?;
            let y = center
                .get(1)
                .and_then(Value::as_f64)
                .ok_or("Circle center y must be numeric.")?;
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or("Circle radius must be numeric.")?
                + tolerance;
            Ok(format!(
                "(({}-{})^2 + ({}-{})^2 <= {}^2)",
                coord(0),
                fmt(x),
                coord(1),
                fmt(y),
                fmt(radius)
            ))
        }
        Some("box") => {
            let min = primitive
                .get("min")
                .and_then(Value::as_array)
                .ok_or("Box primitive is missing min.")?;
            let size = primitive
                .get("size")
                .and_then(Value::as_array)
                .ok_or("Box primitive is missing size.")?;
            if min.len() != 3 || size.len() != 3 {
                return Err("Box min and size must contain three coordinates.".into());
            }
            let mut checks = Vec::new();
            for axis in 0..3 {
                let lo = min[axis].as_f64().ok_or("Box min must be numeric.")?;
                let extent = size[axis].as_f64().ok_or("Box size must be numeric.")?;
                checks.push(format!(
                    "({} >= {} && {} <= {})",
                    coord(axis),
                    fmt(lo - tolerance),
                    coord(axis),
                    fmt(lo + extent + tolerance)
                ));
            }
            Ok(format!("({})", checks.join(" && ")))
        }
        Some("cylinder") => {
            let base = primitive
                .get("base")
                .and_then(Value::as_array)
                .ok_or("Cylinder primitive is missing base.")?;
            let axis = primitive
                .get("axis")
                .and_then(Value::as_array)
                .ok_or("Cylinder primitive is missing axis.")?;
            if base.len() != 3 || axis.len() != 3 {
                return Err("Cylinder base and axis must contain three coordinates.".into());
            }
            let base = [
                base[0].as_f64().ok_or("Cylinder base must be numeric.")?,
                base[1].as_f64().ok_or("Cylinder base must be numeric.")?,
                base[2].as_f64().ok_or("Cylinder base must be numeric.")?,
            ];
            let raw = [
                axis[0].as_f64().ok_or("Cylinder axis must be numeric.")?,
                axis[1].as_f64().ok_or("Cylinder axis must be numeric.")?,
                axis[2].as_f64().ok_or("Cylinder axis must be numeric.")?,
            ];
            let length = raw.iter().map(|value| value * value).sum::<f64>().sqrt();
            if !length.is_finite() || length <= 0.0 {
                return Err("Cylinder axis must be nonzero.".into());
            }
            let d = raw.map(|value| value / length);
            let height = primitive
                .get("height")
                .and_then(Value::as_f64)
                .ok_or("Cylinder height must be numeric.")?;
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or("Cylinder radius must be numeric.")?
                + tolerance;
            let dx = format!("({}-{})", coord(0), fmt(base[0]));
            let dy = format!("({}-{})", coord(1), fmt(base[1]));
            let dz = format!("({}-{})", coord(2), fmt(base[2]));
            let axial = format!(
                "({}*{} + {}*{} + {}*{})",
                dx,
                fmt(d[0]),
                dy,
                fmt(d[1]),
                dz,
                fmt(d[2])
            );
            let radial = format!("({dx}^2 + {dy}^2 + {dz}^2 - {axial}^2)");
            Ok(format!(
                "({axial} >= {} && {axial} <= {} && {radial} <= {}^2)",
                fmt(-tolerance),
                fmt(height + tolerance),
                fmt(radius)
            ))
        }
        Some("union" | "intersection") => {
            let kind = primitive
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("boolean");
            let children = primitive
                .get("features")
                .and_then(Value::as_array)
                .ok_or("Boolean primitive is missing features.")?;
            let checks = children
                .iter()
                .enumerate()
                .map(|(child_index, child)| {
                    primitive_predicate(child, prefix, index + child_index, tolerance)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if checks.is_empty() {
                return Err(format!("{kind} primitive must contain at least one child."));
            }
            Ok(format!(
                "({})",
                checks.join(if kind == "union" { " || " } else { " && " })
            ))
        }
        Some(kind) => Err(format!(
            "Primitive {index} has unsupported type {kind:?} for address selection."
        )),
        None => Err(format!("Primitive {index} is missing its type.")),
    }
}

fn primitive_point_predicates(
    primitive: &Value,
    prefix: &str,
    index: usize,
    tolerance: f64,
) -> Result<(String, String), String> {
    let coord = |axis: usize| format!("{prefix}Coordinate{axis}");
    let tol = fmt(tolerance.max(1e-10));
    match primitive.get("type").and_then(Value::as_str) {
        Some("rectangle") => {
            let min = primitive
                .get("min")
                .and_then(Value::as_array)
                .ok_or("Rectangle primitive is missing min.")?;
            let size = primitive
                .get("size")
                .and_then(Value::as_array)
                .ok_or("Rectangle primitive is missing size.")?;
            let x = min[0].as_f64().ok_or("Rectangle min x must be numeric.")?;
            let y = min[1].as_f64().ok_or("Rectangle min y must be numeric.")?;
            let w = size[0].as_f64().ok_or("Rectangle width must be numeric.")?;
            let h = size[1]
                .as_f64()
                .ok_or("Rectangle height must be numeric.")?;
            let inside = format!(
                "({} >= {} && {} <= {} && {} >= {} && {} <= {})",
                coord(0),
                fmt(x - tolerance),
                coord(0),
                fmt(x + w + tolerance),
                coord(1),
                fmt(y - tolerance),
                coord(1),
                fmt(y + h + tolerance)
            );
            let boundary = format!(
                "((Fabs({}-{}) < {tol} || Fabs({}-{}) < {tol}) && {} >= {} && {} <= {}) || ((Fabs({}-{}) < {tol} || Fabs({}-{}) < {tol}) && {} >= {} && {} <= {})",
                coord(0),
                fmt(x),
                coord(0),
                fmt(x + w),
                coord(1),
                fmt(y - tolerance),
                coord(1),
                fmt(y + h + tolerance),
                coord(1),
                fmt(y),
                coord(1),
                fmt(y + h),
                coord(0),
                fmt(x - tolerance),
                coord(0),
                fmt(x + w + tolerance)
            );
            Ok((inside, format!("({boundary})")))
        }
        Some("circle") => {
            let center = primitive
                .get("center")
                .and_then(Value::as_array)
                .ok_or("Circle primitive is missing center.")?;
            let x = center[0]
                .as_f64()
                .ok_or("Circle center x must be numeric.")?;
            let y = center[1]
                .as_f64()
                .ok_or("Circle center y must be numeric.")?;
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or("Circle radius must be numeric.")?;
            let distance = format!(
                "Sqrt(({}-{})^2+({}-{})^2)",
                coord(0),
                fmt(x),
                coord(1),
                fmt(y)
            );
            Ok((
                format!("({distance} <= {})", fmt(radius + tolerance)),
                format!("Fabs({distance}-{}) < {tol}", fmt(radius)),
            ))
        }
        Some("box") => {
            let min = primitive
                .get("min")
                .and_then(Value::as_array)
                .ok_or("Box primitive is missing min.")?;
            let size = primitive
                .get("size")
                .and_then(Value::as_array)
                .ok_or("Box primitive is missing size.")?;
            let mut checks = Vec::new();
            let mut surfaces = Vec::new();
            for axis in 0..3 {
                let lo = min[axis].as_f64().ok_or("Box min must be numeric.")?;
                let extent = size[axis].as_f64().ok_or("Box size must be numeric.")?;
                let hi = lo + extent;
                checks.push(format!(
                    "({} >= {} && {} <= {})",
                    coord(axis),
                    fmt(lo - tolerance),
                    coord(axis),
                    fmt(hi + tolerance)
                ));
                surfaces.push(format!(
                    "Fabs({}-{}) < {tol} || Fabs({}-{}) < {tol}",
                    coord(axis),
                    fmt(lo),
                    coord(axis),
                    fmt(hi)
                ));
            }
            let inside = format!("({})", checks.join(" && "));
            let boundary = format!("({inside} && ({}))", surfaces.join(" || "));
            Ok((inside, boundary))
        }
        Some("cylinder") => {
            let base = vector3(
                primitive
                    .get("base")
                    .ok_or("Cylinder primitive is missing base.")?,
                "cylinder.base",
            )?;
            let axis = vector3(
                primitive
                    .get("axis")
                    .ok_or("Cylinder primitive is missing axis.")?,
                "cylinder.axis",
            )?;
            let norm = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
            if norm <= 0.0 {
                return Err("Cylinder axis must be nonzero.".into());
            }
            let direction = axis.map(|value| value / norm);
            let height = primitive
                .get("height")
                .and_then(Value::as_f64)
                .ok_or("Cylinder height is missing.")?;
            let radius = primitive
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or("Cylinder radius is missing.")?;
            let delta = (0..3)
                .map(|axis| format!("({}-{})", coord(axis), fmt(base[axis])))
                .collect::<Vec<_>>();
            let axial = format!(
                "({}*{}+{}*{}+{}*{})",
                delta[0],
                fmt(direction[0]),
                delta[1],
                fmt(direction[1]),
                delta[2],
                fmt(direction[2])
            );
            let radial = format!(
                "Sqrt({}^2+{}^2+{}^2-({axial})^2)",
                delta[0], delta[1], delta[2]
            );
            let inside = format!(
                "({axial} >= {} && {axial} <= {} && {radial} <= {})",
                fmt(-tolerance),
                fmt(height + tolerance),
                fmt(radius + tolerance)
            );
            let cap = format!(
                "((Fabs({axial}) < {tol} || Fabs({axial}-{}) < {tol}) && {radial} <= {})",
                fmt(height),
                fmt(radius + tolerance)
            );
            let side = format!(
                "({axial} >= {} && {axial} <= {} && Fabs({radial}-{}) < {tol})",
                fmt(-tolerance),
                fmt(height + tolerance),
                fmt(radius)
            );
            Ok((inside, format!("({cap} || {side})")))
        }
        Some("union" | "intersection") => {
            let kind = primitive
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("boolean");
            let children = primitive
                .get("features")
                .and_then(Value::as_array)
                .ok_or("Boolean primitive is missing features.")?;
            let predicates = children
                .iter()
                .enumerate()
                .map(|(child_index, child)| {
                    primitive_point_predicates(child, prefix, index + child_index, tolerance)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if predicates.is_empty() {
                return Err(format!("{kind} primitive must contain at least one child."));
            }
            let inside = predicates
                .iter()
                .map(|(inside, _)| format!("({inside})"))
                .collect::<Vec<_>>()
                .join(if kind == "union" { " || " } else { " && " });
            let boundary = if kind == "union" {
                predicates
                    .iter()
                    .map(|(_, boundary)| format!("({boundary})"))
                    .collect::<Vec<_>>()
                    .join(" || ")
            } else {
                predicates
                    .iter()
                    .enumerate()
                    .map(|(index, (_, boundary))| {
                        let other_inside = predicates
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .map(|(_, (inside, _))| format!("({inside})"))
                            .collect::<Vec<_>>()
                            .join(" && ");
                        format!(
                            "(({boundary}){}{})",
                            if other_inside.is_empty() { "" } else { " && " },
                            other_inside
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" || ")
            };
            Ok((format!("({inside})"), format!("({boundary})")))
        }
        Some(kind) => Err(format!(
            "Primitive {index} has unsupported type {kind:?} for point address selection."
        )),
        None => Err(format!("Primitive {index} is missing its type.")),
    }
}

fn vector3(value: &Value, path: &str) -> Result<[f64; 3], String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("{path} must be an array."))?;
    if values.len() != 3 {
        return Err(format!("{path} must contain three coordinates."));
    }
    Ok([
        values[0]
            .as_f64()
            .ok_or_else(|| format!("{path}[0] must be numeric."))?,
        values[1]
            .as_f64()
            .ok_or_else(|| format!("{path}[1] must be numeric."))?,
        values[2]
            .as_f64()
            .ok_or_else(|| format!("{path}[2] must be numeric."))?,
    ])
}

fn fmt(value: f64) -> String {
    format!("{:.15e}", if value == 0.0 { 0.0 } else { value })
}
