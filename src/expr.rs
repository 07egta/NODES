//! A tiny, strictly bounded arithmetic expression evaluator for the 式 node.
//!
//! Variables are `x`, `y`, `z` and `w`; constants are `pi` and `e`. Operators are
//! `+ - * / % ^` (with `^` right-associative and binding tighter than unary minus).
//! Functions: trig and inverse trig, hyperbolic, roots, logarithms, rounding,
//! min/max/clamp, hypot, exact lerp and smoothstep (angles in radians).
//! There are no loops, no user functions and no recursion beyond the parser's fixed
//! depth, so any input terminates quickly. Full-width characters typed through a
//! Japanese IME are accepted and normalised first.

/// Text a new 式 node starts with.
pub const DEFAULT_EXPRESSION: &str = "x * 2 + y";
/// Longest accepted expression, in characters.
pub const MAX_EXPR_LEN: usize = 256;
/// Deepest allowed nesting of parentheses, unary signs, powers and function calls.
pub const MAX_EXPR_DEPTH: usize = 24;
/// Most syntax-tree nodes one expression may build.
pub const MAX_EXPR_NODES: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Sqrt,
    Abs,
    Min,
    Max,
    Floor,
    Ceil,
    Round,
    Ln,
    Exp,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Hypot,
    Cbrt,
    Log2,
    Log10,
    Log,
    Trunc,
    Fract,
    Sign,
    Degrees,
    Radians,
    Clamp,
    Lerp,
    Smoothstep,
}
impl Func {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "sin" => Self::Sin,
            "cos" => Self::Cos,
            "tan" => Self::Tan,
            "sqrt" => Self::Sqrt,
            "abs" => Self::Abs,
            "min" => Self::Min,
            "max" => Self::Max,
            "floor" => Self::Floor,
            "ceil" => Self::Ceil,
            "round" => Self::Round,
            "ln" => Self::Ln,
            "exp" => Self::Exp,
            "asin" => Self::Asin,
            "acos" => Self::Acos,
            "atan" => Self::Atan,
            "atan2" => Self::Atan2,
            "sinh" => Self::Sinh,
            "cosh" => Self::Cosh,
            "tanh" => Self::Tanh,
            "hypot" => Self::Hypot,
            "cbrt" => Self::Cbrt,
            "log2" => Self::Log2,
            "log10" => Self::Log10,
            "log" => Self::Log,
            "trunc" => Self::Trunc,
            "fract" => Self::Fract,
            "sign" => Self::Sign,
            "deg" => Self::Degrees,
            "rad" => Self::Radians,
            "clamp" => Self::Clamp,
            "lerp" | "mix" => Self::Lerp,
            "smoothstep" => Self::Smoothstep,
            _ => return None,
        })
    }
    fn arity(self) -> usize {
        match self {
            Self::Min | Self::Max | Self::Atan2 | Self::Hypot | Self::Log => 2,
            Self::Clamp | Self::Lerp | Self::Smoothstep => 3,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    /// 0 = x, 1 = y, 2 = z, 3 = w
    Var(usize),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
}

fn normalise(src: &str) -> Result<Vec<char>, String> {
    let mut out = Vec::new();
    for c in src.chars() {
        if out.len() >= MAX_EXPR_LEN {
            return Err(format!(
                "式が長すぎます。{MAX_EXPR_LEN} 文字以内にしてください。"
            ));
        }
        let mapped = match c {
            '\u{3000}' => ' ',
            '×' | '＊' => '*',
            '÷' => '/',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            other => other,
        };
        out.push(mapped);
    }
    Ok(out)
}

fn tokenize(chars: &[char]) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let mut i = 0usize;
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if toks.len() >= MAX_EXPR_NODES {
            return Err("式が複雑すぎます。短くしてください。".into());
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while chars
                .get(i)
                .is_some_and(|d| d.is_ascii_digit() || *d == '.')
            {
                i += 1;
            }
            // An exponent only counts when digits really follow, so `2e` stays 2 and e.
            if matches!(chars.get(i), Some('e' | 'E')) {
                let mut j = i + 1;
                if matches!(chars.get(j), Some('+' | '-')) {
                    j += 1;
                }
                if chars.get(j).is_some_and(|d| d.is_ascii_digit()) {
                    while chars.get(j).is_some_and(|d| d.is_ascii_digit()) {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text: String = chars.get(start..i).unwrap_or(&[]).iter().collect();
            let value: f64 = text
                .parse()
                .map_err(|_| format!("数値「{text}」を読み取れません。"))?;
            if !value.is_finite() {
                return Err(format!("数値「{text}」が大きすぎます。"));
            }
            toks.push(Tok::Num(value));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while chars
                .get(i)
                .is_some_and(|d| d.is_alphanumeric() || *d == '_')
            {
                i += 1;
            }
            let text: String = chars
                .get(start..i)
                .unwrap_or(&[])
                .iter()
                .collect::<String>()
                .to_lowercase();
            toks.push(Tok::Ident(text));
            continue;
        }
        toks.push(match c {
            '+' => Tok::Plus,
            '-' | '−' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '^' => Tok::Caret,
            '(' | '（' => Tok::LParen,
            ')' | '）' => Tok::RParen,
            ',' | '、' | '，' => Tok::Comma,
            other => return Err(format!("使えない文字「{other}」があります。")),
        });
        i += 1;
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    nodes: usize,
}
const SYNTAX: &str = "式の書き方が正しくありません。例: x * 2 + y";

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn make(&mut self, e: Expr) -> Result<Expr, String> {
        self.nodes += 1;
        if self.nodes > MAX_EXPR_NODES {
            return Err("式が複雑すぎます。短くしてください。".into());
        }
        Ok(e)
    }
    fn bin(&mut self, op: Op, l: Expr, r: Expr) -> Result<Expr, String> {
        self.make(Expr::Bin(op, Box::new(l), Box::new(r)))
    }
    fn additive(&mut self, depth: usize) -> Result<Expr, String> {
        let mut left = self.term(depth)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => Op::Add,
                Some(Tok::Minus) => Op::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.term(depth)?;
            left = self.bin(op, left, right)?;
        }
        Ok(left)
    }
    fn term(&mut self, depth: usize) -> Result<Expr, String> {
        let mut left = self.unary(depth)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => Op::Mul,
                Some(Tok::Slash) => Op::Div,
                Some(Tok::Percent) => Op::Rem,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary(depth)?;
            left = self.bin(op, left, right)?;
        }
        Ok(left)
    }
    fn unary(&mut self, depth: usize) -> Result<Expr, String> {
        if depth > MAX_EXPR_DEPTH {
            return Err(format!(
                "式の入れ子が深すぎます。{MAX_EXPR_DEPTH} 段までにしてください。"
            ));
        }
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                let inner = self.unary(depth + 1)?;
                self.make(Expr::Neg(Box::new(inner)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.unary(depth + 1)
            }
            _ => self.power(depth),
        }
    }
    fn power(&mut self, depth: usize) -> Result<Expr, String> {
        let base = self.primary(depth)?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.pos += 1;
            // Right-associative; the exponent may carry its own sign (2 ^ -1).
            let exponent = self.unary(depth + 1)?;
            return self.bin(Op::Pow, base, exponent);
        }
        Ok(base)
    }
    fn primary(&mut self, depth: usize) -> Result<Expr, String> {
        if depth > MAX_EXPR_DEPTH {
            return Err(format!(
                "式の入れ子が深すぎます。{MAX_EXPR_DEPTH} 段までにしてください。"
            ));
        }
        let tok = self.peek().cloned().ok_or(SYNTAX)?;
        self.pos += 1;
        match tok {
            Tok::Num(v) => self.make(Expr::Num(v)),
            Tok::LParen => {
                let inner = self.additive(depth + 1)?;
                self.expect_close()?;
                Ok(inner)
            }
            Tok::Ident(name) => {
                if matches!(self.peek(), Some(Tok::LParen)) {
                    let func = Func::from_name(&name)
                        .ok_or_else(|| format!("関数「{name}」は使えません。"))?;
                    self.pos += 1;
                    let arity = func.arity();
                    let arity_error = || format!("関数「{name}」には値が {arity} 個必要です。");
                    let mut args = Vec::with_capacity(arity);
                    for index in 0..arity {
                        if index != 0 {
                            if !matches!(self.peek(), Some(Tok::Comma)) {
                                return Err(arity_error());
                            }
                            self.pos += 1;
                        }
                        if matches!(self.peek(), Some(Tok::RParen)) {
                            return Err(arity_error());
                        }
                        args.push(self.additive(depth + 1)?);
                    }
                    if matches!(self.peek(), Some(Tok::Comma)) {
                        return Err(arity_error());
                    }
                    self.expect_close()?;
                    return self.make(Expr::Call(func, args));
                }
                match name.as_str() {
                    "x" => self.make(Expr::Var(0)),
                    "y" => self.make(Expr::Var(1)),
                    "z" => self.make(Expr::Var(2)),
                    "w" => self.make(Expr::Var(3)),
                    "pi" => self.make(Expr::Num(std::f64::consts::PI)),
                    "e" => self.make(Expr::Num(std::f64::consts::E)),
                    _ => Err(format!(
                        "「{name}」は使えません。使える文字: x, y, z, w, pi, e"
                    )),
                }
            }
            _ => Err(SYNTAX.into()),
        }
    }
    fn expect_close(&mut self) -> Result<(), String> {
        if matches!(self.peek(), Some(Tok::RParen)) {
            self.pos += 1;
            Ok(())
        } else {
            Err("かっこが閉じていません。".into())
        }
    }
}

/// Parses an expression. Every failure is a Japanese message; nothing here can panic.
pub fn parse(src: &str) -> Result<Expr, String> {
    let chars = normalise(src)?;
    let toks = tokenize(&chars)?;
    if toks.is_empty() {
        return Err("式が空です。例: x * 2 + y".into());
    }
    let mut parser = Parser {
        toks,
        pos: 0,
        nodes: 0,
    };
    let expr = parser.additive(0)?;
    if parser.pos != parser.toks.len() {
        return Err(SYNTAX.into());
    }
    Ok(expr)
}

const TOO_BIG: &str = "計算結果が大きすぎます。";

fn finite(v: f64) -> Result<f64, String> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(TOO_BIG.into())
    }
}

/// Cubic Hermite easing from 0 to 1, with clamped endpoints.
/// Scale only when the finite endpoint difference would overflow.
fn smoothstep(lo: f64, hi: f64, value: f64) -> Result<f64, String> {
    if lo >= hi {
        return Err("smoothstep の下限は上限より小さくしてください。".into());
    }
    if value <= lo {
        return Ok(0.0);
    }
    if value >= hi {
        return Ok(1.0);
    }
    let width = hi - lo;
    let t = if width.is_finite() {
        (value - lo) / width
    } else {
        (value * 0.5 - lo * 0.5) / (hi * 0.5 - lo * 0.5)
    }
    .clamp(0.0, 1.0);
    Ok(t * t * (3.0 - 2.0 * t))
}

impl Expr {
    /// Evaluates with `vars = [x, y, z, w]`. Division by zero, a root or logarithm outside
    /// its domain, and any non-finite intermediate result are errors, never NaN.
    pub fn eval(&self, vars: [f64; 4]) -> Result<f64, String> {
        match self {
            Self::Num(v) => finite(*v),
            Self::Var(i) => finite(vars.get(*i).copied().ok_or(SYNTAX)?),
            Self::Neg(e) => finite(-e.eval(vars)?),
            Self::Bin(op, l, r) => {
                let x = l.eval(vars)?;
                let y = r.eval(vars)?;
                let v = match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div => {
                        if y == 0. {
                            return Err("式の中でゼロで割っています。".into());
                        }
                        x / y
                    }
                    Op::Rem => {
                        if y == 0. {
                            return Err("式の中でゼロの剰余を計算しています。".into());
                        }
                        x.rem_euclid(y)
                    }
                    Op::Pow => {
                        if x == 0. && y < 0. {
                            return Err("式の中で 0 を負の指数でべき乗しています。".into());
                        }
                        if x < 0. && (y - y.round()).abs() > 1e-9 {
                            return Err("式の中で負の数を小数でべき乗しています。".into());
                        }
                        x.powf(y)
                    }
                };
                finite(v)
            }
            Self::Call(func, args) => {
                if args.len() != func.arity() {
                    return Err(SYNTAX.into());
                }
                let x = args.first().ok_or(SYNTAX)?.eval(vars)?;
                let y = || args.get(1).ok_or(SYNTAX)?.eval(vars);
                let z = || args.get(2).ok_or(SYNTAX)?.eval(vars);
                let v = match func {
                    Func::Sin => x.sin(),
                    Func::Cos => x.cos(),
                    Func::Tan => x.tan(),
                    Func::Sqrt => {
                        if x < 0. {
                            return Err("式の中で負の数の平方根を求めています。".into());
                        }
                        x.sqrt()
                    }
                    Func::Abs => x.abs(),
                    Func::Min | Func::Max => {
                        let y = y()?;
                        if *func == Func::Min {
                            x.min(y)
                        } else {
                            x.max(y)
                        }
                    }
                    Func::Floor => x.floor(),
                    Func::Ceil => x.ceil(),
                    Func::Round => x.round(),
                    Func::Ln => {
                        if x <= 0. {
                            return Err("式の中で 0 以下の数の対数を求めています。".into());
                        }
                        x.ln()
                    }
                    Func::Exp => x.exp(),
                    Func::Asin | Func::Acos => {
                        if !(-1.0..=1.0).contains(&x) {
                            return Err("逆三角関数の入力は -1〜1 にしてください。".into());
                        }
                        if *func == Func::Asin {
                            x.asin()
                        } else {
                            x.acos()
                        }
                    }
                    Func::Atan => x.atan(),
                    // Same signed-zero quadrant convention as Rust f64::atan2.
                    Func::Atan2 => x.atan2(y()?),
                    Func::Sinh => x.sinh(),
                    Func::Cosh => x.cosh(),
                    Func::Tanh => x.tanh(),
                    Func::Hypot => x.hypot(y()?),
                    Func::Cbrt => x.cbrt(),
                    Func::Log2 | Func::Log10 | Func::Log => {
                        if x <= 0.0 {
                            return Err("対数の入力は正の数にしてください。".into());
                        }
                        match func {
                            Func::Log2 => x.log2(),
                            Func::Log10 => x.log10(),
                            _ => {
                                let base = y()?;
                                if base <= 0.0 || base == 1.0 {
                                    return Err("対数の底は 1 以外の正の数にしてください。".into());
                                }
                                x.log(base)
                            }
                        }
                    }
                    Func::Trunc => x.trunc(),
                    Func::Fract => x.fract(),
                    Func::Sign => {
                        if x == 0.0 {
                            x
                        } else {
                            x.signum()
                        }
                    }
                    Func::Degrees => x.to_degrees(),
                    Func::Radians => x.to_radians(),
                    Func::Clamp => {
                        let (lo, hi) = (y()?, z()?);
                        if lo > hi {
                            return Err("clamp の下限は上限以下にしてください。".into());
                        }
                        x.clamp(lo, hi)
                    }
                    Func::Lerp => crate::number_interpolation::lerp(x, y()?, z()?),
                    Func::Smoothstep => {
                        let (hi, value) = (y()?, z()?);
                        smoothstep(x, hi, value)?
                    }
                };
                finite(v)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate(source: &str, vars: [f64; 4]) -> f64 {
        parse(source).unwrap().eval(vars).unwrap()
    }

    #[test]
    fn inverse_trig_and_angle_conversion() {
        let pi = std::f64::consts::PI;
        for (source, expected) in [
            ("asin(1)", pi / 2.0),
            ("acos(-1)", pi),
            ("atan(1)", pi / 4.0),
            ("atan2(1,-1)", 3.0 * pi / 4.0),
            ("deg(pi)", 180.0),
            ("rad(180)", pi),
            ("ＤＥＧ（ｐｉ）", 180.0),
        ] {
            assert!((evaluate(source, [0.0; 4]) - expected).abs() < 1e-12);
        }
        assert_eq!(
            evaluate("atan2(x,-1)", [-0.0, 0.0, 0.0, 0.0]).to_bits(),
            (-pi).to_bits()
        );
    }

    #[test]
    fn roots_logs_rounding_and_hyperbolic_functions() {
        for (source, expected) in [
            ("hypot(3,4)", 5.0),
            ("cbrt(-27)", -3.0),
            ("log2(8)", 3.0),
            ("log10(1000)", 3.0),
            ("log(81,3)", 4.0),
            ("trunc(-2.75)", -2.0),
            ("fract(-2.75)", -0.75),
            ("sign(-4)", -1.0),
            ("sign(0)", 0.0),
            ("sign(4)", 1.0),
            ("sinh(0)", 0.0),
            ("cosh(0)", 1.0),
            ("tanh(0)", 0.0),
            ("cosh(1)^2-sinh(1)^2", 1.0),
        ] {
            assert!((evaluate(source, [0.0; 4]) - expected).abs() < 1e-12);
        }
        assert_eq!(
            evaluate("hypot(x,0)", [f64::MAX, 0.0, 0.0, 0.0]),
            f64::MAX
        );
    }

    #[test]
    fn three_argument_functions_and_extreme_endpoints() {
        for (source, expected) in [
            ("clamp(-2,0,1)", 0.0),
            ("clamp(0.4,0,1)", 0.4),
            ("clamp(2,0,1)", 1.0),
            ("clamp(9,2,2)", 2.0),
            ("lerp(2,8,0.25)", 3.5),
            ("mix(2,8,2)", 14.0),
            ("smoothstep(0,1,-1)", 0.0),
            ("smoothstep(0,1,2)", 1.0),
            ("smoothstep(0,1,0.25)", 0.15625),
            ("smoothstep(0,1,0.5)", 0.5),
            ("smoothstep(0,1,0.75)", 0.84375),
            ("clamp(lerp(0,10,0.5),0,4)", 4.0),
        ] {
            assert_eq!(evaluate(source, [0.0; 4]), expected, "{source}");
        }
        let vars = [-f64::MAX, f64::MAX, 0.5, 0.0];
        assert_eq!(evaluate("lerp(x,y,z)", vars), 0.0);
        assert_eq!(evaluate("smoothstep(x,y,w)", vars), 0.5);
        assert_eq!(
            evaluate("lerp(x,1,0)", [-0.0, 0.0, 0.0, 0.0]).to_bits(),
            (-0.0_f64).to_bits()
        );
    }

    #[test]
    fn reject_wrong_arity_and_invalid_domains() {
        for source in [
            "sin()",
            "min(1)",
            "min(1,2,3)",
            "lerp(1,2)",
            "lerp(1,2,3,4)",
            "lerp(1,2,3,)",
            "smoothstep(0,1,)",
            "clamp(1,,2)",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
        for source in [
            "asin(2)",
            "acos(-2)",
            "log(10,1)",
            "log(10,-2)",
            "log2(0)",
            "log10(-1)",
            "clamp(0,2,1)",
            "smoothstep(1,1,0)",
            "smoothstep(2,1,0)",
            "cosh(1000)",
            "lerp(0,1e308,2)",
        ] {
            assert!(parse(source).unwrap().eval([0.0; 4]).is_err(), "{source}");
        }
    }

    #[test]
    fn nonfinite_leaves_and_malformed_public_ast_are_errors() {
        for source in ["x", "-x", "min(x,1)", "max(x,1)", "x^0", "clamp(0,x,1)"] {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert!(
                    parse(source)
                        .unwrap()
                        .eval([invalid, 0.0, 0.0, 0.0])
                        .is_err(),
                    "{source}"
                );
            }
        }
        assert!(Expr::Var(4).eval([0.0; 4]).is_err());
        assert!(Expr::Num(f64::NAN).eval([0.0; 4]).is_err());
        let malformed = Expr::Call(Func::Sin, vec![Expr::Num(1.0), Expr::Num(2.0)]);
        assert!(malformed.eval([0.0; 4]).is_err());
        assert_eq!(evaluate("2^3^2", [0.0; 4]), 512.0);
        assert_eq!(evaluate("-2^2", [0.0; 4]), -4.0);
        assert_eq!(evaluate("2^-2", [0.0; 4]), 0.25);
    }
}
