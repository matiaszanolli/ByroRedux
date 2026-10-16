# #5894: PAR-D2-2026-10-15-02: FO4 reference-frame decode silently drops the root motion on any malformed field, and finite samples whose difference overflows f32 put NaN into the root translation keys

- **Labels**: medium,bug,animation,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: FO4 reference-frame decode silently drops the root motion on any malformed field, and finite samples whose difference overflows f32 put NaN into the root translation keys. The #5691 overflowing-span policy is not applied.

- **Severity**: MEDIUM (silently wrong output from a mod file)
- **Dimension**: Error Semantics
- **Location**:
  - `crates/hkx/src/animation.rs:639-668` (`read_reference_frame`)
    - every failure path is `.ok()?` / `return None` with no warning (`:646-664`)
    - samples are checked per value with `is_finite` (`:662`)
  - Consumer: `byroredux/src/asset_provider/animation.rs:401-409` (`reference_frame_position`: `a + (b - a) * t`), added to the root track at `:494-497`
- **Status**: NEW. This is the reference-frame twin of the closed #5691 spline-scalar case.
- **Description**:
  - Two separate things go wrong here.
  - **Silent drop.** These all decode as `reference_frame: None`, exactly like a clip that has no reference frame at all: a truncated sample table, an unbound array, `count > 65,536`, a non-finite duration, or one non-finite sample. The clip then plays in place, and no log names the file. Contract (c) wants a warning or an error, never a silent partial result.
  - **Overflowing span.** Each sample only has to be finite. Adjacent samples such as `x = 3e38` and `x = -3e38` give `b - a = -inf`. At `t = 0`, which is every frame landing exactly on a sample, `inf * 0` is NaN. A NaN translation key then reaches the root bone and root-motion accumulation.
- **Evidence**:
  - Static for the decoder paths.
  - The vanilla census is clean: 13,162 FO4 clips carry the class, and every one that decodes yields a reference frame. Vanilla is therefore not affected.
- **Trigger Input**: an `hkaDefaultAnimatedReferenceFrame` with sample count > 65,536, a truncated or unbound sample array, or two adjacent finite samples of opposite sign above about 1.7e38.
- **Suggested Fix**:
  - Return a `Result`-shaped outcome, or a flag the consumer warns on with the clip path (as `dropped_bones` does).
  - Reject sample tables whose adjacent differences are non-finite, following the #5691 policy.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
