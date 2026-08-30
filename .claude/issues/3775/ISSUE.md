# #3775 — AUD-2026-08-30-D4-01: REGN ambient background music is dispatched without a loop region and has no re-trigger, so a region's ambient bed plays exactly once and then goes permanently silent

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, audio, bug

---

**Audit**: `/audit-audio` — `docs/audits/AUDIT_AUDIO_2026-08-30.md` (Dimension 4 — Streaming Music Lifecycle), HEAD `64f64480`
**Finding ID**: `AUD-2026-08-30-D4-01`

- **Severity**: MEDIUM
- **Status**: NEW

## Location

- `crates/audio/src/lib.rs:579-611` — `AudioWorld::play_music`
- `byroredux/src/asset_provider/audio.rs:158-205` — `dispatch_region_ambient_music`
- guards at `byroredux/src/cell_loader/load.rs:552-561` and `byroredux/src/scene/world_setup.rs:534-541`

## Description

`play_music` hands `mgr.play(...)` a `StreamingSoundData` on which only `.volume(db)` and `.fade_in_tween(..)` have been set. kira's `StreamingSoundSettings::default()` is `loop_region: None`, so the track plays through once and stops.

Nothing restarts it: `dispatch_region_ambient_music` is invoked **only** when `music_form` differs from the previously-installed `RegionAmbientRes.music_form`, and a player who stays inside one region never changes that value. There is no polling of `is_music_active()` anywhere in the tree, and no `set_loop_region` call anywhere in the workspace.

Observable behaviour: walk into a REGN-tagged exterior, hear the ambient bed once, then silence for the remainder of the visit; the bed returns only after crossing into a differently-scored region and back.

This is the **entire** shipped REGN audio feature (the 2026-08-23 `ede48ffb`/`3ef05d1b` work, marked `✓` at `docs/feature-matrix.md:148`), so the observable failure is that a feature the matrix reports as complete works for one track length per region entry.

## FNV scope correction (from the sibling `/audit-fnv` pass)

`/audit-fnv` measured that on **FNV**, REGN `RDSB`/`RDSI` are **`MSET`** FormIDs, not `SOUN` — 54 of 55 references target `MSET`, 0 target `SOUN`. `dispatch_region_ambient_music` resolves `music_form` through the `SOUN` map (`resolve_sound_path(sounds, form_id)`), so **on FNV this path cannot execute at all**: the resolve returns `None` and the dispatch falls straight through to `stop_region_ambient_music`.

That FNV-specific defect is owned separately by the FNV report and is being filed by a later publisher; it is recorded here so this finding's scope is not read as "broken on FNV in the once-then-silence way". On FNV the bed never starts; the once-then-silence behaviour described above is the Oblivion (`RDMD`) / Skyrim (`RDMO`) shape.

## Evidence

The whole configuration applied before play (`crates/audio/src/lib.rs:598-610`):

```rust
let db = linear_volume_to_db(volume);
let configured = streaming_sound.volume(db).fade_in_tween(Some(fade));
match mgr.play(configured) {
    Ok(handle) => { self.music = Some(handle); }
    Err(e) => { log::warn!("M44 Phase 5: play_music failed: {e}"); self.music = None; }
}
```

kira's default, from the vendored crate (`kira-0.10.8/src/sound/streaming/settings.rs:37`): `loop_region: None`. The builder that is never called: `kira-0.10.8/src/sound/streaming/data.rs:106` `pub fn loop_region(mut self, loop_region: impl IntoOptionalRegion) -> Self`.

A workspace-wide grep confirms the project never touches it on the music path — `grep -rn "loop_region\|set_loop_region" crates/ byroredux/` returns six hits, all on the **entity/static** path (`crates/audio/src/lib.rs:1161` plus its docstrings at `68`, `320`, `1157`, and one test assertion at `tests.rs:632/645`).

The change guard, verbatim (`byroredux/src/cell_loader/load.rs:552-561`; `scene/world_setup.rs:534-541` is the same shape and the only other call site):

```rust
let previous_music_form = world
    .try_resource::<crate::components::RegionAmbientRes>()
    .and_then(|r| r.music_form);
if previous_music_form != region_ambient.music_form {
    crate::asset_provider::dispatch_region_ambient_music(
        world, &index.sounds, region_ambient.music_form);
}
```

`music_form` is the REGN `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB` (FNV) background-music FormID (`byroredux/src/components.rs:540-542`) — the field whose purpose is a continuous bed, not a stinger.

Re-verified at HEAD.

## What this finding does NOT claim

It does **not** prescribe `loop_region(..)` as the fix, and **no replacement value or policy is proposed here**.

`SounRecord` carries only `{form_id, editor_id, sound_path}` — the `SNDD`/`SNDX` flag word that would tell the engine whether a given SOUN is authored as a looping bed is **not parsed**. So "loop it", "restart on a timer", or "the vanilla asset is already a long pre-looped file and the real bug is elsewhere" cannot be distinguished from the data the engine currently has.

Per the project's no-guessing rule, the continuation policy must be settled against the SOUN flag layout (or a corpus census of REGN-referenced SOUN durations) before a value is chosen. **The defect being reported is the absence of *any* continuation mechanism**, which is verifiable from the code alone.

## Not covered by an open issue

#3301 ("EX-16 items 1+5 remainder") scopes itself explicitly to `incidental`, the non-`Sound` RDAT kinds, and the `sounds` chance list, and opens by asserting that "REGN-driven ambient audio is wired end-to-end for exactly one field: `music`". #2372 is the parent umbrella. Neither mentions playback continuation.

## Recommendation

First parse the SOUN flag word (or census the referenced assets) to establish the intended semantics; only then wire the continuation. Whichever policy wins, add a regression test alongside `dispatch_with_no_music_form_stops_playback_without_panic` that asserts the *post-track-end* state, since none of the 13 existing `asset_provider::audio` tests observe playback past the dispatch call.

## Related

- #3301, #2372 (adjacent REGN scope, neither covering continuation)
- `docs/feature-matrix.md:148` (marks the feature ✓)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the entity/static emitter path already sets `loop_region`; check whether cell-scoped music (MUSC) will inherit the same gap
- [ ] **LOCK_ORDER**: `dispatch_region_ambient_music` already scopes its `SoundArchiveProvider` read guard before `&mut World` use — preserve that if a poll is added
- [ ] **TESTS**: A regression test pins this specific fix — assert the post-track-end state, not just the dispatch call
