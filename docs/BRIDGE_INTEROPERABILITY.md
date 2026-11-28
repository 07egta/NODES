# PROJECT BRIDGE — CAD interoperability (phase 1, draft)

## Scope

The mesh import node now accepts STL, OBJ and a deliberately limited 3MF Core subset.
It reads the ZIP central directory, validates the start-model relationship, checks
CRC/size and imports indexed triangle geometry in millimetres. Stored and raw
DEFLATE entries are supported. Supported unit conversions: micron, millimeter,
centimeter, inch, foot and meter. Build items instantiate meshes with row-vector
affine transforms; reflected instances reverse winding. Multiple build items
are combined in a single mesh without welding across items. Disable the existing
centre/bed inputs to retain absolute file placement. Extra node scale is applied
after unit conversion.

This is geometry-only import: names, colour, base materials, print settings and
the editable source graph are not retained. Only the conventional
/3D/3dmodel.model start part and UTF-8 Core flat model meshes are accepted.
Components/assemblies, required extensions, extension attributes/elements,
non-model object types, alternative model part paths, ZIP64, encrypted or
multidisk ZIP are unsupported and rejected. No archive contents are extracted to
disk; no external relationships are followed. This is not a full OPC/3MF
validator (e.g. content types and unused parts are not validated). It does not
prove self-intersection freedom, closed topology, shell nesting or printability.

Limits: 64 MiB input/model XML, 256 KiB start relationships, 4096 ZIP entries and
resources, 250k total resource and expanded vertices, 500k total resource and
expanded triangles, finite coordinates and bounded XML complexity. Decompression
is bounded but not interruptible internally; cancellation is checked around it.
The existing miniz_oxide 0.9.1 package is now used directly with its existing
MIT-selected license; the lockfile already included it. No proprietary SDK
or new package version is introduced.

## Compatibility baseline

| Format | Existing capability / this branch | Not claimed |
|---|---|---|
| STL | binary/ASCII mesh import/export | units are not embedded in standard STL; CAD history |
| 3MF | experimental Core export; flat Core geometry import | full 3MF, slicer projects, material round-trip |
| OBJ | geometry import/export | texture/material fidelity |
| PLY | binary export | import |
| STEP, IGES | existing B-rep import/export nodes | full AP242, assemblies, external CAD equivalence |
| GHX | bounded inspection and limited scalar conversion | binary GH, arbitrary component execution |
| GLB/glTF, SVG, DXF, 3DM | not added in this change | any new support |
| CSV, JSON | not extended in this change | general geometry round-trip |

## Verification gates

- Run cargo fmt --all -- --check, cargo test --locked --all-targets on Linux/Windows.
- Run the new integration tests for Core round-trip, independent raw-DEFLATE fixture,
  multiple placements, units, reflected winding, corrupt archives, unsupported
  components and extensions, and cancellation.
- In Bambu Studio, OrcaSlicer and PrusaSlicer open a NODES 20 mm cube and two
  separate parts in STL and 3MF; verify dimensions, orientation, body count and
  layer preview. Save a simple geometry 3MF and test NODES import; record rejection
  if an unsupported project/extension is used.
- In Rhino, Fusion, Plasticity and Blender check applicable STL/OBJ/STEP/IGES
  dimensions and topology. Record software versions, actual observations and
  losses. Do not mark formats compatible solely from internal round-trip tests.

The authoring environment cannot execute Rust or launch these applications.
These are pending verification gates, not claims that they passed.

References:
- https://3mf.io/spec/
- https://github.com/3MFConsortium/spec_core
- https://docs.rs/miniz_oxide/0.9.1/miniz_oxide/inflate/fn.decompress_to_vec_with_limit.html
- https://github.com/3MFConsortium/lib3mf (BSD-2-Clause, possible independent future validator; not integrated)

## STL hardening

ASCII STL now uses a facet/loop state machine: a facet must contain exactly
three vertices and finish before the next facet, and geometry after endsolid
is rejected unless a new solid begins. UTF-8 BOM, whitespace and multiple
solids are accepted. The raw facet count is bounded even when degenerate
faces are dropped. Binary STL normals are calculated from the quantized f32
vertices actually written to the file. Regression tests cover both changes.
