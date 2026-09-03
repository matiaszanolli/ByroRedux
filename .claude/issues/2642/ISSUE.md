# #2642 — SF-D9-2026-08-07-03: BGSM distance_field_alpha_texture parsed with no MaterialTextureSet role and no deferral comment

**Severity**: LOW · **Dimension**: 9 (BGSM/BGEM External Material Flow)
**Location**: `crates/bgsm/src/bgsm.rs`, `byroredux/src/asset_provider/material.rs`

## Fix

Verified the premise: `distance_field_alpha_texture` (BGSM v>=17,
FO76/Starfield-era) is parsed cleanly but never referenced anywhere in
`byroredux/src/asset_provider/material.rs`'s merge logic, and carried no
comment explaining the gap — confirmed via a direct grep, matching the
issue's own evidence exactly.

The issue's own suggested fix offered two options — wire a real
`MaterialTextureSet` role, or add a deferral comment. `MaterialTextureSet`
genuinely has no role for distance-field alpha (a data-driven cutout mode
different from ordinary alpha-test/blend, needing its own shader
consumer), so wiring one is a small feature build, not a documentation
fix — applied the lower-risk option instead: a deferral comment at both
sides of the gap, mirroring this session's established convention for
this class of finding (`#3544`, `#3625`, `#3638`).

- `crates/bgsm/src/bgsm.rs::BgsmFile::distance_field_alpha_texture` —
  doc comment stating the field is deliberately unforwarded and why.
- `byroredux/src/asset_provider/material.rs`'s BGSM v>2 texture-slot
  fill block — added the same note right after the neighbouring
  `wrinkle`/`flow` fills, at the exact point a reader would expect to
  find (and be surprised by the absence of) the corresponding `fill()`
  call.

Could not locate the issue's cited precedent (`#2109`, a BGEM
glass-overlay deferral) verbatim anywhere in the current tree to mirror
its exact wording — searched `crates/bgsm/src/bgem.rs` and the BGEM
merge block in `material.rs` directly. Wrote an equivalent,
self-contained deferral note instead, matching this session's own
established style for the same class of finding.

## TESTS (issue's own checklist item — "N/A if only adding a deferral
comment")

Added `distance_field_alpha_texture_deferral_is_documented_at_both_sites`
anyway, matching this session's convention of pinning a deferral marker
with a source-scan test rather than leaving it undefended — a future
editor adding a role without wiring it, or trimming the comment during
an unrelated cleanup, would otherwise leave the gap undocumented again
with no test to catch it.

**Reintroduce-and-revert verification**: temporarily removed the
field-level doc comment in `bgsm.rs` — confirmed the new test failed
(`"bgsm.rs's field declaration must carry the #2642 deferral marker"`).
Restored the fix and reran — all tests in `asset_provider::tests::bgsm_merge`
pass again.

## Verification

- `cargo check -p byroredux-bgsm -p byroredux --tests`: clean, zero
  warnings.
- `cargo test -q -p byroredux-bgsm`: 30 passing, 0 failing.
- `cargo test -q -p byroredux --bin byroredux asset_provider::tests::bgsm_merge::`:
  52 passing, 0 failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7179 passing, 0
  failing**.
