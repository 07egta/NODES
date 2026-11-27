//! Canvas annotations stored in a work: coloured groups behind nodes and free text notes.
//!
//! They never affect evaluation. Old `.morph` files have neither (both fields use
//! `#[serde(default)]`), and empty lists are not written, so files without annotations
//! stay byte-for-byte compatible with older versions of the app.
use crate::model::{Graph, MAX_NODES, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_GROUPS: usize = 256;
pub const MAX_NOTES: usize = 256;
/// Bytes, UTF-8. Long enough for a paragraph of Japanese text.
pub const MAX_NOTE_BYTES: usize = 4_096;
pub const MAX_GROUP_NAME_BYTES: usize = 256;

/// Colours offered for groups (soft tints; drawn translucent behind the cards).
pub const GROUP_COLORS: [[u8; 3]; 6] = [
    [92, 160, 140],
    [96, 140, 200],
    [200, 150, 80],
    [170, 120, 200],
    [210, 110, 110],
    [130, 140, 150],
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub color: [u8; 3],
    /// Member node ids. Ids of deleted nodes are tolerated (ignored when drawing) and
    /// removed by [`prune`].
    pub nodes: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub text: String,
    /// Top-left corner in canvas (world) coordinates.
    pub position: [f32; 2],
}

/// `skip_serializing_if` helper for boolean flags that default to `false`.
pub fn is_false(value: &bool) -> bool {
    !*value
}

/// Size caps and basic sanity for hostile files. Called from `Graph::validate`.
pub fn validate(graph: &Graph) -> Result<(), String> {
    if graph.groups.len() > MAX_GROUPS {
        return Err(format!("グループは {MAX_GROUPS} 個までです。"));
    }
    if graph.notes.len() > MAX_NOTES {
        return Err(format!("メモは {MAX_NOTES} 個までです。"));
    }
    for group in &graph.groups {
        if group.name.len() > MAX_GROUP_NAME_BYTES || group.name.contains('\0') {
            return Err("グループ名が長すぎるか、使えない文字が含まれています。".into());
        }
        if group.nodes.len() > MAX_NODES {
            return Err(format!(
                "1 つのグループに入れられるノードは {MAX_NODES} 個までです。"
            ));
        }
    }
    for note in &graph.notes {
        if note.text.len() > MAX_NOTE_BYTES || note.text.contains('\0') {
            return Err(format!(
                "メモが長すぎるか、使えない文字が含まれています（上限 {MAX_NOTE_BYTES} バイト）。"
            ));
        }
        if !note
            .position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
        {
            return Err("メモの位置が無効です。".into());
        }
    }
    Ok(())
}

/// Drop ids of nodes that no longer exist, then groups that became empty.
pub fn prune(graph: &mut Graph) {
    let live: BTreeSet<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
    for group in &mut graph.groups {
        group.nodes.retain(|id| live.contains(id));
    }
    graph.groups.retain(|g| !g.nodes.is_empty());
}

/// Create a group from `ids` (existing, de-duplicated). Returns its index.
pub fn add_group(graph: &mut Graph, ids: &[NodeId], name: &str) -> Result<usize, String> {
    if graph.groups.len() >= MAX_GROUPS {
        return Err(format!("グループは {MAX_GROUPS} 個までです。"));
    }
    let live: BTreeSet<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
    let members: BTreeSet<NodeId> = ids.iter().copied().filter(|id| live.contains(id)).collect();
    if members.is_empty() {
        return Err("グループにするノードを選んでください。".into());
    }
    let name: String = truncate_bytes(name, MAX_GROUP_NAME_BYTES);
    let color = GROUP_COLORS[graph.groups.len() % GROUP_COLORS.len()];
    graph.groups.push(Group {
        name,
        color,
        nodes: members.into_iter().collect(),
    });
    Ok(graph.groups.len() - 1)
}

/// Add a note at `position`. Returns its index.
pub fn add_note(graph: &mut Graph, text: &str, position: [f32; 2]) -> Result<usize, String> {
    if graph.notes.len() >= MAX_NOTES {
        return Err(format!("メモは {MAX_NOTES} 個までです。"));
    }
    if !position
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
    {
        return Err("メモの位置が無効です。".into());
    }
    graph.notes.push(Note {
        text: truncate_bytes(text, MAX_NOTE_BYTES),
        position,
    });
    Ok(graph.notes.len() - 1)
}

/// Cut a string to at most `max` bytes on a character boundary.
pub fn truncate_bytes(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == '\0' {
            continue;
        }
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}
