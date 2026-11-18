//! Declarative dense local coefficient matrices for typed form contractions.
//! These matrices describe material tensors, not the sparse global system.
use crate::C;
use serde_json::Value;
use std::collections::HashMap;

enum Entry {
    Constant(f64),
    Symbol(String),
}

pub(crate) struct CoefficientMatrixPlan {
    rows: usize,
    columns: usize,
    entries: Vec<Entry>,
}

impl CoefficientMatrixPlan {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let rows = value
            .get("values")
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty() && rows.len() <= 9)
            .ok_or("Coefficient matrix values must contain 1–9 rows.")?;
        let columns = rows[0]
            .as_array()
            .filter(|row| !row.is_empty() && row.len() <= 9)
            .ok_or("Coefficient matrix must contain 1–9 columns.")?
            .len();
        let mut entries = Vec::with_capacity(rows.len() * columns);
        for (i, row) in rows.iter().enumerate() {
            let row = row
                .as_array()
                .filter(|row| row.len() == columns)
                .ok_or_else(|| format!("Coefficient matrix row {i} has inconsistent length."))?;
            for (j, entry) in row.iter().enumerate() {
                if let Some(value) = entry.as_f64().filter(|value| value.is_finite()) {
                    entries.push(Entry::Constant(value));
                } else if let Some(value) = entry
                    .as_str()
                    .filter(|name| !name.is_empty() && name.len() <= 128)
                {
                    entries.push(Entry::Symbol(value.to_string()));
                } else {
                    return Err(format!(
                        "Coefficient matrix [{i}][{j}] must be a finite number or coefficient name."
                    ));
                }
            }
        }
        Ok(Self {
            rows: rows.len(),
            columns,
            entries,
        })
    }

    pub(crate) fn shape(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }

    pub(crate) fn symbols(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|entry| match entry {
            Entry::Symbol(name) => Some(name.as_str()),
            Entry::Constant(_) => None,
        })
    }

    pub(crate) fn evaluate(&self, coefficients: &HashMap<String, C>) -> Result<Vec<C>, String> {
        self.entries
            .iter()
            .map(|entry| {
                let value = match entry {
                    Entry::Constant(value) => C {
                        re: *value,
                        im: 0.0,
                    },
                    Entry::Symbol(name) => *coefficients.get(name).ok_or_else(|| {
                        format!("Coefficient matrix references missing coefficient {name:?}.")
                    })?,
                };
                if !value.re.is_finite() || !value.im.is_finite() {
                    return Err("Coefficient matrix contains a non-finite value.".into());
                }
                Ok(value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn evaluates_complex_entries_and_checks_shape_and_symbols() {
        let matrix =
            CoefficientMatrixPlan::from_json(&json!({"values":[["a",0],[1,"b"]]})).unwrap();
        assert_eq!(matrix.shape(), (2, 2));
        let values = matrix
            .evaluate(&HashMap::from([
                ("a".into(), C { re: 2., im: 3. }),
                ("b".into(), C { re: -1., im: 1. }),
            ]))
            .unwrap();
        assert_eq!(
            (values[0].re, values[0].im, values[2].re, values[3].im),
            (2., 3., 1., 1.)
        );
        assert!(matrix.evaluate(&HashMap::new()).is_err());
        assert!(CoefficientMatrixPlan::from_json(&json!({"values":[[1,2],[3]]})).is_err());
    }

    
}
