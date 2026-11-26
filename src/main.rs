#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod app;
mod math_mapper_ui;
mod canvas;
mod data_view;
mod file_picker;
mod gcode_ui;
mod help;
mod script_ui;
mod stl_export_ui;
mod theme;
mod ui_polish;
mod viewport;

use morphlet::{engine::Evaluator, model::NodeKind, persistence, templates};
use std::path::{Path, PathBuf};

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--node-catalog") {
        match serde_json::to_string_pretty(&morphlet::graph_api::catalog()) {
            Ok(json) => println!("{json}"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--build-graph") {
        let result = match (args.get(1), args.get(2), args.len()) {
            (Some(input), Some(output), 3) => morphlet::graph_api::build_file(Path::new(input), Path::new(output)),
            _ => Err("Usage: --build-graph request.json output.morph".into()),
        };
        if let Err(e) = result { eprintln!("{e}"); std::process::exit(1); }
        return Ok(());
    }
    if matches!(args.first().map(String::as_str), Some("--inspect-ghx" | "--convert-ghx")) {
        let result = match (args.get(1), args.get(2), args.len()) {
            (Some(input), Some(output), 3) if args[0] == "--inspect-ghx" =>
                morphlet::ghx::inspect_file(Path::new(input), Path::new(output)),
            (Some(input), Some(output), 3) =>
                morphlet::ghx::convert_file(Path::new(input), Path::new(output)),
            _ => Err("Usage: --inspect-ghx input.ghx report.json | --convert-ghx input.ghx output.morph".into()),
        };
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--export-demos") {
        let path = args
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("examples"));
        if let Err(e) = export_demos(&path) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--validate") {
        let result = args
            .get(1)
            .ok_or_else(|| "ファイルを指定してください".to_string())
            .and_then(|p| persistence::load(Path::new(p)))
            .and_then(|p| {
                let e = Evaluator::default().evaluate(&p.graph, &|| false);
                let failures: Vec<_> = e
                    .outputs
                    .iter()
                    .filter_map(|(id, r)| r.as_ref().err().map(|m| format!("{id}: {m}")))
                    .collect();
                if failures.is_empty() {
                    println!(
                        "OK: {} · {} nodes · {:.3} ms",
                        p.name,
                        p.graph.nodes.len(),
                        e.elapsed_ms
                    );
                    Ok(())
                } else {
                    Err(failures.join("\n"))
                }
            });
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let screenshot = args
        .windows(2)
        .find(|a| a[0] == "--screenshot")
        .map(|a| PathBuf::from(&a[1]));
    let open_file = args
        .iter()
        .find(|a| a.ends_with(".morph"))
        .map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("NODES · Geometry Studio")
            .with_inner_size([1380., 920.])
            .with_min_inner_size([1100., 720.])
            .with_icon(icon()),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Morphlet",
        options,
        Box::new(move |cc| Ok(Box::new(app::MorphletApp::new(cc, open_file, screenshot)))),
    )
}

fn icon() -> eframe::egui::IconData {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let i = (y * 64 + x) * 4;
            let dx = x as f32 - 31.5;
            let dy = y as f32 - 31.5;
            let inside = dx.abs() < 29.
                && dy.abs() < 29.
                && (dx.abs() < 21.
                    || dy.abs() < 21.
                    || ((dx.abs() - 21.).powi(2) + (dy.abs() - 21.).powi(2)) < 64.);
            if inside {
                rgba[i..i + 4].copy_from_slice(&[28, 48, 48, 255]);
            }
            let d = ((dx.abs() - dy * 0.50 - 6.).abs() < 2.7) && dy > -16. && dy < 17.;
            if d {
                rgba[i..i + 4].copy_from_slice(&[172, 220, 182, 255]);
            }
            if dx * dx + (dy - 7.).powi(2) < 16. {
                rgba[i..i + 4].copy_from_slice(&[232, 201, 120, 255]);
            }
        }
    }
    eframe::egui::IconData {
        rgba,
        width: 64,
        height: 64,
    }
}

fn export_demos(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (index, template) in templates::TEMPLATES.iter().enumerate() {
        let p = templates::create(index);
        persistence::save(&dir.join(format!("{:02}.morph", index + 1)), &p)?;
        let e = Evaluator::default().evaluate(&p.graph, &|| false);
        for (id, r) in &e.outputs {
            if let Err(message) = r {
                return Err(format!("{} #{id}: {message}", template.name));
            }
        }
        let trees: Vec<_> = p
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Output)
            .filter_map(|n| e.outputs.get(&n.id).and_then(|v| v.as_ref().ok()).cloned())
            .collect();
        if trees.iter().any(|t| {
            t.branches
                .values()
                .flatten()
                .any(|v| matches!(v, morphlet::model::Value::Mesh(_)))
        }) {
            let r = persistence::export_stl(&dir.join(format!("{:02}.stl", index + 1)), &trees)?;
            println!(
                "{}: {} triangles / {} open edges / {:.3} ms",
                template.name, r.triangles, r.open_edges, e.elapsed_ms
            );
        } else {
            println!("{}: curve preview / {:.3} ms", template.name, e.elapsed_ms);
        }
    }
    Ok(())
}
