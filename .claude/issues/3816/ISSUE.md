# AUD-2026-09-03: decode Skyrim MUSC/MUST, FNV MSET, and Oblivion RDMD music-type enum for REGN ambient music

**Labels**: game:fnv

**Severity**: LOW · **Source**: split from #3811 (REGN-SIBLING-2026-09-02)

## Background

#3811 confirmed (via xEdit's `wbDefinitionsTES4.pas`/`wbDefinitionsTES5.pas`
record definitions) the true per-era target of REGN's background-music
field, previously assumed to always be a `SOUN` FormID:

- **Oblivion `RDMD`** — NOT a FormID at all. A `uint32` enum: `0 = Default`,
  `1 = Public`, `2 = Dungeon`. Oblivion's `REGN` has no `MUSC` (or
  equivalent) record to point at. FNV's record *definition*
  inherits the `RDMD` field, but its shipped data never uses it: an ESM
  census of all 276 `FalloutNV.esm` regions (2026-09-06, #3915) finds
  **`RDMD` ×0** — FNV authors `RDSB` ×44 / `RDSI` ×11 exclusively
  (`Fallout3.esm` ships two `RDMD`). So the Oblivion-enum piece below does
  not cover FNV at all.
- **Skyrim `RDMO`** — a real FormID, but targets `MUSC` ("Music Type"), not
  `SOUN`. `MUSC` carries no direct audio path itself — it's a container:
  `EDID` + `FNAM` playback flags (`Plays One Selection` / `Abrupt
  Transition` / `Cycle Tracks` / `Maintain Track Order` / `Ducks Current
  Track` / …) + `PNAM` (priority + ducking-dB struct) + `WNAM` (fade
  duration float) + `TNAM` (array of FormIDs pointing at `MUST` "Music
  Track" records). The actual file path and loop points live in `MUST`:
  `ANAM` (track filename), `BNAM` (finale filename), `LNAM` (loop-begin /
  loop-end float pair + loop-count u32).
- **FNV `RDSB`/`RDSI`** — `MSET` (Media Set): census-confirmed 44/44 `RDSB`
  and 10/11 `RDSI` targets (#3787). **Tracked here** since #3915
  (FNV-2026-09-05-D1-01): #3787 closed doc-only (`47f9f068`, a once-only
  diagnostic log + three doc corrections) and deferred the runtime, which
  left the reference title as the one era with no open tracker. `MSET` is
  already parsed (`dispatch_misc_stub.rs` → `EsmIndex::media_sets`, as a
  `MinimalEsmRecord` stub) but nothing in `byroredux/src/` reads
  `media_sets`.

#3811 itself only corrected the documentation (`RegionDataPayload::Sound`,
`RegionAmbientRes`, `dispatch_region_ambient_music`'s diagnostic log) to
stop implying `SOUN` resolution is expected to work on any of these three
eras. This issue is the follow-up feature work #3811 explicitly deferred:
actually decoding `MUSC`/`MUST` (Skyrim) and wiring the Oblivion enum
through to a usable ambient-music selection.

## Why this is real feature work, not a doc-fix-sized change

- Two new record types to parse (`MUSC`, `MUST`), each with a real byte
  layout now pinned by #3811's research (xEdit `wbDefinitionsTES5.pas`
  lines ~10194 and ~9512) but not yet implemented in
  `crates/plugin/src/esm/records/`.
- A genuine policy decision the no-guessing rule flags as needing design,
  not inference: `MUSC.TNAM` is an *array* of `MUST` tracks, gated by
  playback flags (`Plays One Selection` picks one; `Cycle Tracks` walks the
  list; `Maintain Track Order` vs. random selection). Which track(s) to
  play, in what order, and how `MUST.LNAM`'s loop-begin/loop-end points
  interact with `byroredux_audio::AudioWorld::play_music`'s kira-backed
  streaming playback (which currently has no loop-region wiring at all —
  see #3775, a sibling gap on the SOUN/loop side) all need an explicit
  design pass.
- Oblivion's `RDMD` enum is simpler (no new record type), but still needs
  a decision: what does "Public" vs "Dungeon" vs "Default" actually
  select, given there is no per-region FormID to resolve? This likely maps
  to some global/default music track configuration this codebase hasn't
  identified yet — needs its own research pass, not a guess.

## Suggested scope

Three independently landable pieces:
1. Parse `MUSC` + `MUST` (Skyrim), decode `RegionDataPayload::Sound.music`
   as a `MUSC` FormID on Skyrim specifically (keeping the generic `u32`
   representation for the other two eras), and resolve at least the
   simplest case (`Plays One Selection`, first/only `MUST` track) through
   to `dispatch_region_ambient_music` — deferring cycling/randomization
   policy to a follow-up.
2. Research + wire Oblivion's `RDMD` enum — needs its own spec/corpus pass
   before implementation per the no-guessing rule.
3. Decode `MSET` (FNV) — replace the `MinimalEsmRecord` stub in
   `EsmIndex::media_sets` with the real sub-record layout (xEdit
   `wbDefinitionsFNV.pas`: media-set type, per-slot track paths, the
   day/night/loop/battle bank structure), decide the bank/track selection
   policy, then resolve `RegionAmbientRes::music_form` / `incidental_form`
   through it in `dispatch_region_ambient_music`. `SOUN.FNAM` folder-form
   handling (#3914) applies if any `MSET` slot references `SOUN` records
   rather than raw paths.

## Related

- #3811 (REGN-SIBLING-2026-09-02) — the parent finding, doc-only fix
- #3787 (FNV-2026-08-30-D1-01) — the FNV/MSET sibling gap, closed doc-only;
  its runtime half is piece 3 above
- #3915 (FNV-2026-09-05-D1-01) — the tracking-gap finding that folded FNV
  into this issue and corrected the "FNV inherits the identical enum"
  sentence against the zero-`RDMD` census
- #3775 (AUD-2026-08-30-D4-01) — loop-continuation gap on the SOUN side,
  relevant once `MUST.LNAM` loop points need to reach `AudioWorld`
- #3301, #2372 — REGN ambient audio parent umbrella
