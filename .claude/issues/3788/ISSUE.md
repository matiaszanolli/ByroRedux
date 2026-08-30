# #3788: FNV-2026-08-30-D8-01: --game fnv supplies no --sounds-bsa, so footsteps, water splash and REGN ambient all silently early-return on the reference title — 84.7% of FNV SOUN paths live in the one unlisted archive

**Labels**: bug, medium, legacy-compat, game:fnv, audio
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` — FNV-2026-08-30-D8-01 (MEDIUM)
**Dimension**: 8 — launch profiles / asset sourcing
**Location**:
- `assets/debug_profiles.toml:42-61` — `[profiles.fnv]`
- `byroredux/src/boot.rs:1727-1730` — `default_sounds_bsas` → `--sounds-bsa` expansion
- `byroredux/src/asset_provider/texture.rs:100-110` (`try_load_default_footstep`), `:149-155` (`try_load_default_water_splash`)
- `byroredux/src/asset_provider/audio.rs:177-182` (`dispatch_region_ambient_music`)

## Description

The shipped FNV profile declares:

```toml
[profiles.fnv]
default_bsas = ["Fallout - Meshes.bsa"]
default_textures_bsas = ["Fallout - Textures.bsa"]
default_materials_bsas = []
# default_scripts_bsas and default_sounds_bsas absent
```

`expand_game_profile_args` emits one `--sounds-bsa` per `default_sounds_bsas` entry, so `--game fnv` emits **none**. All three consumers of that flag then early-return **silently**:

- `try_load_default_footstep` — `let Some(path) = path else { return }`
- `try_load_default_water_splash` — same shape
- `dispatch_region_ambient_music` — the `Some(p) if !p.is_empty()` guard, whose own comment calls the no-archive case a *"silent no-op, the common case"*

By contrast `[profiles.skyrim_se]` (`debug_profiles.toml:117`) declares `default_sounds_bsas = ["Skyrim - Sounds.bsa", "Skyrim - Voices_en0.bsa"]` plus `default_scripts_bsas = ["Skyrim - Misc.bsa"]`. FNV — the title whose compat bar is the highest — is the one running dark.

Verified at HEAD: `[profiles.fnv]` still has no `default_sounds_bsas` line; `default_sounds_bsas` appears exactly once in the file, on the Skyrim SE profile.

## Evidence — the assets are all there

- `Fallout - Sound.bsa` exists in the vanilla `Data/` with **6,465 entries**.
- `try_load_default_footstep`'s hardcoded canonical path `sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav` is **present in `Fallout - Sound.bsa`**, byte-for-byte the string the loader asks for.
- The water-splash candidates are present too: `sound\fx\phy\water\splash_{l,m,h}\*.wav`, `sound\fx\phy\water\human\npc_human_splash_0{1,2,3}.wav`.
- **2,700 of 3,189 FNV `SOUN.FNAM` paths (84.7%) resolve inside `Fallout - Sound.bsa`** — 1,127 as exact files and 1,573 more as random-pick directory prefixes with at least one matching entry. Only 489 do not resolve there.

A single archive name closes all of it. The omission was not a design decision that was weighed: the profile parser's **own unit test** (`game_profiles.rs:328-352`) uses an FNV fixture that declares `default_sounds_bsas = ["Fallout - Sound.bsa"]` and asserts it round-trips — the intended shape is written down beside the code, just not in the shipped file.

## Impact

Three shipped M44 subsystems (footstep audio, water-splash acoustics, REGN ambient) produce nothing on `--game fnv`, which `ROADMAP.md` and the `/audit-fnv` skill both name as the *preferred* CWD-immune invocation for ad-hoc runs and audits. **Nothing warns.** Any audit or manual check of FNV audio through the recommended launch path measures silence and cannot distinguish "not implemented" from "no archive" — which is precisely how this went unnoticed.

Note #3787 (FNV-2026-08-30-D1-01) independently kills the REGN half: fixing this profile alone restores footsteps and splash, but region ambient stays dead until the MSET routing is settled.

## Suggested Fix

1. Add `default_sounds_bsas = ["Fallout - Sound.bsa"]` to `[profiles.fnv]`.
2. Evaluate `Fallout - Voices1.bsa` separately — 105,517 entries, and no dialogue consumer exists yet, so it is cost with no current benefit.
3. Consider the same audit for `[profiles.fo3]` and `[profiles.oblivion]`, which are shaped identically; scoping that was out of this dimension's remit.
4. Consider making the three silent early-returns log once at `info` when no sound archive was supplied at all, so "no archive" is distinguishable from "not implemented".

## Completeness Checks
- [ ] **SIBLING**: `[profiles.fo3]` and `[profiles.oblivion]` checked for the same omission
- [ ] **TESTS**: A regression test pins that every shipped profile with an audio consumer declares a sound archive (the parser's own FNV fixture already asserts the shape — extend it to the shipped file)
