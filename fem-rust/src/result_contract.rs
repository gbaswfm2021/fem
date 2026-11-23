//! Declarative serialization of prepared numerical results.
//!
//! The plan describes output keys, complex field/cell layouts and CSV columns;
//! a model adapter supplies already computed data. It does not choose physics,
//! evaluate a PDE, perform recovery, or write files.
use crate::C;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(crate) struct ResultData {
    pub(crate) values: HashMap<String, Value>,
    pub(crate) complex: HashMap<String, C>,
    pub(crate) series: HashMap<String, Vec<C>>,
    pub(crate) matrices: HashMap<String, Vec<Vec<C>>>,
}

impl ResultData {
    /// Pack a real scalar solution and per-cell vector/material samples for
    /// any declarative result document. Metadata and output labels come from
    /// the caller and the selected JSON result plan.
    pub(crate) fn scalar_samples(
        values: HashMap<String, Value>,
        primary: &[f64],
        recovered: &[[f64; 3]],
        cell_material: &[f64],
    ) -> Self {
        let real = |value| C { re: value, im: 0.0 };
        Self {
            values,
            complex: HashMap::new(),
            series: HashMap::from([
                (
                    "primary".into(),
                    primary.iter().copied().map(real).collect(),
                ),
                (
                    "cellMaterial".into(),
                    cell_material.iter().copied().map(real).collect(),
                ),
            ]),
            matrices: HashMap::from([(
                "recovered".into(),
                recovered
                    .iter()
                    .map(|row| row.iter().copied().map(real).collect())
                    .collect(),
            )]),
        }
    }
}

enum Expression {
    Literal(Value),
    Input(String),
    Object(Vec<(String, Expression)>),
    Series {
        input: String,
        imaginary: bool,
    },
    Matrix {
        input: String,
        imaginary: bool,
    },
    RepeatSeries {
        input: String,
        imaginary: bool,
        count: usize,
    },
    Field {
        input: String,
        unit: String,
        source: String,
    },
    CellMatrix {
        input: String,
        components: Vec<String>,
        unit: String,
    },
}

struct CsvColumn {
    header: String,
    input: String,
    part: CsvPart,
    format: CsvFormat,
}
enum CsvPart {
    Value,
    Real,
    Imaginary,
}
enum CsvFormat {
    Text,
    Integer,
    Fixed(usize),
    Scientific(usize),
}
pub(crate) struct ResultPlan {
    document: Expression,
    csv: Vec<CsvColumn>,
    csv_variables: Option<CsvVariables>,
    completion: Option<Vec<LogPart>>,
}

struct CsvVariables { real_suffix: String, imaginary_suffix: String, precision: usize }
pub(crate) struct CsvVariable { pub(crate) name: String, pub(crate) real_only: bool }

enum LogPart { Literal(String), Input { name: String, format: CsvFormat } }

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or_else(|| format!("Result declaration {name} must be a nonempty string."))
}

impl Expression {
    fn parse(value: &Value, depth: usize) -> Result<Self, String> {
        if depth > 32 {
            return Err("Result declaration is too deeply nested.".into());
        }
        let object = value
            .as_object()
            .filter(|o| o.len() == 1)
            .ok_or("Result expression requires exactly one operation.")?;
        let (operation, argument) = object.iter().next().unwrap();
        Ok(match operation.as_str() {
            "literal" => Self::Literal(argument.clone()),
            "input" => Self::Input(
                argument
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Result input name is required.")?
                    .to_string(),
            ),
            "object" => {
                let entries = argument
                    .as_object()
                    .ok_or("Result object must contain named expressions.")?;
                Self::Object(
                    entries
                        .iter()
                        .map(|(key, value)| Ok((key.clone(), Self::parse(value, depth + 1)?)))
                        .collect::<Result<_, String>>()?,
                )
            }
            "realSeries" | "imaginarySeries" => Self::Series {
                input: argument
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Result series name is required.")?
                    .into(),
                imaginary: operation == "imaginarySeries",
            },
            "realMatrix" | "imaginaryMatrix" => Self::Matrix {
                input: argument
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Result matrix name is required.")?
                    .into(),
                imaginary: operation == "imaginaryMatrix",
            },
            "repeatSeries" => {
                let imaginary = match string(argument, "part")? {
                    "real" => false,
                    "imaginary" => true,
                    _ => return Err("Repeated series part must be real or imaginary.".into()),
                };
                let count = argument
                    .get("count")
                    .and_then(Value::as_u64)
                    .filter(|&v| v > 0 && v <= 32)
                    .ok_or("Repeated series count must be between 1 and 32.")?
                    as usize;
                Self::RepeatSeries {
                    input: string(argument, "input")?.into(),
                    imaginary,
                    count,
                }
            }
            "field" => Self::Field {
                input: string(argument, "input")?.into(),
                unit: string(argument, "unit")?.into(),
                source: string(argument, "source")?.into(),
            },
            "cellMatrix" => {
                let components = argument
                    .get("components")
                    .and_then(Value::as_array)
                    .filter(|v| !v.is_empty())
                    .ok_or("Result cell matrix requires component labels.")?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .ok_or("Invalid result component label.".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Self::CellMatrix {
                    input: string(argument, "input")?.into(),
                    components,
                    unit: string(argument, "unit")?.into(),
                }
            }
            _ => return Err(format!("Unsupported result expression {operation:?}.")),
        })
    }

    fn evaluate(&self, data: &ResultData) -> Result<Value, String> {
        let series = |name: &str| {
            data.series
                .get(name)
                .ok_or_else(|| format!("Missing result series {name:?}."))
        };
        let finite = |value: C| {
            if value.re.is_finite() && value.im.is_finite() {
                Ok(value)
            } else {
                Err("Result contains a non-finite complex value.".to_string())
            }
        };
        Ok(match self {
            Self::Literal(value) => value.clone(),
            Self::Input(name) => data
                .values
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Missing result input {name:?}."))?,
            Self::Object(entries) => {
                let mut object = Map::new();
                for (key, expr) in entries {
                    object.insert(key.clone(), expr.evaluate(data)?);
                }
                Value::Object(object)
            }
            Self::Series { input, imaginary } => {
                let values = series(input)?
                    .iter()
                    .map(|&v| finite(v).map(|v| if *imaginary { v.im } else { v.re }))
                    .collect::<Result<Vec<_>, _>>()?;
                json!(values)
            }
            Self::Matrix { input, imaginary } => {
                let rows = data
                    .matrices
                    .get(input)
                    .ok_or_else(|| format!("Missing result matrix {input:?}."))?;
                let rows = rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|&v| finite(v).map(|v| if *imaginary { v.im } else { v.re }))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                json!(rows)
            }
            Self::RepeatSeries {
                input,
                imaginary,
                count,
            } => {
                let rows = series(input)?
                    .iter()
                    .map(|&v| finite(v).map(|v| vec![if *imaginary { v.im } else { v.re }; *count]))
                    .collect::<Result<Vec<_>, _>>()?;
                json!(rows)
            }
            Self::Field {
                input,
                unit,
                source,
            } => {
                let values = series(input)?;
                for &value in values {
                    finite(value)?;
                }
                crate::field_output::component_field(values, unit, source)
            }
            Self::CellMatrix {
                input,
                components,
                unit,
            } => {
                let rows = data
                    .matrices
                    .get(input)
                    .ok_or_else(|| format!("Missing result cell matrix {input:?}."))?;
                if rows.iter().any(|row| row.len() != components.len()) {
                    return Err(
                        "Result cell matrix row does not match its component labels.".into(),
                    );
                }
                let mut re = Vec::new();
                let mut im = Vec::new();
                for row in rows {
                    let mut r = Vec::new();
                    let mut i = Vec::new();
                    for &value in row {
                        let value = finite(value)?;
                        r.push(value.re);
                        i.push(value.im);
                    }
                    re.push(r);
                    im.push(i);
                }
                json!({"components":components,"real":re,"imag":im,"unit":unit})
            }
        })
    }
}

impl ResultPlan {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let document = Expression::parse(
            value
                .get("document")
                .ok_or("Result document declaration is required.")?,
            0,
        )?;
        let mut csv = Vec::new();
        if let Some(columns) = value.get("csv") {
            for column in columns
                .as_array()
                .ok_or("Result CSV must be a column array.")?
            {
                let header = string(column, "header")?.to_string();
                if csv.iter().any(|c: &CsvColumn| c.header == header) {
                    return Err("Result CSV column headers must be unique.".into());
                }
                let part = match column
                    .get("part")
                    .and_then(Value::as_str)
                    .unwrap_or("value")
                {
                    "value" => CsvPart::Value,
                    "real" => CsvPart::Real,
                    "imaginary" => CsvPart::Imaginary,
                    _ => return Err("Unsupported result CSV value part.".into()),
                };
                let precision =
                    column.get("precision").and_then(Value::as_u64).unwrap_or(6) as usize;
                if precision > 17 {
                    return Err("Result CSV precision exceeds 17 digits.".into());
                }
                let format = match string(column, "format")? {
                    "text" => CsvFormat::Text,
                    "integer" => CsvFormat::Integer,
                    "fixed" => CsvFormat::Fixed(precision),
                    "scientific" => CsvFormat::Scientific(precision),
                    _ => return Err("Unsupported result CSV format.".into()),
                };
                csv.push(CsvColumn {
                    header,
                    input: string(column, "input")?.into(),
                    part,
                    format,
                });
            }
        }
        let csv_variables = value.get("csvVariables").map(|v| {
            let precision = v.get("precision").and_then(Value::as_u64).ok_or("CSV variable precision is required.")? as usize;
            if precision > 17 { return Err("CSV variable precision exceeds 17 digits.".to_string()); }
            Ok(CsvVariables { real_suffix: string(v, "realSuffix")?.into(), imaginary_suffix: string(v, "imaginarySuffix")?.into(), precision })
        }).transpose()?;
        let completion = value.get("completion").map(|parts| {
            let parts = parts.as_array().filter(|p| !p.is_empty() && p.len() <= 128)
                .ok_or("Result completion must contain 1–128 parts.")?;
            parts.iter().map(|part| {
                let object = part.as_object().ok_or("Result completion part must be an object.")?;
                if let Some(literal) = object.get("literal") {
                    if object.len() != 1 { return Err("Literal completion part cannot contain other fields.".to_string()); }
                    return Ok(LogPart::Literal(literal.as_str().filter(|s| s.len() <= 4096)
                        .ok_or("Completion literal must be a string of at most 4096 bytes.")?.into()));
                }
                for key in object.keys() { if !["input","format","precision"].contains(&key.as_str()) { return Err(format!("Unsupported completion field {key:?}.")); } }
                let name = string(part,"input")?.to_string();
                let precision = match part.get("precision") {
                    None => 6,
                    Some(value) => value.as_u64().filter(|n| *n <= 17).ok_or("Completion precision must be an integer from 0 to 17.")? as usize,
                };
                let format = match string(part,"format")? {
                    "text" => CsvFormat::Text, "integer" => CsvFormat::Integer,
                    "fixed" => CsvFormat::Fixed(precision), "scientific" => CsvFormat::Scientific(precision),
                    _ => return Err("Unsupported completion input format.".into()),
                };
                Ok(LogPart::Input { name, format })
            }).collect::<Result<Vec<_>,String>>()
        }).transpose()?;
        Ok(Self { document, csv, csv_variables, completion })
    }

    pub(crate) fn document(&self, data: &ResultData) -> Result<Value, String> {
        self.document.evaluate(data)
    }

    /// Discard fields, mesh metadata and recovered matrices after their field
    /// artifact has been published, retaining only data needed to regenerate
    /// CSV rows after a sequence-wide variable-column decision.
    pub(crate) fn retain_csv_data(&self, data: &mut ResultData, variables: &[String]) -> Result<(), String> {
        Self::retain_csv_data_across(&[self], data, variables)
    }

    /// Retain the union of inputs required by several declared CSV plans,
    /// for example a sweep and a diagnostic artifact from the same case.
    pub(crate) fn retain_csv_data_across(plans: &[&Self], data: &mut ResultData, variables: &[String]) -> Result<(), String> {
        if plans.is_empty() { return Err("At least one CSV plan is required.".into()); }
        let mut value_names = std::collections::HashSet::new();
        let mut complex_names = std::collections::HashSet::new();
        for plan in plans {
            if !variables.is_empty() && plan.csv_variables.is_none() { return Err("This result plan does not declare variable CSV columns.".into()); }
            for column in &plan.csv {
                match column.part {
                    CsvPart::Value => { if !data.values.contains_key(&column.input) { return Err(format!("Missing CSV input {:?}.", column.input)); } value_names.insert(column.input.as_str()); },
                    CsvPart::Real | CsvPart::Imaginary => { if !data.complex.contains_key(&column.input) { return Err(format!("Missing complex CSV input {:?}.", column.input)); } complex_names.insert(column.input.as_str()); },
                }
            }
        }
        for name in variables {
            if !data.complex.contains_key(name) { return Err(format!("Missing CSV variable {name:?}.")); }
            complex_names.insert(name.as_str());
        }
        data.values.retain(|name, _| value_names.contains(name.as_str()));
        data.complex.retain(|name, _| complex_names.contains(name.as_str()));
        data.series = HashMap::new();
        data.matrices = HashMap::new();
        Ok(())
    }

    /// Render an optional completion line from declared literals and typed
    /// result values. This operation does not print or modify output files.
    pub(crate) fn completion(&self, data: &ResultData) -> Result<Option<String>, String> {
        let Some(parts) = &self.completion else { return Ok(None); };
        let mut out = String::new();
        for part in parts {
            match part {
                LogPart::Literal(text) => out.push_str(text),
                LogPart::Input { name, format } => {
                    let value = data.values.get(name).ok_or_else(|| format!("Missing completion input {name:?}."))?;
                    let rendered = match format {
                        CsvFormat::Text => value.as_str().ok_or("Completion text input must be a string.")?.to_owned(),
                        CsvFormat::Integer => value.as_i64().map(|v| v.to_string()).or_else(|| value.as_u64().map(|v| v.to_string())).ok_or("Completion integer input must be an integer.")?,
                        CsvFormat::Fixed(precision) | CsvFormat::Scientific(precision) => {
                            let number = value.as_f64().filter(|n| n.is_finite()).ok_or("Completion numeric input must be finite.")?;
                            if matches!(format, CsvFormat::Fixed(_)) { format!("{:.*}",*precision,number) } else { format!("{:.*e}",*precision,number) }
                        }
                    };
                    out.push_str(&rendered);
                }
            }
        }
        Ok(Some(out))
    }

    /// Serialize any number of prepared rows using the declared column order.
    pub(crate) fn csv(&self, rows: &[ResultData]) -> Result<String, String> {
        self.csv_with_variables(rows, &[])
    }

    pub(crate) fn csv_with_variables(&self, rows: &[ResultData], variables: &[CsvVariable]) -> Result<String, String> {
        fn escape(text: &str) -> String {
            if text.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
                format!("\"{}\"", text.replace('"', "\"\""))
            } else {
                text.to_string()
            }
        }
        let mut out = self
            .csv
            .iter()
            .map(|column| escape(&column.header))
            .collect::<Vec<_>>()
            .join(",");
        if !variables.is_empty() && self.csv_variables.is_none() { return Err("This result plan does not declare variable CSV columns.".into()); }
        let mut headers = self.csv.iter().map(|c| c.header.clone()).collect::<std::collections::HashSet<_>>();
        if let Some(policy) = &self.csv_variables {
            for variable in variables {
                if variable.name.is_empty() { return Err("CSV variable name is empty.".into()); }
                let names = if variable.real_only { vec![variable.name.clone()] } else { vec![format!("{}{}", variable.name, policy.real_suffix), format!("{}{}", variable.name, policy.imaginary_suffix)] };
                for name in names {
                    if !headers.insert(name.clone()) { return Err(format!("Duplicate CSV column {name:?}.")); }
                    out.push(','); out.push_str(&escape(&name));
                }
            }
        }
        out.push('\n');
        for row in rows {
            let mut fields = Vec::new();
            for column in &self.csv {
                let value = match column.part {
                    CsvPart::Value => row
                        .values
                        .get(&column.input)
                        .cloned()
                        .ok_or_else(|| format!("Missing CSV input {:?}.", column.input))?,
                    CsvPart::Real | CsvPart::Imaginary => {
                        let c = *row.complex.get(&column.input).ok_or_else(|| {
                            format!("Missing complex CSV input {:?}.", column.input)
                        })?;
                        let number = if matches!(column.part, CsvPart::Real) {
                            c.re
                        } else {
                            c.im
                        };
                        if !number.is_finite() {
                            return Err("CSV input is not finite.".into());
                        }
                        json!(number)
                    }
                };
                let field = match column.format {
                    CsvFormat::Text => {
                        escape(value.as_str().ok_or("CSV text input must be a string.")?)
                    }
                    CsvFormat::Integer => value
                        .as_i64()
                        .map(|n| n.to_string())
                        .or_else(|| value.as_u64().map(|n| n.to_string()))
                        .ok_or("CSV integer input must be an integer.")?,
                    CsvFormat::Fixed(precision) => format!(
                        "{:.*}",
                        precision,
                        value
                            .as_f64()
                            .ok_or("CSV numeric input must be a number.")?
                    ),
                    CsvFormat::Scientific(precision) => format!(
                        "{:.*e}",
                        precision,
                        value
                            .as_f64()
                            .ok_or("CSV numeric input must be a number.")?
                    ),
                };
                fields.push(field);
            }
            if let Some(policy) = &self.csv_variables {
                for variable in variables {
                    let value = row.complex.get(&variable.name).ok_or_else(|| format!("Missing CSV variable {:?}.", variable.name))?;
                    if !value.re.is_finite() || !value.im.is_finite() { return Err("CSV variable is not finite.".into()); }
                    fields.push(format!("{:.*e}", policy.precision, value.re));
                    if !variable.real_only { fields.push(format!("{:.*e}", policy.precision, value.im)); }
                }
            }
            out.push_str(&fields.join(","));
            out.push('\n');
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_2d_result_plan_exposes_per_depth_measures() {
        let package: Value = serde_json::from_str(include_str!("scenarios/thermal2d.json")).unwrap();
        let plan = ResultPlan::from_json(&package["resultPlans"]["T"]).unwrap();
        let data = ResultData {
            values: HashMap::from([
                ("studyId".into(), json!("s")), ("nodes".into(), json!([[0,0,0],[1,0,0],[0,1,0]])),
                ("cells".into(), json!([{ "nodes":[0,1,2], "materialTag":1 }])),
                ("minimum".into(), json!(300.)), ("maximum".into(), json!(301.)),
                ("backend".into(), json!("faer")), ("ordering".into(), json!("auto")),
                ("threads".into(), json!(1)), ("quadrature".into(), json!("default")),
                ("residual".into(), json!(0.)), ("unknowns".into(), json!(3)),
                ("meanConstraints".into(), json!(0)), ("meanValue".into(), json!(0.)),
                ("systemUnknowns".into(), json!(3)), ("volume".into(), json!(0.5)),
                ("boundaryArea".into(), json!({"1":2.})),
                ("boundaryPower".into(), json!({"1":-4.})),
                ("netBoundaryPower".into(), json!(-4.)),
            ]),
            complex: HashMap::new(),
            series: HashMap::from([
                ("primary".into(), vec![C {re:300., im:0.}; 3]),
                ("cellMaterial".into(), vec![C {re:1., im:0.}]),
            ]),
            matrices: HashMap::from([("recovered".into(), vec![vec![C {re:0., im:0.}; 2]])]),
        };
        let document = plan.document(&data).unwrap();
        assert_eq!(document["areaM2"], json!(0.5));
        assert_eq!(document["boundaryLengthMByTag"]["1"], json!(2.));
        assert_eq!(document["boundaryOutwardPowerWPerMByTag"]["1"], json!(-4.));
        assert_eq!(document["netBoundaryOutwardPowerWPerM"], json!(-4.));
        assert_eq!(document["schema"], "fullmeta-thermal-field/1");
        assert_eq!(document["temperatureK"], json!([300., 300., 300.]));
    }

    #[test]
    fn declared_vector_and_cell_outputs_preserve_component_order_and_metadata() {
        let plan = ResultPlan::from_json(&json!({
            "document":{"object":{
                "schema":{"literal":"example-field/1"},
                "dimension":{"input":"dimension"},
                "nodes":{"input":"nodes"},
                "fields":{"object":{
                    "ux":{"field":{"input":"ux","unit":"m","source":"solved"}},
                    "uy":{"field":{"input":"uy","unit":"m","source":"solved"}}
                }},
                "stressByCell":{"cellMatrix":{"input":"stress","unit":"Pa","components":["xx","yy","xy"]}},
                "solver":{"input":"solver"}
            }},
            "csv":[{"header":"frequency_Hz","input":"frequency","format":"fixed","precision":2},
                   {"header":"unknowns","input":"unknowns","format":"integer"},
                   {"header":"residual","input":"residual","format":"scientific","precision":3},
                   {"header":"response_imag","input":"response","part":"imaginary","format":"fixed","precision":2},
                   {"header":"label","input":"label","format":"text"}]
        })).unwrap();
        let data = ResultData {
            values: HashMap::from([
                ("dimension".into(), json!(2)),
                ("nodes".into(), json!([[0, 0], [1, 0], [0, 1]])),
                ("solver".into(), json!({"backend":"faer","unknowns":6})),
                ("frequency".into(), json!(123.5)),
                ("unknowns".into(), json!(6)),
                ("residual".into(), json!(0.00012)),
                ("label".into(), json!("a,\"b\"")),
            ]),
            complex: HashMap::from([("response".into(), C { re: 1., im: -2.5 })]),
            series: HashMap::from([
                (
                    "ux".into(),
                    vec![C { re: 1., im: 2. }, C { re: 3., im: 4. }],
                ),
                (
                    "uy".into(),
                    vec![C { re: 5., im: 6. }, C { re: 7., im: 8. }],
                ),
            ]),
            matrices: HashMap::from([(
                "stress".into(),
                vec![vec![
                    C { re: 1., im: 2. },
                    C { re: 3., im: 4. },
                    C { re: 5., im: 6. },
                ]],
            )]),
        };
        let document = plan.document(&data).unwrap();
        assert_eq!(document["fields"]["ux"]["imag"], json!([2., 4.]));
        assert_eq!(document["fields"]["uy"]["unit"], "m");
        assert_eq!(
            document["stressByCell"]["components"],
            json!(["xx", "yy", "xy"])
        );
        assert_eq!(document["stressByCell"]["real"], json!([[1., 3., 5.]]));
        assert_eq!(document["stressByCell"]["imag"], json!([[2., 4., 6.]]));
        assert_eq!(document["solver"]["backend"], "faer");
        assert_eq!(
            plan.csv(&[data]).unwrap(),
            "frequency_Hz,unknowns,residual,response_imag,label\n123.50,6,1.200e-4,-2.50,\"a,\"\"b\"\"\"\n"
        );
    }

    #[test]
    fn declared_scalar_arrays_and_invalid_cell_or_nonfinite_data() {
        let plan = ResultPlan::from_json(&json!({"document":{"object":{
            "temperature":{"realSeries":"T"},"imaginary":{"imaginarySeries":"T"},
            "flux":{"cellMatrix":{"input":"q","components":["x","y","z"],"unit":"W/m^2"}}
        }}}))
        .unwrap();
        let mut data = ResultData {
            values: HashMap::new(),
            complex: HashMap::new(),
            series: HashMap::from([(
                "T".into(),
                vec![C { re: 300., im: 0. }, C { re: 310., im: 0. }],
            )]),
            matrices: HashMap::from([("q".into(), vec![vec![C::default(); 3]])]),
        };
        assert_eq!(
            plan.document(&data).unwrap()["temperature"],
            json!([300., 310.])
        );
        data.matrices
            .insert("q".into(), vec![vec![C::default(); 2]]);
        assert!(plan.document(&data).is_err());
        data.matrices
            .insert("q".into(), vec![vec![C::default(); 3]]);
        data.series.get_mut("T").unwrap()[0].re = f64::NAN;
        assert!(plan.document(&data).is_err());
    }

    #[test]
    fn csv_compaction_preserves_fixed_and_dynamic_columns_for_multiple_cases() {
        let plan = ResultPlan::from_json(&json!({"document":{"literal":{}},
            "csv":[{"header":"frequency","input":"frequency","format":"fixed","precision":1},
                   {"header":"probe","input":"probe","part":"real","format":"scientific","precision":2}],
            "csvVariables":{"realSuffix":"_re","imaginarySuffix":"_im","precision":3}})).unwrap();
        let row = |frequency, imaginary| ResultData {
            values:HashMap::from([("frequency".into(),json!(frequency)),("mesh".into(),json!([1,2,3]))]),
            complex:HashMap::from([("probe".into(),C { re:2.,im:0. }), ("response".into(),C { re:3.,im:imaginary }),
                ("power".into(),C { re:4.,im:0. }), ("unused".into(),C::default())]),
            series:HashMap::from([("field".into(),vec![C::default();100])]),
            matrices:HashMap::from([("cell".into(),vec![vec![C::default();9];30])]),
        };
        let mut rows = vec![row(1.,0.),row(2.,1.)];
        let variables = [CsvVariable { name:"response".into(),real_only:false }, CsvVariable { name:"power".into(),real_only:true }];
        let before = plan.csv_with_variables(&rows,&variables).unwrap();
        for row in &mut rows {
            plan.retain_csv_data(row,&["response".into(),"power".into()]).unwrap();
            assert!(row.series.is_empty() && row.matrices.is_empty());
            assert!(!row.values.contains_key("mesh") && !row.complex.contains_key("unused"));
        }
        assert_eq!(plan.csv_with_variables(&rows,&variables).unwrap(),before);
        assert!(plan.retain_csv_data(&mut rows[0],&["missing".into()]).is_err());
    }

    

    

    

    
}
