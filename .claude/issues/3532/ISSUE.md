# #3532 — LC-2026-08-27-D1-01: #2456's deferred-decision instrumentation now has its corpus answer (1 hit in 642,589 matrices), and its classifier cannot see the one case SVD cannot repair

Labels: low, bug, legacy-compat, nif-parser, nif, tech-debt
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md (base 969d81c8)
Filed: 2026-08-27 via /audit-publish

---

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D1-01) · base `969d81c8`

- **Severity**: LOW
- **Dimension**: 1 — coordinate-system / transform-model fidelity (Dimension 7's "Transform model" bullet is the sibling)
- **Location**: `crates/nif/src/rotation.rs:52-60` (the warning cap + its stated purpose), `:105-162` (`repair_rotation_svd_or_identity` + `sanitize_rotation`), specifically `:117` (`if nearest.determinant() < 0.0`)

## Description

`sanitize_rotation` carries deliberately-temporary instrumentation whose own doc states its purpose:

> "#2456 — this is diagnostic-only instrumentation to measure real corpus incidence before committing to the larger 'decompose into `NiTransform.scale`' fix; it changes no parsed geometry or transform output." (`rotation.rs:55-57`)

and

> "Neither branch folds the discarded factor into `NiTransform.scale` yet — that decomposition is deferred pending real-corpus incidence data." (`rotation.rs:141-143`)

That data now exists and it says the decomposition is not needed for any shipped Bethesda title. Two separate observations:

1. **Measured incidence is effectively zero.** Instrumenting `sanitize_rotation` and running it over 55,949 vanilla NIFs — `Oblivion - Meshes.bsa`, `Fallout - Meshes.bsa` (FO3), `Fallout - Meshes.bsa` (FNV), `Skyrim - Meshes0.bsa` + `Meshes1.bsa` — yields **642,589** `NiTransform` rotation matrices, of which **1** trips `is_degenerate_rotation` (the SVD branch) and **0** trip the `is_non_orthonormal` pass-through branch. The `diag(2, 0.5, 1)`-shaped "baked scale/shear" case the deferred fix was designed for does not occur in any of the four corpora.
2. **The classifier cannot distinguish a reflection from scale/shear, and silently changes orientation rather than losing magnitude.** A pure reflection (`diag(-1, 1, 1)`) is orthonormal — `is_non_orthonormal` returns `false` for it — but `is_degenerate_rotation` returns `true` (|det − 1| = 2), so it takes the SVD branch and logs the fixed text *"NiTransform.rotation is non-orthonormal (baked scale/shear, SVD-orthogonalized) — the singular value information is discarded"* (`rotation.rs:66-70`), which is factually wrong for it: a reflection has all singular values 1 and no scale/shear information to discard. Worse, `:117`'s `if nearest.determinant() < 0.0 { flip column 2 }` does not "repair" a reflection — it converts it into a **different orientation**. Verified by running the code: `diag(-1, 1, 1)` comes back as `diag(-1, 1, -1)`, i.e. a 180° rotation about Y, not an un-mirrored identity. So the eventual scale-decomposition fix would not address reflections at all, and the incidence data the warning gathers silently conflates the two classes.

## Evidence

Temporary counters added to `sanitize_rotation` and driven from a temporary `crates/nif/examples/_tmp_lc_rot.rs` (both reverted): `nifs=55949 matrices=642589 degenerate=1 nonortho_passthrough=0 clean_reflection=0`. Separately, a temporary `#[test]` in `crates/nif/src/rotation.rs` printed `degenerate=true non_ortho=false maxcol=1` / `repaired=[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]` / `det_after=1` for `diag(-1, 1, 1)`.

Re-verified at HEAD: the instrumentation, its "deferred pending real-corpus incidence data" doc, and the unconditional `determinant() < 0.0` column-2 flip are all still present.

## Impact

None on any shipped Bethesda title — this is why it is LOW, and why the reflection half is explicitly *not* filed as a content-mapping gap. Two costs remain: (a) a deferred design decision stays open with its blocking evidence already collectable in ten minutes, and every future audit that reads `rotation.rs:141-143` re-inherits "pending real-corpus incidence data" as an open question; (b) the reflection path is latent for non-Bethesda / mod content, which is live scope (issue #2383, "non-Bethesda titles"), and would fail in the most confusing possible way — a wrongly-*oriented* subtree reported in the log as a scale/shear problem.

## Related

#2456 (the instrumentation), #333 (the unit-quaternion guard downstream), #2383 (non-Bethesda titles). No existing issue covers the reflection classification.

## Suggested Fix

Record the measured incidence in `rotation.rs`'s doc (or close #2456 as "no vanilla incidence; decomposition not warranted") and drop or demote the rate-limited warning. If the instrumentation is kept, split the reflection case out: gate it on `det < 0` before the scale/shear wording, and say plainly in the message that the orientation — not just a magnitude — is being changed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other transform-sanitisation call sites and `#333`'s downstream unit-quaternion guard)
- [ ] **TESTS**: A regression test pins this specific fix (a `diag(-1, 1, 1)` case asserting the classified branch and the emitted message)
