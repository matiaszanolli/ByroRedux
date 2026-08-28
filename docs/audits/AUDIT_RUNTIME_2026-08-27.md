# Runtime Telemetry Audit — 2026-08-27

> Dispatched 2026-08-27; engine captures ran 2026-08-28 04:51–06:20 UTC as the
> clock rolled over mid-session. Filed under the dispatch date so the suite's
> bookkeeping matches. Every number below is from a run at `969d81c8`.

## Scope and execution mode

**Live headless-engine comparison pass: EXECUTED, all five baselined cells,
plus the three playable-slice smoke gates.**

`pgrep -af byroredux` and `pgrep -af byro-dbg` were both empty at dispatch and
stayed empty for the whole session, so the no-parallel-engine rule
(`feedback_no_parallel_engine_launch`) permitted Phase 2–4. Method per
`audit-runtime/SKILL.md`: `xvfb-run -a --server-args="-screen 0 1280x720x24"`,
`--bench-frames 240 --bench-hold`, wait on the `bench-hold:` notice (not a
`byro-dbg` ping — the 2026-08-26 hazard note), then `byro-dbg` capture of
`stats` / `tex.missing` / `mesh.cache failed` / `light.dump`. Serial, one
engine at a time on port 9876, each PID reaped by handle before the next
launch. `pkill -f byroredux` was never used.

**Games measured: all five with a committed baseline — `fnv`, `fo3`,
`oblivion`, `fo4`, `skyrim_se`.** Skipped: `starfield` (no cell baseline
exists; profile ships empty archives and no `sample_cells` — use `--sf-smoke`),
and there is no `fo76` profile. Nothing was inferred: every row in the tables
below came out of a run performed in this session.

Beyond the five baseline runs, this pass performed **11 additional engine
runs** for attribution: `fo4`/`skyrim_se`/`fo3` probes at `fa71f1a2`
(`e0d5ec18^`), an `fo3` probe at `3aebf414`, two `fo3` repeat runs at HEAD,
an instrumented `skyrim_se` run, and the four smoke-gate engines. Two throwaway
release builds were made in a `git worktree` (removed at teardown; the
repository tree was not modified).

## Result summary

| Game | Cell | Baseline date | Verdict |
|---|---|---|---|
| `fnv` | `FreesideAtomicWrangler` | 2026-08-26 | **PASS — exact match on every gated metric** |
| `oblivion` | `ICMarketDistrictTheGildedCarafe` | 2026-08-26 | **PASS** (entities +1, draw split identical) |
| `fo3` | `MegatonPlayerHouse` | 2026-08-27 | REGRESSION (HIGH) — but the *baseline* is wrong, not the engine (RT-03) |
| `fo4` | `InstituteBioScience` | 2026-08-22 | REGRESSION (MEDIUM) — entities +10.4 %, `skin_pool_live` +40.7 %, attributed (RT-04) |
| `skyrim_se` | `WhiterunDragonsreach` | 2026-08-09 | REGRESSION (HIGH) — 23 body meshes never reach the GPU (RT-01); entities +15.2 % (RT-04) |

`fnv` and `oblivion` reproducing their 2026-08-26 baselines **bit-for-bit on
the draw split** is the control for this pass: the capture path is sound, so
the deltas on the other three belong to those cells, not to the harness.

## Measurements

| Metric | fnv | oblivion | fo3 | fo4 | skyrim_se |
|---|---|---|---|---|---|
| `entities_total` | 7174 → 7180 (+0.1 %) | 705 → 706 (+0.1 %) | 3493 → 3493 (0.0 %) | 18256 → **20154 (+10.4 %)** | 8126 → **9363 (+15.2 %)** |
| `tex_missing_unique_paths` | 1 → 1 | 0 → 0 | 0 → 0 | 1 → 1 | 0 → 0 |
| `mesh_cache_failed_count` | 0 → 0 | 0 → 0 | 3 → **0** | 0 → 0 | 9 → **0** |
| `light_count_directional` | 1 | 1 | 1 | 1 | 1 |
| `skin_pool_live` | 206 → 206 | 4 → 4 | 7 → 7 | 248 → **349 (+40.7 %)** | 83 → **133 (+60.2 %)** |
| `skin_pool_max` | 1364 → 1364 | 1364 → 1364 | 1364 → 1364 | 1364 → 1364 | 1364 → 1364 |
| `skin_pool_overflow_attempts` | 0 | 0 | 0 | 0 | 0 |
| `bench_draws_cmds` | 2110 → 2110 | 325 → 325 | 1581 → 1581 | 3949 → 4050 (+2.6 %) | 2342 → 2460 (+5.0 %) |
| `bench_draws_batches` | 109 → 109 | 20 → 20 | 164 → 100 (−39 %) | 296 → 304 (+2.7 %) | 9 → **20 (×2.22)** |
| `bench_draws_gpu_calls` | 26 → 26 | 2 → 2 | 9 → **11 (×1.22)** | 16 → 16 | 2 → **4 (×2.00)** |

Advisory only (`bench_fps_*` / `bench_frame_*_ms`, RT-2 / #1701 — never gating):

| Metric | fnv | oblivion | fo3 | fo4 | skyrim_se |
|---|---|---|---|---|---|
| `bench_fps_p50` | 65.4 → 67.7 | 377.6 → 363.8 | 62.7 → 93.4 | 44.3 → 41.7 | 161.9 → 118.2 |
| `frame_p50_ms` | 15.41 | 2.73 | 7.98 | 23.22 | 8.43 |
| `frame_p95_ms` | 16.21 | 3.06 | 18.90 | 24.15 | 11.10 |
| `frame_max_ms` | 303.73 | 7.22 | 25.75 | 122.70 | 42.88 |

No baseline carries `bench_frame_*_ms` rows yet, so those three are recorded
here as a first capture rather than diffed.

**Uncaptured previously, captured now.** `light.dump` emits a per-cell point
emitter tally that no baseline records: `fnv` 30, `fo3` 11, `oblivion` 8,
`skyrim_se` 28, `fo4` 685. See RT-07.

**Zero `ERROR`-level lines in all five engine logs.** No Vulkan validation
messages, no panics.

## Playable-slice smoke gates

The skill names these the runtime contract for the gameplay slice, which has
no owner audit skill. All four were run to completion in this session.

| Gate | Arm | Result |
|---|---|---|
| `p0-door-interaction.sh` | `skyrim_se` (default) | **PASS** — prompt → E → `ActivateEvent` → Bannered Mare → `WhiterunWorld` |
| `p1-character-traversal.sh` | `skyrim_se` (default) | **PASS** — walk → door → boundary → return → door, 2 crossings |
| `p2-melee-core.sh` | `skyrim_se` (default) | **FAIL** at ESM preflight (RT-05) |
| `p2-melee-core.sh fnv` | `fnv` | **FAIL** in the engine phase (RT-06) |

## Findings

### RT-2026-08-27-01: 23 Skyrim skinned body meshes reach `MeshRegistry::upload` with zero indices and are silently dropped before the GPU

- **Severity**: HIGH
- **Dimension**: runtime telemetry → renderer mesh upload
- **Game**: `skyrim_se`
- **Cell**: `WhiterunDragonsreach`
- **Location**: `crates/renderer/src/mesh.rs:491-517`; `crates/renderer/src/vulkan/buffer.rs:790-817` and `:1326-1330`; `byroredux/src/scene/nif_loader.rs:826-837`
- **Status**: NEW
- **Description**: Twenty-three skinned NPC meshes in `WhiterunDragonsreach`
  arrive at `MeshRegistry::upload` with a non-empty vertex slice and an
  **empty index slice**. `create_vertex_buffer` succeeds; `create_index_buffer`
  computes `size = std::mem::size_of_val(data)` = 0
  (`buffer.rs:799`), hands it to `create_device_local_buffer` →
  `create_staging_buffer(device, allocator, 0, "buffer_staging")`
  (`buffer.rs:1329`), and `gpu_allocator` rejects the allocation outright —
  `if size == 0 || !alignment.is_power_of_two() { return Err(InvalidAllocationCreateDesc) }`
  (`gpu-allocator-0.28.0/src/vulkan/mod.rs:799`). The `?` at `mesh.rs:508`
  propagates, `nif_loader.rs:836` logs a `warn!` and `continue`s, and the
  mesh renders nothing.
- **Evidence**: An instrumented release build (throwaway worktree, one
  `log::error!` inserted between the two buffer creations in `upload`) on
  `--game skyrim_se --cell WhiterunDragonsreach`:

  ```
  23 AUDIT-PROBE hits, all of the form vertices=N indices=0:
       6 vertices=992  indices=0        6 vertices=218  indices=0
       5 vertices=417  indices=0        3 vertices=174  indices=0
       1 vertices=676  indices=0        1 vertices=850  indices=0
       1 vertices=872  indices=0
  Failed to upload NIF mesh : 23        GpuBuffer dropped : 23
  ```

  The 23 failures are 1:1 with the 23 mesh names logged by the uninstrumented
  run, and they are exactly the humanoid skin/underwear set:

  ```
  6x 'BODY'   5x 'MaleUnderwear_1'   5x 'FootMale_Big'   3x 'Feet'
  1x 'HandMaleBig3rd'  1x 'HandFemale3rd'  1x 'FootFemale_Big'  1x 'FemaleUnderwear'
  ```

  VRAM exhaustion is disproved by the same log: `GPU memory: 1294.3 MB
  allocated / 1755.5 MB reserved` on a 12 GB device. `mesh_cache_failed_count`
  is **0** — the NIFs parse cleanly; the geometry that comes out of the decode
  has vertices and no triangles.
- **Impact**: Named Whiterun NPCs (Balgruuf, Hrongar, Farengar, Proventus…)
  lose torso / hands / feet geometry at render time. This directly negates
  `e0d5ec18` (#3357), whose stated purpose was to make exactly these
  `NakedTorso` / `NakedHands` / `NakedFeet` addons resolve: they now resolve
  and then fail to upload. Pre-existing but amplified — a probe at
  `fa71f1a2` (`e0d5ec18^`) records **9** such failures against **23** at HEAD,
  so #3357 multiplied the affected mesh count 2.6×.
- **Related**: Adjacent to, but distinct from, the concurrently-filed
  "351/351 Skyrim vanilla creature-race NPCs lose their body mesh" — that is a
  race/ARMA *resolution* failure; this is a *geometry-decode + upload* failure
  on humanoid actors whose meshes resolve correctly. Amplified by #3357
  (`e0d5ec18`). See also RT-02 for the leak on the same error path.
- **Suggested Fix**: Two layers. (1) Find why the SSE skinned decode emits a
  zero-triangle partition for these shapes — `crates/nif/src/import/mesh/skin.rs`
  and `sse_recon.rs` are the candidates, and `07ca5979` (#3355/#3360, "SSE
  SkinPartition triangles are global indices") is the most recent change to
  that decode, though the failure predates it. (2) Reject the mesh at the
  import boundary rather than at the allocator, so a zero-triangle shape is
  never queued for upload (see RT-02).

### RT-2026-08-27-02: `MeshRegistry::upload` leaks the vertex `GpuBuffer` when the index buffer fails, has no empty-input guard, and swallows the real error

- **Severity**: MEDIUM
- **Dimension**: renderer resource lifetime / diagnosability
- **Location**: `crates/renderer/src/mesh.rs:499-517`; `byroredux/src/scene/nif_loader.rs:830-835`
- **Status**: NEW (related to the CLOSED #656 safety net and CLOSED #87)
- **Description**: Three defects on one error path, all exercised 23× per
  `WhiterunDragonsreach` load today:
  1. `upload()` binds `vertex_buffer` to a local, then `?`-propagates
     `create_index_buffer`. On failure the vertex `GpuBuffer` is dropped
     without `destroy()`, so the #656 `Drop` safety net reclaims it and logs
     `GpuBuffer dropped without destroy() — running cleanup from Drop`
     (`crates/renderer/src/vulkan/buffer.rs:1625`). The buffer is reclaimed,
     so this is not a leak in release — but the same arm carries
     `debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first")`
     (`buffer.rs:1631-1633`), which would **abort a debug build 23 times on a
     single cell load**.
  2. There is no empty-input guard. `vertices.is_empty()` or
     `indices.is_empty()` produces a `VkBufferCreateInfo.size == 0`, which is
     also a spec violation (`VUID-VkBufferCreateInfo-size-00912`) that only
     escapes notice because the allocator rejects the allocation a moment later.
  3. `nif_loader.rs:832` formats the `anyhow::Error` with `{}`, printing only
     the outermost context (`Failed to allocate buffer_staging staging
     memory`) and discarding the `InvalidAllocationCreateDesc` source that
     names the real cause. RT-01 needed an instrumented build only because of
     this.
- **Impact**: A whole class of degenerate geometry is diagnosed as "out of
  staging memory", which is what a reader would reasonably chase first; and
  no debug build can load Dragonsreach.
- **Suggested Fix**: Early-return an explicit error from `upload` when either
  slice is empty; wrap `vertex_buffer` in a guard (or `destroy()` it) on the
  index-buffer error arm, mirroring the `StagingGuard` pattern already used
  inside `create_device_local_buffer`; switch the `nif_loader` log to `{:#}`.

### RT-2026-08-27-03: the committed `fo3` baseline's draw split does not reproduce in six runs across three commits, and #3005 was closed on it

- **Severity**: HIGH
- **Dimension**: regression-guard integrity
- **Game**: `fo3`
- **Cell**: `MegatonPlayerHouse`
- **Location**: `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` (rows `bench_draws_batches`, `bench_draws_gpu_calls`)
- **Status**: NEW
- **Description**: The baseline regenerated on 2026-08-27 by `fb21f9ee`
  records `bench_draws_cmds 1581 / bench_draws_batches 164 /
  bench_draws_gpu_calls 9`. The `cmds` figure reproduces exactly; the other
  two do not reproduce anywhere. Every run of this cell that this audit could
  perform or find reads **`1581/100b/11c`**:

  | Run | Commit | Draw split |
  |---|---|---|
  | `AUDIT_RUNTIME_2026-08-26.md` measurement table | pre-`cc666a48` | 1581/**100**b/**11**c |
  | `fb21f9ee` commit message + the TSV it wrote | `3aebf414` | 1581/**164**b/**9**c |
  | This audit, probe | `3aebf414` (rebuilt) | 1581/**100**b/**11**c |
  | This audit, probe | `fa71f1a2` | 1581/**100**b/**11**c |
  | This audit, main sweep | `969d81c8` | 1581/**100**b/**11**c |
  | This audit, repeat ×2 | `969d81c8` | 1581/**100**b/**11**c |

  The third row is the decisive one: rebuilding **the exact commit
  `fb21f9ee` says it measured at** and running **the exact invocation it
  documents** (`--game fo3 --cell MegatonPlayerHouse --bench-frames 240`
  under xvfb) yields 100/11, not 164/9. This is therefore not run-to-run
  jitter — five of six observations agree, across three different builds,
  and `entities_total` (3493), `meshes` (609) and `cmds` (1581) are identical
  in every single one.
- **Evidence**: Both wrong values have a plausible provenance in the adjacent
  text. `164` is FNV's 2026-08-12 spike batch count, printed two lines away in
  the bisect timeline that `AUDIT_RUNTIME_2026-08-26.md` and the FNV TSV
  header both carry (`9e96a9f9 2026-08-12 2562/164b/35c`). `9` is `fo3`'s own
  *pre-existing* 2026-06-14 `gpu_calls` value — i.e. the row that was
  supposedly "recovered" is the row that was never updated.
- **Impact**: Both directions are broken.
  - `bench_draws_gpu_calls = 9` against a true 11 is **×1.22, permanently
    outside the ×1.1 gate**. Every future `fo3` run reports a regression that
    does not exist — the precise failure mode the 2026-08-26 pass said it was
    refreshing baselines to avoid.
  - `bench_draws_batches = 164` against a true 100 grants **64 batches of
    false headroom**; a real ×1.6 batching regression on this cell would pass
    silently.
  - #3005 (CLOSED 2026-08-27) was closed on the strength of "the gpu_calls
    half of the finding is gone on both scenes". It is gone on `fnv`
    (verified here: 26 → 26, exact). On `fo3` it is not: 9 → 11 is outside
    contract, and was outside contract at the commit the closure measured.
- **Related**: #3005 (CLOSED), #2521 (CLOSED), `AUDIT_RUNTIME_2026-08-26.md`
  RT-2026-08-26-04.
- **Suggested Fix**: Correct the two rows to `bench_draws_batches 100` /
  `bench_draws_gpu_calls 11`, keeping the header's `draw_sort_key` /
  RT-1 / #2215 justification (which is sound and applies to the true 100
  as much as to 164 — merge efficiency is 15.8 cmds/batch, not 9.6). Then
  re-examine whether #3005's `fo3` arm should reopen: against the 2026-06-14
  baseline of 96/9 the true HEAD values are ×1.04 batches (inside) and ×1.22
  gpu_calls (outside), which is the mirror image of what the closure recorded.
  Not regenerated by this audit — no `--regen` was requested, and a baseline
  should not be overwritten by the same pass that found it wrong.

### RT-2026-08-27-04: `fo4` and `skyrim_se` entity + skin-pool counts move past their gates; bisected to #3357's multi-ARMA armor resolve

- **Severity**: MEDIUM
- **Dimension**: runtime telemetry → NPC equip
- **Games**: `fo4`, `skyrim_se`
- **Location**: `crates/plugin/src/equip.rs:192-212` (`resolve_armor_meshes`, pass 1); consumers `byroredux/src/npc_spawn.rs:803-817` and `:918-931`
- **Status**: NEW (attribution of a gate move, not a claim that the fix is wrong)
- **Description**: Four gated metrics moved outside contract:

  | Metric | fo4 | skyrim_se |
  |---|---|---|
  | `entities_total` | 18256 → 20154 (+10.4 %, band ±2 %) | 8126 → 9363 (+15.2 %) |
  | `skin_pool_live` | 248 → 349 (+40.7 %, gate ≤ baseline) | 83 → 133 (+60.2 %) |

  A probe at `fa71f1a2` (`e0d5ec18^`) isolates the cause cleanly:

  ```
  fo4 InstituteBioScience   fa71f1a2  entities=18506  skin=278  armor_meshes=76
                            969d81c8  entities=20154  skin=349  armor_meshes=147
  skyrim WhiterunDragonsreach fa71f1a2 entities= 8685  skin=108  armor_meshes=39
                            969d81c8  entities= 9363  skin=133  armor_meshes=58
  ```

  On `fo4` the armor-mesh delta (+71) and the `skin_pool_live` delta (+71) are
  **identical**, so #3357 accounts for the skinned-mesh rise exactly. `fnv`,
  `fo3` and `oblivion` are untouched — `resolve_armor_meshes` short-circuits
  to the single-`MODL` path for pre-Skyrim games (`equip.rs:169-178`), which
  is why those three baselines reproduce perfectly.
- **Evidence**: The per-actor distribution is where this stops being obviously
  benign. On `fo4 InstituteBioScience`:

  ```
  fa71f1a2:  13 actors x1    12 actors x2    1 actor x7     4 actors  x8
  969d81c8:  13 actors x1    12 actors x3    1 actor x18    4 actors x20
  ```

  `InstM03LvlSynth` and `LvlSynth_Institute_Superbarrel` now equip 20 and 18
  simultaneous armor meshes. Pass 1 returns every race-matching ARMA of an
  ARMO without consulting that addon's own biped region, and #2094's
  slot-occupancy `retain` treats the whole group as one `inv_idx`, so a
  multi-part item's meshes are not subject to slot displacement. That is the
  same *shape* as the over-equip that `bfdc3d3f` removed from `fnv` on
  2026-08-23 (29 meshes from 9 inventory entries), reached by a different
  mechanism. Whether 20 is correct for an Institute synth needs the ARMO/ARMA
  data and is out of scope for a telemetry pass — this audit reports the
  measurement and the mechanism, not a verdict.
- **Impact**: Two baselines are red on two metrics each. If #3357's counts are
  correct, both baselines need regeneration with the attribution recorded in
  the header (the `bfdc3d3f`/`fnv` precedent). If the ×2.5 per-actor rise on
  `fo4` is over-equip, `skin_pool_live` is carrying real waste — 101 extra
  skinned meshes on one interior, against a pool cap of 1364, at 40 % of the
  scene's entire skinned budget.
- **Related**: `e0d5ec18` (#3357), `bfdc3d3f`, #2094, #2093.
  `AUDIT_RUNTIME_2026-08-26.md` RT-2026-08-26-01 is the `fnv` precedent.
  RT-01 is the downstream consequence on `skyrim_se`.
- **Suggested Fix**: Decide the correctness question first (does an FO4 ARMO's
  ARMA set legitimately cover 20 distinct regions for one actor?), then either
  regenerate both baselines with a header recording `e0d5ec18`, or gate pass 1
  on the addon's own biped mask so a region already covered by a higher-priority
  item does not also spawn its ARMA mesh.

### RT-2026-08-27-05: the default `p2-melee-core` gate has never passed — its Skyrim weapon-family assertion was unsatisfiable at the commit that authored it

- **Severity**: MEDIUM
- **Dimension**: playable-slice smoke gates (un-owned subsystem)
- **Location**: `docs/smoke-tests/fixtures/skyrim_se.env:87-90`; assertion at `docs/smoke-tests/p2-melee-core.sh:112-115`
- **Status**: NEW
- **Description**: `p2-melee-core.sh` with no argument selects `skyrim_se`
  (`docs/smoke-tests/lib/fixture.sh:45`), and fails immediately at the ESM
  preflight:

  ```
  smoke[p2-melee-core]: FAIL -- weapon leaf 0001CB64:DraugrBattleAxe:damage=18 is absent from the fixture family
  ```

  The fixture pins two leaves for `BleakFallsBarrow01`; only
  `000236A5:DraugrGreatsword:damage=17` is produced. Across the whole cell the
  probe emits `6x DraugrGreatsword:damage=17`, `3x DraugrWarAxe:damage=9`, and
  **zero** `DraugrBattleAxe`. The frozen target `000383F7` /
  `EncDraugr01AmbushMelee2HHeadM06` resolves to the Greatsword.
- **Evidence**: `probe_combat_fixture` was rebuilt and run at **`3aebf414`**,
  the commit that introduced `fixtures/skyrim_se.env`, and produces the same
  three weapon lines with no `DraugrBattleAxe`. The assertion was therefore
  never satisfiable — the gate has been deterministically RED since it was
  authored on 2026-08-27, and its engine phase (character mode, hit chain,
  Health→`Dead`, 18-body ragdoll) has never executed on the default arm.
- **Impact**: The gameplay slice has no owner audit skill; these gates are its
  only coverage. A gate that fails before launching the engine provides none,
  and a red-by-default gate trains readers to ignore it. The skill's own
  reference text still describes P2 as "passing as of 2026-08-16" — that
  predates the `#3039` fixture parameterisation and is now wrong for the
  default arm.
- **Related**: #3039 (fixture parameterisation), `3aebf414`. `AUDIT_RUNTIME_2026-08-16.md`
  found the same *class* of defect (gates deterministically red from assertion
  drift) in two other gates.
- **Suggested Fix**: Re-derive `P2_PROBE_WEAPON_LINES` from a live
  `probe_combat_fixture` run rather than by hand, or drop the second leaf.
  Worth asking separately whether the leveled weapon list for `000E9895`
  *should* still reach the battle-axe leaf — if it should, the fixture is
  right and the LVLI expansion is the defect.

### RT-2026-08-27-06: on the FNV arm, the P2 melee swing lands on a different actor than the fixture target and applies zero damage

- **Severity**: HIGH
- **Dimension**: playable-slice smoke gates (un-owned subsystem) → combat
- **Game**: `fnv`
- **Cell**: `GSProspectorSaloonInterior`
- **Location**: gate `docs/smoke-tests/p2-melee-core.sh`; fixture `docs/smoke-tests/fixtures/fnv.env:85-100`; runtime `byroredux/src/combat.rs`, `byroredux/src/interaction.rs` (camera ray → `ActorColliderOwner`)
- **Status**: NEW — **not bisected**
- **Description**: `p2-melee-core.sh fnv` clears preflight, reaches the held
  interactive state, resolves the frozen reference `0x104C6D` (`gstrudy`) to
  entity 1088, confirms Character mode and a clean combat state, passes the
  blocked-swing sub-check — and then fails:

  ```
  smoke[p2-melee-core]: FAIL -- the swing did not land on the fixture's target (entity 1088); last_target=927
  ```
- **Evidence**: the retained `combat.status` artifact:

  ```
  Combat status:
    cooldown=0.439 blocking=true attacks=1 hits=1 kills=0
    last_target=927 damage=0.0 health_before=220.0 health_after=220.0 killed=false
    outcome=health 220.0 -> 220.0
  ```

  `inventory.entities` identifies entity 927 as `"gssettlercm"` — a different
  NPC. The fixture's target is `gstrudy` at 240.0 Health; the struck actor has
  220.0. So two things are wrong at once: the camera ray after
  `combat.approach` resolves to the wrong actor, **and** the hit that did land
  applied `damage=0.0` and left Health unchanged.
- **Impact**: This is the melee vertical slice's only end-to-end contract, and
  it is failing on the arm that still reaches the engine. The zero-damage hit
  is independently concerning — `hits=1` with `damage=0.0` means the
  `HitEvent` → Health path ran and resolved to no damage, which the gate would
  have caught as a kill failure even had the ray been correct.
- **Related**: RT-05 (the other arm of the same gate). #2976 (`HitEvent::blocked`
  wiring) is the most recent change to this path. RT-04's armor-mesh growth
  changes what collider geometry the ray can hit, though `fnv` is *not* on the
  #3357 path — so that is a lead, not an attribution.
- **Suggested Fix**: Reproduce with `BYROREDUX_SMOKE_LOG=debug` and dump the
  camera ray's hit list; check whether `combat.approach` still lands the
  capsule where the fixture assumes, and whether `ActorColliderOwner`
  resolution is picking the nearest bone collider irrespective of owner.
  Separately, trace why a landed hit produced `damage=0.0` — `UNARMED_DAMAGE`
  is 8.0 (`byroredux/src/combat.rs`), so zero is not a configured value.

### RT-2026-08-27-07: `light.dump` has emitted a point-light tally for 13 days, the skill says it does not, and `light_count_directional` is a dead gate

- **Severity**: LOW
- **Dimension**: audit-infrastructure doc-rot / test gap
- **Location**: `.claude/commands/audit-runtime/SKILL.md` (Phase 3 metric table + the `light.dump` quirk note); emitter at `byroredux/src/commands/scene.rs:195`
- **Status**: NEW
- **Description**: The skill states that `light.dump` "surfaces the one
  directional sun, not a per-point-light tally, so `light_count_directional`
  is effectively a constant 1 and there is no `light_count_point`". Since
  `5f970bae` (2026-08-15) the command prints `LightSource emitters: {}` plus a
  full per-emitter dump (kind, source, position, radiance, dimmer, range,
  attenuation, visibility and flag words). This audit captured it on every
  game: `fnv` 30, `fo3` 11, `oblivion` 8, `skyrim_se` 28, `fo4` 685.

  The other half is worse than stale. All five baselined cells are interiors;
  every one dumps `directional_color = [0.000, 0.000, 0.000]`, and the
  baselined `light_count_directional` row is not read from any printed count —
  it is inferred from the mere presence of a `CellLightingRes` block. The row
  therefore cannot fail, on any cell, ever. It is a gate that measures nothing
  while a real, discriminating per-cell light count sits uncaptured beside it.
- **Impact**: One of eight structural gates is inert, and a genuinely useful
  one (a light count is exactly the sort of thing a cell-loader or LIGH-parsing
  regression moves) is not collected. `fo4`'s 685 emitters against `oblivion`'s
  8 shows the metric has real dynamic range.
- **Suggested Fix**: Add a `light_count_point` row sourced from
  `LightSource emitters: N`, direction "exact match"; either drop
  `light_count_directional` or redefine it as the count of `kind=Directional`
  entries in the same dump so it is actually parsed rather than assumed.
  Update the skill's Phase 3 quirk note.

## Reconciliation with the two prior runtime audits

Neither prior report's findings are re-filed here. Their current status:

| Prior finding | Status at `969d81c8` |
|---|---|
| `AUDIT_RUNTIME_2026-08-24.md` RT-01 (baselines 18–71 days stale) | **Resolved.** All five were re-measured live in this pass; the oldest is now 19 days (`skyrim_se`) and four are ≤ 6 days. |
| `AUDIT_RUNTIME_2026-08-24.md` RT-02 (could not verify #3005) | **Resolved, with a correction.** #3005 verified live: `fnv` 2110/109b/26c reproduces the baseline exactly. Its `fo3` arm's closing measurement does not reproduce — see RT-03. |
| `AUDIT_RUNTIME_2026-08-26.md` RT-01 (`fnv` −22.6 % traced to `bfdc3d3f`) | **Confirmed.** The regenerated `fnv` baseline reproduces bit-for-bit on all eleven gated rows. Best control in this pass. |
| `AUDIT_RUNTIME_2026-08-26.md` RT-02 (`fnv` ×1.22 / `skyrim_se` ×2.33 batches) | **`fnv` half resolved** (109 → 109, exact, after the `fb21f9ee` refresh). **`skyrim_se` half still red and still unfiled**: batches 9 → 20 (×2.22), gpu_calls 2 → 4 (×2.00) against the 2026-08-09 baseline. Both values are identical at `fa71f1a2`, so this predates #3357 and is not new; it has no GitHub issue (#3005 covered `fnv`/`fo3` only, #2351 was closed as non-reproducing). Recorded, not re-filed. |
| `AUDIT_RUNTIME_2026-08-26.md` RT-03 (`skyrim_se` device-lost during hold) | **Did not reproduce.** Four `skyrim_se` engine launches in this session (one 240-frame `--bench-hold` capture, two probes, one instrumented run) all completed with no `device has been lost` and no `ERROR` line, and the `byro-dbg` telemetry that could not be captured on 2026-08-26 was captured cleanly here. Not closed on 0/4 — recorded as non-reproducing on this session's driver/build. |
| `AUDIT_RUNTIME_2026-08-26.md` RT-04 (`fo3` entities +5.5 %) | **Resolved** by the `fb21f9ee` refresh; `entities_total` 3493 → 3493 exact, and identical at `fa71f1a2` and `3aebf414`. |

Context supplied by concurrent audits and deliberately **not** re-filed:
SpeedTree billboard texture resolution on FNV/FO3/Oblivion; FNV exterior door
transitions and `LoadedCellIndex`; the 351/351 Skyrim creature-race body-mesh
loss (see RT-01's `Related` for how this pass's Skyrim finding differs); the
Scaleform overlay compositing inside the main geometry pass.

## Baseline actions

**None. No baseline file was written by this audit.** `--regen` was not
requested, and each of the three red cells has a reason to hold:

- `fo3` — the committed values are wrong (RT-03); correcting them is a
  deliberate edit that should carry its own justification, not a side effect
  of the pass that discovered the error.
- `fo4` / `skyrim_se` — the deltas are attributed to `e0d5ec18` but the
  correctness question behind it is open (RT-04). Overwriting a baseline
  whose delta you can *explain* but not yet *endorse* is how a real regression
  gets laundered into a new baseline.

`fnv` and `oblivion` needed no write — they matched.

## Reproduction

```bash
# Five-game sweep, serial, one engine at a time on port 9876.
for pair in "fo4 InstituteBioScience" "fnv FreesideAtomicWrangler" \
            "fo3 MegatonPlayerHouse" "oblivion ICMarketDistrictTheGildedCarafe" \
            "skyrim_se WhiterunDragonsreach"; do
  set -- $pair
  xvfb-run -a --server-args="-screen 0 1280x720x24" \
    ./target/release/byroredux --game "$1" --cell "$2" \
    --bench-frames 240 --bench-hold > "/tmp/$1.log" 2>&1 &
  PID=$!
  until grep -q 'bench-hold' "/tmp/$1.log"; do sleep 1; done   # NOT a byro-dbg ping
  sleep 3
  printf "stats\ntex.missing\nmesh.cache failed\nlight.dump\nquit\n" \
    | ./target/release/byro-dbg > "/tmp/$1.telem" 2>&1
  kill -INT $PID; sleep 3; kill -9 $PID 2>/dev/null; wait $PID 2>/dev/null
done   # never `pkill -f byroredux` — it matches the orchestrating script's own argv

# RT-01 instrumentation (throwaway worktree; do not commit):
#   crates/renderer/src/mesh.rs, inside upload(), between the two buffer creations:
#     if vertices.is_empty() || indices.is_empty() {
#         log::error!("AUDIT-PROBE: upload() vertices={} indices={}", vertices.len(), indices.len());
#     }

# RT-03 / RT-04 attribution:
git worktree add /tmp/wt e0d5ec18^ && cd /tmp/wt && \
  CARGO_TARGET_DIR=/tmp/wt-target cargo build --release -p byroredux

# RT-05:
cargo run --quiet -p byroredux-plugin --example probe_combat_fixture -- \
  "$SKYRIM_DATA/Skyrim.esm" BleakFallsBarrow01 | grep -c DraugrBattleAxe   # -> 0
```

## Summary

7 findings: **0 CRITICAL · 3 HIGH · 3 MEDIUM · 1 LOW.**

Suggested next step:

```
/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md
```
