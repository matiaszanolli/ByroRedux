# #3775 — AUD-2026-08-30-D4-01: REGN ambient background music plays once and goes permanently silent

**Severity**: MEDIUM · **Source**: `/audit-audio` — `docs/audits/AUDIT_AUDIO_2026-08-30.md` (Dimension 4)

## Blocking research (the issue's own explicit prerequisite)

The issue's "What this finding does NOT claim" section is explicit: it does
NOT prescribe `loop_region(..)` as the fix, and continuation policy "must
be settled against the SOUN flag layout (or a corpus census of REGN-
referenced SOUN durations) before a value is chosen" — the SNDD/SNDX flag
word was, at the time of writing, unparsed and unspecified in this codebase.

Verified against xEdit's own record definitions (`wbDefinitionsTES4.pas` /
`wbDefinitionsFO3.pas`, the parser source this ecosystem's tools are built
on) before implementing anything:

- `SNDD`/`SNDX`'s `Flags` word carries an explicit, composable **`Loop`
  bit — `0x0010`** — identical across Oblivion, FO3, and FNV. The bit sits
  at the same byte offset (4) in every variant of the sub-record (8-byte
  Oblivion `SNDD`, 12-byte `SNDX` shared by both eras, 36-byte FO3+
  `SNDD`) — only the field's total *width* changed (`u16` → `u32`), never
  the position of this specific bit, so a single byte-offset read decodes
  it correctly for every era without a per-game branch.
- A CS-wiki quote (search-surfaced, not independently re-fetched — noted
  as such) describes `Loop` as engine-authoritative: it forces looping
  "regardless of whether the source file is authored to loop or not."
  This settles the policy question the issue raised: `Loop` is not
  redundant with baked-in asset metadata, so decoding and applying it is
  the correct continuation mechanism, not a guess.
- Skyrim moved this entirely: `SOUN`'s `FNAM`/`SNDD` are `cpIgnore`d
  "leftover, unused" in xEdit's own Skyrim definition, superseded by a
  separate `SNDR` record's `LNAM.Looping` **enum** (value `0x08`, not a
  composable bit, unrelated offset). Not decoded here — `SNDR` is a new
  record type, comparable in scope to #3816's `MUSC`/`MUST` follow-up, not
  a same-issue addition.

## Fix

1. **`crates/plugin/src/esm/records/soun.rs`** — added `SounRecord::looping:
   bool`, decoded from `SNDD`/`SNDX`'s Flags byte at offset 4, bit `0x10`.
   No era/sub-record-type branching needed (see the research above). The
   module's "deliberately NOT decoded" framing narrowed to cover only the
   *remaining* attenuation-curve fields, which still have no spec/corpus
   backing.
2. **`crates/audio/src/lib.rs`** — `AudioWorld::play_music` gained a
   `looping: bool` parameter; when set, applies kira's `.loop_region(0.0..)`
   (loop the whole track from the start) before `mgr.play(..)`. This is
   the actual defect fix: kira's `StreamingSoundSettings` default is
   `loop_region: None`, and nothing else in the codebase ever set it —
   confirmed unreachable via any other path (`play_music` has exactly one
   production caller).
3. **`byroredux/src/asset_provider/audio.rs`** — added
   `sound_loops(sounds, form_id) -> bool`, a pure sibling to the existing
   `resolve_sound_path` (same fail-closed-on-unresolved shape, same
   independent unit-testability). `dispatch_region_ambient_music` looks it
   up alongside the path resolve and threads it through to `play_music`.

## Reachability caveat (found while implementing, not assumed)

#3811 (filed and fixed in this same working session) independently
confirmed that `music_form` — the whole reason `dispatch_region_ambient_music`
would ever call `play_music` — **never resolves as a `SOUN` on any
currently-supported game**: Oblivion's `RDMD` is a music-category enum
(not a FormID at all), Skyrim's `RDMO` targets `MUSC`, FNV's `RDSB`
targets `MSET`. So today, this fix's `dispatch_region_ambient_music` path
is unreachable in production until #3816's `MUSC`/`MUST`/`MSET` decode
work lands — the "walk into a region, hear the bed once, then silence"
*observable* behavior the issue describes cannot currently occur via this
exact path (it never starts at all, matching #3811's finding, not "starts
once"). This does not make the fix premature: `play_music`'s missing
`loop_region` wiring is a real, independently-verified code-level defect
(pinned by its own `#[ignore]`d integration test below, exercised directly
against `AudioWorld` — no REGN/SOUN resolution involved), and it is
exactly the shape of infrastructure-ahead-of-its-consumer this session
already decided is legitimate to land now (`groundcover_translate.rs`'s
`layer_affinity` is the precedent, per #3747's SIBLING check) rather than
deferred until #3816 unblocks the whole chain end-to-end.

## SIBLING (issue's own checklist item)

"the entity/static emitter path already sets `loop_region`; check whether
cell-scoped music (MUSC) will inherit the same gap" — `MUSC` isn't decoded
at all yet (tracked separately, #3816), so there is no MUSC-driven
`play_music` call site today for this gap to apply to. Noted for #3816's
own scope rather than fixed speculatively here.

## LOCK_ORDER (issue's own checklist item)

No poll was added (the fix is data-driven — the loop flag is resolved
once per dispatch, not polled) — `dispatch_region_ambient_music`'s
existing `SoundArchiveProvider` read-guard scoping is untouched; the new
`sound_loops` lookup runs before that scoped block, same as the existing
`resolved` lookup.

## TESTS (issue's own checklist item — "assert the post-track-end state")

- `crates/plugin/src/esm/records/soun.rs` — 4 new tests: Loop bit decodes
  from the 8-byte Oblivion `SNDD`, from the 36-byte FO3+ `SNDD` (same
  offset, wider field), other flag bits don't falsely set `looping`, and
  a missing/too-short sub-record defaults to `false` without panicking.
- `byroredux/src/asset_provider/audio.rs` — 3 new tests for `sound_loops`
  mirroring `resolve_sound_path`'s existing test shape exactly (found,
  not-found, not-looping).
- `crates/audio/src/tests.rs` — `play_music_looping_survives_track_end`,
  mirroring the crate's own existing
  `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`
  pattern (real vanilla-FNV audio, wait well past natural track length,
  assert still active) — this is the literal "assert the post-track-end
  state" the issue's TESTS item asked for, applied to the music path
  instead of the `AudioEmitter` path it was already proven on.
  `#[ignore = "needs a working audio device and FNV game data"]`, same
  gating convention as its sibling tests in this file.

## Verification

- `cargo check -p byroredux-audio -p byroredux-plugin -p byroredux --tests`:
  clean (one pre-existing, unrelated `unused_mut` warning in
  `esm/records/grup_walker.rs` predates this fix).
- `cargo test -q -p byroredux-audio -p byroredux-plugin -p byroredux`: all
  passing.
- `cargo test -q --no-fail-fast` (full workspace): **7084 passing, 0
  failing** (+7 new non-ignored tests; the real-audio integration test is
  `#[ignore]`d like its siblings).
