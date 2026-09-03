# #3811 — REGN-SIBLING-2026-09-02: Oblivion RDMD and Skyrim RDMO also fail to resolve as SOUN

**Severity**: LOW (informational) · **Source**: SIBLING check while fixing #3787

## Research (blocking, per the project's no-guessing policy)

The issue's own completeness checklist explicitly deferred the Oblivion
`RDMD` / Skyrim `RDMO` target-type question ("this was not independently
verified against a format spec"). Verified against xEdit's own record
definitions (`TES5Edit/TES5Edit`'s `wbDefinitionsTES4.pas` /
`wbDefinitionsTES5.pas` — the parser source every ESM-editing tool in this
ecosystem is built on) before touching any code:

- **Oblivion `RDMD`**: `wbInteger(RDMD, 'Music Type', itU32, wbMusicEnum)`
  where `wbMusicEnum = ['Default', 'Public', 'Dungeon']` — a `uint32`
  **enum**, not a FormID at all. Oblivion's `REGN` definition has no
  `MUSC` (or equivalent) record signature anywhere in the file. Matches
  the issue's own observation of near-universal `0` values across every
  vanilla region (`0 = Default`, the first enum member — not a sentinel
  "unset"). FNV's `fopdoc` REGN.md documents the identical enum, confirming
  it's unchanged across the whole pre-Skyrim REGN lineage.
- **Skyrim `RDMO`**: `wbFormIDCk(RDMO, 'Music', [MUSC], …)` — a genuine
  FormID, confirmed targeting `MUSC` ("Music Type"), not `SOUN`. `MUSC`
  itself is a container (`EDID` + `FNAM` playback-mode flags + `PNAM`
  priority/ducking + `WNAM` fade duration + `TNAM` array of `MUST` "Music
  Track" FormIDs) — the actual file path and loop points live one level
  further down, in `MUST` (`ANAM` filename, `BNAM` finale filename, `LNAM`
  loop-begin/loop-end + loop-count).

Both confirm the issue's own hypotheses exactly.

## Fix

Doc-only, matching the scope the issue's own suggested fix laid out
(verify types → correct docs → decide decode scope):

- `RegionDataPayload::Sound.music`'s doc
  (`crates/plugin/src/esm/records/misc/world.rs`) now states the
  per-era target type precisely for all three games (Oblivion enum /
  Skyrim `MUSC`→`MUST` / FNV `MSET`), replacing the "not further
  verified" caveat.
- `RegionAmbientRes::music_form`'s doc (`byroredux/src/components.rs`)
  updated identically.
- `dispatch_region_ambient_music`'s once-per-process diagnostic log
  (`byroredux/src/asset_provider/audio.rs`) was previously worded as if
  the "doesn't resolve as SOUN" failure were FNV-specific ("on FNV,
  RDSB/RDSI are confirmed MSET…") — now correctly states it's universal
  across all three eras, each for its own confirmed reason, rather than
  implying Oblivion/Skyrim might still work.

## Decode scope (issue's own checklist item 4)

Filed #3816 for the actual `MUSC`/`MUST` decode (Skyrim) and Oblivion
`RDMD` enum wiring — real feature work, not a doc-fix-sized change: two
new record types with a byte layout only just pinned by this issue's
research, plus a genuine playback-policy decision (`MUSC`'s
`Plays One Selection`/`Cycle Tracks`/`Maintain Track Order` flags select
among an *array* of `MUST` tracks — which to play and in what order needs
a design pass, not an inferred default) that the no-guessing policy
requires settling deliberately rather than guessed into this fix.

## Verification

- `cargo check -p byroredux-plugin -p byroredux --tests`: clean (one
  pre-existing, unrelated `unused_mut` warning in
  `esm/records/grup_walker.rs` predates this fix).
- `cargo test -q -p byroredux-plugin -p byroredux`: all passing.
- `cargo test -q --no-fail-fast` (full workspace): **7077 passing, 0
  failing** — unchanged from before this fix (doc-only, no behavior
  change, no new mechanism to pin with a test).
