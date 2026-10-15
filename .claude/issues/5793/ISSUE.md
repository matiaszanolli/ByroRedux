# TD2-2026-10-15-02: `synthesize_tangents` and `synthesize_tangents_yup` are the same Lengyel accumulation plus Gram-Schmidt finalize twice; three fix rounds each landed on one copy first

Labels: medium,nif-parser,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-02`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/import/mesh/tangent.rs:170-358` (Z-up) vs `:396-575` (Y-up); callers `bs_tri_shape.rs:217,242`, `bs_geometry.rs:338`, `ni_tri_shape.rs:74,101`
- **Status**: NEW (10-11 / 08-30 only touched this file for the coord-swap call sites)
- **Age**: `df60da808` (#1516) -> `b09cec5c3` (#2632, 2026-08-08, Y-up only) -> `8075133c5` (2026-08-18, "Orthogonalize degenerate Z-up tangents", Z-up catch-up) -> `19742e805` (#3176/#3177, 2026-09-02, "tangent producer parity")
- **Effort**: small
- **Description**: the 40-line per-triangle accumulation (`x1..z2`, `s1..t2`, `det`, `sdir`, `tdir`, the six `tan_u/tan_v` adds) and the 60-line per-vertex finalize (normalize N, degenerate-permutation fallback with `fallback_orthogonal_seed`, T/B Gram-Schmidt, `bitangent_sign`) are line-for-line the same; the Z-up copy differs only by calling `zup_to_yup_pos` on the inputs of the finalize. The comments in the Y-up copy already say "mirroring `synthesize_tangents_yup`'s established guard" and "matching the Y-up producer's #2632 guard" - the duplication is acknowledged in prose and maintained by hand.
- **Evidence**: three commits above, each titled as a catch-up of one producer to the other: #2632 normalised N in Y-up only; the Z-up copy got it 10 days later (#3177 is literally "Z-up sibling"); #3176's zero-tangent seed was added to both in the same commit only because the audit named both.
- **Impact**: tangent basis feeds every normal-mapped surface; a lagging producer shows as the "chrome / posterized" class (see #786's history in the same file).
- **Related**: #786, #1516, #2632, #3176, #3177.
- **Suggested Fix**: make `synthesize_tangents` a thin wrapper: convert `vertices`/`normals_zup` to Y-up with `zup_to_yup_pos` and call `synthesize_tangents_yup` (the finalize already runs in Y-up space in both). Extract the per-vertex Gram-Schmidt into `fn orthonormal_basis(n, t, b) -> ([f32;3],[f32;3])` if the degenerate-permutation choice must stay space-specific (the doc comment calls that divergence "documented"; drop it, any orthogonal-to-N direction is valid).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
