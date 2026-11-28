//! Mesh import (STL and OBJ) on small in-memory samples. Not compiled or run in the
//! session that wrote it; CI is the first check.
use nodes::import::{Format, format_of, parse_obj, parse_stl, place, read_file};
use nodes::model::{Mesh, NodeKind};
use std::collections::BTreeSet;
use std::path::Path;

type Tri = [[f32; 3]; 3];

fn no_cancel() -> bool {
    false
}

/// Unit cube on [0,1]^3 as 12 outward-facing triangles, each corner written out.
fn cube() -> Vec<Tri> {
    let p = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [1., 1., 1.],
        [0., 1., 1.],
    ];
    let faces: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2], // bottom
        [4, 5, 6],
        [4, 6, 7], // top
        [0, 1, 5],
        [0, 5, 4], // front
        [3, 6, 2],
        [3, 7, 6], // back
        [0, 4, 7],
        [0, 7, 3], // left
        [1, 2, 6],
        [1, 6, 5], // right
    ];
    faces.iter().map(|f| [p[f[0]], p[f[1]], p[f[2]]]).collect()
}

fn binary_stl(tris: &[Tri], header: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 80];
    out[..header.len()].copy_from_slice(header);
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for tri in tris {
        out.extend_from_slice(&[0u8; 12]);
        for point in tri {
            for value in point {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0u8; 2]);
    }
    out
}

fn ascii_stl(tris: &[Tri]) -> String {
    let mut text = String::from("solid cube\n");
    for tri in tris {
        text.push_str("  facet normal 0 0 0\n    outer loop\n");
        for point in tri {
            text.push_str(&format!(
                "      vertex {} {} {}\n",
                point[0], point[1], point[2]
            ));
        }
        text.push_str("    endloop\n  endfacet\n");
    }
    text.push_str("endsolid cube\n");
    text
}

/// Every triangle with three corners but no shared vertices, indexed as an OBJ file.
fn obj_text(tris: &[Tri]) -> String {
    let mut text = String::new();
    for tri in tris {
        for point in tri {
            text.push_str(&format!("v {} {} {}\n", point[0], point[1], point[2]));
        }
    }
    for i in 0..tris.len() {
        let b = 3 * i;
        text.push_str(&format!("f {} {} {}\n", b + 1, b + 2, b + 3));
    }
    text
}

/// Each undirected edge is used by exactly two triangles, once in each direction.
fn assert_closed(mesh: &Mesh) {
    let mut directed = BTreeSet::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            directed.insert((t[k], t[(k + 1) % 3]));
        }
    }
    assert_eq!(
        directed.len(),
        mesh.triangles.len() * 3,
        "an edge runs twice in the same direction"
    );
    for &(a, b) in &directed {
        assert!(
            directed.contains(&(b, a)),
            "edge {a}-{b} has no opposite side"
        );
    }
}

#[test]
fn binary_stl_counts_triangles_and_welds_the_cube() {
    let mesh = parse_stl(&binary_stl(&cube(), b"binary cube"), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.vertices.len(), 8);
    assert_closed(&mesh);
}

#[test]
fn ascii_stl_counts_triangles_and_welds_the_cube() {
    let mesh = parse_stl(ascii_stl(&cube()).as_bytes(), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.vertices.len(), 8);
    assert_closed(&mesh);
}

#[test]
fn binary_file_whose_header_starts_with_solid_is_still_binary() {
    let bytes = binary_stl(&cube(), b"solid but binary");
    let mesh = parse_stl(&bytes, &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    assert_closed(&mesh);
}

#[test]
fn obj_triangulates_polygons_and_ignores_normals_and_uvs() {
    let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvn 0 0 1\nvt 0 0\nf 1/1/1 2/2/1 3/3/1 4/4/1\n";
    let mesh = parse_obj(text.as_bytes(), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 2);
    assert_eq!(mesh.vertices.len(), 4);
}

#[test]
fn obj_accepts_negative_indices() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n";
    let mesh = parse_obj(text.as_bytes(), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
    assert_eq!(mesh.vertices.len(), 3);
}

#[test]
fn obj_cube_with_unshared_corners_is_welded_closed() {
    let mesh = parse_obj(obj_text(&cube()).as_bytes(), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.vertices.len(), 8);
    assert_closed(&mesh);
}

#[test]
fn truncated_binary_is_rejected() {
    let bytes = binary_stl(&cube(), b"");
    assert!(parse_stl(&bytes[..bytes.len() - 10], &no_cancel).is_err());
}

#[test]
fn huge_count_header_without_matching_size_is_rejected() {
    let mut bytes = vec![0u8; 84];
    bytes[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_stl(&bytes, &no_cancel).is_err());
}

#[test]
fn count_over_the_triangle_limit_is_rejected_before_parsing() {
    // 500,001 records: the size matches the header, so only the limit can reject it.
    let count: u32 = 500_001;
    let mut bytes = vec![0u8; 84 + 50 * count as usize];
    bytes[80..84].copy_from_slice(&count.to_le_bytes());
    assert!(parse_stl(&bytes, &no_cancel).is_err());
}

#[test]
fn garbage_bytes_are_rejected() {
    assert!(parse_stl(&[0xFF; 200], &no_cancel).is_err());
    assert!(parse_stl(b"hello", &no_cancel).is_err());
    assert!(parse_obj(&[0xFF, 0x00, b'v'], &no_cancel).is_err());
}

#[test]
fn nan_and_infinite_coordinates_are_rejected() {
    let mut bytes = binary_stl(&cube(), b"");
    bytes[84 + 12..84 + 16].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(parse_stl(&bytes, &no_cancel).is_err());

    let text = ascii_stl(&cube()).replacen("vertex 1 0 0", "vertex NaN 0 0", 1);
    assert!(text.contains("NaN"));
    assert!(parse_stl(text.as_bytes(), &no_cancel).is_err());

    let obj = "v inf 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    assert!(parse_obj(obj.as_bytes(), &no_cancel).is_err());
}

#[test]
fn ascii_stl_with_an_incomplete_facet_is_rejected() {
    let text = "solid x\n vertex 0 0 0\n vertex 1 0 0\nendsolid x\n";
    assert!(parse_stl(text.as_bytes(), &no_cancel).is_err());
}

#[test]
fn obj_index_out_of_range_or_zero_is_rejected() {
    let out_of_range = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n";
    assert!(parse_obj(out_of_range.as_bytes(), &no_cancel).is_err());
    let zero = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n";
    assert!(parse_obj(zero.as_bytes(), &no_cancel).is_err());
}

#[test]
fn place_scales_centres_and_puts_the_lowest_point_on_the_bed() {
    let mut mesh = parse_stl(&binary_stl(&cube(), b""), &no_cancel).unwrap();
    place(&mut mesh, 2.0, true, true).unwrap();
    // The unit cube scaled by 2 spans [0,2]^3. Centring X and Y gives [-1,1]; the bed sets Z to [0,2].
    let range = |axis: usize| {
        mesh.vertices
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[axis]), hi.max(p[axis]))
            })
    };
    assert_eq!(range(0), (-1.0, 1.0));
    assert_eq!(range(1), (-1.0, 1.0));
    assert_eq!(range(2), (0.0, 2.0));
}

#[test]
fn place_rejects_a_zero_scale() {
    let mut mesh = parse_stl(&binary_stl(&cube(), b""), &no_cancel).unwrap();
    assert!(place(&mut mesh, 0.0, false, false).is_err());
}

#[test]
fn file_kind_comes_from_the_extension() {
    assert_eq!(format_of(Path::new("part.STL")), Some(Format::Stl));
    assert_eq!(format_of(Path::new("part.obj")), Some(Format::Obj));
    assert_eq!(format_of(Path::new("part.3MF")), Some(Format::ThreeMf));
    assert_eq!(format_of(Path::new("work.morph")), None);
}

#[test]
fn empty_and_missing_paths_are_japanese_errors() {
    assert!(read_file("", &no_cancel).is_err());
    assert!(read_file("/nonexistent-morphlet-test/none.stl", &no_cancel).is_err());
}

#[test]
fn import_node_keeps_its_serde_name_and_is_listed() {
    assert_eq!(
        serde_json::to_string(&NodeKind::ImportMesh).unwrap(),
        "\"import_mesh\""
    );
    assert!(NodeKind::all().contains(&NodeKind::ImportMesh));
}

#[test]
fn ascii_stl_rejects_vertices_spanning_facets() {
    let text = "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\nfacet normal 0 0 1\nouter loop\nvertex 0 1 0\nendloop\nendfacet\nendsolid x\n";
    assert!(parse_stl(text.as_bytes(), &no_cancel).is_err());
}

#[test]
fn ascii_stl_rejects_missing_endsolid() {
    let text = ascii_stl(&cube());
    let truncated = text.strip_suffix("endsolid cube\n").unwrap();
    assert!(parse_stl(truncated.as_bytes(), &no_cancel).is_err());
}

#[test]
fn obj_accepts_inline_comments_on_faces() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3 # triangle\n";
    let mesh = parse_obj(text.as_bytes(), &no_cancel).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
}
