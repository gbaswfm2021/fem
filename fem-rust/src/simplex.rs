//! Geometry and P1 basis gradients for straight simplex elements.

/// Geometric degeneracy policy is an explicit numerical choice. Different
/// current scenarios historically use different tolerances; the caller selects
/// the policy until those contracts are consolidated.
#[derive(Clone, Copy)]
pub(crate) enum DeterminantTolerance {
    /// Reject determinants at or below this fraction of the longest edge cubed.
    Relative(f64),
    /// Reject determinants with absolute magnitude below this value.
    Absolute(f64),
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|axis| a[axis] * b[axis]).sum()
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Return physical volume and barycentric gradients on a tetrahedron. The
/// gradients retain the actual vertex ordering even for a reversed element.
pub(crate) fn tetra_p1(points: [[f64; 3]; 4], tolerance: DeterminantTolerance)
    -> Result<(f64, [[f64; 3]; 4]), String> {
    let a = sub(points[1], points[0]);
    let b = sub(points[2], points[0]);
    let c = sub(points[3], points[0]);
    let determinant = dot(a, cross(b, c));
    if !determinant.is_finite() {
        return Err("Tetrahedron has a non-finite determinant.".into());
    }
    let valid = match tolerance {
        DeterminantTolerance::Relative(factor) if factor.is_finite() && factor >= 0.0 => {
            let scale = dot(a, a).max(dot(b, b)).max(dot(c, c)).sqrt();
            scale != 0.0 && determinant.abs() > factor * scale.powi(3)
        },
        DeterminantTolerance::Absolute(minimum) if minimum.is_finite() && minimum >= 0.0 => {
            determinant != 0.0 && determinant.abs() >= minimum
        },
        _ => return Err("Invalid tetrahedron degeneracy tolerance.".into()),
    };
    if !valid {
        return Err("Tetrahedron is degenerate under the declared tolerance.".into());
    }
    let mut gradients = [[0.0; 3]; 4];
    for (index, cofactor) in [cross(b, c), cross(c, a), cross(a, b)].into_iter().enumerate() {
        for axis in 0..3 {
            gradients[index + 1][axis] = cofactor[axis] / determinant;
        }
    }
    for axis in 0..3 {
        gradients[0][axis] = -gradients[1][axis] - gradients[2][axis] - gradients[3][axis];
    }
    Ok((determinant.abs() / 6.0, gradients))
}


/// Coordinates in physical three-dimensional space.
pub(crate) type V3 = [f64; 3];

pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(crate) fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}


#[derive(Clone, Copy)]
pub(crate) struct Geo {
    pub(crate) p0: V3,
    pub(crate) inv: [[f64; 3]; 3],
    pub(crate) j: [[f64; 3]; 3],
    pub(crate) det: f64,
}

pub(crate) fn geometry(points: [V3; 4]) -> Result<Geo, String> {
    let p0 = points[0];
    let mut j = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            j[r][c] = points[c + 1][r] - p0[r];
        }
    }
    let [a, b, c] = j[0];
    let [d, e, f] = j[1];
    let [g, h, i] = j[2];
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det == 0.0 {
        return Err("Tetrahedral mesh contains a degenerate or non-finite element.".into());
    }
    let inv = [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ];
    if inv.iter().flatten().any(|value| !value.is_finite()) {
        return Err("A tetrahedral coordinate transform exceeds the finite numeric range.".into());
    }
    Ok(Geo { p0, inv, j, det })
}
pub(crate) fn ref_point(g: Geo, p: V3) -> V3 {
    let d = sub(p, g.p0);
    [dot(g.inv[0], d), dot(g.inv[1], d), dot(g.inv[2], d)]
}
pub(crate) fn grad_lambda(g: Geo) -> [V3; 4] {
    let s = add(add(g.inv[0], g.inv[1]), g.inv[2]);
    [scale(s, -1.0), g.inv[0], g.inv[1], g.inv[2]]
}

pub(crate) fn points_for(tet: [usize; 4], nodes: &[V3]) -> [V3; 4] {
    std::array::from_fn(|i| nodes[tet[i]])
}
pub(crate) fn face_area(p: [V3; 3]) -> f64 {
    norm(cross(sub(p[1], p[0]), sub(p[2], p[0]))) / 2.0
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn p1_gradients_follow_vertex_order_and_reproduce_affine_fields() {
        let points = [[0.,0.,0.],[2.,0.,0.],[0.,3.,0.],[0.,0.,4.]];
        let (volume, gradients) = tetra_p1(points, DeterminantTolerance::Absolute(1e-30)).unwrap();
        assert!((volume - 4.0).abs() < 1e-12);
        for axis in 0..3 {
            let derivative: f64 = (0..4).map(|i| (2.0*points[i][0]-3.0*points[i][1]+4.0*points[i][2]+1.0)*gradients[i][axis]).sum();
            assert!((derivative - [2.,-3.,4.][axis]).abs() < 1e-12);
        }
        let reversed = [points[0], points[2], points[1], points[3]];
        let (other_volume, other_gradients) = tetra_p1(reversed, DeterminantTolerance::Relative(1e-14)).unwrap();
        assert_eq!(volume, other_volume);
        assert_eq!(gradients[1], other_gradients[2]);
        assert_eq!(gradients[2], other_gradients[1]);
    }
    #[test]
    fn rejects_degenerate_or_invalid_tolerance() {
        let points = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [1.,1.,0.]];
        assert!(tetra_p1(points, DeterminantTolerance::Relative(1e-14)).is_err());
        assert!(tetra_p1(points, DeterminantTolerance::Absolute(-1.0)).is_err());
    }

    #[test]
    fn affine_transform_retains_orientation_and_reference_coordinates() {
        let points = [[2., 4., 6.], [4., 4., 6.], [2., 7., 6.], [2., 4., 10.]];
        let transform = geometry(points).unwrap();
        assert!((transform.det - 24.0).abs() < 1e-12);
        let point = [3., 5.5, 8.];
        let reference = ref_point(transform, point);
        for axis in 0..3 { assert!((reference[axis] - 0.5).abs() < 1e-12); }
        let reversed = geometry([points[0], points[2], points[1], points[3]]).unwrap();
        assert!((reversed.det + 24.0).abs() < 1e-12);
        assert!(geometry([points[0], points[1], points[2], points[2]]).is_err());
    }
}
