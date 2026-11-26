//! Pure helpers for the node search popup, the recently used list, grid snapping,
//! the saved theme choice and the shortcut cheat-sheet. No egui drawing happens here,
//! so the ranking and persistence rules can be unit tested on their own.

use morphlet::model::{DataType, NodeKind};
use std::path::PathBuf;

/// How many recently added nodes are remembered (newest first).
pub const RECENT_LIMIT: usize = 8;
/// Bonus per position in the recent list: the newest node gets `RECENT_LIMIT * RECENT_STEP`.
const RECENT_STEP: i32 = 4;
/// Bonus for a node that can take the wire being dragged from a port.
const COMPAT_BONUS: i32 = 40;
/// Typo tolerance (subsequence match) is only used for tokens at least this long.
const MIN_FUZZY_CHARS: usize = 3;

/// Lower-cases ASCII, turns full-width spaces into spaces and katakana into hiragana,
/// so that「マル」「まる」「Maru」-style input can be compared on one footing.
pub fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
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
        "ならべる" => "配列",
        "おしだし" => "押し出し",
        _ => token,
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
    let tokens: Vec<String> = normalize(query)
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
/// Recently used nodes get a bonus that is larger for newer entries, and when a wire
/// is being dropped, nodes that can take it get a bonus too.
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
            let mut score = query_score(*kind, query)?;
            if let Some(position) = recent.position(*kind) {
                let freshness = RECENT_LIMIT.saturating_sub(position) as i32;
                score += freshness * RECENT_STEP;
            }
            if wire.is_some_and(|t| accepts_from(*kind, t)) {
                score += COMPAT_BONUS;
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
    ("Tab / Ctrl+K", "ノードを検索して追加（ポインタがキャンバス上ならその位置）"),
    ("Space を押して離す", "キャンバス上でノードを検索して追加（Space+ドラッグは移動）"),
    ("ノード棚のボタンをドラッグ", "キャンバスの好きな場所に配置"),
    ("空きをダブルクリック", "その場所にノードを追加"),
    ("出力の丸から空きへドラッグ", "検索して、つないだ状態で追加"),
    ("Ctrl+E", "選択ノードを無効 / 有効"),
    ("Ctrl+G", "選択をグループで囲む"),
    ("Ctrl+Shift+G", "格子への吸着を切り替え"),
    ("F / Home", "形とキャンバスを画面に収める"),
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
