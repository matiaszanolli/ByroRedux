# #3734 — NIFAL-2026-08-30-D8-02: values() is the one role walk whose test cannot catch an omission

**Severity**: LOW · **Location**: `crates/nif/src/import/types.rs` — `MaterialTextureSet::values()`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` (NIFAL-2026-08-30-D8-02)

`MaterialTextureSet` carries three hand-written role walks: `map_ref`
(compiler-protected — a forgotten role is a compile error), `roles()`
(protected by `roles_covers_every_field_in_the_set`, added #3349), and
`values()` — designated by `docs/engine/nifal.md` as the **exhaustive
lifecycle contract** (cell unload uses it directly for texture release) but
the one walk with no omission guard. Its existing test builds a literal
with sequential integers and asserts `values() == (0..26)`, which catches a
*reordering* but not an *omission*: add a 23rd role, give it `26` in the
test literal (which the compiler forces), forget it in `values()` — the
assert still passes with the 26 elements `0..=25`, and the new role
silently drops out of every exhaustive visit including texture release on
cell unload (a compounding GPU resource leak with no compile error and no
failing test). Latent today — the 22 named roles + 4 decals are currently
correct — but with no guard.

## Fix implemented

Exactly the issue's own suggested fix: added the six-line sibling of
`roles_covers_every_field_in_the_set`, `values_covers_every_field_in_the_set`
— counts `map_ref`'s visits and asserts `values().count()` equals it, same
pattern, same crate.

Verified directly (issue's own TESTS checklist item — "the new assertion
must fail if a role is added to the struct and omitted from `values()`"):
temporarily dropped one field from `values()`'s literal, confirmed the new
test fails with exactly the expected "26 slots but map_ref touches 26"
message shape (25 vs 26), restored the field, confirmed both tests pass
again.

**SIBLING** (issue's own checklist item): checked `supplemental_texture_indices`
(#2697), the fourth role walk the issue's own Related section names as
"still open" — that was stale. **#2697 is already closed**:
`static_meshes.rs` now builds it via indexed writes against named
`supplemental_texture_slot::*` constants, not a positional array literal —
the original reordering-fragility is already structurally closed. The
remaining gap is real but a **different shape**: `supplemental_texture_slot`
has only 16 constants against `values()`'s 26 roles (most roles route to
their own dedicated `GpuMaterial` field and never touch this generic
side-array), so #3734's `values()`-vs-`map_ref` lockstep pattern doesn't
directly transfer — a naive count comparison would compare an exhaustive
walk against a deliberate subset and fail permanently. Filed as a
separately, correctly-scoped follow-up: #3814 (needs its own test shape —
"every declared slot constant is written exactly once" — not a drop-in
port of this fix's pattern).

Full workspace: `cargo test --no-fail-fast` 7060 passing, 0 failing (+1 new
test).
