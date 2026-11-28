# OMEGA 55 — MATTER
Base main: c1213c885647c2e708017a63f6b7bdc8f9b66620. Dedicated branch; no direct main writes.

## Scope
First slice: independent Rust rigid rotating-square lattice, periodic pitch ratio, signed plate clearance, analytic inverse pitch angle, editable example and focused tests. No new dependencies or shared persistence migration. Avoid duplicating #91 (Miura origami), #95 (XPBD) and #98 (planar mechanisms); SolidCraft remains available for exact fabrication in a future slice, but the first rectangular plate mesh does not require a NURBS kernel.

## Research
- Grima & Evans, *Auxetic behavior from rotating squares*, J. Mater. Sci. Lett. 19 (2000), 1563–1565.
- Grima-Cornish et al., rotating rigid units review, https://doi.org/10.1002/pssr.202100322
- Uddin et al., experimental hinge thickness and rate effects, https://doi.org/10.1016/j.compstruct.2025.118931
- Experimental PLA/TPU rotating square structures, https://doi.org/10.1007/s40964-025-01260-9

The ideal model is kinematic, not a constitutive model. No strength, fatigue, bending or printable hinge guarantee. The initial mesh represents disconnected rigid plates.

## Validation
GitHub connector available; local Rust toolchain and GitHub network clone unavailable. Full gates must run on exact PR head; pending, not claimed passed.
