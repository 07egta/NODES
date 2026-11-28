# OMEGA 16 — Computational mechanism design (phase 1)

## Scope and interfaces
- Branch based on main ec3b840e81c76aa7e426da5dc47b716ba975a98e. PR #48 (merged) already provides SolidCraft NURBS/B-rep and the existing exact extrusion path. PR #80's 17 basic components are unrelated to mechanical hardware; no shared file from that branch is copied.
- New Japanese category 機構, eight append-only node kinds, stable snake_case IDs, no dependency, no persistence-schema or Value changes. Existing item access and ordinal matching apply. A numeric list is one output containing several items, not a new multi-port format.
- Mechanical profile and planar pose output are Curve with explicit sampled vertices (geometry=None). The existing curve/mesh and SolidCraft extrusion paths can consume them; the B-rep of a sampled profile has straight edges, not exact involute flanks. Exact extrusion and printing still need integration/physical tests.
- Physics (OMEGA 03) consumes the geometry via existing nodes and should own forces and dynamics. Manufacturing (OMEGA 06) owns tolerances/material compensation, printer verification. Constraint and core (OMEGA 10) should coordinate any future rigid-body/joint graph types, part IDs and multi-port migration. This branch is analytic and stateless.

## Nodes and mathematical conventions
- MechSpurGearProfile: full-depth, unshifted external spur model; pitch radius m*z/2; base radius r*cos(alpha); tip r+m; root r-1.25*m. Involute parameter t gives r(t)=base*sqrt(1+t²), inv(t)=t-atan(t). Half angular tooth thickness is pi/(2*z) - thinning/(2*r) + inv(tan(alpha)) - inv(t). Thinning is a single-gear circular tooth thickness reduction, not guaranteed pair backlash. Reject z*sin(alpha)² < 2, or vanishing tip/root gap. Flanks, tip arcs and root gaps are sampled; below base circle the root connection is radial, not a generated trochoid/fillet. No profile shift or undercut modeling.
- MechGearAngle: driven incremental angle = +/- driver teeth / driven teeth * driver angle. Same direction for internal relation; no contact proof.
- MechRackTravel: signed travel = pitch radius * angle in radians.
- MechFourBar: O2=(0,0), O4=(ground,0); select one of the two circle intersections for moving joint. Closed polyline O2-A-B-O4. Reject non-intersection or coincident centers; near tangencies use relative numeric tolerance 1e-12.
- MechSliderCrank: guide y=offset; slider x=a*cos(theta) +/- sqrt(b²-(offset-a*sin(theta))²). Reject unreachable guide.
- MechArm2r: open polyline origin-elbow-tip, relative second angle.
- MechArm2rInverse: cosine-law solution with positive/negative elbow angle; numeric list [q1,q2] in degrees; rejects unreachable targets and ambiguous origin.
- MechJacobian2r: four row-major partial derivatives of planar endpoint position, per radian, no pseudoinverse or singularity avoidance.
- Lengths use model units (normally mm); angles degrees except Jacobian derivative. No load, fatigue, ISO/JIS or print-fit guarantee.

## Example recipes (manual graph construction; no shipped .morph fixtures yet)
1. Gear: add 平歯車の歯形, m=2, z=24, alpha=20, thinning=0, samples=8. Connect to existing extrusion for a solid; use existing cylinder/circle and cut operations for shaft hole. Test mesh before STL. Expected pitch diameter 48, tip diameter 52, root diameter 43.
2. Four-bar: ground=60, crank=20, coupler=60, rocker=40. Edit angle or feed a Range/Series (one pose per item) to preview multiple states. At 45 degrees both assembly branches preserve all four lengths. This does not animate or solve collisions.
3. Robot arm: inverse (40,30,target 40,30) yields two angles; for branch 0 expected [0,90]. Pick list items 0 and 1 and wire to forward q1/q2.
4. Rack: m=2, z=20, angle=360 yields 40*pi model units; drive existing Move with signed displacement.

## Public references (behavior/math, not borrowed implementation)
- Autodesk Fusion Joint types: https://help.autodesk.com/cloudhelp/ENU/Fusion-Assemble/files/GUID-8818AE31-958A-4A59-989B-9875A174C67A.htm
- Onshape Relations: https://cad.onshape.com/help/Content/Assembly/relations.htm
- Northwestern Modern Robotics, velocity kinematics/Jacobian: https://modernrobotics.northwestern.edu/nu-gm-book-resource/velocity-kinematics-and-statics/
- KHK Technical Data, gear dimensions and undercutting, manufacturer engineering reference (not an ISO/JIS certification).
- FreeCAD public user-facing FCGear workbench description only. No copyleft implementation read or copied.

## Verification and continuation
- Rust tests authored in tests/mechanism_nodes_tests.rs; include default evaluation, persistence, analytical dimensions/periodicity, two linkage branches, unreachable geometry, FK/IK roundtrip, finite-difference Jacobian and cancellation.
- The Chat execution container cannot resolve github.com and has no cargo/rustc on PATH. Rust formatting, clippy, compile, tests, GUI, extrusion and STL are not yet verified. Do not mark this branch as complete or merge before CI is green.
- Next: independently validate tooth outline chord error and contact geometry; add actual example fixtures, extruded gear tests with SolidCraft, shaft/bore and root fillet, then planetary gears, revolute/prismatic joint graph, clearance/interference, travel visualization and multi-part BOM. General collision, time animation, strength and fit remain outside this phase.
