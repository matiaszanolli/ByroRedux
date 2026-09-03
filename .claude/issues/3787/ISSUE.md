# #3787 — FNV-2026-08-30-D1-01: REGN RDSB/RDSI are MSET FormIDs on FNV but the ambient dispatcher resolves them through the SOUN map — region ambient music is a guaranteed silent no-op on the reference title (54 of 55 refs are MSET, 0 are SOUN)

**Severity**: MEDIUM · **Location**: `byroredux/src/components.rs`, `byroredux/src/asset_provider/audio.rs`, `crates/plugin/src/esm/records/misc/world.rs`
**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` (FNV-2026-08-30-D1-01)

FNV's REGN `RDSB`/`RDSI` fields are documented (and coded) as `SOUN` FormIDs, but a census
across all 276 `FalloutNV.esm` REGN records found 44/44 `RDSB` + 10/11 `RDSI` targets resolve as
`MSET` (Media Set), 0 as `SOUN`. `dispatch_region_ambient_music` resolves `music_form` against
the `SounRecord` map regardless, so region ambient music never plays on FNV via this path —
silently, since the existing "no archive supplied" warn split never distinguishes this case.

**Explicit constraint from the issue**: do not point the lookup at `RDSD` without first settling
`chance_raw`'s unresolved fixed-point scale — that would be exactly the guess the no-guessing
policy exists to prevent. Two options offered: (a) decode MSET and route through it, or (b) if
out of scope, correct both doc sites and log once that region ambient is unsupported pending an
MSET runtime.

## Fix implemented (option b)

MSET decode + a full track-bank runtime is a genuinely large new-format feature (out of
proportion for this issue), so implemented option (b) as the issue's own accepted fallback:

- **`crates/plugin/src/esm/records/misc/world.rs`**: `RegionDataPayload::Sound`'s doc corrected
  — states the census-confirmed MSET result for FNV, and reports (without asserting a fix)
  the SIBLING-check finding that Oblivion/Skyrim also fail to resolve as SOUN.
- **`byroredux/src/components.rs`**: `RegionAmbientRes`'s doc corrected to match, pointing at the
  fuller census doc.
- **`byroredux/src/asset_provider/audio.rs`**: `dispatch_region_ambient_music` now logs once
  (`std::sync::Once`, matching the existing pattern in `asset_provider/material.rs`) when a real
  `music_form` was authored but doesn't resolve as SOUN — distinguishing "no archive supplied"
  (silent, the documented common case) from "an ambient directive was authored but this engine
  build can't resolve its target type at all" (structurally unsupported, not a content gap).
  Once, not per-region-transition, since this fires on every cell load with a resident RDSB-
  authoring region otherwise.
- **LOCK_ORDER**: verified — no new resource access added, only local-variable restructuring of
  the existing `music_form` resolution; the function's resource acquisition order is unchanged.

## Completeness Checks

- [x] **SIBLING**: Oblivion's `RDMD` and Skyrim's `RDMO` checked via a throwaway census probe
  (`crates/plugin/examples/_tmp_regn_music_type_census.rs`, deleted after use) against
  `index.sounds`. Result: **0/127 Oblivion `RDMD` values and 0/28 Skyrim `RDMO` values resolve
  as SOUN either** — Oblivion's are near-universally the raw value `0` (consistent with a
  music-type enum rather than a FormID at all, not independently verified against a spec);
  Skyrim's are real non-trivial FormIDs that still don't match any SOUN record (consistent with
  a `MUSC` target, likewise not independently verified). This is a genuine finding beyond this
  issue's FNV scope — filed separately as **#3811** rather than folded into this fix, since
  neither alternate target type was verified with the rigor this issue applied to FNV's MSET
  finding.
- [x] **LOCK_ORDER**: verified unchanged (see above).
- [x] **TESTS**: the existing `dispatch_with_unresolvable_form_id_stops_playback` test already
  pinned the exact behavior this issue's fix relies on (a real-but-non-SOUN FormID fails closed,
  never fabricates a lookup hit) — its doc comment now explicitly connects it to #3787's
  MSET-not-SOUN framing rather than leaving that connection implicit.
- [x] **DOCS**: both named sites (`components.rs` and `world.rs`) updated together.

Full workspace: `cargo test --no-fail-fast` 7037 passing, 0 failing.
