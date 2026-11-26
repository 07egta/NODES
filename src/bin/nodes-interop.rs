//! Separate console-subsystem entry point for Windows and other platforms.
use morphlet::{ghx, graph_api, node_packs};
use std::path::Path;

fn run(args: &[String]) -> Result<(),String> {
    match args {
        [command,input,output] if command == "inspect-ghx" =>
            ghx::inspect_file(Path::new(input),Path::new(output)),
        [command,input,output] if command == "convert-ghx" =>
            ghx::convert_file(Path::new(input),Path::new(output)),
        [command] if command == "catalog" => {
            println!("{}",serde_json::to_string_pretty(&graph_api::catalog()).map_err(|e| e.to_string())?);
            Ok(())
        }
        [command,input,output] if command == "build-graph" =>
            graph_api::build_file(Path::new(input),Path::new(output)),
        [command,manifest,part,output] if command == "instantiate-pack" =>
            node_packs::instantiate_file(Path::new(manifest),Path::new(part),Path::new(output)),
        _ => Err("Usage: nodes-interop inspect-ghx input.ghx report.json | convert-ghx input.ghx output.morph | catalog | build-graph request.json output.morph | instantiate-pack manifest.json part.morphpart output.morph".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
