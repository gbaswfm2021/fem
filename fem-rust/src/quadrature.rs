//! Simplex quadrature with weights normalized to the physical cell measure.
//!
//! These rules describe integration geometry only; formulation-specific
//! restrictions on allowed rules belong in the scenario contract.

pub(crate) fn triangle(rule: &str) -> Result<Vec<[f64; 4]>, String> {
    match rule {
        "standard" => Ok(crate::TRI_Q.to_vec()),
        "centroid" => Ok(vec![[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 1.0]]),
        _ => Err(format!(
            "Unknown constitutive quadrature rule {rule:?}; use 'standard' or 'centroid'."
        )),
    }
}

pub(crate) fn tetrahedron(rule: &str) -> Result<Vec<[f64; 5]>, String> {
    match rule {
        "standard" => Ok(crate::TET_Q
            .iter()
            .map(|q| [q[0], q[1], q[2], q[3], q[4] * 6.0])
            .collect()),
        "centroid" => Ok(vec![[0.25, 0.25, 0.25, 0.25, 1.0]]),
        _ => Err(format!(
            "Unknown constitutive quadrature rule {rule:?}; use 'standard' or 'centroid'."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_rules_integrate_constant_and_linear_barycentric_fields() {
        for rule in ["standard", "centroid"] {
            let triangle = triangle(rule).unwrap();
            assert!((triangle.iter().map(|q| q[3]).sum::<f64>() - 1.0).abs() < 1e-13);
            for axis in 0..3 {
                assert!(
                    (triangle.iter().map(|q| q[axis] * q[3]).sum::<f64>() - 1.0 / 3.0).abs()
                        < 1e-13
                );
            }
            let tetrahedron = tetrahedron(rule).unwrap();
            assert!((tetrahedron.iter().map(|q| q[4]).sum::<f64>() - 1.0).abs() < 1e-13);
            for axis in 0..4 {
                assert!(
                    (tetrahedron.iter().map(|q| q[axis] * q[4]).sum::<f64>() - 0.25).abs() < 1e-13
                );
            }
        }
        assert!(triangle("unknown").is_err());
        assert!(tetrahedron("unknown").is_err());
    }
}
