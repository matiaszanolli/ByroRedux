# ByroRedux — Roadmap

A Rust + Vulkan rebuild of the Gamebryo/Creation engine lineage, targeting
the full Oblivion → Starfield range. This document is the live source of
truth for **what works, what's next, and why**. Session narratives live in
[HISTORY.md](HISTORY.md); per-commit archaeology lives in `git log`.

**Keeping this document honest.** Run `/session-close` at the end of each
working session. It diffs stated facts against ground truth (test count,
LOC, open issues, bench freshness, completeness of repro commands) and
proposes a single synchronised edit across ROADMAP / HISTORY / README.
Ritual-driven, not hook-driven — one checkpoint per session, not N per
commit.

**Last verified**: 2026-04-26.
**Bench-of-record**: Prospector Saloon 172.6 FPS / 5.79 ms — commit
`6a6950a`, wall-clock bench. Scene is glass-heavy (bottles, pitcher,
marquee sign); RT refraction/reflection cost is representative of a
tough FNV interior. Frame is GPU-bound (fence=4.34 ms, 75% of wall).
Slight regression from `e6e8091` (192.8 FPS / 5.19 ms) — within
compositor-jitter range over 42 intervening commits; brd_ms unchanged
at 0.86, fence_ms unchanged at 4.34. Companion benches refreshed in
the same pass: Skyrim Whiterun 253.3 FPS @ 1932 entities, FO4 MedTek
92.5 FPS @ 7434 entities. See Repro commands.

---

## Status

**Rendering, today.** Interior cells load and render end-to-end from
unmodified Bethesda game data (Oblivion Anvil Heinrich Oaken Halls,
FNV Prospector Saloon, FO3 Megaton at 929 REFRs). Exterior renders
3×3 grids from FNV WastelandNV with landscape terrain (LAND
heightmap + LTEX/TXST splat). Skyrim SE loads individual meshes with
BSTriShape geometry. Single-mesh sweetroll ~3000-5000 FPS
(2026-04-22, RTX 4070 Ti @ 1280×720).

**RT lighting.** Full pipeline: SSBO multi-light, ray-query shadows
with streaming weighted reservoir sampling (8 reservoirs/fragment,
unbiased weight clamped at 64×), RT reflections + 1-bounce GI, SVGF
temporal denoiser with motion-vector reprojection and mesh-id
disocclusion, composite + ACES tone map, TAA with Halton(2,3) jitter
and YCoCg variance clamp. BLAS per-mesh with compaction + LRU
eviction, TLAS refit when layout unchanged. Pipeline cache threaded
through every create site with disk persistence (10–50 ms cold → <1
ms warm). SPIR-V reflection cross-checks descriptor layouts against
shader declarations at pipeline-create time.

**Parser coverage.** NIF parses across seven games (184 886 files
on the latest sweep — see compatibility matrix below). FO3 / FNV /
Skyrim SE land at 100% clean; Oblivion / FO4 / FO76 in the 95–97%
band (drift-induced truncation per #687 / #688); Starfield at 0.80%
clean (recent BA2 v3 LZ4 chunked content the parser doesn't yet
fully cover). Recoverable rate is 100% on all seven games except
Oblivion (99.99%, single hard-fail on a corrupt-by-design debug
marker — #698). ESM parses structured records across ~25 types on
FNV; 62 219 records on the latest sweep. Archive readers cover BSA
v103/v104/v105 and BA2 v1/v2/v3/v7/v8 (GNRL + DX10 with
reconstructed DDS headers, zlib + LZ4).

**Scripting, physics, UI.** Papyrus lexer + expression parser shipped
(Phase 1). Rapier3D physics bridge with dynamic capsule player
body. Ruffle/SWF UI overlay renders Skyrim SE menus. ECS-native
scripting (events + timers) exists; the Papyrus runtime consuming
1 257 parsed FO3 SCPT records is Tier 3 work.

**What doesn't work yet.** No skinned rendering (every NPC is in
bind pose, M29). No world streaming — cells load once and persist
(M40). Oblivion needs BSA v103 decompression before its cells
load. Weather transitions (fade between WTHR states) and cloud layers
2/3 closed in M33.1 (`2bfb622`).

### Compatibility matrix

Parse-rate columns measured 2026-04-26 against vanilla mesh archives
on commit 0681fc7 (`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`).
Clean = no NiUnknown placeholders + no truncation. Recoverable = file
parses end-to-end (counting NiUnknown / truncation as recoverable).
The audit-publish run #684–#688 / #697 / #698 tracks the open
parse-rate work for the games where clean < 100%.

| Game              | Archive       | NIF parse rate (clean / recoverable)         | Cells                                                    |
|-------------------|---------------|----------------------------------------------|----------------------------------------------------------|
| Oblivion          | BSA v103      | **95.21%** (7 647 / 8 032) · recover 99.99%  | Interior (Anvil Heinrich Oaken Halls). Exterior blocked on TES4 worldspace + LAND wiring (same shape as FO3 was). #687 / #688 / #698 track the open clean-rate gaps. |
| Fallout 3         | BSA v104      | 100% (10 989)                                | Interior (Megaton, 929 REFRs). Exterior wired; fresh GPU bench pending (R6a). |
| Fallout New Vegas | BSA v104      | 100% (14 881)                                | Interior (Prospector 1200 entities @ 172.6 FPS / 5.79 ms on RTX 4070 Ti, bench 6a6950a). Exterior 3×3. |
| Skyrim SE         | BSA v105 LZ4  | 100% (18 862)                                | Interior (WhiterunBanneredMare 1932 entities @ 253.3 FPS / 3.95 ms, bench 6a6950a; entity count up from 1258 since M32.5 close — more REFRs land now). |
| Fallout 4         | BA2 v1/v7/v8  | **96.46%** (33 757 / 34 995) · recover 100%  | Interior (MedTekResearch01 7434 entities @ 92.5 FPS / 10.82 ms, bench 6a6950a). FaceGen NIFs dominate the truncation tail (1 235 of 1 238 truncated files). |
| Fallout 76        | BA2 v1        | **97.34%** (56 915 / 58 469) · recover 100%  | —                                                        |
| Starfield         | BA2 v2/v3 LZ4 | **0.80%** (248 / 31 058) · recover 100%      | — Mesh archive expanded from 31 k → 320 k files; the 0.80% reflects new BA2 v3 LZ4 chunked content the parser doesn't fully cover yet. |

---

## Active Roadmap

Priority: **shortest path to a playable cell**, not shortest path to a
shinier frame. The renderer is mature (RT + RIS + SVGF + TAA + POM)
and the content pipeline parses recoverably across every target
(clean rates per the matrix above; tracked under #687 / #688 / #697
/ #698); next bottlenecks are *consumers* — things that make what we
parse actually do something on screen or at the speakers.

**Two axes.** Milestones (`M…`) ship user-visible capability.
Risk-reducers (`R…`) are structural fixes flagged in the 2026-04-22
architectural review — not new features, but prevention work to stop
known growth patterns from calcifying. Each R has a "why now" and
typically gates a specific milestone.

### Tier 1 — Playable exterior (blocks "you can walk around")

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                        | Depends on         |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------|
| PERF-1 | CPU frame-time audit           | ~~(1) Fix bench~~ done `e6e8091`. ~~(2) Profile CPU hotpath~~ done `b7deb4c` — **we are GPU-bound**: fence_wait=4.28 ms (76%) of 5.64 ms wall frame. brd=0.87 ms, ssbo=0.03 ms, tlas=0.02 ms. CPU work is not the bottleneck. (3) RT glass ray cost in `triangle.frag` is the real target — refraction+reflection on Prospector's bottle-heavy interior drives the GPU stall. See Tier 5 renderer polish. | —                  |
| ~~M33.1~~ | ~~Sky & atmosphere (follow-up)~~ | **Closed** `2bfb622`. Cloud layers 2/3 (ANAM/BNAM) sampled with parallax scroll. Weather fades over 8 s via `WeatherTransitionRes` + post-TOD-sample color blend. All 4 cloud layers active in exterior cells.                                                                                              | —                  |
| ~~M34~~ | ~~Exterior lighting~~         | **Closed.** Per-frame sun arc from game time in `weather_system`. TOD ambient + fog + directional from WTHR NAM0. Interior fill at 0.6× + `radius=-1` (unshadowed) in `render.rs`; `triangle.frag` line 1321 gates RT shadow on `radius >= 0`. All pieces were complete before this session.                                | —                  |
| ~~M32.5~~ | ~~Per-game cell loader parity~~ | **Closed.** Skyrim SE WhiterunBanneredMare 1258 entities @ 237 FPS. FO4 MedTekResearch01 7434 entities @ 90 FPS. No code changes — session 14 infrastructure was complete. Oblivion exterior still blocked on BSA v103 decompression.                                                                     | —                  |
| ~~R6a~~ | ~~Prospector re-bench~~       | **Closed.** 192.8 FPS / 5.19 ms at `e6e8091` with wall-clock bench. Scene is glass-heavy (RT refraction/reflection); representative tough-case FNV interior.                                                                                                                                                | —                  |

### Tier 2 — Actors visible & animated (blocks "cells are populated")

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                        | Depends on         |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------|
| ~~M29~~ | ~~Skinning chain verification~~ | **Closed.** End-to-end skinning chain (`SkinnedMesh` ECS → bone-palette → vertex shader) verified on FNV NiTriShape path via 7 integration tests in `byroredux/tests/skinning_e2e.rs` (4 FNV + 3 SSE). Bones populate, names round-trip, partition-local→global remap correct, palette responds to bone Transform mutations. CPU palette eval shipped; compute-shader dispatch deferred to M29.5 (gated on M41 producing measurable load). Defensive `MAX_TOTAL_BONES` overflow guard added (`render.rs:204`, `Once`-gated warn) so the silent truncation past 32 skinned meshes is no longer invisible. SSE BSTriShape per-vertex skin path filed as #638 (separate parser bug, not in M29 scope). | —                  |
| M29.5  | Compute-shader palette dispatch | Move CPU palette eval to a Vulkan compute pass (workgroup-per-skinned-mesh, DEVICE_LOCAL bone SSBO + COMPUTE→VERTEX barrier). Gated on M41 (workload exists; today's bench has 0 skinned meshes) and R1 (DrawCommand → material_id reduces sibling churn).                                                                    | M41, R1            |
| M41.0  | FaceGen heads render           | Spawn NPC entities with HDPT / EYES / HAIR meshes assembled into the NPC body. Parse already lands via #458 (misc stubs) + #440 (FaceGen NIF geometry fix). Skinning chain verified by M29.                                                                                                                                | M29, #458          |
| M41    | NPC spawning                   | Resolve NPC_ / CREA records → ECS entities with race/class/equipped armor + weapons. Spawn ACHR references from CELL REFRs. Movement is fly-by-waypoint until M42. SSE actors will hit #638 until that lands.                                                                                                              | M24, M29, M41.0    |
| M40    | World streaming                | Cell load/unload based on player position. Multi-cell exterior grid with async loading. BLAS streaming (evict/reload) ties into M31's LRU eviction.                                                                                                                                                                          | M32, M35           |
| ~~R6~~ | ~~Scratch-buffer instrumentation~~ | **Closed.** `ScratchTelemetry` resource refreshed per frame from `VulkanContext::fill_scratch_telemetry`, surfaced via the `ctx.scratch` console command. Reports per-Vec `len` / `capacity` / `bytes_used` / `wasted` for all 5 scratches (gpu_instances, batches, indirect_draws, terrain_tile, tlas_instances). On Prospector (1200 ent / 773 draws): 337 KB total, 320 B wasted — well right-sized; M40 cell transitions can now be diffed against this baseline. | —                  |

### Tier 3 — Scripting runtime (unblocks 1 257 FO3 SCPT records)

Hooks-first so terminals, doors, traps, lights, and activator
callbacks work before we try to boot the full Papyrus surface.

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                                                                                                      | Depends on      |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------|
| M47.0  | Event hooks runtime            | Bytecode-less ECS event handlers that respond to the canonical `OnActivate` / `OnHit` / `OnTriggerEnter` / `OnCellLoad` / `OnEquip` set. Reads the SCPT source text (M30 parser) when present and compiles to ECS systems at cell load; opaque SCDA bytecode is ignored. Terminals, doors, traps, lights in vanilla FO3 / FNV use this subset heavily.                                                      | M30, #443       |
| M47.1  | Condition eval                 | The ~300 condition function vocabulary (GetIsID, GetCurrentTime, GetQuestStage, GetFactionRank, …) evaluated against ECS state. Shared evaluator used by AI packages, perks, dialogue triggers, terminal branches.                                                                                                                                                                                         | M47.0           |
| **R5** | Papyrus quest prototype        | Before committing to the full "ECS-native, no VM" bet in M47.2, pick *one* real Skyrim quest with latent `Utility.Wait()`, a state change, and a cross-script callback. Transpile by hand. If the ECS shape holds up, proceed. If it fights you, fall back to Papyrus stack-VM semantics run *as an ECS system* — still a huge improvement over the original engine. **Why now:** de-risks M47.2 scope.      | M30, M47.0      |
| M47.2  | Full scripting runtime         | Papyrus transpiler (M30 AST → ECS components + systems), ESM-native 136-event dispatch, perk entry-point composition. Closes the loop for Skyrim+ Papyrus content. Shape determined by R5 outcome.                                                                                                                                                                                                         | R5, M30.2, M43  |

### Tier 4 — Audio & save/load (unblocks "it feels like a game")

| #     | Milestone   | Scope                                                                                                                                                                                                                                  | Depends on                                      |
|-------|-------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| M44   | Audio       | 3D spatial audio via `rodio` or `kira`. Footsteps from FOOT/IMPD. Ambient soundscapes from REGN. Music from MUSC / hardcoded. Basic crossfade + occlusion via raycast. No 5.1, no reverb zones initially.                              | —                                               |
| M45   | Save/Load   | Serialize world state (ECS components relevant to game-state + change forms). Simple serde-based snapshot format for v1 — full cosave compatibility is follow-up. Unblocks playtest iteration.                                         | M40 (world streaming dictates what to serialize) |

### Tier 5 — Renderer polish (quality, not capability)

Each of these buys 10–30% visual quality but no new feature. Keep
active for incremental wins; don't let them block Tier 1–4.

| #       | Milestone             | Scope                                                                                                                                                                                                                                                                                                                                                                                         | Depends on |
|---------|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| **R1**  | MaterialTable refactor | `DrawCommand` has ~40 fields and ~10 shader-variant payloads (skin_tint, hair_tint, multi_layer_*, eye_*, sparkle, terrain_tile_index, POM, glow/detail/gloss, UV transform, material_alpha, z_*). Most are **per-material, not per-draw**. Collapse material fields to a single `material_id: u32` indexing a per-frame material table. GpuInstance encodes from the table. **Why now:** every new material feature grows DrawCommand + GpuInstance + DrawBatch + sort key + 3 shaders in lockstep. **Blocks M38.** | —          |
| M35     | Terrain LOD            | Parse `.btr` terrain LOD meshes + `.bto` object LOD. Distance-based LOD selection. Gameplay-relevant half is world streaming (M40); pure LOD is quality.                                                                                                                                                                                                                                        | M32        |
| M37     | SVGF spatial filter    | A-trous wavelet filter using existing moments data. 3 iterations, edge-stopping on normal/depth/variance. 1-SPP → ~8-SPP visual quality on GI.                                                                                                                                                                                                                                                 | —          |
| M37.3   | ReSTIR-DI              | Full spatiotemporal reservoir reuse. Drops shadow rays to 1/pixel while sampling hundreds of lights. Streaming-RIS already shipped as M31.5.                                                                                                                                                                                                                                                    | M31.5, M37 |
| M38     | Transparency & water   | OIT or depth-peeled transparency. Water plane mesh with reflection/refraction. NIF alpha sort correctness.                                                                                                                                                                                                                                                                                      | R1         |
| M39     | Texture streaming      | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Memory budget with LRU eviction.                                                                                                                                                                                                                                                                              | —          |
| M29.3   | Pre-skinned raster path | Phase 3 of the GPU pre-skinning arc (`SkinComputePipeline` + per-skinned-entity BLAS refit shipped in `1ae235b`, RT shadows / reflections / GI now see this-frame skinned pose). Migrate `triangle.vert:147-204` to read pre-skinned vertices from the per-skinned-entity `SkinSlot` output buffer rather than doing inline weighted-bone-matrix-sum. The same commit must re-add `VERTEX_BUFFER` to the output buffer's usage mask — dropped in `#681` (`MEM-2-6`) so deferred-Phase-3 doesn't bloat memory-type masks today. Single source of truth, drops ~50 ALU ops per skinned vertex, but adds a critical-path dependency on the compute pass: a failed slot would now break raster too. **Defer-rationale:** the rasterized skinning path is well-understood and tested on real content; the new compute path is not. Ship only after the M41 NPC-spawning rollout proves the compute + BLAS-refit chain stable on visible animated content. | `1ae235b`, M41 stable, `#681` re-add |

### Tier 6 — Engine infrastructure (enablers)

| #       | Milestone                           | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Depends on     |
|---------|-------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------|
| ~~R7~~  | ~~Scheduler access declarations~~   | **Closed.** `Access` builder + `System::access()` opt-in declaration + `Scheduler::add_to_with_access` registration-side override + `access_report()` per-stage conflict analysis (`None` / `Conflict { pairs }` / `Unknown`). Snapshot stored as `SchedulerAccessReport` resource and surfaced via the `sys.accesses` console command. Current state on the engine binary: 12 systems registered, 3 declared (`fly_camera_system` / `spin_system` / `log_stats_system`), 9 undeclared, 0 known conflicts, 4 unknown pairs. M27 can now flip on with diagnosable contention; further system migrations driven by `sys.accesses` output. | —              |
| M27     | Parallel system dispatch            | Rayon-based parallel ECS system execution. TypeId-sorted lock acquisition already in place. Mostly pure optimisation — bumps frame budget for Tier 2–4 work. R7 (closed) gives `sys.accesses` for pre-flip contention analysis; remaining work is migrating undeclared systems to `add_to_with_access` so the conflict report has zero `Unknown` rows before flipping the `parallel-scheduler` feature on.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | R7             |
| M28.5   | Character controller                | Kinematic capsule with step-up, slope limiting, ground snapping. Replaces the dynamic-body fly camera for on-foot movement.                                                                                                                                                                                                                                                                                                                                                                                                                                                            | M28, M32       |
| **R2**  | ESM typed subrecord decoder         | `crates/plugin/src/esm/cell.rs` is 3217 lines — the biggest file in the repo — because sub-record dispatch is inlined in big walkers. Tier 3 adds QUST, DIAL, INFO, PERK, MGEF, SPEL, ENCH, AVIF, PACK, NAVM — a ~7× record-type surface growth. Extract a typed sub-record reader API (`read_sub::<Edid>(stream)?`, compile-time layouts). NIF's `NifStream` is already at that shape; ESM is not. **Why now:** doing the new records on the current shape is O(2K-line-file) edits; with a typed decoder it's O(new file). Prevention win, **not a rewrite**. **Blocks M24.2.**       | —              |
| M24.2   | ESM Phase 2                         | QUST / DIAL / INFO / PERK / MGEF / SPEL / ENCH / AVIF semantic parsing. Quest stages, dialogue trees, perk entry points, magic effects.                                                                                                                                                                                                                                                                                                                                                                                                                                                | R2             |
| M30.2   | Papyrus Phase 2–4                   | Statement parser, script declarations, FO4 extensions. Full `.psc` → AST for the entire Skyrim / FO4 corpus.                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | M30            |
| M46.0   | Multi-plugin CLI                    | Thread `parse_esm_with_load_order` (#445, landed) through `--esm` so the CLI can accept a load order. FormID remap is done; CLI surface is the missing piece.                                                                                                                                                                                                                                                                                                                                                                                                                          | #445 (done)    |
| ~~R3~~  | ~~NIF per-block-type parse histogram~~ | **Closed.** `nif_stats --tsv` emits a per-header-type `parsed` vs `NiUnknown` histogram; `crates/nif/tests/per_block_baselines.rs` integration test (opt-in via `cargo test -- --ignored`) compares against checked-in TSV baselines for all 7 games and fails on any `unknown` growth or `parsed` shrinkage. `BYROREDUX_REGEN_BASELINES=1` regenerates after intentional changes. Oblivion baseline refreshed 2026-04-26 to track the post-session-18 truncation drift surfaced by the audit (#687/#688/#697); the underlying drift sources stay open as separate issues (R3's job is to surface them, not fix them). Today the gate runs as a manual `cargo test … -- --ignored` invocation — there is no GitHub Actions pipeline yet, so "fail CI on regression" is the test's *contract* rather than an enforced workflow. | —              |

### Tier 7 — Deep gameplay systems (deferred until Tier 1–4 proves out)

| #       | Milestone                    | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Depends on                                      |
|---------|------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| M42     | AI packages                  | 30 composable procedures, package stack, Sandbox. Patrol paths from NAVM. Basic wander/follow/travel. PACK records need parsing first (#446).                                                                                                                                                                                                                                                                                                                                                         | M28.5, M41, #446                                |
| M43     | Quests & dialogue            | Quest stages, condition eval (~300 functions via M47.1), dialogue trees, Story Manager event triggers. Biggest single surface in the engine; ~50% of M24.2 Phase 2 feeds this.                                                                                                                                                                                                                                                                                                                        | M24.2, M41, M47.1                               |
| M46     | Full plugin loading          | Discover, sort, merge, resolve conflicts across the full load order. Builds on M46.0 (CLI wiring) + the existing `plugin/resolver.rs` DAG.                                                                                                                                                                                                                                                                                                                                                            | M24.2, M46.0                                    |
| **R4**  | SWF/GFx strategic decision   | M20 works for static SWF menus. M48 needs Scaleform GFx extensions (`_global.gfx`, text replacement, Papyrus callbacks, fonts, 34 menus). Ruffle has no GFx extension support and isn't pinned — it drags wgpu into an otherwise ash-only tree. Honest exits: (a) in-house AS2+GFx-subset interpreter (Papyrus-parser-adjacent patience), or (b) rebuild menus in egui/iced, treat Scaleform compat as out of scope. **Why now (decision, not implementation):** don't sleepwalk into a 3–6 month rabbit hole in Tier 7. Pick a direction so M48 has a plan, then defer until Tier 4 ships. | M20                                             |
| M48     | UI integration               | Papyrus ↔ UI bridge, input routing, menu callbacks. Shape determined by R4 decision.                                                                                                                                                                                                                                                                                                                                                                                                                  | R4, M20, M47.2                                  |

### Parking lot (nice-to-have, no active work)

| #       | Notes                                                                                                                                                                            |
|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| M37.6   | DLSS2. Proprietary, 4070 Ti target. Post-M37 TAA is already solid; DLSS is a later polish pass if it ever becomes relevant.                                                     |
| M25     | Vulkan Compute — partially realised (clustered lighting / SSAO / SVGF temporal are compute-backed). Remaining work folds into M29 (skinning) and M37 (spatial filter).          |
| Full cosave save/load | M45 v1 ships a simple snapshot. Byte-compatible cosave format (load original-engine save into Redux, or vice versa) is speculative and not a priority.                         |

---

## Architecture Decisions

### The keep list — what *not* to change despite temptation

These were re-examined in the 2026-04-22 architectural review and
deliberately kept. Document them here so they survive hype-driven
rewrite pressure.

| Decision                                     | Choice                                                         | Why kept                                                                                                                                                                                                              |
|----------------------------------------------|----------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| ECS storage model                            | Per-component `Component::Storage` (SparseSet or Packed)       | No archetype store. Cheap to maintain, easy to reason about, fine at Bethesda entity counts (~1 200 dense interior, a few thousand streamed exterior). Archetypes would be seductive and wrong at these query shapes. |
| Lock model                                   | Per-storage `RwLock`, TypeId-sorted acquisition, lock_tracker   | Query methods take `&self`; multi-component queries acquire in `TypeId` order; `#313` cross-thread ABBA graph + debug tracker catches deadlock pre-parallel. Mature. Keep.                                          |
| NIF block dispatch                           | `Box<dyn NiObject>` over 186 types                              | Enum dispatch would cost more in maintenance than it gains in perf at these branch counts. Keep.                                                                                                                       |
| NIF versioning                               | Raw `bsver()` checks inline, not trait dispatch via `NifVariant` | Per `#437` / `#160` / `#323`: byte-level versioning is genuinely per-game-per-version. A trait would lie. The semantic `NifVariant` flags are used where useful; raw bsver is used where versioning is byte-level. Keep. |
| Plugin identity                              | Content-addressed Form IDs                                      | Eliminates load-order dependency + slot limits. Best single architectural call in the project. Keep.                                                                                                                  |
| Coordinate system                            | Z-up→Y-up with CW angle negation                                | Documented in `docs/engine/coordinate-system.md`. Keep.                                                                                                                                                                |
| Rendering                                    | RT-first, rasterized fallback                                   | Scoped to RTX 4070 Ti target. Correct for this hardware. Keep.                                                                                                                                                         |
| Legacy compat                                | Parse data, don't emulate engine                                | Better results, clean room, no copyright issues. Keep.                                                                                                                                                                 |
| Scripting                                    | ECS-native (no VM)                                              | Eliminates Papyrus queue latency, stack serialization, orphaned stacks. Philosophically correct, but see R5 — prototype one representative quest before committing the full M47.2 shape.                             |

### Risk-reducers (R1–R7, new 2026-04-22)

Not new features — structural fixes to keep known growth patterns
from calcifying. Each is folded into the tier where it blocks, above.
Index:

- **R1** — MaterialTable refactor (collapse DrawCommand). Tier 5, blocks M38.
- **R2** — ESM typed subrecord decoder. Tier 6, blocks M24.2.
- **R3** — NIF per-block-type parse histogram (closed via `nif_stats --tsv` + `per_block_baselines.rs` + checked-in 7-game baselines). Tier 6, prevention.
- **R4** — SWF/GFx strategic decision. Tier 7, gates M48.
- **R5** — Papyrus quest prototype. Tier 3, gates M47.2.
- **R6** — Scratch-buffer instrumentation (closed via `ctx.scratch` + `ScratchTelemetry`). Tier 2 prevention, landed before M40. **R6a** — Prospector re-bench. Tier 1.
- **R7** — Scheduler access declarations (closed via `Access` builder + `sys.accesses` console command). Tier 6, M27 unblocked on tooling; remaining work is migrating the 9 still-undeclared systems.

### Growth discipline

The project's single biggest risk is **scope growth without
compression** (64K → ~94K LOC across the last three sessions). Tier
ordering gives top-level backpressure; apply it inside crates too. If
a single file crosses 3 500 lines, a struct crosses 50 fields, or a
context struct crosses 60 fields, treat it as a signal rather than a
stat to report — investigate before adding.

---

## Completed Milestones

One-liners grouped by area. Per-milestone scope is in `git log`;
session-level context is in [HISTORY.md](HISTORY.md).

**Graphics foundation**
M1 Vulkan init chain · M2 GPU geometry · M4 ECS-driven rendering ·
M7 depth buffer · M8 texturing · M13 directional lighting.

**ECS, plugins, coordinates**
M3 ECS foundation (World, Component, Storage, Query, Scheduler,
Resources, string interning) · M5 plugin system (stable Form IDs,
DAG resolver) · M6 legacy bridge (per-game parser stubs) ·
M17 coordinate system fix (CW rotation, SVD degenerate repair).

**NIF parser overhaul (N23 series)**
N23.1 trait hierarchy · N23.2 shader completeness ·
N23.3 Oblivion block types · N23.4 FO3/FNV validation ·
N23.5 skinning · N23.6 Havok collision skip + compressed mesh ·
N23.7 Fallout 4 · N23.8 particles · N23.9 FO76/Starfield ·
N23.10 test infrastructure. **Current: 186 registered type names,
156 parsed + 30 Havok skip.**

**Asset pipeline**
M9 NIF parser · M10 NIF→ECS import · M11 BSA reader ·
M14 DDS texture loading · M16 ESM parser & cell loading ·
M18 Skyrim SE NIF · M19 full cell loading · M26 BA2 archive
support (v1/v2/v3/v7/v8, zlib + LZ4). Per-game clean-parse rates
in the compat matrix above; recoverable rate at 100% across all
seven games except Oblivion's single hard-fail (#698).

**ESM records (M24 Phase 1)**
Items (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE), containers,
leveled lists (LVLI/LVLN), NPC_, RACE, CLAS, FACT, GLOB, GMST.
13 684 structured records on FNV.esm. SCPT pre-Papyrus bytecode
records parsed (#443). CREA + LVLC dispatched (#442/#448). PACK /
QUST / DIAL / MESG / PERK / SPEL / MGEF stubs (#446/#447).

**Animation**
M21 animation playback (.kf, linear/Hermite/TBC, 8 controller types,
blending stack) · KFM binary parser (Gamebryo 1.2.0.0 → 2.2.0.0) ·
BSAnimNote / BSAnimNotes with IK hints · skeletal skinning
end-to-end (#178) — `SkinnedMesh` ECS component, 4 096-slot bone
palette SSBO, unified vertex shader.

**RT renderer**
M22 RT-first multi-light (SSBO lights, ray query shadows, RT
reflections, 1-bounce GI, SVGF temporal, composite + ACES) ·
M31 RT performance at scale (batched BLAS, TLAS culling,
importance-sorted shadow budget, distance-based ray fallback, BLAS
LRU eviction, deferred SSBO rebuild) ·
M31.5 streaming RIS direct lighting ·
M32 landscape terrain (LAND + LTEX/TXST splatting) ·
M33 sky & atmosphere (sky gradient, sun disc with game-time arc, TOD
interpolation across 10 color groups × 6 TOD slots, dual cloud layers
DNAM + CNAM with parallax, horizon fog, procedural fallback; all WTHR
parser bugs M33-01–M33-06 fixed with regression tests) ·
M34 exterior lighting (full: per-frame sun arc, TOD ambient/fog/directional, interior fill split) ·
M32.5 per-game cell loader parity (Skyrim SE WhiterunBanneredMare 237 FPS, FO4 MedTekResearch01 90 FPS — zero code changes) ·
M33.1 cloud layers 2/3 ANAM/BNAM + weather fade transitions (8 s blend via WeatherTransitionRes) ·
PERF-1 bench fix (wall-clock frame counting + FrameTimings sub-phases; GPU-bound finding: fence=4.28ms/76%) ·
M36 BLAS compaction (20–50% memory reduction) ·
M37.5 TAA (Halton jitter, motion-vector reprojection, YCoCg clamp,
mesh-id disocclusion).

**Scripting, physics, UI**
M12 ECS-native scripting foundation (events + timers) ·
M28 Phase 1 physics (Rapier3D bridge) ·
M30 Phase 1 Papyrus parser (logos lexer + Pratt expression parser,
full AST) · M20 Scaleform/SWF UI via Ruffle.

**Debug & diagnostics**
M15 debug logging & diagnostics · debug CLI (`byro-dbg`) with
TCP protocol and Papyrus-expression query language ·
live ECS inspection (`find`, `entities(Component)`, screenshot).

---

## Known Issues

### Open — Tier 1 / 2 blockers

- [x] No sky, sun, clouds, or atmosphere — **closed (M33 + M33.1)**. Sky gradient, sun disc with game-time arc, TOD interpolation, 4-layer clouds (DNAM/CNAM/ANAM/BNAM with parallax scroll), fog, weather fade transitions (8 s blend), procedural fallback all working.
- [x] Bench measured GPU submit time only — **fixed** in `e6e8091`. Wall-clock bench now counts rendered frames; ticks_per_frame confirms ~1 on this compositor. 192.8 FPS / 5.19 ms at Prospector.
- [x] ~~No skinned mesh rendering — every NPC / creature is stuck in bind pose (M29)~~ — **closed**. Skinning chain verified end-to-end on FNV NiTriShape via 7 integration tests; CPU palette eval ships today, compute path deferred to M29.5. SSE BSTriShape per-vertex skin extraction is gap #638 (separate parser bug, fires only for SSE actors).
- [x] ~~RT shadows / reflections / GI see bind-pose only on skinned meshes~~ — **closed (M29 Phase 1.5+2)** in `1ae235b`. New `SkinComputePipeline` pre-skins vertices each frame; per-skinned-entity BLAS (keyed on `EntityId`, separate from the per-mesh `blas_entries` table) refits via `VK_BUILD_ACCELERATION_STRUCTURE_MODE_UPDATE_KHR` against the compute output. TLAS build relocated to after the skin chain so RT picks up this-frame's pose with zero lag. Phase 3 (raster reads pre-skinned vertices, dropping inline skinning math from `triangle.vert`) deferred to **M29.3** — gated on M41 NPC rollout proving the compute + BLAS-refit chain stable on visible content.
- [ ] NPCs + creatures don't spawn as ECS entities even when parsed (M41 / M41.0)
- [ ] No world streaming — entire cell re-imported from scratch on every load (M40)
- [ ] BSA v103 (Oblivion) decompression not working — blocks Oblivion exterior cell loading
- [x] Skyrim + FO4 cells not wired through `cell_loader` — **closed M32.5**, both render end-to-end

### Open — Tier 3 / 4 gaps

- [ ] 1 257 FO3 SCPT records parsed; no runtime executes them (M47.0)
- [ ] No audio subsystem of any kind (M44)
- [ ] No save/load — playtest iterations require cold cell re-load (M45)
- [ ] `PACK` (AI packages) records have stubs only — no evaluator (#446, M42)

### Open — Risk-reducers (2026-04-22)

- [ ] **R1** DrawCommand has ~40 fields + 10 shader-variant payloads — collapse to `material_id` indirection (blocks M38)
- [ ] **R2** ESM sub-record decoder is ad-hoc across 3 000+-line walkers — typed `read_sub::<T>` API (blocks M24.2)
- [x] **R3** NIF `NiUnknown` soft-fail masks per-block regressions — **closed**. `nif_stats --tsv` emits per-type `parsed` vs `unknown`; `crates/nif/tests/per_block_baselines.rs` (opt-in) compares against checked-in 7-game baselines and fails on any unknown growth or parsed shrinkage. Oblivion baseline refreshed 2026-04-26 against the audit-flagged truncation drift; `#687`/`#688`/`#697` track the underlying parser drift sources (R3 surfaces them, doesn't fix them).
- [ ] **R4** SWF/GFx strategic decision needed before M48 — Ruffle+GFx-stubs vs rewrite menus natively
- [ ] **R5** Papyrus full-runtime prototype on one real quest before M47.2 scope commitment
- [x] **R6** `VulkanContext` scratch buffers have no capacity telemetry — **closed**. `ctx.scratch` console command + `ScratchTelemetry` resource cover all 5 persistent scratches; per-frame refresh via `VulkanContext::fill_scratch_telemetry`. Prospector baseline: 337 KB total, 320 B wasted.
- [x] **R6a** Prospector re-bench — **closed**. 192.8 FPS / 5.19 ms at `e6e8091`, wall-clock bench.
- [x] **R6a-stale** Bench-of-record refreshed at `6a6950a` (2026-04-24). Prospector 172.6 FPS / 5.79 ms (was 192.8 / 5.19 — slight regression in compositor-jitter range; fence_ms unchanged at 4.34, GPU still the bottleneck). Skyrim Whiterun 253.3 FPS / 3.95 ms at 1932 entities (was 237 FPS at 1258 entities — entity count up 53% while FPS improved, indicating more REFRs land now without perf cost). FO4 MedTek 92.5 FPS / 10.82 ms (was 90, 7434 entities unchanged).
- [ ] **R6a-stale-4** Bench-of-record `6a6950a` is now 65 commits stale. Sessions 20 + 21 added M29 GPU pre-skinning, per-entity BLAS refit, AS scratch barrier, SVGF/TAA stage-mask widening, GI tMin tightening, reflection N_view flip, caustic flags + RT-enable gates — all sync / correctness work that doesn't move the Prospector / Whiterun / MedTek bench measurably without an actor-skinning workload. `#652` cluster_cull workgroup parallelisation could move populated-exterior cell-load timing, but the bench-of-record cells are interiors. Refresh still deferred until M41 lands actor spawning; not blocking.
- [x] **R7** Scheduler access declarations — **closed**. `Access` builder + `System::access()` opt-in + `Scheduler::add_to_with_access` for closures + `sys.accesses` console command surface a per-stage Conflict / Unknown report. 3 of 12 systems declared so far (fly_camera, spin, log_stats); 4 Unknown pairs remaining. M27 flip is diagnosable now; eliminating the Unknown rows is incremental migration work.

### Open — Misc

- [ ] `parry3d` panics on nested compound collision shapes (catch_unwind guard in place)
- [ ] `--esm` accepts only one plugin; `parse_esm_with_load_order` is wired but CLI isn't (M46.0)
- [ ] `BSBoneLODExtraData` has no parser — surfaced by R3 baselines: 0/34 on FO4, 0/52 on Skyrim SE, 0/56 on FO76 (no instances on the other four games). Single-fix candidate matching the Session 18 R3-driven pattern.
- [ ] `BSClothExtraData` 0/298 on Starfield — biggest single-game unparsed type in the R3 baselines. Cloth simulation extra data; out of scope for current rendering but blocks any future cloth animation work.
- [ ] One Starfield NIF (`meshes\marker_radius.nif`) requests a 318 MB single-buffer allocation at parse time, exceeding `byroredux_nif::stream::MAX_SINGLE_ALLOC_BYTES = 256 MB`. Per-allocation cap is a different trade-off from the BA2 chunk cap bumped in Session 18 — bumping this one weakens defence against attacker-controlled `u32` sizes inside individual NIF blocks. Tracked separately; one file out of 320 483 in the Starfield mesh archive.

---

## Project Stats

Ground-truth as of 2026-04-26, verified by `/session-close`.

| Metric                                  | Value                        |
|-----------------------------------------|------------------------------|
| Rust source lines (non-test)            | ~108 815                     |
| Rust total lines                        | ~111 862                     |
| Source files (non-test)                 | 208                          |
| Workspace members                       | 16                           |
| Tests (last reported by ROADMAP)        | 1273                         |
| Open issue directories                  | 665 (`.claude/issues/`)       |
| NIFs in per-game integration sweeps     | 184 886                       |
| Per-game NIF clean-parse rate           | 100% on FO3 / FNV / Skyrim SE; Oblivion 95.21%, FO4 96.46%, FO76 97.34%, Starfield 0.80% (see compat matrix). Recoverable 100% on all except Oblivion 99.99%. |
| Supported archive formats               | BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8 |

### Repro commands for every bench claim

| Claim                                                                     | Command                                                                                                                                                                                        |
|---------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Prospector Saloon 172.6 FPS / 5.79 ms (commit `6a6950a`, 2026-04-24, wall-clock bench) | `cargo run --release -- --esm "Fallout New Vegas/Data/FalloutNV.esm" --cell GSProspectorSaloonInterior --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa" --bench-frames 300` |
| Skyrim SE WhiterunBanneredMare 1932 entities @ 253.3 FPS / 3.95 ms (commit `6a6950a`, 2026-04-24) | `cargo run --release -- --esm "Skyrim Special Edition/Data/Skyrim.esm" --cell WhiterunBanneredMare --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" --textures-bsa "Skyrim - Textures0.bsa" --textures-bsa "Skyrim - Textures1.bsa" --textures-bsa "Skyrim - Textures2.bsa" --bench-frames 300` |
| FO4 MedTekResearch01 7434 entities @ 92.5 FPS / 10.82 ms (commit `6a6950a`, 2026-04-24) | `cargo run --release -- --esm "Fallout 4/Data/Fallout4.esm" --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2" --textures-bsa "Fallout4 - Textures1.ba2" --textures-bsa "Fallout4 - Textures2.ba2" --bench-frames 300` |
| Skyrim sweetroll single-mesh ~3000-5000 FPS (2026-04-22, RTX 4070 Ti @ 1280×720)        | `cargo run --release -- --bsa "Skyrim Special Edition/Data/Skyrim - Meshes0.bsa" --mesh meshes\\clutter\\ingredients\\sweetroll01.nif --textures-bsa "Skyrim Special Edition/Data/Skyrim - Textures3.bsa"` |
| Megaton interior parse-side 929 REFRs (2026-04-19)                        | `cargo test -p byroredux-plugin --release --test parse_real_esm parse_real_fo3_megaton_cell_baseline -- --ignored`                                                                             |
| Per-game full mesh sweep (clean rates above; recoverable 100% gate)       | `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`                                                                                                          |
| Full ESM record counts (FNV 62 219 / FO3 31 101)                          | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored`                                                                                                                   |

**Rule**: every "FPS / ms / count" claim in this document must have a
repro command in this table. `/session-close` refuses edits that add
a new claim without one.

---

## Reference Materials

| Resource                   | Location                                               | Purpose                                              |
|----------------------------|--------------------------------------------------------|------------------------------------------------------|
| nif.xml (niftools)         | `docs/legacy/nif.xml` (authoritative at `/mnt/data/src/reference/nifxml/nif.xml`) | NIF format spec (8 563 lines)                        |
| Gamebryo 2.3 source        | External drive                                         | Byte-exact serialization reference                   |
| FNV / FO3 / SkyrimSE data  | Steam library (env var overrides, see README.md)       | Primary test content                                 |
| Creation Kit wiki          | uesp.net                                               | Record type documentation                            |
| Coordinate system docs     | `docs/engine/coordinate-system.md`                     | Transform pipeline, CW convention, winding chain     |

---

## Crate Map

| Crate                         | Focus                                                                                                           |
|-------------------------------|-----------------------------------------------------------------------------------------------------------------|
| `byroredux-core`              | ECS, math, animation engine, string interning, Form IDs                                                         |
| `byroredux-renderer`          | Vulkan + RT (ash, gpu-allocator, acceleration manager, pipelines, SVGF, TAA, composite, caustic, SSAO)          |
| `byroredux-platform`          | winit, raw handles                                                                                              |
| `byroredux-plugin`            | Plugin manifests, DAG resolver, ESM/ESP/ESL parser, cell loader helpers                                         |
| `byroredux-nif`               | NIF binary parser (~186 block types), import-to-ECS, animation import                                           |
| `byroredux-bsa`               | BSA (v103/v104/v105) + BA2 (v1/v2/v3/v7/v8, GNRL + DX10) readers                                                 |
| `byroredux-physics`           | Rapier3D bridge (M28 Phase 1)                                                                                    |
| `byroredux-scripting`         | ECS-native events + timers                                                                                       |
| `byroredux-papyrus`           | Papyrus `.psc` parser (lexer + Pratt expression parser + full AST)                                               |
| `byroredux-ui`                | Scaleform/SWF via Ruffle                                                                                         |
| `byroredux-debug-protocol`    | Wire types + component registry for debug CLI                                                                    |
| `byroredux-debug-server`      | TCP debug server (Late-stage exclusive system)                                                                   |
| `byroredux-cxx-bridge`        | C++ interop via cxx                                                                                              |
| `byroredux` (binary)          | Game loop, cell loader, fly camera, animation system, render data collection                                     |
| `tools/byro-dbg`              | Standalone debug CLI (TCP client, REPL)                                                                          |
