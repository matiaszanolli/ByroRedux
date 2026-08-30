# #3776 — AUD-2026-08-30-D7-01: the footstep and splash loaders consult only the first --sounds-bsa, while the same flag is repeatable for the REGN provider

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, audio, bug

---

**Audit**: `/audit-audio` — `docs/audits/AUDIT_AUDIO_2026-08-30.md` (Dimension 7 — Gameplay Audio Wiring), HEAD `64f64480`
**Finding ID**: `AUD-2026-08-30-D7-01`

- **Severity**: MEDIUM
- **Status**: NEW

## Location

- `byroredux/src/asset_provider/texture.rs:100-110` — `try_load_default_footstep`
- `byroredux/src/asset_provider/texture.rs:149-155` — `try_load_default_water_splash`
- against `byroredux/src/asset_provider/audio.rs:104-124` — `build_sound_archive_provider`

## Description

Three consumers parse the same `--sounds-bsa` flag out of `args`, and they **disagree about its arity**.

`build_sound_archive_provider` walks the whole arg list and pushes **every** match into `SoundArchiveProvider.archives`, with first-hit-wins resolution at extract time — its own doc calls the flag *"repeatable (list override/mod archives before the vanilla one — first hit wins)"* (`asset_provider/audio.rs:54-57`), and `docs/engine/exterior-readiness-plan.md:1197` records the same contract.

The two one-off loaders take the **first** occurrence and stop.

A user who follows that documented ordering — mod/override archive listed first, vanilla `Fallout - Sound.bsa` second — gets a `SoundArchiveProvider` that resolves REGN ambient music out of either archive, but a `FootstepConfig.default_sound` and a `WaterAudioConfig.splash_sound` that are **both `None`**, because the canonical `sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav` and the three splash candidates live only in the vanilla archive that was never opened.

Footsteps and water splashes then no-op for the whole session (`footstep_system` returns at `systems/audio.rs:123`; `water_audio_system` at `:245`), leaving only a one-line boot WARN — *"'<mod.bsa>' missing canonical footstep '<path>'"* — that reads as a bad archive rather than as a flag-arity mismatch.

## Evidence

`try_load_default_footstep`, verbatim (`texture.rs:101-110`):

```rust
let mut path: Option<&str> = None;
let mut i = 0;
while i < args.len() {
    if args[i] == "--sounds-bsa" {
        path = args.get(i + 1).map(|s| s.as_str());
        break;
    }
    i += 1;
}
let Some(path) = path else { return };
```

`try_load_default_water_splash` (`texture.rs:149-155`) uses a different spelling of the same first-match semantics:

```rust
let Some(path) = args
    .windows(2)
    .find(|pair| pair[0] == "--sounds-bsa")
    .map(|pair| pair[1].as_str())
else { return; };
```

versus `build_sound_archive_provider` (`asset_provider/audio.rs:106-123`), which has no `break` and pushes each successfully-opened archive onto a `Vec`.

The documentation is split the same way: `docs/engine/game-loop.md:55` lists the flag as `--sounds-bsa PATH` (singular), while `docs/engine/exterior-readiness-plan.md:1197` describes it as repeatable. Each is accurate about the consumer it was written for, which is exactly why the split has survived.

Re-verified at HEAD.

## Distinct from #3189

#3189 is about the *duplication* of the scan and the repeated `Archive::open` of the **same** path — a cleanliness/boot-cost concern. **This is a behavioural gap**: the two loaders cannot see archives 2..n at all.

The natural fix is the one #3189's own remediation note already proposes (migrate both one-off loads onto the persistent `SoundArchiveProvider`, which already iterates every archive), so the two should be worked together — but **a fix that only deduplicates the scan without switching to the multi-archive provider would close #3189 and leave this defect standing.** Worth noting on #3189 when it is next picked up.

## Recommendation

Settle the flag's arity in one place. Either make all three parsers repeatable (preferred — it is the contract the provider and the plan doc already state) or make the provider single-valued and correct `exterior-readiness-plan.md`. Then align `docs/engine/game-loop.md:55` with whichever wins.

## Related

- #3189 (duplicate scan / re-open — same code, different defect; fix together)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other repeatable CLI flag with more than one parser (`--bsa`, `--textures-bsa`, `--master`)
- [ ] **TESTS**: A regression test pins this specific fix — two `--sounds-bsa` values where the canonical footstep lives only in the second must still load it
