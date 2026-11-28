//! Real editable-example workflows: change a fixed design parameter, save to disk,
//! reload, evaluate with a fresh cache, and edit again. No invalid sample is skipped.
//! STL checks preserve triangle count and bounds within f32/1e-6 mm welding error;
//! they are interchange regressions, not evidence of successful physical printing.
use nodes::{
    engine::{Evaluation, Evaluator},
    import,
    model::{DataType, Mesh, NodeId, Tree, Value},
    persistence::{self, Project},
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
struct Fixture {
    path: &'static str,
    edit_node: NodeId,
    parameter: &'static str,
    value: f64,
    terminal: NodeId,
    output_kind: DataType,
    stl_roundtrip: bool,
}

// Each edit changes a design dimension or field, not node position or preview detail.
// Output-only examples use Output; analysis examples explicitly select a terminal
// point/curve/numeric result. A terminal is checked to have no downstream consumers.
macro_rules! example_cases {
    ($(($test:ident, $path:literal, $node:literal, $key:literal, $value:expr,
        $output:literal, $kind:ident, $stl:literal)),+ $(,)?) => {
        const FIXTURES: &[Fixture] = &[$(Fixture {
            path: $path, edit_node: $node, parameter: $key, value: $value,
            terminal: $output, output_kind: DataType::$kind, stl_roundtrip: $stl,
        }),+];
        $(#[test]
        fn $test() {
            verify_edit_workflow(Fixture {
                path: $path, edit_node: $node, parameter: $key, value: $value,
                terminal: $output, output_kind: DataType::$kind, stl_roundtrip: $stl,
            });
        })+
    }
}

example_cases! {
    (twisted_vase, "01.morph", 0, "height", 112.0, 2, Mesh, true),
    (light_ring, "02.morph", 1, "x", 75.0, 3, Mesh, true),
    (star_totem, "03.morph", 1, "height", 120.0, 3, Mesh, false),
    (helix_sketch, "04.morph", 0, "radius", 35.0, 1, Curve, false),
    (wave_blocks, "05.morph", 2, "amplitude", 25.0, 3, Mesh, false),
    (profile_loft, "06.morph", 1, "step", 24.0, 5, Mesh, true),
    (sine_tower, "07.morph", 2, "amp", 16.0, 6, Mesh, false),
    (power_mountains, "08.morph", 2, "b", 4.0, 7, Mesh, false),
    (noise_vessel, "09.morph", 2, "height", 132.0, 7, Mesh, false),
    (attractor_pattern, "10.morph", 1, "x", 45.0, 8, Mesh, false),
    (pipe_helix_lamp, "11.morph", 0, "radius", 34.0, 2, Mesh, false),
    (gyroid_box, "12.morph", 0, "height", 36.0, 3, Mesh, false),
    (circle_divisions, "curves/11-circle-divisions.morph", 0, "radius", 36.0, 1, Point, false),
    (helix_frames, "curves/12-helix-frames.morph", 0, "radius", 30.0, 2, Point, false),
    (furniture_profile, "curves/13-furniture-profile.morph", 2, "amp", 25.0, 4, Curve, false),
    (offset_preview, "curves/14-offset-preview.morph", 1, "distance", 5.0, 1, Curve, false),
    (line_intersection, "curves/15-line-intersection.morph", 2, "x", -10.0, 6, Point, false),
    (trimmed_loft, "curves/16-trim-loft.morph", 1, "end", 72.0, 4, Mesh, false),
    (twisted_tower, "generative/01-twisted-tower.morph", 1, "height", 180.0, 4, Mesh, false),
    (voronoi_lamp, "generative/02-voronoi-lamp.morph", 2, "radius", 32.0, 3, Curve, false),
    (wave_facade, "generative/03-wave-facade.morph", 2, "amplitude", 12.0, 4, Mesh, true),
    (slat_chair, "generative/04-slat-chair.morph", 0, "depth", 50.0, 11, Mesh, false),
    (spiral_stair, "generative/05-spiral-stair.morph", 3, "step", 5.0, 6, Mesh, true),
    (honeycomb, "generative/06-honeycomb.morph", 1, "height", 6.0, 2, Mesh, false),
    (organic_sculpture, "generative/07-organic-sculpture.morph", 1, "amp", 8.0, 3, Mesh, false),
    (fractal_tree, "generative/08-fractal-tree.morph", 0, "length", 42.0, 1, Curve, false),
    (density_wall, "generative/09-density-wall.morph", 7, "height", 7.0, 10, Mesh, false),
    (surface_lattice, "generative/10-surface-lattice.morph", 1, "radius", 38.0, 2, Curve, false),
    (ornament_truchet_cylinder, "ornament/truchet-cylinder.morph", 0, "seed", 27.0, 2, Curve, false),
    (ornament_asanoha_screen, "ornament/asanoha-screen.morph", 0, "size", 15.0, 1, Curve, false),
    (ornament_shippo_jewelry, "ornament/shippo-jewelry.morph", 0, "ratio", 0.8, 1, Curve, false),
    (ornament_weighted_cells, "ornament/weighted-cells.morph", 1, "step", 25.0, 3, Curve, false),
    (ornament_centroidal_cells, "ornament/centroidal-cells.morph", 1, "steps", 8.0, 3, Curve, false),
    (hollow_vessel_checks, "mesh-fabrication/hollow-vessel-check.morph", 2, "height", 132.0, 7, Mesh, false),
    (multiple_parts, "mesh-fabrication/multi-part-check.morph", 0, "width", 24.0, 3, Mesh, true),
    (rounded_box, "mesh-fabrication/rounded-box.morph", 0, "width", 60.0, 3, Mesh, true),
    (twisted_vase_checks, "mesh-fabrication/twisted-vase-check.morph", 0, "height", 112.0, 2, Mesh, false),
    (wave_panel_checks, "mesh-fabrication/wave-panel-check.morph", 2, "amplitude", 25.0, 3, Mesh, false),
    (solid_box_intersection, "surface-box-intersection.morph", 1, "x", 18.0, 5, Mesh, true),
    (surface_curvature, "surface-curvature-study.morph", 0, "minor", 10.0, 5, Number, false),
    (exact_surface_cylinder, "surface-exact-cylinder.morph", 1, "height", 80.0, 6, Surface, false),
    (nurbs_panel, "surface-panel.morph", 0, "rise", 32.0, 2, Surface, false),
    (ruled_surface, "surface-ruled.morph", 0, "height", 28.0, 3, Surface, false),
    (revolved_nurbs_vase, "surface-vase.morph", 0, "middle", 42.0, 2, Surface, false),
}

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn new() -> Self {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nodes-example-edit-{}-{time}-{sequence}",
            std::process::id()
        ));
        // Create exactly one new directory: never reuse or clean an existing path.
        fs::create_dir(&path).expect("create private example test directory");
        Self(path)
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}
fn collect_examples(root: &Path, dir: &Path, found: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_examples(root, &path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "morph")
        {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            assert!(found.insert(relative));
        }
    }
}

#[test]
fn every_committed_example_has_an_explicit_parameter_edit_fixture() {
    let root = examples_root();
    let mut discovered = BTreeSet::new();
    collect_examples(&root, &root, &mut discovered);
    let covered: BTreeSet<_> = FIXTURES
        .iter()
        .map(|fixture| fixture.path.to_owned())
        .collect();
    assert!(
        FIXTURES.len() >= 20,
        "at least twenty distinct editable example graphs"
    );
    assert_eq!(covered.len(), FIXTURES.len(), "duplicate fixture path");
    assert_eq!(
        covered, discovered,
        "new/renamed examples need explicit edit fixtures"
    );
    assert!(
        FIXTURES
            .iter()
            .filter(|fixture| fixture.stl_roundtrip)
            .count()
            >= 3
    );
}

fn verify_evaluation(project: &Project, result: &Evaluation, fixture: Fixture, phase: &str) {
    assert!(!result.cancelled, "{}: {phase} cancelled", fixture.path);
    assert_eq!(
        result.outputs.len(),
        project.graph.nodes.len(),
        "{}: {phase} missing results",
        fixture.path
    );
    for (id, output) in &result.outputs {
        assert!(
            output.is_ok(),
            "{}: {phase} node {id} failed: {output:?}",
            fixture.path
        );
    }
    let terminal = result.outputs[&fixture.terminal].as_ref().unwrap();
    assert!(
        terminal.item_count() > 0,
        "{}: {phase} terminal result is empty",
        fixture.path
    );
    for value in terminal.branches.values().flatten() {
        assert_eq!(
            value.kind(),
            fixture.output_kind,
            "{}: {phase} changed the result type",
            fixture.path
        );
    }
}

fn verify_edit_workflow(fixture: Fixture) {
    let path = examples_root().join(fixture.path);
    let mut project =
        persistence::load(&path).unwrap_or_else(|error| panic!("{}: load: {error}", fixture.path));
    assert!(
        project.graph.node(fixture.terminal).is_some(),
        "{}: terminal missing",
        fixture.path
    );
    assert!(
        !project.graph.nodes.iter().any(|node| node
            .inputs
            .values()
            .any(|&source| source == fixture.terminal)),
        "{}: fixture must observe a terminal result",
        fixture.path
    );
    let mut evaluator = Evaluator::default();
    let original = evaluator.evaluate(&project.graph, &|| false);
    verify_evaluation(&project, &original, fixture, "original");
    let original_value;
    {
        let node = project.graph.node_mut(fixture.edit_node).unwrap();
        assert!(!node.off, "{}: cannot test a disabled edit", fixture.path);
        assert!(
            !node.inputs.contains_key(fixture.parameter),
            "{}: wire overrides edited parameter",
            fixture.path
        );
        let spec = node
            .kind
            .spec()
            .inputs
            .iter()
            .find(|port| port.key == fixture.parameter && port.default.is_some())
            .expect("fixture parameter must be a declared editable numeric input");
        assert!(
            fixture.value.is_finite() && fixture.value >= spec.min && fixture.value <= spec.max,
            "{}: fixture edit is outside declared limits",
            fixture.path
        );
        assert!(!spec.integer || fixture.value.fract() == 0.0);
        original_value = *node
            .params
            .get(fixture.parameter)
            .expect("fixture must name an explicitly saved parameter");
        assert_ne!(
            original_value, fixture.value,
            "{}: fixture makes no edit",
            fixture.path
        );
        node.params.insert(fixture.parameter.into(), fixture.value);
    }
    project
        .validate()
        .unwrap_or_else(|error| panic!("{}: edited project invalid: {error}", fixture.path));
    let edited = evaluator.evaluate(&project.graph, &|| false);
    verify_evaluation(&project, &edited, fixture, "edited");
    assert!(
        edited.computed > 0,
        "{}: edit reused every stale result",
        fixture.path
    );
    assert_ne!(
        edited.outputs[&fixture.terminal], original.outputs[&fixture.terminal],
        "{}: design edit did not change its final output",
        fixture.path
    );

    let temporary = TemporaryDirectory::new();
    let saved = temporary.0.join("edited.morph");
    persistence::save(&saved, &project)
        .unwrap_or_else(|error| panic!("{}: save: {error}", fixture.path));
    assert!(fs::metadata(&saved).unwrap().len() > 0);
    let mut loaded = persistence::load(&saved)
        .unwrap_or_else(|error| panic!("{}: reload: {error}", fixture.path));
    assert_eq!(
        loaded, project,
        "{}: real file roundtrip changed project data",
        fixture.path
    );
    let mut fresh = Evaluator::default();
    let restored = fresh.evaluate(&loaded.graph, &|| false);
    verify_evaluation(&loaded, &restored, fixture, "reloaded with fresh evaluator");
    assert_eq!(restored.cached, 0);
    assert_eq!(
        restored.outputs, edited.outputs,
        "{}: persisted edit changed evaluation",
        fixture.path
    );
    if fixture.stl_roundtrip {
        verify_stl_roundtrip(
            restored.outputs[&fixture.terminal].as_ref().unwrap(),
            &temporary.0,
            fixture,
        );
    }

    // A second edit after reopening proves the saved project remains editable.
    loaded
        .graph
        .node_mut(fixture.edit_node)
        .unwrap()
        .params
        .insert(fixture.parameter.into(), original_value);
    let edited_again = fresh.evaluate(&loaded.graph, &|| false);
    verify_evaluation(&loaded, &edited_again, fixture, "edited after reload");
    assert_eq!(
        edited_again.outputs, original.outputs,
        "{}: restoring the original parameter did not restore the original model",
        fixture.path
    );
}

fn triangle_bounds(meshes: &[&Mesh]) -> [[f64; 3]; 2] {
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    // STL carries triangle corners; unused vertices cannot be expected to survive.
    for mesh in meshes {
        for triangle in &mesh.triangles {
            for &index in triangle {
                let point = mesh.vertices[index as usize];
                for axis in 0..3 {
                    assert!(point[axis].is_finite());
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
            }
        }
    }
    bounds
}

fn verify_stl_roundtrip(tree: &Tree, directory: &Path, fixture: Fixture) {
    assert_eq!(fixture.output_kind, DataType::Mesh);
    let meshes: Vec<&Mesh> = tree
        .branches
        .values()
        .flatten()
        .map(|value| match value {
            Value::Mesh(mesh) => mesh.as_ref(),
            _ => panic!(
                "{}: STL fixture would silently skip non-mesh data",
                fixture.path
            ),
        })
        .collect();
    let triangles: usize = meshes.iter().map(|mesh| mesh.triangles.len()).sum();
    assert!(triangles > 0);
    let before = triangle_bounds(&meshes);
    let path = directory.join("edited.stl");
    let report = persistence::export_stl(&path, std::slice::from_ref(tree))
        .unwrap_or_else(|error| panic!("{}: STL export: {error}", fixture.path));
    assert_eq!(report.triangles, triangles);
    assert_eq!(report.skipped_values, 0);
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        84 + 50 * triangles as u64
    );
    let imported = import::read_file(path.to_str().unwrap(), &|| false)
        .unwrap_or_else(|error| panic!("{}: STL reimport: {error}", fixture.path));
    assert_eq!(
        imported.triangles.len(),
        triangles,
        "{}: STL importer dropped or added triangles",
        fixture.path
    );
    let after = triangle_bounds(&[&imported]);
    for end in 0..2 {
        for axis in 0..3 {
            // One f32 relative epsilon plus two 1e-6 mm weld cells. This is a
            // representation tolerance, not a guessed manufacturing tolerance.
            let tolerance = before[end][axis].abs().max(1.) * f64::from(f32::EPSILON) + 2e-6;
            assert!(
                (after[end][axis] - before[end][axis]).abs() <= tolerance,
                "{}: STL bounds {end}/{axis}: {} != {} (tolerance {tolerance})",
                fixture.path,
                after[end][axis],
                before[end][axis]
            );
        }
    }
    let imported_tree = Tree::single(Value::Mesh(Arc::new(imported)));
    let imported_report = persistence::mesh_stats(&[imported_tree]).unwrap();
    assert_eq!(imported_report.degenerate_triangles, 0);
    if report.is_closed() {
        assert!(
            imported_report.is_closed(),
            "{}: STL roundtrip broke closed indexed topology",
            fixture.path
        );
    }
}
