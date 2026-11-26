//! Deterministic UI stress fixtures. A real geometry demo plus numeric subgraphs.
use morphlet::{model::NodeKind, persistence::{self, Project}, templates};
use std::path::Path;

pub fn make(count: usize) -> Result<Project, String> {
    let mut project = templates::create(0);
    let initial = project.graph.nodes.len();
    if initial == 0 || count < initial {
        return Err("UI fixture size is smaller than the demo".into());
    }
    project.name = format!("UI fixture · {count} nodes");
    let mut previous = None;
    for i in 0..count-initial {
        let col = i % 8;
        let row = i / 8;
        let kind = if col == 0 { NodeKind::Number } else { NodeKind::Add };
        let id = project.graph.add(kind, [650. + col as f32 * 290., 30. + row as f32 * 175.])?;
        if col != 0 && let Some(from) = previous {
            project.graph.connect(from, id, "a")?;
        }
        previous = Some(id);
        if let Some(node) = project.graph.node_mut(id) {
            node.name = format!("{} {:04}", kind.spec().label, i+1);
            node.preview = false;
        }
    }
    project.validate()?;
    Ok(project)
}

pub fn export(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for count in [10, 100, 1000] {
        persistence::save(&dir.join(format!("{count}.morph")), &make(count)?)?;
    }
    Ok(())
}
