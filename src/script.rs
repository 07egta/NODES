//! Sandboxed user scripts for the 「スクリプト」 node, powered by the `rhai` crate.
//!
//! Safety model (see docs/ARCHITECTURE.md, section "スクリプトのサンドボックス"):
//! * Rhai has no file, network or process access of its own, and we register none.
//!   `import` is removed with the `no_module` feature and `eval` is disabled.
//! * Hard Engine limits: operations, call depth, expression depth, string/array/map size.
//! * The script runs on a helper thread. The calling (worker) thread polls the existing
//!   cooperative cancel callback every few milliseconds and tells the script to stop
//!   through an atomic flag that `Engine::on_progress` checks on every operation.
//!   A wall-clock limit is checked in the same place.
//! * Meshes made by scripts are charged against a budget and finally pass through the
//!   engine's normal `checked_tree` (size limits, finite coordinates, no zero-area faces).
//! * Every failure becomes a Japanese `Err(String)`; nothing here panics on bad input.
use crate::model::*;
use rhai::{Array, Dynamic, Engine, EvalAltResult, OptimizationLevel, Scope};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub const MAX_SCRIPT_BYTES: usize = 8 * 1024;
pub const MAX_OPERATIONS: u64 = 200_000;
pub const MAX_CALL_LEVELS: usize = 16;
pub const MAX_EXPR_DEPTH: usize = 64;
pub const MAX_FUNCTION_EXPR_DEPTH: usize = 32;
pub const MAX_STRING_SIZE: usize = 4_096;
pub const MAX_MAP_SIZE: usize = 1_000;
pub const TIME_LIMIT: Duration = Duration::from_secs(2);
const SCRIPT_STACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_MERGE_PARTS: usize = 4_096;
const CANCELLED: &str = "計算を停止しました。";

type UnaryMathFunction = fn(f64) -> f64;
type BinaryMathFunction = fn(f64, f64) -> f64;

const fn number_input(key: &'static str, label: &'static str, default: f64) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Number,
        default: Some(default),
        min: -1e6,
        max: 1e6,
        step: 0.1,
        integer: false,
    }
}
static SCRIPT_INPUTS: [InputSpec; 5] = [
    number_input("a", "a", 8.),
    number_input("b", "b", 30.),
    number_input("c", "c", 25.),
    number_input("d", "d", 12.),
    InputSpec {
        key: "g",
        label: "g（形状・任意）",
        kind: DataType::Any,
        default: None,
        min: 0.,
        max: 0.,
        step: 0.,
        integer: false,
    },
];
pub static SCRIPT_SPEC: NodeSpec = NodeSpec {
    label: "スクリプト",
    category: "データ",
    description: "短いスクリプト（Rhai）で数値・数値のリスト・点・メッシュを作ります。入力 a〜d と形状 g を使えます。設定パネルで書きます。",
    keywords: "script rhai code program 式 コード プログラム スクリプト 自由 関数",
    inputs: &SCRIPT_INPUTS,
    output: DataType::Any,
};

/// 例1: らせん階段（a=段数, b=半径, c=1段の回転角(度), d=1段の高さ）
pub const EXAMPLE_STAIRS: &str = "\
// a=段数  b=半径  c=1段ごとの回転角(度)  d=1段の高さ
let n = to_int(a);
let parts = [];
for i in 0..n {
    let ang = radians(c * i);
    let step = mesh_box(b * 0.5, b * 0.25, d * 0.4);
    step = mesh_rotate(step, c * i);
    step = mesh_move(step, b * cos(ang), b * sin(ang), d * i);
    parts.push(step);
}
mesh_merge(parts)
";
/// 例2: 数値のリスト（a=個数, b=振幅, c=1つごとの角度(度)）。「移動」の Z などへつなぐ。
pub const EXAMPLE_WAVE: &str = "\
// a=個数  b=振幅  c=1つごとの角度(度)  → 数値のリストを返す
let n = to_int(a);
let out = [];
for i in 0..n {
    out.push(b * sin(radians(c * i)));
}
out
";

/// Inputs handed to a script. A list with exactly one number becomes a plain number in the
/// script; any other length becomes an array. `geometry` is `None` when `g` is unconnected.
#[derive(Clone, Debug, Default)]
pub struct ScriptInputs {
    pub numbers: [Vec<f64>; 4],
    pub geometry: Option<Vec<Value>>,
}

#[derive(Clone)]
struct SPoint([f64; 3]);
#[derive(Clone)]
struct SMesh(Arc<Mesh>);

type ScriptResult<T> = Result<T, Box<EvalAltResult>>;

/// Evaluate a `Script` node (called from `cluster::evaluate_any`).
pub fn evaluate_node(
    node: &Node,
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let mut inputs = ScriptInputs::default();
    for (slot, spec) in SCRIPT_SPEC.inputs.iter().take(4).enumerate() {
        if let Some(source) = node.inputs.get(spec.key) {
            let tree = linked(outputs, *source, spec.label)?;
            let mut list = Vec::new();
            for value in tree.branches.values().flatten() {
                list.push(
                    value
                        .number()
                        .map_err(|_| format!("「{}」には数値を接続してください。", spec.label))?,
                );
            }
            inputs.numbers[slot] = list;
        } else {
            let v = node
                .params
                .get(spec.key)
                .copied()
                .or(spec.default)
                .unwrap_or(0.);
            if !v.is_finite() || v < spec.min || v > spec.max {
                return Err(format!(
                    "「{}」は {} 〜 {} の範囲にしてください。",
                    spec.label, spec.min, spec.max
                ));
            }
            inputs.numbers[slot] = vec![v];
        }
    }
    if let Some(source) = node.inputs.get("g") {
        let tree = linked(outputs, *source, "g")?;
        inputs.geometry = Some(tree.branches.values().flatten().cloned().collect());
    }
    let tree = run_script(&node.script, &inputs, cancel)?;
    crate::engine::checked_tree(tree, cancel)
}

fn linked(
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    source: NodeId,
    label: &str,
) -> Result<Tree, String> {
    match outputs.get(&source) {
        Some(Ok(t)) => Ok(t.clone()),
        Some(Err(_)) => Err(format!(
            "「{label}」の入力元にエラーがあります。上流の赤いノードを確認してください。"
        )),
        None => Err(format!("「{label}」の計算結果がありません。")),
    }
}

/// Run `source` in the sandbox. `cancel` is the engine's cooperative cancel callback.
/// The result is a single branch `{0}`; callers should still run `checked_tree`.
pub fn run_script(
    source: &str,
    inputs: &ScriptInputs,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(format!(
            "スクリプトが長すぎます（{} バイト）。上限は {} バイトです。",
            source.len(),
            MAX_SCRIPT_BYTES
        ));
    }
    if source.trim().is_empty() {
        return Err("スクリプトが空です。右側の「ノードの設定」で書いてください。".into());
    }
    if cancel() {
        return Err(CANCELLED.into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let (sender, receiver) = mpsc::channel();
    let source = source.to_owned();
    let inputs = inputs.clone();
    let handle = std::thread::Builder::new()
        .name("morphlet-script".into())
        .stack_size(SCRIPT_STACK_BYTES)
        .spawn(move || {
            let result = execute(&source, &inputs, worker_stop);
            let _ = sender.send(result);
        })
        .map_err(|e| format!("スクリプトを実行する準備に失敗しました: {e}"))?;
    let mut cancelled = false;
    let result = loop {
        match receiver.recv_timeout(Duration::from_millis(4)) {
            Ok(r) => break Some(r),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !cancelled && cancel() {
                    cancelled = true;
                    stop.store(true, Ordering::SeqCst);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
        }
    };
    let _ = handle.join();
    if cancelled {
        return Err(CANCELLED.into());
    }
    result.unwrap_or_else(|| Err("スクリプトの実行中に内部エラーが起きました。".into()))
}

fn execute(source: &str, inputs: &ScriptInputs, stop: Arc<AtomicBool>) -> Result<Tree, String> {
    let engine = build_engine(stop);
    let ast = engine.compile(source).map_err(|e| {
        format!(
            "スクリプトの書き方に誤りがあります{}: {}",
            line_suffix(e.position().line()),
            e.err_type()
        )
    })?;
    let mut scope = Scope::new();
    for (slot, name) in ["a", "b", "c", "d"].into_iter().enumerate() {
        scope.push_dynamic(name, numbers_to_dynamic(&inputs.numbers[slot]));
    }
    match &inputs.geometry {
        Some(values) => {
            if values.iter().any(|v| {
                matches!(
                    v,
                    Value::Surface(_)
                        | Value::Solid(_)
                        | Value::Brep(_)
                        | Value::Cage(_)
                        | Value::Gcode(_)
                )
            }) {
                return Err("精密曲面と B-rep はスクリプトで未対応です。先に明示的にメッシュ化してください。".into());
            }
            let items: Vec<Dynamic> = values.iter().map(value_to_dynamic).collect();
            let g = if items.len() == 1 {
                items.into_iter().next().unwrap_or(Dynamic::UNIT)
            } else {
                Dynamic::from_array(items)
            };
            scope.push_dynamic("g", g);
        }
        None => {
            scope.push_dynamic("g", Dynamic::UNIT);
        }
    }
    let result = engine
        .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
        .map_err(|e| describe_error(&e))?;
    let values = output_values(result)?;
    if values.len() > MAX_ITEMS {
        return Err("返した値が多すぎます。".into());
    }
    Ok(Tree {
        branches: BTreeMap::from([(vec![0], values)]),
    })
}

fn line_suffix(line: Option<usize>) -> String {
    line.map(|l| format!("（{l} 行目）")).unwrap_or_default()
}

fn describe_error(error: &EvalAltResult) -> String {
    let line = line_suffix(error.position().line());
    match error {
        EvalAltResult::ErrorTooManyOperations(..) => format!(
            "スクリプトの実行回数が上限（{MAX_OPERATIONS} 回）を超えたため止めました{line}。無限ループになっていないか確認してください。"
        ),
        EvalAltResult::ErrorTerminated(token, _) => {
            if token.to_string() == "timeout" {
                format!(
                    "スクリプトの実行時間が上限（{} 秒）を超えたため止めました。",
                    TIME_LIMIT.as_secs()
                )
            } else {
                CANCELLED.to_string()
            }
        }
        EvalAltResult::ErrorDataTooLarge(what, _) => format!(
            "スクリプトのデータが大きすぎます{line}（{what}）。文字列は {MAX_STRING_SIZE} 文字、リストは {MAX_ITEMS} 個までです。"
        ),
        EvalAltResult::ErrorStackOverflow(..) => {
            format!("関数の呼び出しが深すぎます{line}（上限 {MAX_CALL_LEVELS} 段）。")
        }
        EvalAltResult::ErrorFunctionNotFound(signature, _) => {
            format!("使えない関数、または引数の数・型が合わない呼び出しです{line}: {signature}")
        }
        EvalAltResult::ErrorVariableNotFound(name, _) => {
            format!("変数「{name}」が見つかりません{line}。")
        }
        EvalAltResult::ErrorArithmetic(message, _) => {
            format!("計算できません{line}: {message}")
        }
        EvalAltResult::ErrorParsing(kind, _) => {
            format!("スクリプトの書き方に誤りがあります{line}: {kind}")
        }
        EvalAltResult::ErrorRuntime(value, _) => format!("{value}{line}"),
        other => format!("スクリプトを実行できません{line}: {other}"),
    }
}

fn build_engine(stop: Arc<AtomicBool>) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_expr_depths(MAX_EXPR_DEPTH, MAX_FUNCTION_EXPR_DEPTH);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ITEMS);
    engine.set_max_map_size(MAX_MAP_SIZE);
    // Nothing is evaluated at compile time, so no native call escapes the progress checks.
    engine.set_optimization_level(OptimizationLevel::None);
    engine.disable_symbol("eval");
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    let started = Instant::now();
    engine.on_progress(move |ops| {
        if stop.load(Ordering::Relaxed) {
            return Some(Dynamic::from("cancel".to_string()));
        }
        if ops % 64 == 0 && started.elapsed() > TIME_LIMIT {
            return Some(Dynamic::from("timeout".to_string()));
        }
        None
    });
    register_api(&mut engine);
    engine
}

fn num(d: &Dynamic) -> ScriptResult<f64> {
    let v = if let Ok(f) = d.as_float() {
        f
    } else if let Ok(i) = d.as_int() {
        i as f64
    } else {
        return Err("数値を渡してください。".into());
    };
    finite(v)
}
fn finite(v: f64) -> ScriptResult<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err("計算結果が有限の数値になりません。".into())
    }
}
fn bounded(d: &Dynamic, name: &str, low: f64, high: f64) -> ScriptResult<f64> {
    let v = num(d)?;
    if v < low || v > high {
        return Err(format!("{name} は {low} 〜 {high} の範囲にしてください。").into());
    }
    Ok(v)
}

fn register_api(engine: &mut Engine) {
    engine.register_type_with_name::<SPoint>("Point");
    engine.register_type_with_name::<SMesh>("Mesh");
    engine.register_get("x", |p: &mut SPoint| p.0[0]);
    engine.register_get("y", |p: &mut SPoint| p.0[1]);
    engine.register_get("z", |p: &mut SPoint| p.0[2]);

    // Math. Arguments may be integers or floats (Rhai itself is strict about that).
    let unary: [(&str, UnaryMathFunction); 15] = [
        ("sin", f64::sin),
        ("cos", f64::cos),
        ("tan", f64::tan),
        ("asin", f64::asin),
        ("acos", f64::acos),
        ("atan", f64::atan),
        ("sqrt", f64::sqrt),
        ("exp", f64::exp),
        ("ln", f64::ln),
        ("abs", f64::abs),
        ("floor", f64::floor),
        ("ceil", f64::ceil),
        ("round", f64::round),
        ("radians", f64::to_radians),
        ("degrees", f64::to_degrees),
    ];
    for (name, f) in unary {
        engine.register_fn(name, move |x: Dynamic| -> ScriptResult<f64> {
            finite(f(num(&x)?))
        });
    }
    let binary: [(&str, BinaryMathFunction); 4] = [
        ("atan2", f64::atan2),
        ("pow", f64::powf),
        ("min", f64::min),
        ("max", f64::max),
    ];
    for (name, f) in binary {
        engine.register_fn(name, move |x: Dynamic, y: Dynamic| -> ScriptResult<f64> {
            finite(f(num(&x)?, num(&y)?))
        });
    }
    engine.register_fn("pi", || std::f64::consts::PI);
    engine.register_fn(
        "clamp",
        |x: Dynamic, low: Dynamic, high: Dynamic| -> ScriptResult<f64> {
            let (x, low, high) = (num(&x)?, num(&low)?, num(&high)?);
            if low > high {
                return Err("clamp は 最小 <= 最大 にしてください。".into());
            }
            Ok(x.clamp(low, high))
        },
    );
    engine.register_fn(
        "lerp",
        |a: Dynamic, b: Dynamic, t: Dynamic| -> ScriptResult<f64> {
            let (a, b, t) = (num(&a)?, num(&b)?, num(&t)?);
            finite(crate::math_nodes::lerp(a, b, t))
        },
    );
    // `n` evenly spaced numbers from a to b, both included (like the 「範囲」 node).
    // Rhai's built-in integer `range(a, b)` keeps its usual meaning.
    engine.register_fn(
        "linspace",
        |a: Dynamic, b: Dynamic, n: Dynamic| -> ScriptResult<Array> {
            let (a, b) = (num(&a)?, num(&b)?);
            let n = bounded(&n, "個数", 1., MAX_ITEMS as f64)?.round() as usize;
            let mut out = Array::with_capacity(n);
            for i in 0..n {
                let t = if n == 1 {
                    0.
                } else {
                    i as f64 / (n - 1) as f64
                };
                out.push(Dynamic::from(finite(crate::math_nodes::lerp(a, b, t))?));
            }
            Ok(out)
        },
    );

    engine.register_fn(
        "point",
        |x: Dynamic, y: Dynamic, z: Dynamic| -> ScriptResult<SPoint> {
            let p = [num(&x)?, num(&y)?, num(&z)?];
            if p.iter().any(|v| v.abs() > 1e9) {
                return Err("点の座標が大きすぎます。".into());
            }
            Ok(SPoint(p))
        },
    );

    // Everything below that makes a mesh is charged against one budget per script run.
    let budget = Rc::new(Cell::new(0usize));
    let b = budget.clone();
    engine.register_fn(
        "mesh_box",
        move |w: Dynamic, d: Dynamic, h: Dynamic| -> ScriptResult<SMesh> {
            let mesh = box_mesh(
                bounded(&w, "幅", 0.01, 10_000.)?,
                bounded(&d, "奥行", 0.01, 10_000.)?,
                bounded(&h, "高さ", 0.01, 10_000.)?,
            );
            made(&b, mesh)
        },
    );
    let b = budget.clone();
    engine.register_fn(
        "mesh_cylinder",
        move |r: Dynamic, h: Dynamic, n: Dynamic| -> ScriptResult<SMesh> {
            let r = bounded(&r, "半径", 0.01, 10_000.)?;
            let h = bounded(&h, "高さ", 0.01, 10_000.)?;
            let n = bounded(&n, "分割数", 3., 256.)?.round() as usize;
            made(&b, cylinder_mesh(r, h, n))
        },
    );
    let b = budget.clone();
    engine.register_fn(
        "mesh_move",
        move |m: SMesh, x: Dynamic, y: Dynamic, z: Dynamic| -> ScriptResult<SMesh> {
            let d = [num(&x)?, num(&y)?, num(&z)?];
            transformed(&b, &m, |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]])
        },
    );
    let b = budget.clone();
    engine.register_fn(
        "mesh_scale",
        move |m: SMesh, s: Dynamic| -> ScriptResult<SMesh> {
            let s = bounded(&s, "倍率", 0.001, 1_000.)?;
            transformed(&b, &m, |p| [p[0] * s, p[1] * s, p[2] * s])
        },
    );
    let b = budget.clone();
    engine.register_fn(
        "mesh_rotate",
        move |m: SMesh, degrees: Dynamic| -> ScriptResult<SMesh> {
            let (sin, cos) = num(&degrees)?.to_radians().sin_cos();
            transformed(&b, &m, |p| {
                [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos, p[2]]
            })
        },
    );
    let b = budget.clone();
    engine.register_fn(
        "mesh_merge",
        move |a: SMesh, c: SMesh| -> ScriptResult<SMesh> { merged(&b, &[a.0, c.0]) },
    );
    let b = budget.clone();
    engine.register_fn("mesh_merge", move |list: Array| -> ScriptResult<SMesh> {
        if list.is_empty() {
            return Err("まとめるメッシュが 1 つもありません。".into());
        }
        if list.len() > MAX_MERGE_PARTS {
            return Err(format!("まとめられるメッシュは {MAX_MERGE_PARTS} 個までです。").into());
        }
        let mut parts = Vec::with_capacity(list.len());
        for item in list {
            match item.try_cast::<SMesh>() {
                Some(m) => parts.push(m.0),
                None => return Err("リストにメッシュ以外が含まれています。".into()),
            }
        }
        merged(&b, &parts)
    });
    engine.register_fn("mesh_triangles", |m: SMesh| m.0.triangles.len() as i64);
    engine.register_fn("mesh_vertices", |m: SMesh| m.0.vertices.len() as i64);
}

fn charge(budget: &Cell<usize>, mesh: &Mesh) -> ScriptResult<()> {
    if mesh.vertices.len() > MAX_VERTICES || mesh.triangles.len() > MAX_TRIANGLES {
        return Err("メッシュが大きすぎます。分割数や個数を減らしてください。".into());
    }
    let total = budget
        .get()
        .saturating_add(mesh.vertices.len())
        .saturating_add(mesh.triangles.len().saturating_mul(3));
    if total > MAX_TOTAL_WEIGHT / 2 {
        return Err(
            "スクリプトが作るメッシュの合計が大きすぎます。個数や分割数を減らしてください。".into(),
        );
    }
    budget.set(total);
    Ok(())
}
fn made(budget: &Cell<usize>, mesh: Mesh) -> ScriptResult<SMesh> {
    charge(budget, &mesh)?;
    Ok(SMesh(Arc::new(mesh)))
}
fn transformed(
    budget: &Cell<usize>,
    m: &SMesh,
    f: impl Fn([f64; 3]) -> [f64; 3],
) -> ScriptResult<SMesh> {
    let vertices: Vec<[f64; 3]> = m.0.vertices.iter().map(|p| f(*p)).collect();
    if vertices
        .iter()
        .any(|p| p.iter().any(|v| !v.is_finite() || v.abs() > 1e9))
    {
        return Err("座標が大きすぎるか、無効な値になりました。".into());
    }
    made(
        budget,
        Mesh {
            vertices,
            triangles: m.0.triangles.clone(),
        },
    )
}
fn merged(budget: &Cell<usize>, parts: &[Arc<Mesh>]) -> ScriptResult<SMesh> {
    let vertices: usize = parts.iter().map(|m| m.vertices.len()).sum();
    let triangles: usize = parts.iter().map(|m| m.triangles.len()).sum();
    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
        return Err("まとめたメッシュが大きすぎます。".into());
    }
    let mut out = Mesh {
        vertices: Vec::with_capacity(vertices),
        triangles: Vec::with_capacity(triangles),
    };
    for part in parts {
        let offset = out.vertices.len() as u32;
        out.vertices.extend(&part.vertices);
        out.triangles.extend(
            part.triangles
                .iter()
                .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
        );
    }
    made(budget, out)
}

/// Closed box, XY-centred, base at Z=0, outward-facing triangles.
fn box_mesh(w: f64, d: f64, h: f64) -> Mesh {
    let (x, y) = (w / 2., d / 2.);
    Mesh {
        vertices: vec![
            [-x, -y, 0.],
            [x, -y, 0.],
            [x, y, 0.],
            [-x, y, 0.],
            [-x, -y, h],
            [x, -y, h],
            [x, y, h],
            [-x, y, h],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ],
    }
}
/// Closed cylinder, base at Z=0, `n` sides.
fn cylinder_mesh(r: f64, h: f64, n: usize) -> Mesh {
    let mut vertices = Vec::with_capacity(2 * n + 2);
    for z in [0., h] {
        for i in 0..n {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            vertices.push([r * a.cos(), r * a.sin(), z]);
        }
    }
    vertices.push([0., 0., 0.]);
    vertices.push([0., 0., h]);
    let (bottom, top) = (2 * n as u32, 2 * n as u32 + 1);
    let n32 = n as u32;
    let mut triangles = Vec::with_capacity(4 * n);
    for i in 0..n32 {
        let j = (i + 1) % n32;
        triangles.push([i, j, n32 + j]);
        triangles.push([i, n32 + j, n32 + i]);
        triangles.push([bottom, j, i]);
        triangles.push([top, n32 + i, n32 + j]);
    }
    Mesh {
        vertices,
        triangles,
    }
}

fn numbers_to_dynamic(list: &[f64]) -> Dynamic {
    if list.len() == 1 {
        Dynamic::from(list[0])
    } else {
        Dynamic::from_array(list.iter().map(|v| Dynamic::from(*v)).collect())
    }
}
fn value_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Number(v) => Dynamic::from(*v),
        Value::Point(p) => Dynamic::from(SPoint(*p)),
        Value::Mesh(m) => Dynamic::from(SMesh(m.clone())),
        Value::Surface(_) | Value::Solid(_) | Value::Brep(_) | Value::Cage(_) | Value::Gcode(_) => {
            Dynamic::UNIT
        } // rejected before conversion
        // A curve is given to scripts as a list of points (open/closed is not kept).
        Value::Curve(c) => {
            Dynamic::from_array(c.points.iter().map(|p| Dynamic::from(SPoint(*p))).collect())
        }
    }
}

fn output_values(result: Dynamic) -> Result<Vec<Value>, String> {
    if result.is_array() {
        let list = result
            .try_cast::<Array>()
            .ok_or("リストを読み取れませんでした。")?;
        if list.len() > MAX_ITEMS {
            return Err("返したリストが長すぎます。".into());
        }
        list.iter().map(single_value).collect()
    } else {
        Ok(vec![single_value(&result)?])
    }
}
fn single_value(d: &Dynamic) -> Result<Value, String> {
    if d.is_unit() {
        return Err(
            "スクリプトの最後に値（数値・数値のリスト・点・メッシュ）を書いてください。".into(),
        );
    }
    if let Ok(f) = d.as_float() {
        return if f.is_finite() {
            Ok(Value::Number(f))
        } else {
            Err("結果が有限の数値ではありません。".into())
        };
    }
    if let Ok(i) = d.as_int() {
        return Ok(Value::Number(i as f64));
    }
    if let Some(p) = d.clone().try_cast::<SPoint>() {
        return Ok(Value::Point(p.0));
    }
    if let Some(m) = d.clone().try_cast::<SMesh>() {
        return Ok(Value::Mesh(m.0));
    }
    Err(format!(
        "返せるのは数値・数値のリスト・点・メッシュ（のリスト）です（今回の型: {}）。",
        d.type_name()
    ))
}
