# Runtime Telemetry Audit — 2026-08-30

Scope: `/audit-runtime --game all`. Real device runs on an **RTX 4070 Ti (12 GB,
driver 580.173.02)** under `xvfb-run`, Ryzen 7950X (16c/32t), release build at
`HEAD` (`64f64480`). Six games attempted, **five captured**, one (Starfield)
ran but never produced a frame.

> ROADMAP's "no GPU in this environment" caveat is **stale** — every number
> below is measured device telemetry, not static analysis.

## Games actually run

| Game | Cell | Result |
|------|------|--------|
| `oblivion` | `ICMarketDistrictTheGildedCarafe` | CAPTURED |
| `fnv` | `FreesideAtomicWrangler` | CAPTURED (re-run after harness fix, see RT-15) |
| `fo3` | `MegatonPlayerHouse` | CAPTURED |
| `skyrim_se` | `WhiterunDragonsreach` | CAPTURED |
| `fo4` | `InstituteBioScience` | CAPTURED |
| `starfield` | `citycydoniamainlevel` | **RAN, HARD STALL — no telemetry capturable** |
| `fo76` | — | **NOT RUN**: no `[profiles.fo76]` block exists; out of this skill's surface |

## Per-game baseline comparison

| Game | entities (Δ) | tex_missing total / base_color-only | mesh_fail | skin L/M+S | draws N/Mb/Kc | fps (advisory) | Status |
|------|-------------|------------------------------------|-----------|------------|---------------|----------------|--------|
| oblivion | 718 vs 705 (+1.84%) PASS | 8 vs 0 / **1** vs 0 | 0 vs 0 | 4/1364+0 | 330/20b/2c vs 325/20/2 PASS | 222.6 vs 377.6 | REGRESSION (LOW) |
| fnv | 7342 vs 7174 (**+2.34%**) | 6 vs 1 / **1** vs 1 EXACT | 0 vs 0 | 217/1364+0 vs 206 | 2197/109b/26c vs 2110/109/26 PASS | 63.9 vs 65.4 | REGRESSION (MEDIUM) |
| fo3 | 3493 vs 3493 **EXACT** | 12 vs 0 / **0** vs 0 EXACT | 0 vs 3 IMPROVED | 7/1364+0 EXACT | 1581/100b/11c **EXACT** | 83.7 vs 62.7 | PASS |
| skyrim_se | 9363 vs 8126 (**+15.2%**) | 10 vs 0 / **0** vs 0 EXACT | 0 vs 9 IMPROVED | 133/1364+0 vs 83 | 2460/**20b**/**4c** vs 2342/9/2 | 114.8 vs 161.9 | **REGRESSION (HIGH)** |
| fo4 | 19399 vs 18256 (**+6.26%**) | 16 vs 1 / **1** vs 1 EXACT | 0 vs 0 | 299/1364+0 vs 248 | 4000/300b/16c vs 3949/296/16 PASS | 41.6 vs 44.3 | REGRESSION (MEDIUM) |
| starfield | — | — | — | — | — | **0 fps** | **CRITICAL** |

`skin_pool_overflow_attempts == 0` and `skin_pool_max == 1364` on all five
captured games — the #1284 cap is under no pressure anywhere.

Per-frame tail latency (new `frame_p*` fields, all advisory):

| Game | p50 ms | p95 ms | max ms |
|------|--------|--------|--------|
| oblivion | 3.55 | 15.85 | 19.95 |
| fnv | 15.88 | 16.77 | **29164.91** |
| fo3 | 8.35 | 19.85 | **9960.93** |
| skyrim_se | 8.58 | 11.55 | 41.11 |
| fo4 | 23.61 | 24.71 | 120.14 |

---

## Findings

### RT-1: Starfield `citycydoniamainlevel` never renders a frame — 10-minute hard stall, single-threaded, 20.6 GB RSS
- **Severity**: CRITICAL
- **Game**: starfield · **Cell**: `citycydoniamainlevel`
- **Evidence**: two independent runs. The profile ships empty archives, so the
  cell was driven with explicit `--bsa Meshes01/Meshes02/MeshesPatch
  --textures-bsa Textures01 --materials-ba2 Materials`. The cell **does** load:
  `M28.5 static collider AABB … (95095 fixed colliders); rapier_bodies=95651`,
  character controller placed and `grounded=true`.
  Then the frame loop stops dead at `M28.5 frame 0`.
  - Run 1 (240-frame bench window): all four `byro-dbg` commands returned
    `Error: timeout waiting for engine response`; **zero** `bench:` lines.
  - Run 2 (10 min sustained, sampled every 30 s): still on `M28.5 frame 0` at
    t=570 s. CPU pinned at **61–97 % of ONE core** on a 16c/32t 7950X — a
    single-threaded stall, which per the project's own hardware contract is by
    definition a bug. RSS oscillated **12.0 → 20.6 GB**, peaking at 20.6 GB on
    a 29 GB machine.
- **Impact**: the only Starfield render path this project has is unrenderable at
  HEAD, and it is a live OOM hazard for anyone running it — a single process at
  20.6 GB. No Starfield runtime baseline can be created until this is fixed.
- **Suggested Fix**: profile the frame-0 path with 95 k Rapier bodies. The
  oscillating RSS (12→20 GB and back, repeatedly) points at a per-frame
  allocate/free of a collider- or BLAS-sized working set rather than a pure
  spin. Prime suspects: the static-collider AABB/broad-phase build in
  `byroredux/src/systems/character.rs` (M28.5), and BLAS construction over
  95 k bodies in `crates/renderer/src/vulkan/acceleration/blas_static.rs`.
  Gate SF cell load behind a collider-count budget until then.

### RT-2: Skyrim `WhiterunDragonsreach` draw split regressed past the ×1.1 gate on both batch axes
- **Severity**: HIGH
- **Game**: skyrim_se · **Cell**: `WhiterunDragonsreach`
- **Baseline** (2026-08-06): `2342/9b/2c` · **Current**: `2460/20b/4c`
  - `bench_draws_cmds` 2342 → 2460 = **×1.050 PASS**
  - `bench_draws_batches` 9 → 20 = **×2.22 FAIL**
  - `bench_draws_gpu_calls` 2 → 4 = **×2.00 FAIL**
- **Impact**: `cmds` moving only +5 % while batches more than double means the
  same geometry is being split into more, smaller batches — merge efficiency
  fell 260.2 → 123.0 cmds/batch.
- **Note on premise**: this cell's baseline header explicitly argued it does
  **not** share the #2215 `draw_sort_key` alpha-over mechanism that moved fnv /
  fo3 / fo4, and closed #2351 as non-reproducing on that basis. That reasoning
  no longer holds — the cell has now moved on exactly those two axes.
- **Suggested Fix**: re-open the #2215 question for this cell specifically.
  Bisect `byroredux/src/render/mod.rs` `draw_sort_key` / `group_state` between
  2026-08-06 and HEAD. Small-N noise is a real possibility at 9→20 batches, so
  confirm with a second run before committing a fix.

### RT-3: every Starfield skinned mesh has 100 % unresolved bones — all SF actors render in bind pose
- **Severity**: HIGH
- **Game**: starfield
- **Evidence**: `401` of `485` skinned-mesh log lines report `UNRESOLVED`,
  across **73 unique meshes**, each with *every* bone unresolved:
  `Skinned mesh 'Naked_M:0': 41 bones (41 UNRESOLVED — names: ["Bone0",
  "Bone1", …]), root=Some("ExportScene")`. Same for `Hands_3rd_M`,
  `Outfit_NewAtlantis_FashionableSuit_01_*`, `Outfit_Miner_Jumpsuit_*`,
  `Outfit_Baseball_Cap_*`.
- **Contrast — measured, not assumed**: the identical counter is **0** on every
  other game: fnv 0/217, skyrim_se 0/133, fo4 0/299, oblivion 0/4, fo3 0/7.
  This is Starfield-specific, not a general skinning defect.
- **Impact**: the bone names Starfield NIFs carry are generic placeholders
  (`Bone0`…`BoneN`) under a `root="ExportScene"`, and nothing matches them to
  the skeleton's node names. Every SF NPC and every piece of SF apparel renders
  in bind pose.
- **Suggested Fix**: SF skin data evidently indexes bones positionally rather
  than by name. Resolve SF `BSSkin::Instance` bone references by **index into
  the skeleton's bone array** instead of by string, gated on
  `bsver >= SF_FORM_ID`, in `crates/nif/src/import/mesh/skin.rs`.

### RT-4: the `tex_missing_unique_paths` baseline contract is broken — #3349 widened the metric after all five baselines were captured
- **Severity**: MEDIUM
- **Games**: all five
- **Evidence**: `ff177576` (2026-08-28, **#3349** "per-slot tex.missing")
  changed `tex.missing` from walking the single `TextureHandle` (base-color
  only) to walking the full **26-role** `MaterialTextureHandles` set.
  Every committed baseline predates it: oblivion 2026-08-26, fnv 2026-08-26/27,
  fo3 2026-06-14 (byro-dbg rows), fo4 2026-08-22, skyrim_se 2026-08-06.
- **The naive diff produces five false HIGHs.** Re-scored against the
  *pre-#3349* surface (base-color slot only), the picture inverts:

  | Game | total (new surface) | base_color only | baseline | true verdict |
  |------|--------------------|-----------------|----------|--------------|
  | fnv | 6 | 1 | 1 | **EXACT PASS** |
  | fo3 | 12 | 0 | 0 | **EXACT PASS** |
  | fo4 | 16 | 1 | 1 | **EXACT PASS** |
  | skyrim_se | 10 | 0 | 0 | **EXACT PASS** |
  | oblivion | 8 | 1 | 0 | **+1 real** (see RT-9) |

- **Suggested Fix**: regenerate all five `tex_missing_unique_paths` rows against
  the current surface with `--regen`, or split the metric into
  `tex_missing_base_color` (the strict gate) + `tex_missing_all_slots`
  (informational). Do **not** file the raw deltas as regressions.

### RT-5: `derive_normal_map_path` is not game-gated — it fabricates `_n.dds` probes on every game, not just Oblivion/FO3
- **Severity**: MEDIUM
- **Games**: all five (worst on fo4 / skyrim_se / fnv)
- **Evidence**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:158-164` calls
  `derive_normal_map_path` unconditionally when a mesh leaves its normal slot
  empty. The #1303 rationale in the comment directly above it is
  game-specific — *"Oblivion/FO3 ship normal maps via the `<base>_n.dds`
  load-time convention"* — but `derive_normal_map_path`
  (`byroredux/src/asset_provider/texture.rs:278`) takes no game parameter and
  no caller gates it.
- **Measured cost** — share of `tex.missing` entries that are fabricated
  `src=derived-normal` paths rather than authored ones:
  oblivion 7/8, fnv 5/6, fo3 12/12, skyrim_se 8/10, fo4 13/16.
  FO4 and Skyrim author normals explicitly via BGSM / `BSLightingShaderProperty`
  and have no `_n.dds` convention, so every one of those is a wasted
  archive lookup per shape plus pure telemetry noise.
- **Suggested Fix**: gate the derive on `GameKind::{Oblivion, Fallout3}` (and
  FNV if the convention holds there), or mark derived paths as speculative so
  a miss is not bucketed by `tex.missing` at all.

### RT-6: the P2 playable-slice gate 5 fails on a working engine — anchored regex vs. escaped single-line `byro-dbg` output
- **Severity**: MEDIUM
- **Evidence**: `docs/smoke-tests/p2-melee-core.sh` reports
  `FAIL -- slot 9 carries no Inventory column — gate 5 is unassertable`.
  The save **does** carry it — the captured `save.info` reads
  `… EquippedWeapon: 17 rows\n  Inventory: 91 rows\n  LightFlicker: 56 rows …`
  and `inventory.status` reads
  `stack_rows=22 item_count=205 occupied_slots=6 equipped_weapon=0x00013790`.
  The gate uses `grep -Eq '^  Inventory: [0-9]+ rows'` (line 334), but console
  output returns as `DebugResponse::Value { data }` and is printed by
  `serde_json::to_string_pretty` (`tools/byro-dbg/src/display.rs:8`), which
  renders a JSON *string* on **one** line with `\n` escaped. A `^`-anchored
  match can never succeed. Every other assertion in the script uses `grep -F`
  and passes.
- **Impact**: P2 reports FAIL on a healthy build, and the save→exit→reload
  round trip that gate 5 exists to prove is never actually exercised.
- **Suggested Fix**: replace the anchored regex with
  `grep -Fq 'Inventory: '` (or `grep -Eq 'Inventory: [0-9]+ rows'` without `^`),
  matching the `-F` style the rest of the script already uses.

### RT-7: `skin_pool_live` grew against its `≤ baseline` direction on three of five games
- **Severity**: MEDIUM
- **Evidence**: fnv 206 → **217** (+11), skyrim_se 83 → **133** (+50),
  fo4 248 → **299** (+51). fo3 (7) and oblivion (4) are exact.
- **Mitigating**: `skin_pool_overflow_attempts` is **0** everywhere and
  `skin_pool_max` is **1364** everywhere, so nothing is rendering in bind pose
  for lack of a slot and the #1284 cap has ample headroom (299/1364 = 22 %).
  This continues the documented benign-creep line from the skin-version-gate
  work, not a new spill.
- **Suggested Fix**: regenerate the three rows; keep the gate on
  `skin_pool_overflow_attempts == 0`, which is the row that actually matters.

### RT-8: `entities_total` left the ±2 % band on three games
- **Severity**: MEDIUM
- **Evidence**: skyrim_se 8126 → **9363 (+15.2 %)**, fo4 18256 → **19399
  (+6.26 %)**, fnv 7174 → **7342 (+2.34 %)**. oblivion +1.84 % is inside the
  band; fo3 is **exact**.
- **Corroboration that the skyrim rise is not purely benign**: the standard
  defence for this creep (per #1705 / #2216) is that `bench_draws_cmds` *falls*
  while entities rise — more non-rendering bodies, not more rendering. That
  holds for fo4 here (cmds +1.3 % against entities +6.3 %) but **not** for
  skyrim_se, where cmds rose +5.0 % alongside the +15.2 % and the draw split
  broke contract (RT-2). Treat skyrim_se's entity rise as coupled to RT-2
  rather than as independent benign drift.
- **Suggested Fix**: bisect skyrim_se between 2026-08-06 and HEAD; regen the
  fnv/fo4 rows once RT-2 is understood.

### RT-9: Oblivion gained one genuine base-color texture miss
- **Severity**: LOW
- **Game**: oblivion · **Cell**: `ICMarketDistrictTheGildedCarafe`
- **Baseline**: 0 base-color misses (the "cleanest path" cell) · **Current**: 1
- **Path**: `facegen\ears\human\earshuman.dds  [slot=base_color]`
- **Note**: this survives the RT-4 correction — it is a real miss on the old
  metric surface, not an artifact of #3349. Its `_n` sibling
  (`facegen\ears\human\earshuman_n.dds`) also misses, so both an authored
  diffuse and its derived normal are unresolved for FaceGen ear geometry.
- **Suggested Fix**: check whether `facegen\` needs the same archive-layout
  resolution the `.spt` `trees\` case needed — a top-level prefix that the
  mesh-path normalizer is prepending `textures\` to, or failing to.

### RT-10: `light_count_directional` baseline is 1 on all five games; the measured value is 0 on four and 2 on the fifth
- **Severity**: LOW
- **Evidence**: with the real per-emitter dump now parsed (`kind=` rows):

  | Game | emitters | Directional | Point | baseline row |
  |------|----------|-------------|-------|--------------|
  | fnv | 30 | **0** | 30 | 1 |
  | fo3 | 11 | **0** | 11 | 1 |
  | oblivion | 10 | **2** | 8 | 1 |
  | skyrim_se | 28 | **0** | 28 | 1 |
  | fo4 | 685 | **0** | 685 | 1 |

- This confirms **#3424** live: the old row was derived from the mere presence
  of a `CellLightingRes` block, so it was a gate that could never fail. The
  emitter totals independently reproduce the skill's 2026-08-27 observations
  (fnv 30, fo3 11, skyrim_se 28, fo4 685) **exactly**; only oblivion differs
  (8 → 10), fully explained by the two synthetic directionals in RT-11.
- **Suggested Fix**: on the next `--regen`, replace the row with the measured
  `light_count_directional` plus a new `light_count_point` row.

### RT-11: Oblivion emits two byte-identical synthetic `__max_default_light` directional emitters
- **Severity**: LOW
- **Game**: oblivion
- **Evidence**: entities `142` and `143`, both
  `name="__max_default_light" kind=Directional source=nif/synthetic (no FormId ancestor)`,
  with identical `direction=[0.8947,0.3716,0.2478]`,
  `radiant=[1,1,1] dimmer=1.000 range_m=58.514`,
  `legacy_flags=0x00001000`. Oblivion is the only game of the five that
  synthesises a directional at all in an interior.
- **Impact**: an interior lit by two stacked full-intensity directionals
  receives double the intended synthetic contribution. Likely why oblivion is
  the one cell whose emitter total drifted from the skill's recorded value.
- **Suggested Fix**: de-duplicate synthetic default lights by name at import,
  or hoist to one per scene rather than one per contributing NIF.

### RT-12: FO4 counts one asset twice under two path spellings
- **Severity**: LOW
- **Game**: fo4
- **Evidence**: `tex.missing` lists both
  `90x textures\setdressing\wallconsoles\wallconsole01_sm_d_n.dds` and
  `18x setdressing/wallconsoles/wallconsole01_sm_d_n.dds` — same asset, one
  with a `textures\` prefix and backslashes, one without and forward-slashed.
  Same pattern on `effects/colorwhiteutility_n.dds` and
  `actors/character/hair/hair{short,long}01grayscale_d_n.dds`.
- **Impact**: inflates `tex_missing_unique_paths`, and more importantly implies
  two separate cache keys for one texture — a second archive lookup and
  potentially a second resident copy for any path that *does* resolve.
- **Suggested Fix**: normalize separator and `textures\` prefix before the
  cache key is taken, not just before the archive lookup.

### RT-13: first-frame hitch of 29 s (fnv) / 10 s (fo3) blocks the render thread
- **Severity**: LOW (advisory metric, real magnitude)
- **Evidence**: `frame_max_ms` = **29164.91** (fnv), **9960.93** (fo3), against
  `frame_p50_ms` of 15.88 / 8.35. The p95s (16.77 / 19.85) are unremarkable, so
  this is one blocking frame — cell load running on the render thread — not a
  distribution problem.
- **Note**: reported for visibility only. Per RT-2/#1701 `bench_frame_*_ms` is
  advisory under xvfb and is **not** raised as a gating regression here.
  Skyrim (41 ms) and oblivion (20 ms) do not show it, so it scales with cell
  content rather than being universal.

### RT-14: the runtime audit harness itself silently mis-attributes telemetry between games
- **Severity**: MEDIUM (audit-infrastructure)
- **Evidence**: **reproduced live in this run.** The skill's documented teardown
  (`kill -INT $PID` on the backgrounded `xvfb-run …` job) kills the *wrapper*,
  not the engine: `xvfb-run` execs the binary as a child, so the engine keeps
  running and keeps port 9876. The FNV run that followed Oblivion connected to
  the **still-live Oblivion engine** and captured Oblivion's numbers —
  `Entities: 718`, Oblivion's exact 8-path `tex.missing` list — under the FNV
  filename, with `dbg up at 1s` (impossible for a cell that takes ~40 s to
  load) as the only tell.
- **Impact**: this is exactly the RT-1/#1619 mis-attribution the skill warns
  about, but reached through teardown failure rather than parallelism — so
  running serially, as the skill instructs, does **not** prevent it. Any past
  `--game all` sweep using the documented teardown may carry silently shifted
  telemetry.
- **Fix applied for this audit** (recommend folding into the skill):
  1. pre-flight assert `pgrep -x byroredux` is empty and port 9876 is unbound
     (note: `pgrep -f 'target/release/byroredux'` self-matches the harness
     shell — use `pgrep -x`);
  2. resolve the real engine PID with `pgrep -x byroredux` after launch and
     sweep any survivor after teardown;
  3. **cross-check** `Entities:` from the `byro-dbg` `stats` line against
     `entities=` on the `bench:` line and hard-fail on divergence.
  All five captured runs in this report pass that cross-check.

---

## Playable-slice gates (2026-08-16 contract)

| Script | Result |
|--------|--------|
| `p0-door-interaction.sh` (skyrim_se) | **PASS** — 11/11: prompt → KeyE edge → ActivateEvent → persistent destination → deferred transition → Bannered Mare → WhiterunWorld; source cell 5817 entities |
| `p1-character-traversal.sh` | **PASS** — full round trip: walk → door → 2 streaming boundary crossings → return → door; exactly two door activations consumed; grounded settle held at every waypoint |
| `p2-melee-core.sh` | **PARTIAL** — combat core passes (50.0 Health → 7 bound attacks at 8.0 damage → 7 canonical `HitEvent`s → `Dead` → ragdoll). Gate 5 reports FAIL, but the failure is in the gate, not the engine — see **RT-6** |

---

## Cross-audit corroboration

Independent runtime evidence for and against claims raised elsewhere in this
suite. Reported as measured, including where it contradicts.

| Sibling claim | Runtime verdict |
|---------------|-----------------|
| `--game fnv` emits no `--sounds-bsa`, so footsteps/splash/REGN ambient silently no-op | **CONFIRMED.** `[profiles.fnv]` in `assets/debug_profiles.toml` has zero `default_sounds_bsas` entries; `[profiles.skyrim_se]` has two. Structural, verified directly. |
| CI's `BYRO_LOCK_ORDER_CHECK=1` gate is RED at HEAD (5 ragdoll test failures) | **CONTRADICTED — does not reproduce.** All **18** ragdoll tests pass with the detector enabled, in **both** profiles: `--release` (18 passed, 0 failed) and debug (18 passed, 0 failed). The detector is `#[cfg(debug_assertions)]`-gated (`crates/core/src/ecs/lock_tracker.rs:248`) and `debug-assertions` is unset in `Cargo.toml`, so it defaults ON for dev/test — the debug run genuinely exercised it. `cargo test -p byroredux-physics` is also 153/153 green under the same env var. |
| All 1,282 Starfield FaceMeshes NIFs import ZERO meshes | **PARTIALLY OBSERVED, not quantifiable from this cell.** Cydonia logged 14 zero-mesh NIF imports (markers, `skeleton.nif`), not 1,282 — this cell does not load the FaceMeshes archive. **However**, an adjacent and arguably worse SF defect surfaced instead: 100 % unresolved skin bones (RT-3). |
| No `synthesize_normals` → constant `[0,1,0]` normal on 20.4 % Skyrim / 100 % Oblivion distant-LOD / 90.01 % FO4 distant-LOD meshes | **NOT EXERCISED.** All five baselined cells are interiors; distant-object LOD never loads. Neither confirmed nor contradicted — needs an exterior cell run. |
| All 154 vanilla `.spt` TREE MODLs miss the archive (live under top-level `trees\`) | **NOT EXERCISED** — interiors have no trees. Note RT-9 is the same *shape* of bug (a top-level `facegen\` prefix missing) on a path that interiors do reach. |
| Shared cell walker drops placed records: 350 FO3, 2,928 FO4, 1,643 Starfield | **NOT MEASURABLE** from these logs — per-record drops are not individually logged at default verbosity, and the aggregate drop/skip line counts (fnv 20, oblivion 31, fo3 1, fo4 3, skyrim 3, sf 2) count log *lines*, not records. Needs a dedicated counter. |

---

## Baseline actions recommended (none applied — `--regen` was not passed)

| Baseline | Action |
|----------|--------|
| `skyrim_se-WhiterunDragonsreach.tsv` | **HOLD** — RT-2 / RT-8 are unexplained regressions; leave stale so the evidence survives |
| `fnv-FreesideAtomicWrangler.tsv` | regen `tex_missing_unique_paths`, `skin_pool_live`, `entities_total`, `light_count_directional` after RT-4/RT-10 are resolved |
| `fo4-InstituteBioScience.tsv` | same as fnv |
| `fo3-MegatonPlayerHouse.tsv` | regen `tex_missing_unique_paths` (0→12, surface change) and `mesh_cache_failed_count` (3→0, genuine improvement); everything else is **exact** — this file is the healthiest of the five and independently re-confirms #3407's `1581/100b/11c` correction |
| `oblivion-ICMarketDistrictTheGildedCarafe.tsv` | **HOLD** on `tex_missing` until RT-9 is fixed |
| starfield | **no baseline created** — nothing capturable (RT-1) |

## Severity summary

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 1 | RT-1 |
| HIGH | 2 | RT-2, RT-3 |
| MEDIUM | 6 | RT-4, RT-5, RT-6, RT-7, RT-8, RT-14 |
| LOW | 5 | RT-9, RT-10, RT-11, RT-12, RT-13 |
| **Total** | **14** | |

## Method notes

- Release build at `64f64480`, `CARGO_BUILD_JOBS=4`. No test binary was ever run
  with `--ignored` / `--include-ignored`.
- Games driven **serially** under `xvfb-run -a --server-args="-screen 0
  1280x720x24"`, `--bench-frames 240 --bench-hold`, telemetry over `byro-dbg`
  on port 9876.
- Every captured run passes the bench-vs-`stats` entity cross-check added in
  RT-14. The one run that failed it (the first FNV attempt) was discarded and
  re-run, not reported.
- `bench_fps_*` and `bench_frame_*_ms` are reported for visibility only and are
  **not** raised as findings, per RT-2/#1701.
