//! A tiny, strictly bounded arithmetic expression evaluator for the 式 node.
//!
//! Variables are `x`, `y`, `z` and `w`; constants are `pi` and `e`. Operators are
//! `+ - * / % ^` (with `^` right-associative and binding tighter than unary minus).
//! Functions include trigonometry (radians), roots, logarithms, hyperbolic
//! functions, bounded interpolation and smoothstep. See the user guide.
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
    Cbrt,
    Hypot,
    Log2,
    Log10,
    Log1p,
    Expm1,
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
            "cbrt" => Self::Cbrt,
            "hypot" => Self::Hypot,
            "log2" => Self::Log2,
            "log10" => Self::Log10,
            "log1p" => Self::Log1p,
            "expm1" => Self::Expm1,
            "clamp" => Self::Clamp,
            "lerp" => Self::Lerp,
            "smoothstep" => Self::Smoothstep,
            _ => return None,
        })
    }
    fn arity(self) -> usize {
        match self {
            Self::Min | Self::Max | Self::Atan2 | Self::Hypot => 2,
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

type LiteralUnit = fn(&str) -> Option<(usize, f64)>;

struct Parser {
    literal_units: Option<LiteralUnit>,
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
            Tok::Num(v) => {
                // Only the precision editor enables unit suffixes. Their canonical
                // values share the existing AST and dimensional checker.
                if let (Some(unit), Some(Tok::Ident(name))) = (self.literal_units, self.peek())
                    && let Some((index, scale)) = unit(name)
                {
                    self.pos += 1;
                    let canonical = v * scale;
                    if !canonical.is_finite() || (v != 0.0 && canonical == 0.0) {
                        return Err("単位変換で数値の表現範囲を超えました。".into());
                    }
                    let value = self.make(Expr::Num(canonical))?;
                    let dimension = self.make(Expr::Var(index))?;
                    return self.bin(Op::Mul, value, dimension);
                }
                self.make(Expr::Num(v))
            }
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
                    let mut args = Vec::new();
                    loop {
                        args.push(self.additive(depth + 1)?);
                        if !matches!(self.peek(), Some(Tok::Comma)) {
                            break;
                        }
                        if args.len() == func.arity() {
                            return Err(format!(
                                "関数「{name}」には値が {} 個必要です。",
                                func.arity()
                            ));
                        }
                        self.pos += 1;
                    }
                    self.expect_close()?;
                    if args.len() != func.arity() {
                        return Err(format!(
                            "関数「{name}」には値が {} 個必要です。",
                            func.arity()
                        ));
                    }
                    return self.make(Expr::Call(func, args));
                }
                match name.as_str() {
                    "x" | "y" | "z" | "w" if self.literal_units.is_some() => {
                        Err("直接入力では変数を使用できません。".into())
                    }
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
    parse_inner(src, None)
}

/// Opt-in numeric literal suffixes. Ordinary Expression nodes retain their
/// original syntax; direct entry cannot access graph variables.
pub(crate) fn parse_with_literal_units(src: &str, unit: LiteralUnit) -> Result<Expr, String> {
    parse_inner(src, Some(unit))
}

fn parse_inner(src: &str, literal_units: Option<LiteralUnit>) -> Result<Expr, String> {
    let chars = normalise(src)?;
    let toks = tokenize(&chars)?;
    if toks.is_empty() {
        return Err("式が空です。例: x * 2 + y".into());
    }
    let mut parser = Parser {
        literal_units,
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

impl Expr {
    /// Evaluates with `vars = [x, y, z, w]`. Division by zero, a root or logarithm outside
    /// its domain, and any non-finite intermediate result are errors, never NaN.
    pub fn eval(&self, vars: [f64; 4]) -> Result<f64, String> {
        match self {
            Self::Num(v) => finite(*v),
            Self::Var(i) => finite(*vars.get(*i).ok_or(SYNTAX)?),
            Self::Neg(e) => Ok(-e.eval(vars)?),
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
                let x = args[0].eval(vars)?;
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
                        let y = args.get(1).ok_or(SYNTAX)?.eval(vars)?;
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
                    Func::Asin => x.asin(),
                    Func::Acos => x.acos(),
                    Func::Atan => x.atan(),
                    Func::Atan2 => x.atan2(args[1].eval(vars)?),
                    Func::Sinh => x.sinh(),
                    Func::Cosh => x.cosh(),
                    Func::Tanh => x.tanh(),
                    Func::Cbrt => x.cbrt(),
                    Func::Hypot => x.hypot(args[1].eval(vars)?),
                    Func::Log2 => x.log2(),
                    Func::Log10 => x.log10(),
                    Func::Log1p => x.ln_1p(),
                    Func::Expm1 => x.exp_m1(),
                    Func::Clamp => {
                        let lo = args[1].eval(vars)?;
                        let hi = args[2].eval(vars)?;
                        if lo > hi {
                            return Err("clamp の下限は上限以下にしてください。".into());
                        }
                        x.clamp(lo, hi)
                    }
                    Func::Lerp => {
                        let b = args[1].eval(vars)?;
                        let t = args[2].eval(vars)?;
                        crate::number_interpolation::lerp(x, b, t)
                    }
                    Func::Smoothstep => {
                        let lo = args[1].eval(vars)?;
                        let hi = args[2].eval(vars)?;
                        if lo >= hi {
                            return Err("smoothstep の下限は上限より小さくしてください。".into());
                        }
                        if x <= lo {
                            0.
                        } else if x >= hi {
                            1.
                        } else {
                            let span = hi - lo;
                            let t = if span.is_finite() {
                                (x - lo) / span
                            } else {
                                // Finite opposite-sign endpoints can have an infinite span.
                                // Halve before subtraction so the ratio stays representable.
                                (x * 0.5 - lo * 0.5) / (hi * 0.5 - lo * 0.5)
                            };
                            let t = t.clamp(0., 1.);
                            t * t * (3. - 2. * t)
                        }
                    }
                };
                finite(v)
            }
        }
    }
}
