# #3787: FNV-2026-08-30-D1-01: REGN RDSB/RDSI are MSET FormIDs on FNV but the ambient dispatcher resolves them through the SOUN map — region ambient music is a guaranteed silent no-op on the reference title (54 of 55 refs are MSET, 0 are SOUN)

**Labels**: bug, medium, legacy-compat, game:fnv, esm-plugin, audio
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` — FNV-2026-08-30-D1-01 (MEDIUM)
**Dimension**: 1 — ESM / REGN ambient audio
**Location**:
- `byroredux/src/components.rs:530-544` — `RegionAmbientRes { music_form, incidental_form }`
- `byroredux/src/asset_provider/audio.rs:158-190` — `dispatch_region_ambient_music`
- `byroredux/src/asset_provider/audio.rs:31` — `resolve_sound_path(sounds: &HashMap<u32, SounRecord>, form_id)`
- `crates/plugin/src/esm/records/misc/world.rs:719-724` — the `RegionDataPayload::Sound` doc

## Description

`RegionAmbientRes`'s doc reads *"`RDMD` (Oblivion) / `RDMO` (Skyrim) / **`RDSB` (FNV)** — the winning entry's background-music/ambient-bed FormID"*, and `world.rs:723` calls `RDSI` the *"incidental **sound** form (FNV)"*.

`dispatch_region_ambient_music` acts on that premise: it takes `music_form` straight to `resolve_sound_path`, which is a lookup into the parsed **`SounRecord`** map, and uses the returned `FNAM` path as the archive key.

**The premise is false on FNV.** `RDSB` / `RDSI` are **MSET** (Media Set) FormIDs there, not SOUN.

## Evidence

Across all 276 REGN records in `FalloutNV.esm` — 10 carry `RDSB`, 11 carry `RDSI`, 55 FormID references in total:

| Sub-record | Target type | Count |
|---|---|---:|
| `RDSB` | **MSET** (Media Set) | 44 |
| `RDSI` | **MSET** | 10 |
| `RDSI` | unresolved (other master / DLC) | 1 |
| either | **SOUN** | **0** |

Sampled EDIDs of the targets: `musSetBTTLRuralGood3Perc`, `musSetBTTLCityGood1Perc`, `musSetINCDesert`, `musSetINCHostile`, `musSetINCMountain` — Media Sets, each carrying the `NAM1`/`NAM2`/`ANAM`…`INAM` track-slot bank, not a `FNAM` file path. `FalloutNV.esm` holds 140 MSET records; the engine parses none of them.

The confirming detail is what **is** SOUN-typed: the **`RDSD` ambient-loop list**, which the same `RegionDataPayload::Sound` arm parses into `sounds: Vec<RegionSound>` and which `RegionAmbientRes` deliberately does **not** surface (its doc explains why — the `chance_raw` fixed-point scale is unverified, correctly invoking the no-guessing policy). Measured: **1,039 of 1,081 `RDSD` entry targets are SOUN records.**

So the engine consumes the sub-record that is MSET-typed and skips the one that is SOUN-typed.

Verified at HEAD by symbol: `resolve_sound_path` (`audio.rs:31`) still takes `&HashMap<u32, SounRecord>`; `dispatch_region_ambient_music` (`audio.rs:158`) still routes `music_form` through it; the `RDMD | RDMO | RDSB` arm at `world.rs:1057` and the `RDSI` arm at `:1065` are unchanged.

## Impact

On FNV, `resolve_sound_path` misses on **100%** of `music_form` values, so `archive_path` is always `None`, `stop_region_ambient_music` runs unconditionally, and **region ambient music can never play — independently of which archives are opened.**

It fails **silently by design**: the `provider_present` split at `audio.rs:177-182` warns only when an archive was supplied but lacked the file, so the FNV path emits nothing at all. A shipped, documented feature is structurally dead on the project's reference title, and two doc sites assert a premise the data contradicts.

Not a crash, not a regression, and audio-only — hence MEDIUM, not HIGH.

## Cross-reference — this blocks a sibling finding

`docs/audits/AUDIT_AUDIO_2026-08-30.md:291` restates the same "`RDSB` (FNV) → sound FormID" premise while filing AUD-2026-08-30-D4-01 about the dispatched track's loop/re-trigger behaviour ("REGN music plays once then silence"). **That finding describes a code path that cannot execute on FNV at all.** This issue should be resolved first; the loop/re-trigger behaviour is unobservable on the reference title until it is.

FNV-2026-08-30-D8-01 (no `--sounds-bsa` on `--game fnv`) is an *independent* second reason REGN ambient produces nothing; fixing the profile alone does not fix this.

## Suggested Fix

Either:

- **(a)** decode MSET and route FNV's `RDSB` / `RDSI` through it; or
- **(b)** if that is out of scope, correct **both** doc sites to say `RDSB` / `RDSI` are **MSET** FormIDs on FNV, and make the FNV arm log once at `info` that region ambient is unsupported pending an MSET runtime.

Silence that looks like success is the worst of the three states.

**Do not** "fix" it by pointing the lookup at `RDSD` without first settling `chance_raw`'s scale — that would be exactly the guess the existing doc refuses.

## Completeness Checks
- [ ] **SIBLING**: The Oblivion `RDMD` and Skyrim `RDMO` arms share `resolve_sound_path` — confirm each game's target record type before assuming the SOUN route is right there too
- [ ] **LOCK_ORDER**: If `dispatch_region_ambient_music`'s resource access changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins the FNV target-type expectation (a fixture REGN whose `RDSB` points at an MSET must not silently resolve through the SOUN map)
- [ ] **DOCS**: `components.rs:539` and `world.rs:719-724` updated together — they are the two sites asserting the false premise
