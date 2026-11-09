# NIFAL-D9-2026-11-08-02: `cornell/reference.rs:434` hand-builds a canonical `AnimationClip` outside the declared boundaries

GitHub: #6822 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,animation,documentation,doc-rot

- **Severity**: LOW (doc gap, fixture-only)
- **Dimension**: Animation / cross-cutting (found by Dims 7 and 9)
- **Tier Violated**: single-boundary (declaration only, no behavioural violation)
- **Location**: `byroredux/src/cornell/reference.rs:434`; `docs/engine/nifal.md` §3 (names only `cornell/builders.rs`)
- **Status**: NEW
- **Description**: It is the only non-test producer of the canonical `AnimationClip` besides `convert_nif_clip` (`anim_convert.rs:635`) and `convert_hkx_clip` (`animation.rs:713`). It is a Cornell-box fixture and falls inside the existing Cornell exemption in spirit, but §3 names only `builders.rs`. The Dim 9 auditor confirmed that every Cornell `MeshHandle` insert goes through `builders.rs`.
- **Suggested fix**: Extend the §3 Cornell exemption to name `cornell/reference.rs`, or route the clip through the builder module.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
