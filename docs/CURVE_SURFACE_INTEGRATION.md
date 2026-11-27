# Exact curve/surface integration

Status: local Rust 1.100 compilation and the 23 curve/surface integration tests passed on 2026-11-26. Command: `cargo test --locked --test curve_geometry_tests --test curve_surface_integration --test surface_kernel_tests`. The integration owner records final whole-product results in `TEST_RESULTS.md` and `docs/development/STATUS.md`. This document does not declare GH compatibility.

## Preserved mathematical data

`Curve.geometry` and `SurfaceValue.geometry` retain mathematical geometry; display samples are separate. Surface nodes now consume exact NURBS profiles when present. A curve display resolution of 8, 16, 96 or 256 points must not change the resulting extrusion.

- **SurfaceExtrude:** exact extrusion of a full, clamped, nonperiodic NURBS curve in Z. Maximum 32 U controls and degree 8 in the surface representation; old polyline inputs remain piecewise linear. The original profile's mathematical geometry is translated with its display points.
- **RuledSurface:** exact Euclidean interpolation between compatible NURBS profiles with equal normalized knots, degree, rational weights and closure. Unequal weights, trimmed intervals and periodic splines are rejected explicitly. A reversed complete profile can be realigned by the reverse option.
- **SurfaceIsoCurve:** exact tensor contraction in homogeneous coordinates for NURBS surfaces. Planes produce lines; frusta, spheres and tori produce exact rational conics or lines. Surface poles that collapse an iso curve to a single point are rejected.
- Analytic conic isocurves preserve the locus and endpoints; rational curve parameterization is not equal to the source analytic surface's angular parameter.
- Surface tessellation includes active NURBS knots, so nonuniform degree-one corners are not bridged by a display chord.
- `iso_polyline` remains an explicit approximation API, with no mathematical geometry attached.

## Independent regressions

`tests/curve_surface_integration.rs` verifies radius and height of exact cylinder extrusions at four display resolutions; iso-curve length against `2*pi*r`; rational tensor iso points and first derivatives against surface evaluation; analytic sphere/torus/frustum lengths and degenerate poles; Euclidean ruled interpolation; reverse/trim/unequal-weight policies; and graph save/reload/change/re-evaluation.

`examples/surface-exact-cylinder.morph` is an editable precise-circle → surface-extrusion → isocurve → equal-arc-length point-division workflow with explicit mesh conversion for downstream exporters.

## Remaining limitations

The restricted B-rep kernel is axis-aligned boxes, not a general-purpose Boolean/trim kernel. General lofts, sweeps, surface fitting, arbitrary trimmed/periodic profiles and differing-rational-weight ruled interpolation are incomplete. The exact surface isocurve parameter may differ from the analytic source surface's angular parameter. Surface meshes remain approximations.

No new dependency or copied commercial kernel code.
