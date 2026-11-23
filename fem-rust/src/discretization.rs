//! Validated, scenario-neutral finite-element selection from JSON metadata.
//!
//! This describes the current simplex element vocabulary; choosing DOFs and
//! evaluating basis functions remains the responsibility of the element driver.
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cell {
    Triangle,
    Tetrahedron,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Lagrange,
    Nedelec,
    NedelecH1DirectSum,
}

pub(crate) struct Discretization {
    cell: Cell,
    family: Family,
    components: usize,
    orders: Vec<usize>,
}

impl Discretization {
    pub(crate) fn dimension(&self) -> usize {
        match self.cell {
            Cell::Triangle => 2,
            Cell::Tetrahedron => 3,
        }
    }

    pub(crate) fn components(&self) -> usize {
        self.components
    }

    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let cell = match value.get("cell").and_then(Value::as_str) {
            Some("triangle") => Cell::Triangle,
            Some("tetrahedron") => Cell::Tetrahedron,
            _ => return Err("Discretization cell must be triangle or tetrahedron.".into()),
        };
        let family = match value.get("family").and_then(Value::as_str) {
            Some("lagrange") => Family::Lagrange,
            Some("nedelec") => Family::Nedelec,
            Some("nedelecH1DirectSum") => Family::NedelecH1DirectSum,
            _ => return Err("Unsupported finite-element family.".into()),
        };
        let components = value
            .get("fieldComponents")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=3).contains(value))
            .ok_or("Discretization fieldComponents must be 1, 2 or 3.")?;
        let orders = value
            .get("orders")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty())
            .ok_or("Discretization orders must be a nonempty array.")?
            .iter()
            .map(|order| {
                order
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| {
                        "Discretization order must be a nonnegative integer.".to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut unique = orders.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != orders.len() {
            return Err("Discretization orders must be unique.".into());
        }
        let supported = match (&cell, &family) {
            (Cell::Triangle, Family::Lagrange) => orders.iter().all(|&order| order == 1),
            (Cell::Tetrahedron, Family::Lagrange) => orders.iter().all(|&order| order == 1),
            (Cell::Tetrahedron, Family::Nedelec) => {
                components == 3 && orders.iter().all(|&order| order == 1 || order == 2)
            }
            (Cell::Triangle, Family::NedelecH1DirectSum) => {
                components == 3 && orders.iter().all(|&order| order == 1)
            }
            _ => false,
        };
        if !supported {
            return Err("Unsupported cell, family, field shape or element order.".into());
        }
        Ok(Self {
            cell,
            family,
            components,
            orders,
        })
    }

    pub(crate) fn require(
        &self,
        cell: Cell,
        family: Family,
        components: usize,
        order: usize,
    ) -> Result<(), String> {
        if self.cell != cell
            || self.family != family
            || self.components != components
            || !self.orders.contains(&order)
        {
            return Err(
                "Selected element is incompatible with the scenario discretization declaration."
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn accepts_supported_fields_and_rejects_incompatible_order() {
        let plan = Discretization::from_json(
            &json!({"cell":"tetrahedron","family":"nedelec","fieldComponents":3,"orders":[1,2]}),
        )
        .unwrap();
        assert!(
            plan.require(Cell::Tetrahedron, Family::Nedelec, 3, 2)
                .is_ok()
        );
        assert!(
            plan.require(Cell::Tetrahedron, Family::Nedelec, 3, 3)
                .is_err()
        );
        assert!(
            plan.require(Cell::Tetrahedron, Family::Lagrange, 3, 1)
                .is_err()
        );
    }
    #[test]
    fn rejects_unsupported_or_duplicate_combinations() {
        assert!(
            Discretization::from_json(
                &json!({"cell":"triangle","family":"nedelec","fieldComponents":2,"orders":[1]})
            )
            .is_err()
        );
        assert!(
            Discretization::from_json(
                &json!({"cell":"triangle","family":"lagrange","fieldComponents":1,"orders":[1,1]})
            )
            .is_err()
        );
    }
}
