//! Declared Cartesian coordinate frames for mesh and field adapters.
//!
//! Translation affects positions only. Polar and axial vectors transform
//! differently under an odd axis permutation, as required for curls.
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct CoordinateFrame {
    /// Each internal axis names its corresponding model axis.
    order: [usize; 3],
    origin: [f64; 3],
    orientation: f64,
}

impl CoordinateFrame {
    pub(crate) fn from_json(declaration: &Value, nodes: &[[f64; 3]]) -> Result<Self, String> {
        let order = declaration.get("axisOrder").and_then(Value::as_array)
            .filter(|axes| axes.len() == 3).ok_or("Coordinate frame requires three axisOrder entries.")?;
        let mut permutation = [0; 3];
        let mut seen = [false; 3];
        for (i, value) in order.iter().enumerate() {
            let axis = value.as_u64().filter(|&axis| axis < 3)
                .ok_or("Coordinate axisOrder entries must be integers from 0 to 2.")? as usize;
            if seen[axis] { return Err("Coordinate axisOrder must be a permutation.".into()); }
            seen[axis] = true;
            permutation[i] = axis;
        }
        if nodes.iter().flatten().any(|value| !value.is_finite()) {
            return Err("Coordinate frame requires finite mesh positions.".into());
        }
        let origin = match declaration.get("origin").and_then(Value::as_str) {
            Some("zero") => [0.; 3],
            Some("boundsMinimum") => {
                if nodes.is_empty() { return Err("A bounds-based coordinate frame requires mesh positions.".into()); }
                let mut minimum = [f64::INFINITY; 3];
                for node in nodes { for axis in 0..3 { minimum[axis] = minimum[axis].min(node[axis]); } }
                minimum
            }
            _ => return Err("Coordinate frame origin must be zero or boundsMinimum.".into()),
        };
        let inversions = (0..3).flat_map(|i| (i+1..3).map(move |j| (i,j)))
            .filter(|&(i,j)| permutation[i] > permutation[j]).count();
        Ok(Self { order: permutation, origin, orientation: if inversions % 2 == 0 { 1. } else { -1. } })
    }

    pub(crate) fn point_to_internal(&self, point: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| point[self.order[i]] - self.origin[self.order[i]])
    }

    pub(crate) fn point_to_model(&self, point: [f64; 3]) -> [f64; 3] {
        let mut model = self.origin;
        for i in 0..3 { model[self.order[i]] += point[i]; }
        model
    }

    pub(crate) fn vector_to_internal<T: Copy>(&self, vector: [T; 3]) -> [T; 3] {
        std::array::from_fn(|i| vector[self.order[i]])
    }

    pub(crate) fn vector_to_model<T: Copy>(&self, vector: [T; 3]) -> [T; 3] {
        let mut model = vector;
        for i in 0..3 { model[self.order[i]] = vector[i]; }
        model
    }

    /// The sign by which an axial vector changes relative to a polar vector.
    pub(crate) fn orientation(&self) -> f64 { self.orientation }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn translated_cyclic_frame_round_trips_positions_and_vectors() {
        let nodes = [[10.,20.,30.], [15.,23.,40.], [12.,18.,31.]];
        let frame = CoordinateFrame::from_json(&json!({"axisOrder":[1,2,0],"origin":"boundsMinimum"}), &nodes).unwrap();
        let position = nodes[1];
        assert_eq!(frame.point_to_internal(position), [5.,10.,5.]);
        assert_eq!(frame.point_to_model(frame.point_to_internal(position)), position);
        assert_eq!(frame.vector_to_internal([2.,3.,5.]), [3.,5.,2.]);
        assert_eq!(frame.vector_to_model(frame.vector_to_internal([2.,3.,5.])), [2.,3.,5.]);
        assert_eq!(frame.orientation(), 1.);
    }

    #[test]
    fn reflection_has_negative_axial_sign_and_invalid_declarations_fail() {
        let frame = CoordinateFrame::from_json(&json!({"axisOrder":[1,0,2],"origin":"zero"}), &[]).unwrap();
        assert_eq!(frame.orientation(), -1.);
        assert!(CoordinateFrame::from_json(&json!({"axisOrder":[0,0,2],"origin":"zero"}), &[]).is_err());
        assert!(CoordinateFrame::from_json(&json!({"axisOrder":[0,1,3],"origin":"zero"}), &[]).is_err());
        assert!(CoordinateFrame::from_json(&json!({"axisOrder":[0,1,2],"origin":"boundsMinimum"}), &[]).is_err());
        assert!(CoordinateFrame::from_json(&json!({"axisOrder":[0,1,2],"origin":"zero"}), &[[f64::NAN,0.,0.]]).is_err());
    }
}
