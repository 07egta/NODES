//! Bounded, unit-aware named dimensions independent of the graph evaluator.
//! Expressions reuse the existing arithmetic parser; they cannot execute scripts.
//! A table is applied to existing Number nodes as one validated graph edit.

use crate::expr::{self, Expr, Func, Op};
use crate::model::{Graph, NodeId, NodeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PARAMETERS: usize = 256;
pub const MAX_BINDINGS: usize = 1024;
pub const MAX_TABLE_BYTES: usize = 256 * 1024;
const MAX_NAME: usize = 64;
const CANCELLED: &str = "パラメーターの計算を中止しました。";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Scalar,
    Mm,
    Cm,
    M,
    Inch,
    Rad,
    Deg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dimension(i16, i16);

const SCALAR: Dimension = Dimension(0, 0);
const LENGTH: Dimension = Dimension(1, 0);
const ANGLE: Dimension = Dimension(0, 1);

impl Dimension {
    fn checked(self) -> Result<Self, String> {
        if self.0.abs() <= 32 && self.1.abs() <= 32 {
            Ok(self)
        } else {
            Err("式の単位の指数が大きすぎます。".into())
        }
    }
}

impl Unit {
    fn dimension(self) -> Dimension {
        match self {
            Self::Scalar => SCALAR,
            Self::Mm | Self::Cm | Self::M | Self::Inch => LENGTH,
            Self::Rad | Self::Deg => ANGLE,
        }
    }

    fn scale(self) -> f64 {
        match self {
            Self::Scalar | Self::Mm | Self::Rad => 1.0,
            Self::Cm => 10.0,
            Self::M => 1000.0,
            Self::Inch => 25.4,
            Self::Deg => std::f64::consts::PI / 180.0,
        }
    }

    fn canonical(self, value: f64) -> Result<f64, String> {
        checked_conversion(value, value * self.scale())
    }

    fn display(self, canonical: f64) -> Result<f64, String> {
        checked_conversion(canonical, canonical / self.scale())
    }
}

fn checked_conversion(before: f64, after: f64) -> Result<f64, String> {
    if !before.is_finite() || !after.is_finite() || (before != 0.0 && after == 0.0) {
        return Err("単位変換で数値の表現範囲を超えました。".into());
    }
    Ok(after)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Literal { value: f64 },
    Formula { expression: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub name: String,
    pub unit: Unit,
    pub source: Source,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub public: bool,
}

/// Bind a parameter to an unconnected Number node, with a name check to catch
/// accidental reuse of a deleted node ID. The node receives a number in unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub parameter: String,
    pub node_id: NodeId,
    pub node_name: String,
    pub unit: Unit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterTable {
    pub version: u32,
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedParameter {
    pub unit: Unit,
    /// Value in the parameter's declared unit.
    pub value: f64,
    /// All lengths in mm and angles in radians.
    canonical: f64,
}

impl ResolvedParameter {
    pub fn in_unit(self, unit: Unit) -> Result<f64, String> {
        if self.unit.dimension() != unit.dimension() {
            return Err("異なる種類の単位には変換できません。".into());
        }
        if self.unit == unit {
            Ok(self.value)
        } else {
            unit.display(self.canonical)
        }
    }
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= MAX_NAME
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn has_variable(expression: &Expr) -> bool {
    match expression {
        Expr::Num(_) => false,
        Expr::Var(_) => true,
        Expr::Neg(inner) => has_variable(inner),
        Expr::Bin(_, left, right) => has_variable(left) || has_variable(right),
        Expr::Call(_, args) => args.iter().any(has_variable),
    }
}

/// Parse a constant expression for an existing numeric port, in its native
/// display unit. Bare numbers and unitless expressions use that unit. Explicit
/// units must resolve to the target dimension; scalar results from unit-bearing
/// expressions are accepted only for scalar ports. No graph references or scripts.
pub fn parse_numeric_input(text: &str, target: Unit) -> Result<f64, String> {
    if text.len() > expr::MAX_EXPR_LEN * 4 || text.chars().count() > expr::MAX_EXPR_LEN {
        return Err("数値の式が長すぎます。".into());
    }
    // Preserve binary64 round trips and the sign bit of zero.
    let direct = text.trim().replace('−', "-");
    if let Ok(value) = direct.parse::<f64>() {
        return if value.is_finite() {
            Ok(value)
        } else {
            Err("有限の数値を入力してください。".into())
        };
    }
    let expression = expr::parse_with_literal_units(text, |name| {
        let unit = match name {
            "mm" => Unit::Mm,
            "cm" => Unit::Cm,
            "m" => Unit::M,
            "in" | "inch" => Unit::Inch,
            "rad" => Unit::Rad,
            "deg" => Unit::Deg,
            _ => return None,
        };
        let index = if unit.dimension() == LENGTH { 0 } else { 1 };
        Some((index, unit.scale()))
    })?;
    if !has_variable(&expression) {
        return expression.eval([0.0; 4]);
    }
    let actual = expression_dimension(&expression, &[LENGTH, ANGLE])?;
    same_dimension(actual, target.dimension())?;
    target.display(expression.eval([1.0, 1.0, 0.0, 0.0])?)
}

struct Formula {
    expression: Expr,
    references: Vec<String>,
}

/// A dollar-prefixed name is explicit, so it cannot collide with pi, e,
/// existing function names or scientific notation. Four distinct names per
/// expression share the existing parser's four variable slots.
fn compile_formula(source: &str) -> Result<Formula, String> {
    if source.chars().count() > expr::MAX_EXPR_LEN {
        return Err("パラメーターの式が長すぎます。".into());
    }
    let mut chars = source.chars().peekable();
    let mut references = Vec::<String>::new();
    let mut substituted = String::new();
    let mut masked = String::new();
    while let Some(c) = chars.next() {
        if c != '$' {
            substituted.push(c);
            masked.push(c);
            continue;
        }
        let mut name = String::new();
        while chars
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
        {
            if let Some(c) = chars.next() {
                name.push(c);
            }
        }
        if !valid_name(&name) {
            return Err("参照名は $width のように指定してください。".into());
        }
        let index = match references.iter().position(|r| r == &name) {
            Some(index) => index,
            None if references.len() < 4 => {
                references.push(name);
                references.len() - 1
            }
            None => return Err("1つの式で参照できる名前は4種類までです。".into()),
        };
        substituted.push_str([" x ", " y ", " z ", " w "][index]);
        masked.push_str(" 0 ");
    }
    // Reject direct x/y/z/w, including full-width and upper-case spellings,
    // rather than accidentally binding a raw variable to a named parameter.
    if has_variable(&expr::parse(&masked)?) {
        return Err("名前付きの参照には $ を付けてください。".into());
    }
    Ok(Formula {
        expression: expr::parse(&substituted)?,
        references,
    })
}

fn same_dimension(left: Dimension, right: Dimension) -> Result<Dimension, String> {
    if left == right {
        Ok(left)
    } else {
        Err("式の単位が一致していません。".into())
    }
}

fn literal_exponent(expression: &Expr) -> Option<i16> {
    let value = match expression {
        Expr::Num(value) => *value,
        Expr::Neg(inner) => return literal_exponent(inner).map(|value| -value),
        _ => return None,
    };
    if value.fract() == 0.0 && (-16.0..=16.0).contains(&value) {
        Some(value as i16)
    } else {
        None
    }
}

fn expression_dimension(expression: &Expr, vars: &[Dimension]) -> Result<Dimension, String> {
    match expression {
        Expr::Num(_) => Ok(SCALAR),
        Expr::Var(index) => vars.get(*index).copied().ok_or("不明な参照です。".into()),
        Expr::Neg(inner) => expression_dimension(inner, vars),
        Expr::Bin(op, left, right) => {
            let a = expression_dimension(left, vars)?;
            let b = expression_dimension(right, vars)?;
            match op {
                Op::Add | Op::Sub | Op::Rem => same_dimension(a, b),
                Op::Mul => Dimension(a.0 + b.0, a.1 + b.1).checked(),
                Op::Div => Dimension(a.0 - b.0, a.1 - b.1).checked(),
                Op::Pow => {
                    same_dimension(b, SCALAR)?;
                    if a == SCALAR {
                        return Ok(SCALAR);
                    }
                    let exponent = literal_exponent(right)
                        .ok_or("単位付きのべき乗には -16〜16 の整数定数を指定してください。")?;
                    Dimension(a.0 * exponent, a.1 * exponent).checked()
                }
            }
        }
        Expr::Call(func, args) => {
            let first = args.first().ok_or("関数の引数がありません。")?;
            let a = expression_dimension(first, vars)?;
            if matches!(func, Func::Abs) {
                Ok(a)
            } else if matches!(func, Func::Min | Func::Max) {
                let second = args.get(1).ok_or("関数の引数が足りません。")?;
                same_dimension(a, expression_dimension(second, vars)?)
            } else if matches!(func, Func::Sqrt) {
                if a.0 % 2 == 0 && a.1 % 2 == 0 {
                    Ok(Dimension(a.0 / 2, a.1 / 2))
                } else {
                    Err("平方根で単位の指数を半分にできません。".into())
                }
            } else if matches!(func, Func::Sin | Func::Cos | Func::Tan) {
                if a == ANGLE || a == SCALAR {
                    Ok(SCALAR)
                } else {
                    Err("三角関数には角度か無次元数（ラジアン）を指定してください。".into())
                }
            } else if matches!(
                func,
                Func::Floor | Func::Ceil | Func::Round | Func::Ln | Func::Exp
            ) {
                same_dimension(a, SCALAR)
            } else {
                // Future expression functions require an explicit dimensional contract.
                Err("この関数は単位付きパラメーターの式では未対応です。".into())
            }
        }
    }
}

impl ParameterTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_TABLE_BYTES {
            return Err("パラメーターファイルが大きすぎます。".into());
        }
        let table: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        table.check_limits()?;
        Ok(table)
    }

    fn check_limits(&self) -> Result<(), String> {
        if self.version != 1
            || self.parameters.len() > MAX_PARAMETERS
            || self.bindings.len() > MAX_BINDINGS
        {
            return Err("パラメーター形式または件数の上限が無効です。".into());
        }
        Ok(())
    }

    /// Deterministic topological evaluation, with a strict size bound and
    /// cancellation between formulas. Graph topology is not changed.
    pub fn evaluate(
        &self,
        cancel: &dyn Fn() -> bool,
    ) -> Result<BTreeMap<String, ResolvedParameter>, String> {
        self.check_limits()?;
        let mut definitions = BTreeMap::new();
        let mut formulas = BTreeMap::new();
        for p in &self.parameters {
            if cancel() {
                return Err(CANCELLED.into());
            }
            if !valid_name(&p.name) || definitions.insert(p.name.as_str(), p).is_some() {
                return Err(format!("無効または重複した名前: {}", p.name));
            }
            if p.min.is_some_and(|v| !v.is_finite())
                || p.max.is_some_and(|v| !v.is_finite())
                || p.min.zip(p.max).is_some_and(|(min, max)| min > max)
            {
                return Err(format!("{}: 許容範囲が無効です。", p.name));
            }
            if let Source::Formula { expression } = &p.source {
                let formula =
                    compile_formula(expression).map_err(|e| format!("{}: {e}", p.name))?;
                formulas.insert(p.name.as_str(), formula);
            }
        }
        for (name, formula) in &formulas {
            for reference in &formula.references {
                if !definitions.contains_key(reference.as_str()) {
                    return Err(format!("{name}: 参照先 {reference} が見つかりません。"));
                }
            }
        }

        let mut result = BTreeMap::<String, ResolvedParameter>::new();
        while result.len() < definitions.len() {
            let previous = result.len();
            for (&name, &p) in &definitions {
                if cancel() {
                    return Err(CANCELLED.into());
                }
                if result.contains_key(name) {
                    continue;
                }
                let value = match &p.source {
                    Source::Literal { value } => ResolvedParameter {
                        unit: p.unit,
                        value: *value,
                        canonical: p.unit.canonical(*value)?,
                    },
                    Source::Formula { .. } => {
                        let formula = formulas.get(name).ok_or("式が見つかりません。")?;
                        if !formula.references.iter().all(|r| result.contains_key(r)) {
                            continue;
                        }
                        let mut variables = [0.0; 4];
                        let mut dimensions = Vec::new();
                        for (index, reference) in formula.references.iter().enumerate() {
                            let resolved = result.get(reference).ok_or("参照が見つかりません。")?;
                            variables[index] = resolved.canonical;
                            dimensions.push(resolved.unit.dimension());
                        }
                        let actual = expression_dimension(&formula.expression, &dimensions)
                            .map_err(|e| format!("{name}: {e}"))?;
                        same_dimension(actual, p.unit.dimension())
                            .map_err(|e| format!("{name}: {e}"))?;
                        let canonical = formula
                            .expression
                            .eval(variables)
                            .map_err(|e| format!("{name}: {e}"))?;
                        ResolvedParameter {
                            unit: p.unit,
                            value: p.unit.display(canonical)?,
                            canonical,
                        }
                    }
                };
                if p.min.is_some_and(|min| value.value < min)
                    || p.max.is_some_and(|max| value.value > max)
                {
                    return Err(format!("{name}: 値が許容範囲外です。"));
                }
                result.insert(name.to_owned(), value);
            }
            if result.len() == previous {
                let unresolved: Vec<_> = definitions
                    .keys()
                    .filter(|&&name| !result.contains_key(name))
                    .copied()
                    .collect();
                return Err(format!(
                    "パラメーターが循環しているか、循環に依存しています: {}",
                    unresolved.join(", ")
                ));
            }
        }
        if cancel() {
            return Err(CANCELLED.into());
        }
        Ok(result)
    }

    /// Update only named Number nodes; never overwrite a wire or a different
    /// kind of node. A failure or cancellation leaves the original graph intact.
    /// Callers can checkpoint the original Project once for Undo/Redo.
    pub fn apply_to_graph(
        &self,
        graph: &mut Graph,
        cancel: &dyn Fn() -> bool,
    ) -> Result<BTreeMap<String, ResolvedParameter>, String> {
        graph.validate()?;
        let values = self.evaluate(cancel)?;
        let mut candidate = graph.clone();
        let mut targets = BTreeSet::new();
        for binding in &self.bindings {
            if cancel() {
                return Err(CANCELLED.into());
            }
            if !targets.insert(binding.node_id) {
                return Err(format!(
                    "ノード {} に複数の割り当てがあります。",
                    binding.node_id
                ));
            }
            let value = values
                .get(&binding.parameter)
                .ok_or_else(|| format!("不明なパラメーター: {}", binding.parameter))?
                .in_unit(binding.unit)?;
            let node = candidate
                .node_mut(binding.node_id)
                .ok_or_else(|| format!("ノード {} が見つかりません。", binding.node_id))?;
            if node.kind != NodeKind::Number
                || node.name != binding.node_name
                || node.off
                || node.inputs.contains_key("value")
            {
                return Err(format!(
                    "ノード {} は編集可能な数値ノードではありません。",
                    node.id
                ));
            }
            let spec = node
                .kind
                .spec()
                .inputs
                .iter()
                .find(|input| input.key == "value" && input.default.is_some())
                .ok_or("数値ノードの入力が見つかりません。")?;
            if !value.is_finite()
                || value < spec.min
                || value > spec.max
                || (spec.integer && value.fract() != 0.0)
            {
                return Err(format!("ノード {} の入力範囲外です。", node.id));
            }
            node.params.insert("value".into(), value);
        }
        candidate.validate()?;
        if cancel() {
            return Err(CANCELLED.into());
        }
        *graph = candidate;
        Ok(values)
    }
}

#[cfg(test)]
mod numeric_input_tests {
    use super::{Unit, parse_numeric_input};

    #[test]
    fn direct_precision_and_constant_arithmetic() {
        let precise = 30.123_456_789_012_344_f64;
        assert_eq!(
            parse_numeric_input(&precise.to_string(), Unit::Mm)
                .unwrap()
                .to_bits(),
            precise.to_bits()
        );
        assert_eq!(
            parse_numeric_input("−0", Unit::Scalar).unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(parse_numeric_input("20*3", Unit::Mm).unwrap(), 60.0);
        assert_eq!(parse_numeric_input("1e-3", Unit::Scalar).unwrap(), 0.001);
        assert_eq!(
            parse_numeric_input("pi/4", Unit::Rad).unwrap(),
            std::f64::consts::FRAC_PI_4
        );
    }

    #[test]
    fn explicit_units_are_dimension_checked() {
        for (text, target, expected) in [
            ("10mm", Unit::Mm, 10.0),
            ("2 cm + 5 mm", Unit::Mm, 25.0),
            ("1e-3m", Unit::Mm, 1.0),
            ("2inch", Unit::Mm, 50.8),
            ("sin(30deg)", Unit::Scalar, 0.5),
            ("90deg", Unit::Rad, std::f64::consts::FRAC_PI_2),
            ("1rad", Unit::Deg, 180.0 / std::f64::consts::PI),
            ("10mm/2mm", Unit::Scalar, 5.0),
            ("sqrt(4mm^2)", Unit::Mm, 4.0),
        ] {
            let actual = parse_numeric_input(text, target).unwrap();
            assert!((actual - expected).abs() < 1e-12, "{text}: {actual}");
        }
    }

    #[test]
    fn rejects_invalid_or_unbounded_input_and_keeps_expression_syntax() {
        for text in [
            "NaN",
            "Infinity",
            "1/0",
            "sqrt(-1)",
            "1e308m",
            "10mm+2deg",
            "2mm+3",
            "2deg",
            "x",
            "x*1mm",
            "unknown(1)",
            "1 2",
            "1mm^2",
        ] {
            assert!(parse_numeric_input(text, Unit::Mm).is_err(), "{text}");
        }
        assert!(parse_numeric_input(&"(".repeat(300), Unit::Scalar).is_err());
        assert!(crate::expr::parse("2cm").is_err());
        assert_eq!(
            crate::expr::parse("x*2")
                .unwrap()
                .eval([3.0, 0.0, 0.0, 0.0])
                .unwrap(),
            6.0
        );
    }
}
