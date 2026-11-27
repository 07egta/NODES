//! Pure helpers for the node search popup, the recently used list, grid snapping,
//! the saved theme choice and the shortcut cheat-sheet. No egui drawing happens here,
//! so the ranking and persistence rules can be unit tested on their own.

use morphlet::model::{DataType, NodeKind};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// How many recently added nodes are remembered (newest first).
pub const RECENT_LIMIT: usize = 8;
/// Bonus per position in the recent list: the newest node gets `RECENT_LIMIT * RECENT_STEP`.
const RECENT_STEP: i32 = 4;
/// Typo tolerance (subsequence match) is only used for tokens at least this long.
const MIN_FUZZY_CHARS: usize = 3;

/// Lower-cases ASCII, turns full-width spaces into spaces and katakana into hiragana,
/// so that「マル」「まる」「Maru」-style input can be compared on one footing.
pub fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0)
                .unwrap_or(c)
                .to_ascii_lowercase(),
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Short words people type for a node whose label is different.
fn alias(token: &str) -> &str {
    match token {
        "まる" | "えん" => "円",
        "はこ" => "箱",
        "しかく" => "長方形",
        "はなびん" | "かびん" => "花器",
        "ならべる" => "並べる",
        "おしだし" => "押し出し",
        _ => token,
    }
}

/// A small, explicit intent dictionary. These are suggestions for supported operations,
/// not a claim that the editor implements every Grasshopper component or understands
/// arbitrary Japanese sentences. No labels or aliases become persistent node IDs.
fn intent(query: &str) -> Option<&'static [NodeKind]> {
    use NodeKind::*;
    match query {
        "円を作りたい"
        | "円をつくりたい"
        | "まるを作りたい"
        | "まるをつくりたい"
        | "円を作る"
        | "円をつくる"
        | "まるを作る"
        | "まるをつくる" => Some(&[Circle]),
        "花瓶を作りたい"
        | "花瓶をつくりたい"
        | "花器を作りたい"
        | "かびんをつくりたい"
        | "花瓶を作る"
        | "花瓶をつくる"
        | "花器を作る"
        | "かびんをつくる"
        | "はなびんをつくる"
        | "vase" => Some(&[Vase]),
        "点を作りたい"
        | "点をつくりたい"
        | "点を置きたい"
        | "点を作る"
        | "点をつくる"
        | "点を置く"
        | "constructpoint" => Some(&[Point]),
        "点を並べたい" | "点をならべたい" | "点を並べる" | "点をならべる" | "点の配列" => {
            Some(&[LinearArray, RadialArray, Populate])
        }
        "面を作りたい" | "面をつくりたい" | "面を作る" | "面をつくる" => {
            Some(&[Plane, Loft])
        }
        "曲面を作りたい" | "曲面をつくりたい" | "曲面を作る" | "曲面をつくる" => {
            Some(&[Loft, Revolve])
        }
        "花瓶をねじりたい"
        | "花器をねじりたい"
        | "かびんをねじりたい"
        | "花瓶を捻りたい"
        | "ねじりたい"
        | "捻りたい"
        | "ねじる"
        | "捻る" => Some(&[Twist]),
        "箱を作りたい" | "箱をつくりたい" | "箱を作る" | "箱をつくる" | "はこをつくる" => {
            Some(&[Box])
        }
        "押し出したい" | "押出したい" | "おしだしたい" | "押し出す" | "押出す" | "おしだす" => {
            Some(&[Extrude])
        }
        "直線配列" | "lineararray" => Some(&[LinearArray]),
        "円形配列" | "polararray" | "radialarray" => Some(&[RadialArray]),
        "numberslider" => Some(&[Number]),
        "deconstructpoint" => Some(&[PointComponent]),
        "listlength" => Some(&[ListLength]),
        "listitem" => Some(&[ListItem]),
        "remapnumbers" => Some(&[Remap]),
        _ => None,
    }
}

/// True when every character of `needle` appears in `hay`, in order.
pub fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut rest = hay.chars();
    needle.chars().all(|n| rest.any(|h| h == n))
}

/// Score of one query token against the label, keywords and category of a node.
/// Exact substring hits outrank typo-tolerant hits. Zero means no match.
fn token_score(token: &str, label: &str, keywords: &str, category: &str) -> i32 {
    if label == token {
        return 160;
    }
    if label.contains(token) {
        return 100;
    }
    if keywords.contains(token) {
        return 80;
    }
    if category.contains(token) {
        return 25;
    }
    if token.chars().count() >= MIN_FUZZY_CHARS {
        if is_subsequence(token, label) {
            return 60;
        }
        if is_subsequence(token, keywords) {
            return 30;
        }
    }
    0
}

/// Total match score of a query for one node, or `None` if any token fails to match.
/// An empty query matches everything with score 0.
pub fn query_score(kind: NodeKind, query: &str) -> Option<i32> {
    let normalized = normalize(query);
    let compact: String = normalized.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(kinds) = intent(&compact) {
        return kinds
            .iter()
            .position(|k| *k == kind)
            .map(|i| 400 - i as i32 * 50);
    }
    // A simple object + "make" phrase also works for nodes outside the dictionary.
    let query = ["を作りたい", "をつくりたい", "を作る", "をつくる"]
        .iter()
        .find_map(|suffix| normalized.trim().strip_suffix(*suffix))
        .unwrap_or(&normalized);
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| alias(t).to_owned())
        .collect();
    let spec = kind.spec();
    let label = normalize(spec.label);
    let keywords = normalize(spec.keywords);
    let category = normalize(spec.category);
    let mut total = 0;
    for token in &tokens {
        let score = token_score(token, &label, &keywords, &category);
        if score == 0 {
            return None;
        }
        total += score;
    }
    Some(total)
}

/// True when some input of `kind` accepts a value of type `source`.
pub fn accepts_from(kind: NodeKind, source: DataType) -> bool {
    kind.spec().inputs.iter().any(|s| s.kind.accepts(source))
}

/// The first input of `kind` that can take `source`, used when a dropped wire auto-connects.
pub fn first_compatible_input(kind: NodeKind, source: DataType) -> Option<&'static str> {
    kind.spec()
        .inputs
        .iter()
        .find(|s| s.kind.accepts(source))
        .map(|s| s.key)
}

/// Nodes added recently, newest first, without duplicates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    kinds: Vec<NodeKind>,
}

impl Recent {
    pub fn push(&mut self, kind: NodeKind) {
        self.kinds.retain(|k| *k != kind);
        self.kinds.insert(0, kind);
        self.kinds.truncate(RECENT_LIMIT);
    }
    pub fn kinds(&self) -> &[NodeKind] {
        &self.kinds
    }
    fn position(&self, kind: NodeKind) -> Option<usize> {
        self.kinds.iter().position(|k| *k == kind)
    }
}

/// Ranks the nodes for the search popup.
/// `base` is the default order (used for ties and for an empty query).
/// Recently used nodes get a small bonus. When a wire is being dropped, incompatible
/// nodes are excluded rather than merely ranked lower.
pub fn rank(
    base: &[NodeKind],
    query: &str,
    recent: &Recent,
    wire: Option<DataType>,
) -> Vec<NodeKind> {
    let mut scored: Vec<(i32, usize, NodeKind)> = base
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            if wire.is_some_and(|source| !accepts_from(*kind, source)) {
                return None;
            }
            let mut score = query_score(*kind, query)?;
            if let Some(position) = recent.position(*kind) {
                let freshness = RECENT_LIMIT.saturating_sub(position) as i32;
                score += freshness * RECENT_STEP;
            }
            Some((score, index, *kind))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, kind)| kind).collect()
}

/// The first line of `text`, cut to `max` characters with an ellipsis.
pub fn one_line(text: &str, max: usize) -> String {
    let first = text.lines().next().unwrap_or("");
    if first.chars().count() <= max {
        first.to_owned()
    } else {
        let mut cut: String = first.chars().take(max).collect();
        cut.push('…');
        cut
    }
}

/// Snaps a world coordinate to the nearest grid line.
pub fn snap(value: f32, grid: f32) -> f32 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).round() * grid
    }
}

/// Parses the saved theme line. Unknown text gives `None`.
pub fn parse_dark(text: &str) -> Option<bool> {
    match text.trim() {
        "theme=dark" => Some(true),
        "theme=light" => Some(false),
        _ => None,
    }
}

pub fn format_dark(dark: bool) -> &'static str {
    if dark { "theme=dark" } else { "theme=light" }
}

/// Settings file beside the autosave file (same folder rules as `recovery.morph`).
pub fn settings_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Morphlet").join("settings.txt"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("morphlet/settings.txt"))
    }
}

/// Saved theme; dark when nothing is saved yet.
pub fn load_dark() -> bool {
    // The settings file holds other lines too (node lists), so only the theme line is read.
    read_setting("theme")
        .and_then(|value| parse_dark(&format!("theme={value}")))
        .unwrap_or(true)
}

pub fn save_dark(dark: bool) -> Result<(), String> {
    save_setting_line(format_dark(dark))
}

/// Value of the `key=value` line in the settings file (the text after `key=`).
pub fn read_setting(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(|value| value.trim().to_owned())
}

/// Writes a `key=value` line, replacing an older line with the same key and keeping every
/// other line, so the theme and the node lists can be saved independently.
pub fn save_setting_line(line: &str) -> Result<(), String> {
    let path = settings_path().ok_or("設定を保存する場所が見つかりません。")?;
    let key = line.split_once('=').map_or("", |(key, _)| key);
    let prefix = format!("{key}=");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut text: String = old
        .lines()
        .filter(|l| !l.starts_with(prefix.as_str()))
        .map(|l| format!("{l}\n"))
        .collect();
    text.push_str(line);
    text.push('\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("設定フォルダーを作れません: {e}"))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("設定を保存できません: {e}"))
}

/// Favorites are stored separately from the project and theme, using stable enum IDs.
/// Corrupt or missing settings are harmless; the list is limited to real library nodes.
fn favorites_path() -> Option<PathBuf> {
    settings_path().map(|p| p.with_file_name("favorites.json"))
}

pub fn load_favorites() -> BTreeSet<NodeKind> {
    favorites_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<BTreeSet<NodeKind>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|k| NodeKind::all().contains(k))
        .collect()
}

pub fn save_favorites(kinds: &BTreeSet<NodeKind>) -> Result<(), String> {
    let path = favorites_path().ok_or("設定を保存する場所が見つかりません。")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("設定フォルダーを作れません: {e}"))?;
    }
    let text = serde_json::to_string(kinds).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("お気に入りを保存できません: {e}"))
}

/// Shortcut cheat-sheet rows: (keys, what it does).
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+S", "保存"),
    ("Ctrl+Shift+S", "名前を付けて保存"),
    ("Ctrl+O", "開く"),
    ("Ctrl+Z / Ctrl+Y", "元に戻す / やり直す"),
    ("Ctrl+D", "選択ノードを複製"),
    ("Delete / Backspace", "選択ノードを削除"),
    ("Ctrl+A", "すべて選択"),
    ("Shift+クリック", "選択に追加・外す"),
    ("空きをドラッグ", "範囲選択"),
    (
        "Tab / Ctrl+K",
        "ノードを検索して追加（ポインタがキャンバス上ならその位置）",
    ),
    (
        "Space を押して離す",
        "キャンバス上でノードを検索して追加（Space+ドラッグは移動）",
    ),
    ("ノード棚のボタンをドラッグ", "キャンバスの好きな場所に配置"),
    ("空きをダブルクリック", "その場所にノードを追加"),
    ("出力の丸から空きへドラッグ", "検索して、つないだ状態で追加"),
    ("Ctrl+E", "選択ノードを無効 / 有効"),
    ("Ctrl+G", "選択をグループで囲む"),
    ("Ctrl+Shift+G", "格子への吸着を切り替え"),
    ("F / Home", "形とキャンバスを画面に収める"),
    ("Shift+F", "選択ノードを画面に収める"),
    ("Ctrl+C / Ctrl+V", "ノードをコピー / 貼り付け（同じ作品内）"),
    ("F1", "選択ノードのヘルプ（ノード辞典）"),
    ("?", "このショートカット一覧"),
    ("検索欄: ↑ ↓ / Enter", "候補を選ぶ / 追加"),
    ("Esc", "検索・線の接続を中止"),
    ("Space + ドラッグ / 中ボタン", "キャンバスを動かす"),
    ("ホイール", "キャンバスの拡大・縮小"),
    ("3D: 左ドラッグ / Shift+ドラッグ", "回転 / 移動"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<NodeKind> {
        NodeKind::all().to_vec()
    }

    #[test]
    fn katakana_and_hiragana_match_the_same_label() {
        assert_eq!(normalize("マル"), normalize("まる"));
        assert!(query_score(NodeKind::Circle, "マル").is_some());
        assert!(query_score(NodeKind::Circle, "まる").is_some());
    }

    #[test]
    fn exact_keyword_beats_typo_tolerant_match() {
        let exact = query_score(NodeKind::Twist, "twist").unwrap_or(0);
        let fuzzy = query_score(NodeKind::Twist, "twst").unwrap_or(0);
        assert!(exact > fuzzy);
        assert!(fuzzy > 0);
    }

    #[test]
    fn every_token_must_match() {
        assert!(query_score(NodeKind::Circle, "円 twist").is_none());
        assert!(query_score(NodeKind::Circle, "円 ring").is_some());
    }

    #[test]
    fn short_english_tokens_do_not_fuzzy_match() {
        // "bx" is too short for typo tolerance, so it must not match a box by accident.
        assert!(query_score(NodeKind::Circle, "bx").is_none());
    }

    #[test]
    fn ranking_puts_the_best_match_first() {
        let ranked = rank(&all(), "twist", &Recent::default(), None);
        assert_eq!(ranked.first(), Some(&NodeKind::Twist));
    }

    #[test]
    fn empty_query_shows_recent_nodes_first_in_recency_order() {
        let mut recent = Recent::default();
        recent.push(NodeKind::Box);
        recent.push(NodeKind::Twist);
        let ranked = rank(&all(), "", &recent, None);
        assert_eq!(ranked[0], NodeKind::Twist);
        assert_eq!(ranked[1], NodeKind::Box);
        assert_eq!(ranked.len(), NodeKind::all().len());
    }

    #[test]
    fn recent_list_has_no_duplicates_and_is_capped() {
        let mut recent = Recent::default();
        for kind in NodeKind::all().iter().take(RECENT_LIMIT + 3) {
            recent.push(*kind);
        }
        recent.push(NodeKind::all()[RECENT_LIMIT + 2]);
        assert_eq!(recent.kinds().len(), RECENT_LIMIT);
        assert_eq!(recent.kinds()[0], NodeKind::all()[RECENT_LIMIT + 2]);
        let unique: std::collections::BTreeSet<_> = recent.kinds().iter().collect();
        assert_eq!(unique.len(), recent.kinds().len());
    }

    #[test]
    fn dropped_wire_prefers_nodes_that_accept_it() {
        let ranked = rank(&all(), "", &Recent::default(), Some(DataType::Number));
        let first = ranked.first().copied();
        assert!(first.is_some_and(|k| accepts_from(k, DataType::Number)));
    }

    #[test]
    fn natural_japanese_and_full_width_english_find_supported_nodes() {
        for (query, expected) in [
            ("円を作る", NodeKind::Circle),
            ("円を作りたい", NodeKind::Circle),
            ("円 を つくりたい", NodeKind::Circle),
            ("点を並べたい", NodeKind::LinearArray),
            ("点をならべたい", NodeKind::LinearArray),
            ("花瓶をねじりたい", NodeKind::Twist),
            ("花器をねじりたい", NodeKind::Twist),
            ("面を作りたい", NodeKind::Plane),
            ("曲面を作りたい", NodeKind::Loft),
            ("花瓶を作りたい", NodeKind::Vase),
            ("　ＣＩＲＣＬＥ　", NodeKind::Circle),
            ("点を並べる", NodeKind::LinearArray),
            ("面を作る", NodeKind::Plane),
            ("花瓶を作る", NodeKind::Vase),
            ("ねじる", NodeKind::Twist),
            ("Construct Point", NodeKind::Point),
            ("Number Slider", NodeKind::Number),
        ] {
            assert_eq!(
                rank(&all(), query, &Recent::default(), None).first(),
                Some(&expected),
                "{query}"
            );
        }
    }

    #[test]
    fn wire_search_never_suggests_an_incompatible_node() {
        for source in [
            DataType::Number,
            DataType::Point,
            DataType::Curve,
            DataType::Mesh,
            DataType::Geometry,
            DataType::Any,
        ] {
            let ranked = rank(&all(), "", &Recent::default(), Some(source));
            assert!(!ranked.is_empty());
            assert!(ranked.iter().all(|kind| accepts_from(*kind, source)));
        }
        assert!(rank(&all(), "円を作る", &Recent::default(), Some(DataType::Mesh)).is_empty());
        assert!(
            rank(
                &all(),
                "円を作りたい",
                &Recent::default(),
                Some(DataType::Mesh)
            )
            .is_empty()
        );
        let twists = rank(
            &all(),
            "花瓶をねじりたい",
            &Recent::default(),
            Some(DataType::Mesh),
        );
        assert_eq!(twists, vec![NodeKind::Twist]);
    }

    #[test]
    fn first_compatible_input_skips_incompatible_ports() {
        let key = first_compatible_input(NodeKind::Move, DataType::Number);
        assert!(key.is_some());
    }

    #[test]
    fn one_line_keeps_the_first_line_only() {
        assert_eq!(one_line("abc\ndef", 10), "abc");
        assert_eq!(one_line("あいうえお", 3), "あいう…");
        assert_eq!(one_line("", 3), "");
    }

    #[test]
    fn snapping_rounds_to_the_grid() {
        assert_eq!(snap(13.0, 24.0), 24.0);
        assert_eq!(snap(11.0, 24.0), 0.0);
        assert_eq!(snap(-13.0, 24.0), -24.0);
        assert_eq!(snap(13.0, 0.0), 13.0);
    }

    #[test]
    fn saved_theme_round_trips() {
        assert_eq!(parse_dark(format_dark(true)), Some(true));
        assert_eq!(parse_dark(format_dark(false)), Some(false));
        assert_eq!(parse_dark("garbage"), None);
    }
}
