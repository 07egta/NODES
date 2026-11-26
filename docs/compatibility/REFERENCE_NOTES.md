# Reference boundaries and inventory provenance

Reviewed on 2026-11-26. These notes record public specifications, not results
from running Rhino or Grasshopper. No proprietary SDK binary or source is
required by, or copied into, NODES.

## What the generated inventory means

Run `cargo run --locked --example coverage_inventory` after every integration.
The JSON is generated from the compiled `NodeKind::all()` and `NodeSpec`, not
from a manually maintained count. `Cluster` is listed separately as a
definition-only registration. The report includes serialized IDs, port order,
types, numeric defaults and bounds, descriptions and search keywords.

Use `--source-revision <full-git-sha>` to attach the commit being examined. The
revision is supplied by the caller and does not assert that their worktree was
clean. Record `git status --short` separately when publishing measurements.
`spec_fingerprint` is a deterministic FNV-1a change detector over metadata,
not a cryptographic signature, implementation hash or equivalence certificate.

The baseline `NodeSpec` does not declare Item/List/Tree access. Therefore the
exporter uses `access: null` rather than guessing from a port type or numeric
default. An integration that introduces explicit access metadata can extend
the exporter from that single source. A UI access label alone must not be
mistaken for complete mixed-access scheduling: some legacy setting inputs
only read their first item.

`verified_count` is null because registration cannot establish verification.
`evaluation_status` and `compatibility` are `NOT_INFERRED`. The compatibility
catalog uses the requested statuses VERIFIED, IMPLEMENTED, PARTIAL,
APPROXIMATE, MISSING, BLOCKED and UNKNOWN. IMPLEMENTED is deliberately weaker
than VERIFIED. Each VERIFIED row must identify a GH family, exact build and
GUID and point to an archived JSON oracle fixture with matching provenance
and actual cases. NODES unit tests alone do not establish GH compatibility.

The curated catalog is a starting set, not a denominator for a percentage of
all GH functionality. Its `baseline_commit` must be advanced by a deliberate
implementation audit; regenerating the registration inventory cannot safely
upgrade compatibility statuses or limitations.

## Version-specific official catalogs

- [GH1 online component documentation](https://www.rhino3d.com/docs/grasshopper/)
  is a JavaScript application. Search results can expose individual component
  pages, while a plain HTTP text view may return only the application shell.
  For example the official
  [Curve-Curve page](https://www.rhino3d.com/docs/grasshopper/Intersect/Curve-Curve/)
  documents its curve inputs and parameter outputs. A failed text extraction
  is not evidence that the component or official documentation is absent.
- The [GH2 documentation home](https://www.rhino3d.com/docs/grasshopper2/)
  identifies its export as 2026-11-25, for Rhino/GH2 9.3.26329.6001. Its
  [component index](https://www.rhino3d.com/docs/grasshopper2/specs/) contains
  1,419 entries, including components and parameters. Only 69 have expanded
  articles; other descriptions are generated from component metadata.
  These numbers belong to this export, not GH1 or an immutable final standard.

The inventories must keep GH1 and GH2 separate. Plugin components, historic
UI behavior and experimental functionality require their own classification.
An official description establishes intended behavior but is not an
executable oracle for defaults, tolerances or malformed-input handling.

## Data access and matching

The official [GH1 Python scripting guide](https://developer.rhino3d.com/guides/scripting/scripting-gh-python/)
shows the distinction concretely: Item access receives a value, List access
receives one branch's list, and Tree access receives the complete tree.
The [GH_ParamAccess enum](https://developer.rhino3d.com/api/gh1/type/grasshopper.kernel.gh-paramaccess/)
should be read with these examples, because its terse List description can
otherwise be mistaken for access to the whole tree.

[Introduction to Data Structures, section 2.4](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/data-structures/)
defines longest matching by repeating the shorter list's last element,
shortest matching by truncation, and cross matching by all combinations.
Repeating the entire shorter list is a distinct operation. The historic
matching menus were replaced by explicit matching components, as described
by [David Rutten in 2013](https://www.grasshopper3d.com/m/discussion?id=2985220%3ATopic%3A977909).

[Advanced Data Structures, section 3.3](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/advanced-data-structures/)
documents branch repetition. [David Rutten's branch-order explanation](https://discourse.mcneel.com/t/data-matching-tree/59746)
confirms that ordinary component evaluation pairs branches by order, rather
than joining equal path labels. Components with custom access rules may
override this. Mixed-depth output path construction still needs reference
fixtures; a NODES topology policy must be labeled as such.

### Empty, null, missing and simplification are not interchangeable

- [GH_Path](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_Data_GH_Path.htm)
  marks an empty path or a path containing negative indices invalid.
- [DataTree<T>](https://developer.rhino3d.com/api/gh1/type/grasshopper.datatree-1/)
  counts null items and can clear items while retaining branches. An empty
  tree, an empty branch, a null item and an empty string are distinct cases.
- [GH_GraftMode](https://developer.rhino3d.com/api/gh1/type/grasshopper.kernel.data.gh-graftmode/)
  has independent flags for retaining nulls and empty lists.
- [GH_SimplificationMode](https://developer.rhino3d.com/api/gh1/type/grasshopper.kernel.data.gh-simplificationmode/)
  distinguishes removing shared leading indices from removing shared indices
  throughout paths. Those are separate contracts.
- GH1's UI simplification of a single branch differs from the scripting
  helper. [David Rutten's UI explanation](https://discourse.mcneel.com/t/problem-with-simplify-in-gh-data-structures/75055)
  leaves a lone branch alone; the
  [DataTree<T>.SimplifyPaths API](https://developer.rhino3d.com/api/grasshopper/html/M_Grasshopper_DataTree_1_SimplifyPaths.htm)
  documents retaining its last index. NODES must name the policy it implements.
- [David Rutten on Flip Matrix](https://www.grasshopper3d.com/m/discussion?id=2985220%3ATopic%3A927847)
  describes matrix-like trees whose paths differ at only one position. An
  arbitrary-path transpose should not be labeled equivalent without testing.

The [GH2 migration guide](https://developer.rhino3d.com/en/guides/grasshopper2/migrating-components-to-gh2/)
describes immutable trees/twigs, per-item metadata, distinct null/missing
requirements, and an inherently parallel and cancellable solver. Its types
and scheduling rules cannot simply be inferred from GH1 behavior.
The [GH2 data-tree definition](https://www.rhino3d.com/docs/grasshopper2/terms/data-tree.html)
explicitly allows empty lists and null values. The
[GH2 Expand component](https://www.rhino3d.com/docs/grasshopper2/specs/0000000097e941db8ff90a49b27bd09c.html)
documents eight lengthening methods, with trailing repetition as the default.

## Curve and surface representation

[NURBS Geometry Overview](https://developer.rhino3d.com/guides/opennurbs/nurbs-geometry-overview/)
and [Superfluous Knots](https://developer.rhino3d.com/en/guides/opennurbs/superfluous-knots/)
explain Rhino's knot convention. With N control points and degree d, Rhino
stores N+d-1 knots, while the complete textbook vector has N+d+1. Conversion
must account for the two omitted end knots; degree is order minus one.
Weights, degree and knots are mathematical geometry, not tessellation data.

The automatically generated
[GH2 Curve Control Points page](https://www.rhino3d.com/docs/grasshopper2/specs/00000000fa22444194cbf8e625c38513.html)
contains a degree/count relationship inconsistent with the overview above.
Treat it as an unresolved documentation discrepancy, not an algorithm to
copy. Validate rational evaluation against analytic geometry independently.

Useful independent fixtures are a degree-one segment, a rational quadratic
quarter circle, a bilinear plane patch, and a rational cylinder. Check points,
derivatives, curvature and domain endpoints; knot insertion should preserve
the mathematical curve. Display tessellation changes must not change those
analytic results. Approximate operations need a stated error measure.

## Performance evidence

`coverage_perf` measures headless evaluator cold/warm runs and 50 successive
slider edits. Its measured cache hits are useful for incremental-evaluation
regressions. It does not measure application startup, rendering frame time,
idle CPU, memory, Windows performance or GPU behavior. An attempted graph
above `MAX_NODES` is reported as REJECTED rather than as a successful large
graph benchmark. Preserve the source revision, compiler, build profile and
host alongside a published report; do not infer workstation performance
from a container run.
