# Batch: #2321, #2323

## #2321 — FO3-D1-05/D2-01: FO3/FNV fire-refraction heat-haze never classified

**Severity**: MEDIUM
**Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/src/import/material/legacy_properties.rs` (`apply_pp_lighting_property`),
decode site `crates/nif/src/blocks/shader.rs`, Skyrim-only classifier
`crates/nif/src/import/material/dedicated_shader.rs`, missing constants
`crates/nif/src/shader_flags.rs`

`BSShaderPPLightingProperty::parse` decodes `refraction_strength`/`refraction_fire_period`
for FO3/FNV content but neither value was ever mirrored into `MaterialInfo`, and the only
site promoting `material_kind = 103` (fire-refraction heat-haze) tested Skyrim-only SLSF1
bits with no FO3/FNV equivalent declared.

### Fix
- Added `fo3nv_f1::REFRACTION` / `fo3nv_f1::FIRE_REFRACTION` (bits 15/16, confirmed against
  nif.xml `BSShaderFlags` — same position + semantic as `skyrim_slsf1`).
- `apply_pp_lighting_property` now mirrors `shader.refraction_strength` into
  `info.refraction_strength` unconditionally (matches the Skyrim+ path — the scalar can be
  driven independently by `BSRefractionStrengthController`).
- Gated `material_kind = 103` promotion + synthesized alpha-over state on the FO3/FNV
  `REFRACTION | FIRE_REFRACTION` pair, mirroring `dedicated_shader.rs`'s Skyrim+ path exactly
  (same blend state: `src=SRC_ALPHA`, `dst=INV_SRC_ALPHA`, `z_write=false`).
- Added `fo3nv_shares_fire_refraction_bits_with_skyrim` pinning the cross-game bit agreement.
- Added `fo3nv_fire_refraction_tests.rs` (3 tests): promotion fires on the flag pair,
  does NOT fire on `Refraction` alone, and the scalar still mirrors when the promotion
  doesn't fire.

Sibling check: `BSShaderNoLightingProperty` (the other FO3/FNV legacy shader) has no
`refraction_strength` field at all — no equivalent gap there.

## #2323 — FO3-D2-02: nif_stats per-block histogram doc/impl mismatch

**Severity**: MEDIUM
**Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/examples/nif_stats.rs`

The module doc claimed blocks are attributed to their "header-advertised type name, not
parsed Rust type." The actual implementation (and the equivalent, already-correct
`tests/common::PerBlockHistogram` used by `per_block_baselines.rs`, #1883/NIF-D3-001) keys
`unknown` by the header-advertised name and `parsed` by `block.block_type_name()` — the
parsed Rust struct's fixed name. This causes shared-struct family collapse (e.g. ~28
`NiPSys*Modifier` header types all folding into one `NiPSysBlock` `parsed` bucket).

### Investigation finding
This is a **known, already-documented, deliberately deferred** design limitation on the
`tests/common::PerBlockHistogram` side (which the per-block-baseline regression test
actually exercises) — full per-type resolution on the `parsed` side requires `NifScene` to
carry per-block header-advertised type names, which it does not today, and fixing that
would require regenerating all 7 baseline TSVs against real game archives unavailable in
this environment. The *actual* bug in scope for #2323 is narrower: `nif_stats.rs`'s own
module doc had drifted out of sync with its own (correct, matching) implementation.

### Fix
Rewrote the module doc and `record_blocks`'s doc comment to accurately describe the
mixed-key attribution rule, matching `tests/common::PerBlockHistogram`'s existing correct
writeup, including the collapsed-family caveat and the deferred per-block header-name
plumbing. No behavioral change — `nif_stats.rs`'s histogram already matched the correct,
tested behavior; only the comment was wrong.

Full header-name plumbing + baseline regeneration (the issue's more ambitious suggested
fix) is left as follow-up work gated on access to the full per-game NIF corpus, consistent
with the existing `#1883 / NIF-D3-001` deferral note.
