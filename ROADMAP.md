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

**Last verified**: 2026-05-08 (post-Session 32, FNV-D5 + Renderer-D11
audit closeouts — `skin_compute` pool 32 → 64 + retry-suppression cache,
shader-only TAA + SVGF NaN/Inf history guards + 15-bit mesh_id mask;
M41-EQUIP Phase 2 close-out — LVLI dispatch in NPC outfit + inventory
walks via `expand_leveled_form_id`, `--bench-hold` CLI flag, smoke-test
framework with hard / soft assertions, `Inventory` / `EquipmentSlots`
debug-server registration; standing-queue closures — #337
`NiStencilProperty` capture, #720 `BSEyeCenterExtraData` dispatch, #873
BSGeometry bulk reads, #848 `footstep_system` Stage::PostUpdate, #891
`NiTextureEffect` Phase 1 import — see [HISTORY.md](HISTORY.md)).
**Bench-of-record**: Prospector Saloon 133.5 FPS / 7.49 ms @ 2562
entities — commit `220e8e1`, 2026-05-11, wall-clock bench, 300
frames, RTX 4070 Ti @ 1280×720. Scene is glass-heavy (bottles,
pitcher, marquee sign); RT refraction/reflection cost is
representative of a tough FNV interior. Frame is GPU-bound
(fence=5.81 ms, 78% of wall; brd=0.97 ms; draw=6.21 ms with
tlas=0.19 / ssbo=0.03 / cmd=0.04 / submit=0.09). Refresh from
`6a6950a` (172.6 FPS / 5.79 ms @ 1200 entities, 2026-04-24):
**entity count more than doubled (+114%) while wall_ms grew +29%**
— sub-linear scaling, consistent with RT cost amortising across
the BLAS hierarchy as more REFRs land in the cell. Two M41-EQUIP
re-attributions help close the gap: the Phase 2 scaffold (`#896`
A.0 → B.2) drove most of the entity inflation by spawning NPC
inventory roots; the REFR Euler→Y-up composition fix landed every
REFR through a corrected `Rx · Ry · Rz` order (was `Rz · Ry · Rx`).
Companion benches refreshed in the same pass: Skyrim Whiterun
217.3 FPS / 4.60 ms @ 3209 entities (was 253.3 @ 1932 entities —
+66% entities, -14% FPS, sub-linear), FO4 MedTek 68.5 FPS / 14.61
ms @ 10 809 entities (was 92.5 @ 7434 — +45% entities, -26% FPS).
See Repro commands.

---

## Status

**Rendering, today.** Interior cells load and render end-to-end from
unmodified Bethesda game data (Oblivion Anvil Heinrich Oaken Halls,
FNV Prospector Saloon, FO3 Megaton at 929 REFRs). Exterior renders
7×7 (radius 3, default) grids from FNV WastelandNV with landscape terrain (LAND
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
shader declarations at pipeline-create time. **R1 (2026-05-01)**:
per-material data deduplicated into a `MaterialBuffer` SSBO indexed
by `material_id`; `GpuInstance` collapsed 400 → 112 B (72%
reduction); future shading variants land in `GpuMaterial` only,
no longer lockstep across 4 shaders + DrawCommand + GpuInstance.

**Parser coverage.** NIF parses across seven games (184 886 files
on the latest sweep — see compatibility matrix below). FO3 / FNV /
Skyrim SE land at 100% clean; Oblivion / FO4 / FO76 in the 95–97%
band (drift-induced truncation per #687 / #688); Starfield at 97.19%
clean (recent BA2 v3 LZ4 chunked content the parser doesn't yet
fully cover). Recoverable rate is 100% on all seven games except
Oblivion (99.99%, single hard-fail on a corrupt-by-design debug
marker — #698). ESM parses structured records across ~25 types on
FNV; 73 054 structured records on the latest sweep, plus a 5 625-
record long-tail bucket (sounds / idle / grasses / debris). Archive
readers cover BSA v103/v104/v105 and BA2 v1/v2/v3/v7/v8 (GNRL + DX10
with reconstructed DDS headers, zlib + LZ4).

**Scripting, physics, UI.** Papyrus lexer + expression parser shipped
(Phase 1). Rapier3D physics bridge with dynamic capsule player
body. Ruffle/SWF UI overlay renders Skyrim SE menus. ECS-native
scripting (events + timers) exists; the Papyrus runtime consuming
1 257 parsed FO3 SCPT records is Tier 3 work.

**What doesn't work yet.** No skinned rendering (every NPC is in
bind pose, M29). No world streaming — cells load once and persist
(M40). Oblivion exterior needs TES4 worldspace + LAND wiring
(same shape FO3 was in pre-cell-loader era — the long-running "BSA
v103 decompression" framing is a stale premise refuted by the
2026-04-17 + 2026-04-25 sweeps; v103 extracts 147 629 / 147 629
vanilla files end-to-end, see #699). Weather transitions (fade
between WTHR states) and cloud layers 2/3 closed in M33.1
(`2bfb622`).

**Per-fragment normal mapping (2026-05-02).** Re-enabled and shipped:
**M-NORMALS** ([#783](https://github.com/matiaszanolli/ByroRedux/issues/783),
commits 91e9011 + 82a4563) parses Bethesda's
`NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` blob
when present and falls back to a Rust port of nifly's
`NiTriShapeData::CalcTangentSpace` per-triangle accumulator
(`crates/nif/src/import/mesh.rs::synthesize_tangents`) for FO3 / FNV /
Oblivion content that ships without authored tangents. Vertex stride
84 → 100 B (`tangent: [f32; 4]` at offset 84, attribute location 8 /
RGBA32_SFLOAT); `triangle.vert/frag`, `ui.vert`, and
`skin_vertices.comp` updated in lockstep. **LIGHT-N2**
([#784](https://github.com/matiaszanolli/ByroRedux/issues/784),
commit 18bbeae) moves the composite fog mix from HDR-linear pre-ACES
to display space post-ACES, removing the residual interior yellow
distance wash. Both ship; the renderer is at the doorstep of
Oblivion-class interior fidelity for properly-textured cells.

**The "chrome posterized walls" diagnosis was a red herring.** Three
sessions converged on screen-space-TBN discontinuity as the cause;
Session 27 (2026-05-02) found the actual bug. `BYROREDUX_RENDER_DEBUG=0x10`
(`DBG_BYPASS_NORMAL_MAP`, added in commit b2354a4) skips `perturbNormal`
entirely — bypass and baseline screenshots came out pixel-identical at
the same camera position. `byro-dbg`'s `tex.missing` reported 39
unique missing textures × 263 entities for FNV `GSDocMitchellHouse`
(walls, floor, trim — `nvcraftsmanhomes_interiorwall01.dds` and
friends). The "chrome" was the magenta-checker placeholder
compositing with the (correctly loaded) tangent-space normal map.
Root cause: FNV ships its base textures across `Fallout - Textures.bsa`
**and** `Fallout - Textures2.bsa`; only the former was loaded.
Fixed by `open_with_numeric_siblings` in `byroredux/src/asset_provider.rs`
(commit b2354a4): when `--bsa` / `--textures-bsa` points at an
unsuffixed `.bsa` / `.ba2`, the loader now also opens
`<stem>2.bsa` … `<stem>9.bsa` siblings on disk. Inert for Skyrim's
already-numeric `Skyrim - Meshes0.bsa` style. With the helper in
place `tex.missing` drops 39 → 1 (the remainder is `<no path,
no material>` placeholder geometry, legitimate). New diagnostic
order: when an artifact reads as "chrome / posterized", run
`tex.missing` *before* opening shader files. The full triage is in
[docs/engine/debug-cli.md](docs/engine/debug-cli.md) under
"Fragment-shader bypass / viz bits"; the session narrative is in
[HISTORY.md](HISTORY.md).

### Compatibility matrix

Parse-rate columns measured 2026-04-26 against vanilla mesh archives
on commit 0681fc7 (`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`).
Clean = no NiUnknown placeholders + no truncation. Recoverable = file
parses end-to-end (counting NiUnknown / truncation as recoverable).
The audit-publish run #684–#688 / #697 / #698 tracks the open
parse-rate work for the games where clean < 100%.

| Game              | Archive       | NIF parse rate (clean / recoverable)         | Cells                                                    |
|-------------------|---------------|----------------------------------------------|----------------------------------------------------------|
| Oblivion          | BSA v103      | **96.24%** (7 730 / 8 032) · recover 99.99%  | Interior (Anvil Heinrich Oaken Halls). Exterior blocked on TES4 worldspace + LAND wiring (same shape as FO3 was). `#687` closed via two perpetrator-parser fixes (NiGeomMorpherController trailing bsver-gated u32 + NiControllerSequence Phase field for v=10.2.0.0); 83 truncations recovered. `#688` / `#698` track the remaining clean-rate gap. |
| Fallout 3         | BSA v104      | 100% (10 989)                                | Interior (Megaton, 929 REFRs). Exterior wired; fresh GPU bench pending (R6a). |
| Fallout New Vegas | BSA v104      | 100% (14 881)                                | Interior (Prospector 2562 entities @ 133.5 FPS / 7.49 ms on RTX 4070 Ti, bench `220e8e1` 2026-05-11). Exterior 7×7 (radius 3). |
| Skyrim SE         | BSA v105 LZ4  | 100% (18 862)                                | Interior (WhiterunBanneredMare 3209 entities @ 217.3 FPS / 4.60 ms, bench `220e8e1` 2026-05-11; entity count up from 1258 since M32.5 close — M41 NPC scaffold + every REFR through the corrected Euler→Y-up composition land more refs now). |
| Fallout 4         | BA2 v1/v7/v8  | **96.46%** (33 757 / 34 995) · recover 100%  | Interior (MedTekResearch01 10 809 entities @ 68.5 FPS / 14.61 ms, bench `220e8e1` 2026-05-11). FaceGen NIFs dominate the truncation tail (1 235 of 1 238 truncated files). |
| Fallout 76        | BA2 v1        | **97.34%** (56 915 / 58 469) · recover 100%  | —                                                        |
| Starfield         | BA2 v2/v3 LZ4 | **98.6%** aggregate · recover 100% (all 5 archives, 2026-04-27 post-#754) | Per-archive: Meshes01 97.21% (31 058 NIFs), Meshes02 **100%** (7 552; was 0% pre-#754 BSWeakReferenceNode), MeshesPatch 98.11% (29 849; was 74% pre-#754), LODMeshes 99.92% (19 535), FaceMeshes **100%** (1 282). Truncation tail in Meshes01/MeshesPatch is residual drift (#746/#747). |

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

### Priority review — 2026-05-03

A direction reset to keep work pointed at *capability* rather than
recursive renderer polish. Sessions 25–28 closed 70+ commits chasing
interior fidelity (Frostbite falloff, env_map_scale, depth-bias
ladder, perturbNormal) — real wins, but bench-of-record is now 266
commits stale because the visible-actor workload that would justify
re-running it (M41) hasn't shipped. 16 distinct renderer audits in
the last 30 days; 28 of 54 open issues are renderer-tagged; **0
issues open at HIGH or CRITICAL severity**. The renderer has reached
diminishing returns until new content classes (NPCs, audio, multi-
cell exterior) exercise the existing surface differently.

Three concrete adjustments:

1. **Renderer-audit moratorium**: pause new full-renderer audits
   until a visible regression is reported on real content *or* M41
   produces a refreshed bench-of-record. The 49→51-issue backlog
   from prior audits is the working set; close from it, don't grow
   it. `/audit-renderer` runs only on user request, not as part of
   session cadence. **The gate to re-open the audit cycle is M41
   landing visible NPCs in a cell** — that's the workload that
   would surface anything genuinely worth auditing.
2. **Audio promoted to Tier 2** (was Tier 4). M44 depends on nothing
   shipped or unshipped, takes 1–2 weeks, and is the single biggest
   "feels like a game" gap. Footsteps + ambient + music + spatial
   raycast occlusion lands in parallel with M41/M40 and converts
   "we render Bethesda content" into "we run Bethesda content."
3. **R5 (Papyrus quest prototype) ahead of M47.0** in Tier 3. The
   ECS-native-scripting bet is the single biggest architectural
   risk we haven't validated against real content. One transpiled
   Skyrim quest with `Utility.Wait()` + state change + cross-script
   callback tells us whether M47.0/M47.2 are 3 weeks or 3 months.
   Currently M47.0 is sequenced first, which would commit hook
   shape before the bet is de-risked.

**The "better, not clone" trade-off.** When in doubt during this
phase, prefer the axis where ByroRedux can credibly *improve* on
Bethesda — proper async streaming, parallel ECS, structured save
state, 3D positional audio with reverb zones, native UI — over
chasing per-pixel reference parity with a 2008–2015 forward
renderer's interior look.

### Tier 1 — Playable exterior (blocks "you can walk around")

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                        | Depends on         |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------|
| PERF-1 | CPU frame-time audit           | ~~(1) Fix bench~~ done `e6e8091`. ~~(2) Profile CPU hotpath~~ done `b7deb4c` — **we are GPU-bound**: fence_wait=4.28 ms (76%) of 5.64 ms wall frame. brd=0.87 ms, ssbo=0.03 ms, tlas=0.02 ms. CPU work is not the bottleneck. (3) RT glass ray cost in `triangle.frag` is the real target — refraction+reflection on Prospector's bottle-heavy interior drives the GPU stall. See Tier 5 renderer polish. | —                  |
| ~~M33.1~~ | ~~Sky & atmosphere (follow-up)~~ | **Closed** `2bfb622`. Cloud layers 2/3 (ANAM/BNAM) sampled with parallax scroll. Weather fades over 8 s via `WeatherTransitionRes` + post-TOD-sample color blend. All 4 cloud layers active in exterior cells.                                                                                              | —                  |
| ~~M34~~ | ~~Exterior lighting~~         | **Closed.** Per-frame sun arc from game time in `weather_system`. TOD ambient + fog + directional from WTHR NAM0. Interior fill at 0.6× + `radius=-1` (unshadowed) in `render.rs`; `triangle.frag` line 1321 gates RT shadow on `radius >= 0`. All pieces were complete before this session.                                | —                  |
| ~~M32.5~~ | ~~Per-game cell loader parity~~ | **Closed.** Skyrim SE WhiterunBanneredMare 1258 entities @ 237 FPS. FO4 MedTekResearch01 7434 entities @ 90 FPS. No code changes — session 14 infrastructure was complete. Oblivion exterior gated on TES4 worldspace + LAND wiring (same shape FO3 was — *not* BSA v103 decompression; that was a stale framing closed via #699).                                                                     | —                  |
| ~~R6a~~ | ~~Prospector re-bench~~       | **Closed.** 192.8 FPS / 5.19 ms at `e6e8091` with wall-clock bench. Scene is glass-heavy (RT refraction/reflection); representative tough-case FNV interior.                                                                                                                                                | —                  |

### Tier 2 — Actors visible & animated (blocks "cells are populated")

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                        | Depends on         |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------|
| ~~M29~~ | ~~Skinning chain verification~~ | **Closed.** End-to-end skinning chain (`SkinnedMesh` ECS → bone-palette → vertex shader) verified on FNV NiTriShape path via 7 integration tests in `byroredux/tests/skinning_e2e.rs` (4 FNV + 3 SSE). Bones populate, names round-trip, partition-local→global remap correct, palette responds to bone Transform mutations. CPU palette eval shipped; compute-shader dispatch deferred to M29.5 (gated on M41 producing measurable load). Defensive `MAX_TOTAL_BONES` overflow guard added (`render.rs:204`, `Once`-gated warn) so the silent truncation past 32 skinned meshes is no longer invisible. SSE BSTriShape per-vertex skin path filed as #638 (separate parser bug, not in M29 scope). | —                  |
| M29.5  | Compute-shader palette dispatch | Move CPU palette eval to a Vulkan compute pass (workgroup-per-skinned-mesh, DEVICE_LOCAL bone SSBO + COMPUTE→VERTEX barrier). Gated on M41 (workload exists; today's bench has 0 skinned meshes) and R1 (DrawCommand → material_id reduces sibling churn).                                                                    | M41, R1            |
| ~~M41.0~~ | ~~FaceGen heads render~~      | **Closed (2026-05-05).** Phases 0–4 shipped in Session 24; #772 closed via FLT_MAX-pose gate; #794 closed via three-layer regression suite (parser diagnostic + 4 synthetic e2e + 1 real-data e2e in `crates/nif/tests/mtidle_motion_diagnostic.rs` and `byroredux/src/systems.rs::animation_system_e2e_tests`). Real FNV `mtidle.kf` → `animation_system` produces a 1.49 component-wise rotation delta on `Bip01 Spine` after 4 ticks — animation pipeline is healthy end-to-end. The remaining "rigid NPC" symptom is the **already-known Phase 1b.x body-skinning artifact** (`npc_spawn.rs:402-431`: long-spike vertex artifact, `0 unresolved` bones, palette composition bug); not in the animation chain. M41.0.5 (GPU per-vertex morph runtime) + M41.x (Havok `.hkx` stub) deferred to Tier 5. | M29, #458          |
| M41    | NPC spawning (Phase 1 + Phase 2 closed) | **Phase 1 closed (2026-05-07).** FNV `GSDocMitchellHouse` renders Doc Mitchell as a coherent T-pose humanoid at the REFR position — skeleton + body + hands + head + FaceGen morphs all compose without artifact. The closure-bar workload defined 2026-05-03 ("at least one Skyrim/FO4/FNV cell renders NPCs visible at REFR positions, even in T-pose") is met. **#841 closes** — the long-spike body-skinning regression no longer reproduces on the canonical FNV repro; root cause appears resolved as a side-effect of #771 (palette ground truth pin) + subsequent animation fixes since `b386eb3`. The runtime `skin <id>` debug command (`InspectSkinnedMesh` extended with per-bone resolved `GlobalTransform` + computed palette + identity-dropout flagging) lands as the regression guard so any future resurgence localizes in one round-trip. **Phase 2 scaffold shipped (2026-05-07/08, #896 Phases A.0 → B.2):** `Inventory` + `EquipmentSlots` ECS components + `ItemInstancePool` resource land in `0a0d652`; `f1b3156` walks NPC inventory and spawns ARMO meshes (concurrent body+armor as deliberate spike); `21ae560` pre-scans for body-slot armor and skips `upperbody.nif` when present (kills z-fight + 2× bone-palette overhead); `121c705` / `24a7bd8` / `4ec9bb6` build the per-game `resolve_armor_mesh` helper (Skyrim+ ARMO → ARMA → worn-mesh chain) and wire it into both spawn paths. **Phase 2 close-out advanced (Session 32, 2026-05-08):** LVLI dispatch landed in `be4663b` via `byroredux_plugin::equip::expand_leveled_form_id` — both `OTFT.items` outfit walks (prebaked path) and `npc.inventory` CNTO entries (kf-era path) flatten leveled-list refs into base ARMO/WEAP form IDs gated on `actor_level`. Pre-fix vanilla Skyrim+ NPCs whose default outfits referenced LVLI silently spawned with no gear (the loop's `index.items.get(&form_id)` failed silently). Single-pick (highest-level eligible) is the Bethesda flag-bit-0-unset default; multi-pick lands all eligible. 8 new resolver tests cover passthrough / level gating / multi-pick / nested recursion / circular cap / unknown id. Plus `--bench-hold` CLI flag (`73adffb`) keeps the engine open after `--bench-frames` so `byro-dbg` can attach against the loaded scene; `Inventory` + `EquipmentSlots` registered with the debug-server (`9b957bb`) so `byro-dbg`'s `entities <Component>` lights them up; runnable smoke at `docs/smoke-tests/m41-equip.sh` (`9b957bb` + `085321d` + `3422884`) with hard / soft pass-fail assertions parsing the bench summary line + byro-dbg output. **First smoke run on FO4 MedTekResearch01 at 10 809 entities / 57.9 FPS** — the engine + LVLI dispatch produce real geometry; visual A/B remains. **Phase 2 closed (2026-05-11).** Smoke gate green on both targets after hoisting `build_npc_equip_state` above skeleton load in `spawn_prebaked_npc_entity`: SSE WhiterunBanneredMare shows 6 named NPCs with `Inventory` + `EquipmentSlots`, `tex.missing=0`, 3209 entities / 2052 draws / ~84 FPS (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda — equipped via OTFT.items + LVLI dispatch). FO4 MedTekResearch01 surfaces 23 NPCs with `Inventory` + `EquipmentSlots` (LvlFeralGhoul / LvlTurretBubble / LvlFeralGhoulAmbush / Loot_CorpseFeralGhoul01 leveled-creature templates; no humanoid named NPCs at cell-load on this dungeon — that's a cell-content property, not an equip-pipeline gap), 10 809 entities / 8162 draws / ~49 FPS. **The equip pipeline is now observable independent of mesh-load success** — even when the FO4 humanoid 3rd-person skeleton.nif is absent (vanilla ships only `_1stperson\skeleton.nif` + `.hkx` on the character path; the SSE-shaped `character assets\skeleton.nif` is not in any Fallout4 BA2, verified by BA2 scan of Meshes / MeshesExtra / Animations / Misc / Startup), the per-NPC `Inventory` + `EquipmentSlots` components still land on the placement root via the early-build hoist. Follow-ups left open: (1) FO4 humanoid skeleton resolution needs a Havok `.hkx` loader or `_1stperson` placeholder before armor *meshes* materialize on FO4 actors (M41.x Havok stub); (2) `spawn_prebaked_npc_entity` returns `Some(placement_root)` on every early-return path, inflating the cell-loader's "NPCs spawned" count to include mesh-less husks; (3) kf-era `spawn_npc_entity` could use the same equip-build hoist for symmetry (FNV/FO3 work today because their skeleton.nif resolves; not urgent). **Renderer-audit moratorium gate cleared** — bench-of-record refresh (R6a-stale-7 / #902), GPU skinning compute (M29.5), and skinned BLAS coverage are now the next steady-state moves. | M24, M29, M41.0    |
| M40    | World streaming                | **Phase 1a/1b shipped** in Session 23 — `streaming` module with diff logic (`cdfef07`), `WorldStreamingState` wired into App (`80e2966`), async cell-pre-parse worker thread (`592e7bf`), shutdown drain (`7dc354a`). Single-cell-at-a-time today. Remaining: multi-cell exterior grid + BLAS streaming (evict/reload) ties into M31's LRU eviction. | M32, M35           |
| **M44** | Audio (3D spatial)            | **Phases 1–6 shipped (2026-05-05).** Foundation: `byroredux-audio` crate on [`kira`](https://crates.io/crates/kira) `0.10` — `AudioWorld` + ECS components (`AudioListener` / `AudioEmitter` / `OneShotSound`); BSA decode via `StaticSoundData::from_cursor` + `SoundCache`; `audio_system` spatial sub-track model with lazy listener creation, per-emitter `SpatialTrackHandle`, prune-on-Stopped. Phase 3.5: `play_oneshot` queue API + `FootstepEmitter` + `footstep_system` (XZ-plane stride accumulator, vertical motion excluded). `--sounds-bsa <path>` decodes canonical FNV dirt-walk WAV. **Phase 4**: `AudioEmitter.looping = true` applies kira's `StaticSoundData::loop_region(..)`; the prune sweep notices when a looping sound's source entity has lost its `AudioEmitter` (despawn-by-cell-unload, or explicit removal) and issues a tweened `stop()`. **Phase 5**: `load_streaming_sound_from_bytes` / `_from_file` for multi-minute music via kira's `StreamingSoundData`; `AudioWorld::play_music` (single-slot, non-spatial, crossfade) + `stop_music` + `is_music_active`. **Phase 6**: global reverb send. On manager init, the audio crate creates a kira `SendTrackBuilder.with_effect(ReverbBuilder)` at full wet; spatial sub-tracks opt in via `with_send(reverb.id(), reverb_send_db)`. `AudioWorld::set_reverb_send_db` toggles per cell type (`f32::NEG_INFINITY` = silent default, `-12 dB` = subtle interior, `0 dB` = full wet). Already-playing sounds keep their construction-time send level. **Tests**: 12 default + 5 `#[ignore]`d real-data integrations on cpal (BSA decode, full lifecycle, queue-driven lifecycle, looping survives natural duration + stops on AudioEmitter remove, streaming music play/stop on real OGG). Workspace 1680 / 1680 passing. **Phases 3.5b + REGN-driven ambient pending**: FOOT records → per-material lookup (drops dirt hardcode); REGN region-keyed ambient layers; raycast-occlusion attenuation. **Cell-load reverb-toggle wiring closed 2026-05-08 (#846)** — `cell_loader::dispatch_reverb_for_cell_type` flips `set_reverb_send_db` to `-12 dB` on interior cells, `f32::NEG_INFINITY` (silent) on exterior. | —                  |
| ~~R6~~ | ~~Scratch-buffer instrumentation~~ | **Closed.** `ScratchTelemetry` resource refreshed per frame from `VulkanContext::fill_scratch_telemetry`, surfaced via the `ctx.scratch` console command. Reports per-Vec `len` / `capacity` / `bytes_used` / `wasted` for all 5 scratches (gpu_instances, batches, indirect_draws, terrain_tile, tlas_instances). On Prospector (1200 ent / 773 draws): 337 KB total, 320 B wasted — well right-sized; M40 cell transitions can now be diffed against this baseline. | —                  |

### Tier 3 — Scripting runtime (unblocks 1 257 FO3 SCPT records)

**Reordered 2026-05-03**: R5 now comes first. Hooks-first sequencing
risks committing M47.0's event-hook shape before validating the
ECS-native-no-VM bet, then having to rework hooks if R5 falls back
to "Papyrus stack-VM as an ECS system." De-risk first.

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                                                                                                      | Depends on      |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------|
| **R5** | Papyrus quest prototype        | **Sequenced first (2026-05-03 priority review).** Before committing to the full "ECS-native, no VM" bet in M47.2 *or* the M47.0 hook shape that will be consumed by it, pick *one* real Skyrim quest with latent `Utility.Wait()`, a state change, and a cross-script callback. Transpile by hand. If the ECS shape holds up, proceed. If it fights you, fall back to Papyrus stack-VM semantics run *as an ECS system* — still a huge improvement over the original engine. **Why now:** de-risks the entire M47 surface (hook shape + transpiler scope) for the cost of one quest. The original Tier-3 sequencing (M47.0 first) commits to a hook contract before the bet is validated. | M30             |
| M47.0  | Event hooks runtime            | Bytecode-less ECS event handlers that respond to the canonical `OnActivate` / `OnHit` / `OnTriggerEnter` / `OnCellLoad` / `OnEquip` set. Reads the SCPT source text (M30 parser) when present and compiles to ECS systems at cell load; opaque SCDA bytecode is ignored. Terminals, doors, traps, lights in vanilla FO3 / FNV use this subset heavily. **Hook shape locked by R5 outcome.**                  | R5, #443        |
| M47.1  | Condition eval                 | The ~300 condition function vocabulary (GetIsID, GetCurrentTime, GetQuestStage, GetFactionRank, …) evaluated against ECS state. Shared evaluator used by AI packages, perks, dialogue triggers, terminal branches.                                                                                                                                                                                         | M47.0           |
| M47.2  | Full scripting runtime         | Papyrus transpiler (M30 AST → ECS components + systems), ESM-native 136-event dispatch, perk entry-point composition. Closes the loop for Skyrim+ Papyrus content. Shape determined by R5 outcome.                                                                                                                                                                                                         | R5, M30.2, M43  |

### Tier 4 — Save/load (unblocks "it feels like a game")

| #     | Milestone   | Scope                                                                                                                                                                                                                                  | Depends on                                      |
|-------|-------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| ~~M44~~ | Audio (moved to Tier 2) | See Tier 2 row — promoted 2026-05-03.                                                                                                                                                                                                | —                                               |
| M45   | Save/Load   | Serialize world state (ECS components relevant to game-state + change forms). Simple serde-based snapshot format for v1 — full cosave compatibility is follow-up. Unblocks playtest iteration. **Better-than-Bethesda axis**: structured ECS snapshot beats Bethesda's notorious save-bloat format. | M40 (world streaming dictates what to serialize) |

### Tier 5 — Renderer polish (quality, not capability)

Each of these buys 10–30% visual quality but no new feature. Keep
active for incremental wins; don't let them block Tier 1–4.

| #       | Milestone             | Scope                                                                                                                                                                                                                                                                                                                                                                                         | Depends on |
|---------|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| ~~R1~~  | ~~MaterialTable refactor~~ | **Closed (2026-05-01)** across 6 phases (`aa48d64`..`22f294a`). `GpuMaterial` (272 B std430) + `MaterialTable` with byte-level dedup; per-frame `MaterialBuffer` SSBO at scene set 1 binding 13. Every per-material read in `triangle.frag` + `ui.vert` migrated to `materials[gpuInstance.material_id]`. `GpuInstance` collapsed **400 → 112 B (72% reduction)**, dropping ~30 fields (PBR / texture indices / alpha state / POM / UV transform / NiMaterialProperty diffuse-ambient / Skyrim+ shader-variant payloads / BSEffect falloff). Two intentional deferrals (filed as R1-followup): caustic compute path still reads `avg_albedo` off its own descriptor set (set 0); `DrawCommand` still carries the legacy per-material fields consumed by `to_gpu_material`. M38 is unblocked. | —          |
| M35     | Terrain LOD            | Parse `.btr` terrain LOD meshes + `.bto` object LOD. Distance-based LOD selection. Gameplay-relevant half is world streaming (M40); pure LOD is quality.                                                                                                                                                                                                                                        | M32        |
| M37     | SVGF spatial filter    | A-trous wavelet filter using existing moments data. 3 iterations, edge-stopping on normal/depth/variance. 1-SPP → ~8-SPP visual quality on GI.                                                                                                                                                                                                                                                 | —          |
| M37.3   | ReSTIR-DI              | Full spatiotemporal reservoir reuse. Drops shadow rays to 1/pixel while sampling hundreds of lights. Streaming-RIS already shipped as M31.5.                                                                                                                                                                                                                                                    | M31.5, M37 |
| M38     | Transparency & water   | OIT or depth-peeled transparency. Water plane mesh with reflection/refraction. NIF alpha sort correctness. **R1 unblocked 2026-05-01** — material-table indirection means new shading variants land in `GpuMaterial` only, not lockstep across `DrawCommand` + `GpuInstance` + 4 shaders.                                                                                                            | ~~R1~~     |
| M39     | Texture streaming      | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Memory budget with LRU eviction.                                                                                                                                                                                                                                                                              | —          |
| M29.3   | Pre-skinned raster path | Phase 3 of the GPU pre-skinning arc (`SkinComputePipeline` + per-skinned-entity BLAS refit shipped in `1ae235b`, RT shadows / reflections / GI now see this-frame skinned pose). Migrate `triangle.vert:147-204` to read pre-skinned vertices from the per-skinned-entity `SkinSlot` output buffer rather than doing inline weighted-bone-matrix-sum. The same commit must re-add `VERTEX_BUFFER` to the output buffer's usage mask — dropped in `#681` (`MEM-2-6`) so deferred-Phase-3 doesn't bloat memory-type masks today. Single source of truth, drops ~50 ALU ops per skinned vertex, but adds a critical-path dependency on the compute pass: a failed slot would now break raster too. **Defer-rationale:** the rasterized skinning path is well-understood and tested on real content; the new compute path is not. Ship only after the M41 NPC-spawning rollout proves the compute + BLAS-refit chain stable on visible animated content. | `1ae235b`, M41 stable, `#681` re-add |
| ~~M-NORMALS~~ | ~~Per-vertex tangents~~ | **Closed (2026-05-02)** — commits 91e9011 (decode `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` for Skyrim+/FO4) + 82a4563 (`synthesize_tangents` — Rust port of nifly's `CalcTangentSpace` per-triangle accumulator for FO3/FNV/Oblivion content that ships without authored tangents). Vertex stride 84 → 100 B (`tangent: [f32; 4]` at offset 84 / location 8 / RGBA32_SFLOAT); `triangle.vert/frag`, `ui.vert`, `skin_vertices.comp` updated in lockstep. `perturbNormal` re-enabled with `vertexTangent.xyz`-driven Path 1 (TBN from authored / synthesized tangent + `sign × cross(N, T)` bitangent reconstruction) and screen-space-derivative Path 2 fallback for content with neither authored nor synthesizable tangents. See [#783](https://github.com/matiaszanolli/ByroRedux/issues/783). | NIF parser |
| ~~LIGHT-N2~~ | ~~Display-space fog blend~~ | **Closed (2026-05-02, commit 18bbeae)** — composite fog mix moved from HDR-linear pre-ACES to display space post-ACES, removing the residual interior yellow/sepia distance wash on far interior surfaces. ~10-line `composite.frag` change. See [#784](https://github.com/matiaszanolli/ByroRedux/issues/784). | ~~M-NORMALS~~ |

### Tier 6 — Engine infrastructure (enablers)

| #       | Milestone                           | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Depends on     |
|---------|-------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------|
| ~~R7~~  | ~~Scheduler access declarations~~   | **Closed.** `Access` builder + `System::access()` opt-in declaration + `Scheduler::add_to_with_access` registration-side override + `access_report()` per-stage conflict analysis (`None` / `Conflict { pairs }` / `Unknown`). Snapshot stored as `SchedulerAccessReport` resource and surfaced via the `sys.accesses` console command. Current state on the engine binary: 12 systems registered, 3 declared (`fly_camera_system` / `spin_system` / `log_stats_system`), 9 undeclared, 0 known conflicts, 4 unknown pairs. M27 can now flip on with diagnosable contention; further system migrations driven by `sys.accesses` output. | —              |
| M27     | Parallel system dispatch            | Rayon-based parallel ECS system execution. TypeId-sorted lock acquisition already in place. Mostly pure optimisation — bumps frame budget for Tier 2–4 work. R7 (closed) gives `sys.accesses` for pre-flip contention analysis; remaining work is migrating undeclared systems to `add_to_with_access` so the conflict report has zero `Unknown` rows before flipping the `parallel-scheduler` feature on.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | R7             |
| M28.5   | Character controller                | Kinematic capsule with step-up, slope limiting, ground snapping. Replaces the dynamic-body fly camera for on-foot movement.                                                                                                                                                                                                                                                                                                                                                                                                                                                            | M28, M32       |
| **R2**  | ESM typed subrecord decoder         | `crates/plugin/src/esm/cell.rs` is 3217 lines — the biggest file in the repo — because sub-record dispatch is inlined in big walkers. Tier 3 adds QUST, DIAL, INFO, PERK, MGEF, SPEL, ENCH, AVIF, PACK, NAVM — a ~7× record-type surface growth. Extract a typed sub-record reader API (`read_sub::<Edid>(stream)?`, compile-time layouts). NIF's `NifStream` is already at that shape; ESM is not. **Why now:** doing the new records on the current shape is O(2K-line-file) edits; with a typed decoder it's O(new file). Prevention win, **not a rewrite**. **Blocks M24.2.**       | —              |
| M24.2   | ESM Phase 2                         | QUST / DIAL / INFO / PERK / MGEF / SPEL / ENCH / AVIF semantic parsing. Quest stages, dialogue trees, perk entry points, magic effects.                                                                                                                                                                                                                                                                                                                                                                                                                                                | R2             |
| M30.2   | Papyrus Phase 2–4                   | Statement parser, script declarations, FO4 extensions. Full `.psc` → AST for the entire Skyrim / FO4 corpus.                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | M30            |
| ~~M46.0~~ | ~~Multi-plugin CLI~~              | **Closed** via #561. Repeatable `--master <path>` CLI arg + `load_cell_with_masters` / `load_exterior_cells_with_masters` entry points. Each plugin's TES4 master_files header drives a per-plugin `FormIdRemap` so cross-plugin REFRs land in the merged `EsmIndex` under their global FormIDs. Last-write-wins on key collisions (canonical Bethesda load-order semantics). `EsmIndex::merge_from` + `EsmCellIndex::merge_from` carry the merge across the 30+ record-type maps. The unresolved-REFR diagnostic now names the missing plugin instead of silently rendering empty. Usage: `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01`. | #445 (done)    |
| ~~R3~~  | ~~NIF per-block-type parse histogram~~ | **Closed.** `nif_stats --tsv` emits a per-header-type `parsed` vs `NiUnknown` histogram; `crates/nif/tests/per_block_baselines.rs` integration test (opt-in via `cargo test -- --ignored`) compares against checked-in TSV baselines for all 7 games and fails on any `unknown` growth or `parsed` shrinkage. `BYROREDUX_REGEN_BASELINES=1` regenerates after intentional changes. Oblivion baseline refreshed 2026-04-26 to track the post-session-18 truncation drift surfaced by the audit (#687/#688/#697); the underlying drift sources stay open as separate issues (R3's job is to surface them, not fix them). Today the gate runs as a manual `cargo test … -- --ignored` invocation — there is no GitHub Actions pipeline yet, so "fail CI on regression" is the test's *contract* rather than an enforced workflow. | —              |

### Tier 7 — Deep gameplay systems (deferred until Tier 1–4 proves out)

| #       | Milestone                    | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Depends on                                      |
|---------|------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| M42     | AI packages                  | 30 composable procedures, package stack, Sandbox. Patrol paths from NAVM. Basic wander/follow/travel. PACK records need parsing first (#446).                                                                                                                                                                                                                                                                                                                                                         | M28.5, M41, #446                                |
| M43     | Quests & dialogue            | Quest stages, condition eval (~300 functions via M47.1), dialogue trees, Story Manager event triggers. Biggest single surface in the engine; ~50% of M24.2 Phase 2 feeds this.                                                                                                                                                                                                                                                                                                                        | M24.2, M41, M47.1                               |
| M46     | Full plugin loading          | Discover, sort, merge, resolve conflicts across the full load order. Builds on M46.0 (CLI wiring) + the existing `plugin/resolver.rs` DAG.                                                                                                                                                                                                                                                                                                                                                            | M24.2, M46.0                                    |
| **R4**  | SWF/GFx strategic decision   | M20 works for static SWF menus. M48 needs Scaleform GFx extensions (`_global.gfx`, text replacement, Papyrus callbacks, fonts, 34 menus). Ruffle has no GFx extension support and isn't pinned — it drags wgpu into an otherwise ash-only tree. Honest exits: (a) in-house AS2+GFx-subset interpreter (Papyrus-parser-adjacent patience), or (b) rebuild menus in egui/iced, treat Scaleform compat as out of scope. **Why now (decision, not implementation):** don't sleepwalk into a 3–6 month rabbit hole in Tier 7. Pick a direction so M48 has a plan, then defer until Tier 4 ships. | M20                                             |
| M48     | UI integration               | Papyrus ↔ UI bridge, input routing, menu callbacks. Shape determined by R4 decision.                                                                                                                                                                                                                                                                                                                                                                                                                  | R4, M20, M47.2                                  |

### Tier 8 — Visual fidelity stretch (post-Tier-4 horizon)

Take the existing rendered content (Prospector Saloon, MedTek
Research, Whiterun's Bannered Mare, FO3 Megaton) and make it *as
good as it can possibly look*. Beauty axis only — pure-performance
work moves to Tier 11. Each entry leverages the existing RT
investment rather than bolting on a parallel pipeline: volumetric
shadows are RT, hair shadows are RT, SSS is optionally RT-traced,
decals participate in GI. ByroRedux's "RT-first" posture means the
visual ceiling here is genuinely above what any 2008–2015 Bethesda
forward renderer can reach. Goal: a screenshot of any vanilla
Bethesda interior that holds up against modern offline-rendered
output. **No active work** — Tier 1–4 ships first.

Sequencing within the tier is impact-first. M55 (volumetrics) and
M59 (decals + material layering) are the highest "wow factor per
dollar" — they transform the look of every existing cell with no
M41 dependency. M56 / M57 (SSS / hair) are gated on M41 producing
visible NPCs to render onto. M51 (PT reference) and M-LIGHT crown
the pipeline; M54 unlocks once the scene-data volume justifies a
trained model.

| #        | Milestone                          | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Depends on              |
|----------|------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------|
| M55      | Volumetric lighting                | God rays / light shafts via single-scattering volumetric integration in a frustum-aligned 3D froxel texture, RT-shadowed (no shadow-map cascade hack — we already have RT visibility). Exponential height fog with per-cell density driven by REGN region records and weather state. The cinematic moment Bethesda interiors most lack — sunlight through Megaton's church windows, dust motes in Doc Mitchell's hallway, fog rolling off WastelandNV grass at dusk. Reference: Hillaire (Frostbite, SIGGRAPH 2015) Frostbite volumetrics. | M34, M44 (REGN parsing) |
| M59      | Material & decal layering          | Decal projection (blood splatters, bullet holes, footprints in dust, water puddles, scorch marks), micro-detail normal maps (concrete, fabric, metal grain, leather pores), anisotropic specular (brushed metal, hair, satin, vinyl), parallax occlusion improvements for masonry / stones. RT decals participate in GI — Bethesda decals never have. Material slot extensions to `GpuMaterial` only; the R1 promise holds (no DrawCommand or shader-lockstep growth).                                          | R1                      |
| M58      | Reference-quality post-process     | Kawase-blur bloom (5-pass dual filter, ~2 ms total), scatter-as-gather DOF for cinematic mode, per-object motion blur reusing existing motion vectors, color grading via 3D LUT (per-cell-type mood — interior warm, exterior cool, irradiated green), AgX or Tony McMapface tone mapping selectable alongside ACES, optional vignette / film grain. Single compute dispatch chain layered onto the existing composite pass; no extra render-pass churn.                                                       | —                       |
| M56      | Subsurface scattering              | Burley normalized SSS (preferred) or screen-space SSS for skin / wax / fruit / soft organic materials. M41's NPCs become visibly human only once skin gets SSS — flat Lambert reads as plastic. Eyes get cornea refraction + caustic (existing M22 caustic compute path is reused, not duplicated). Optional RT-traced SSS variant for closeup actors in cinematic mode.                                                                                                                                       | M41, R1                 |
| M57      | Hair / fur shading                 | Marschner three-lobe BRDF (R / TT / TRT) for hair; Disney's hair shading model as the simpler default. RT hair shadows with stochastic transparency. Bethesda-vintage hair plate meshes look like clay under standard PBR; correct hair shading is the difference between "T-pose mannequin" and "T-pose person." Pairs naturally with M56 — face closeups need both.                                                                                                                                          | M41                     |
| M-LIGHT  | Reference-quality lighting         | Soft shadow penumbras filtered in screen space using RT visibility samples (no PCF, no cascade tricks), contact shadows on dielectrics (close-range RT for groundedness), IBL from per-cell HDR sky probe captured once at cell load, multi-bounce GI (≥ 2 bounces in PT mode, currently 1 in raster mode). Closes the gap between "lit correctly" (today) and "lit as well as the data allows."                                                                                                              | M51                     |
| M51      | Path tracing reference mode        | Full PT (no rasterized fallback), ReSTIR-PT spatiotemporal reservoirs, SHARC radiance cache for diffuse, optional NRC neural radiance cache. Reference mode for screenshots / cinematics; demonstrates "RT-first" wasn't a positioning claim. References: Bitterli et al. (ReSTIR PT, 2022), Pharr et al. (SHARC, 2024). Reuses existing reservoir + denoiser plumbing from M31.5 / M37.                                                                                                                       | M37, M37.3              |
| M54      | Neural denoiser                    | Small NN (~1–2 MB weights) replaces SVGF spatial filter for indirect lighting once enough scene data exists to train. Targets visual parity + 30% runtime. References: NRD-NN, Intel Open Image Denoise GPU. Implementable as a Vulkan compute pass; no proprietary SDK dependency. At PT scales (M51) it stops being a perf optimisation and becomes a quality multiplier.                                                                                                                                  | M37, M51                |

### Tier 9 — Better-than-Bethesda capability stretch (post-Tier-4 horizon)

Plays to the ECS-native architecture and clean-room rebuild. Each
entry is something Bethesda demonstrably cannot ship on top of the
Papyrus stack-VM + non-deterministic save system. **No active work**
until M40 (streaming) and M45 (save/load) prove the underlying
state model holds up.

| #     | Milestone                     | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Depends on                              |
|-------|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------|
| M50   | In-engine world editor        | ECS-native Creation Kit replacement: edit cells, place REFRs, paint terrain, edit lighting, with hot-reload to a running engine via the `byro-dbg` protocol. Linux-native, no Wine; undo/redo is free off the ECS snapshot machinery; saves diff against the parsed ESM into a content-addressed plugin. The largest single user-visible win the project can ship — and the existing debug-protocol crate already does ~60% of the wire shape this needs.        | M45, debug protocol expansion           |
| M60   | P2P co-op (≤ 4 players)       | Deterministic ECS state replication, host authority with host migration. Was always a community-mod hack on every Bethesda title — practical here because we don't have Papyrus stack-VM semantics fighting determinism. Engine ships P2P only; **no central server, no matchmaking, no telemetry.** R5's quest prototype must include a determinism analysis before this can ship.                                                                              | M45, R5                                 |
| M62   | LLM dialogue plugin (opt-in)  | Optional plugin wiring DIAL / INFO + custom Papyrus events to a local LLM (Llama 3.x via candle / mistral.rs). Off by default, opt-in per quest or per NPC; lives entirely in plugin space. Demonstrates what "ECS event hooks" enables that stack-VM Papyrus cannot. No network calls; LLM weights ship with the mod.                                                                                                                                            | M47.0, M43                              |
| M63   | OpenXR / VR                   | Full VR via openxrs. RT-first renderer is genuinely useful here — VR is the genre most starved of well-running RT content, and our forward-pass cost is already low. Stereo rendering through the existing pipeline; controller input through the existing input layer; M50 expanded for VR-aware authoring.                                                                                                                                                     | M27, M50                                |
| M64   | Procedural exterior cells     | Generate exterior cells from heightmap + biome rules + noise. Effectively unlimited worldspace; complements rather than replaces vanilla Bethesda exteriors. Starfield's procedural-planet model is the obvious comparison; ours can do better because cells are first-class ECS state, not save-file blobs.                                                                                                                                                       | M40, M50                                |

### Tier 10 — Ecosystem unlock (post-Tier-4 horizon)

Realizes the architectural promise of content-addressed Form IDs and
clean-room data-only legacy compat. **No active work** until the
engine ships something playable for the existing community to mod.

| #     | Milestone                     | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Depends on              |
|-------|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------|
| M70   | Cross-platform builds         | Windows native (vanilla Vulkan, no portability layer) and macOS via MoltenVK. The project is Linux-first by primary maintainer setup, **not Linux-only by design.** CI matrix expands to all three platforms; Steam Deck falls out of the Linux build for free.                                                                                                                                                                                                   | —                       |
| M72   | Decentralized mod hosting     | IPFS-style content-addressed mod distribution: mod-id = content hash, dependency graph auto-resolves through the existing `plugin/resolver.rs` DAG. Realizes the full payoff of "no LOOT, no slot limits" — mods are addressable globally, no central marketplace, no Bethesda.net-style platform tax.                                                                                                                                                            | M46, M50                |
| M80   | glTF / USD export             | Round-trip Bethesda assets to industry-standard formats. Open NIF / NifSkope replacement: load any vanilla mesh, export to glTF + materials, edit in Blender / Houdini, re-import as a content-addressed plugin. Massive creative-pipeline unlock for modders who don't want to fight 3ds Max 2010 and a 15-year-old NIF plugin.                                                                                                                                  | NIF parser stable       |
| M81   | Visual scripting (BT-style)   | Behavior-tree node graph layered on M47.0 event hooks, for modders who don't write code. Same surface as Papyrus would expose, but with no language to learn. Complements rather than replaces M47.0 / M47.2.                                                                                                                                                                                                                                                       | M47.0, M50              |
| M82   | Asset preprocessing pipeline  | One-shot bake: BSA / BA2 → ByroRedux native asset format with optimal layouts (texture-streaming-aware mip ordering, cluster-aware mesh layout for M53). Ships once at install / mod-publish; runtime loads are 10× faster. Replaces the current "open the BSA, decode on demand" hot path with a memory-mappable bundle.                                                                                                                                          | M39, M53                |

### Tier 11 — Performance ceiling (post-Tier-4 horizon)

Pure-performance entries that don't add visual capability or new
gameplay surface — they raise the per-frame ceiling once a
real-content benchmark identifies one of them as the bottleneck.
**No active work** until that benchmark exists. Today's bench is
GPU-bound on RT cost (`fence=5.81 ms / 78% wall` on Prospector at
HEAD `220e8e1`, 2026-05-11); CPU-side draw submission is not the constraint at
current entity counts. Listed for direction, not on the active
path. Split out from Tier 8 (2026-05-08) once Tier 8 was reframed
around visual fidelity — these are honest ceiling raisers, not
beauty work, and conflating the two muddied both.

| #     | Milestone                          | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                              | Depends on |
|-------|------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| M52   | GPU-driven rendering               | Mesh shaders + task shaders, GPU frustum + cluster cone culling, indirect-from-GPU draw command generation. Eliminates CPU draw-submission overhead at 7 000+ entities (FO4 MedTek bench's actual ceiling). Falls back to current path on hardware without `VK_EXT_mesh_shader`. Closes the loop on R7 / M27: parallel ECS dispatch on the CPU side + GPU-driven submission on the GPU side.                                                       | M27        |
| M53   | Virtual geometry (Nanite-class LOD) | Cluster mesh format with deterministic simplification chain, GPU cluster selection per pixel. Makes M35 (.btr terrain LOD) obsolete in the good direction: load full-resolution geometry, GPU picks the right level. Same data structure works for static and skinned meshes (with care around bone-influenced clusters).                                                                                                                       | M52        |

### Parking lot (nice-to-have, no active work)

| #       | Notes                                                                                                                                                                            |
|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| M37.6   | DLSS2. Proprietary, 4070 Ti target. Post-M37 TAA is already solid; DLSS is a later polish pass if it ever becomes relevant.                                                     |
| M25     | Vulkan Compute — partially realised (clustered lighting / SSAO / SVGF temporal are compute-backed). Remaining work folds into M29 (skinning) and M37 (spatial filter).          |
| Full cosave save/load | M45 v1 ships a simple snapshot. Byte-compatible cosave format (load original-engine save into Redux, or vice versa) is speculative and not a priority.                         |
| Morrowind (TES3)      | NIF v3.x / v4.x is fundamentally different from the v10+ era ByroRedux supports — separate parser, separate ESM dialect, no BSA. Gamebryo 2.3 source we reference predates Morrowind's release; OpenMW is the canonical clean-room re-implementation. **Out of scope** unless explicit demand surfaces — supporting it would double the parser surface for one extra game. |

### What we are NOT doing (anti-scope)

Documented to keep "wouldn't it be cool if" suggestions from
silently growing the cone:

- **Per-pixel parity with Bethesda's interior look.** Sessions 25–28
  showed the trap: chasing "chrome cushion" / "yellow fog" took 3
  sessions to find a missing texture and a Frostbite-curve adoption.
  ByroRedux is RT-first; matching a 2008–2015 forward-renderer's
  specific look is *not* the goal. Render correctly, ship.
- **Original-engine save-format compatibility.** M45 ships a
  structured ECS snapshot. Loading vanilla saves is a 6-month
  reverse-engineering project for an audience of approximately zero.
- **Mod-load-order tooling (LOOT-equivalent).** Content-addressed
  Form IDs are explicitly the architectural alternative. We do not
  ship a sorter.
- **Console releases / non-Linux primary support.** Linux-first.
  Windows + macOS are downstream if they happen.
- **Hosted online services.** No telemetry, no updater, no crash
  reporter posting upstream, no central server, no skin /
  monetization surface, no Bethesda.net-style mod marketplace.
  P2P co-op (M60, Tier 9) is **in scope** — engine ships the
  replication layer, never a server. Decentralized mod hosting
  (M72, Tier 10) is in scope — content-addressed, no central
  registry.
- **Cloning Papyrus VM semantics.** R5 may make us run "Papyrus
  bytecode as an ECS system" if the pure transpiler bet fails — but
  even then we are not implementing OpcodeFetch / OpcodeDispatch /
  StackFrame / StackUnwind in their original shapes. Better, not
  same.

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

- **R1** — MaterialTable refactor (collapse DrawCommand). **Closed 2026-05-01** across 6 phases — `GpuInstance` collapsed 400 → 112 B (72% reduction); per-frame `MaterialBuffer` SSBO with byte-level dedup. M38 unblocked.
- **R2** — ESM typed subrecord decoder. Tier 6, blocks M24.2.
- **R3** — NIF per-block-type parse histogram (closed via `nif_stats --tsv` + `per_block_baselines.rs` + checked-in 7-game baselines). Tier 6, prevention.
- **R4** — SWF/GFx strategic decision. Tier 7, gates M48.
- **R5** — Papyrus quest prototype. Tier 3, gates M47.2.
- **R6** — Scratch-buffer instrumentation (closed via `ctx.scratch` + `ScratchTelemetry`). Tier 2 prevention, landed before M40. **R6a** — Prospector re-bench. Tier 1.
- **R7** — Scheduler access declarations (closed via `Access` builder + `sys.accesses` console command). Tier 6, M27 unblocked on tooling; remaining work is migrating the 9 still-undeclared systems.

### Growth discipline

The project's single biggest risk is **scope growth without
compression** (64K → ~130K LOC over the last six sessions). Tier
ordering gives top-level backpressure; apply it inside crates too. If
a single file crosses 3 500 lines, a struct crosses 50 fields, or a
context struct crosses 60 fields, treat it as a signal rather than a
stat to report — investigate before adding.

**Tripwire today**: `crates/plugin/src/esm/cell.rs` is at 3 217 lines
— under threshold but R2 (typed subrecord decoder) was filed
specifically because Tier-3 record growth would push it past 3 500
on the current shape. R2 lands before M24.2 starts, not after.

### Pacing discipline (added 2026-05-03)

Audit cadence is a load-bearing risk. The renderer alone has 16
distinct audit reports filed in 30 days; each generates LOW/MEDIUM
findings that absorb commit budget. Without backpressure, audits
become the work product instead of the work.

- **Renderer audits**: paused. Re-open trigger is M41 visible NPCs
  on real content (the workload that would change what an audit
  surfaces). Until then, close from the existing 51-issue backlog;
  do not run `/audit-renderer` on session cadence.
- **Per-game audits** (`/audit-fnv`, `/audit-skyrim`, etc.):
  on-demand only when working in that game's path, not periodic.
- **Safety / ECS / NIF audits**: keep on session cadence —
  these tend to surface real correctness issues, not visual nits.
- **LOW-severity findings**: bundle into single PRs rather than
  one-commit-per-finding. The Session 28 audit-bundle pattern (6
  closes / 6 commits / 6 tests) was healthy; the alternative
  (one audit → 30 small commits) is what calcifies.
- **Stale-bench discipline**: any roadmap change that touches a
  numbered claim must either refresh the claim or move it under a
  `~~stale~~` block. No silent drift.

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
- [ ] NPCs + creatures don't spawn as visible entities — **Phase 0–4 of M41.0 shipped Session 24**: kf-era spawn (skeleton + body + head + FGGS+FGGA face morphs) and Skyrim+ pre-baked FaceGen dispatch land, AnimationPlayer attach env-var-gated (#772). Visible-content QA + #772 unblock + #774/#775 (FO3 audit residue) close out M41.0; M41 (NPC behavior beyond spawn) remains open.
- [ ] No world streaming — entire cell re-imported from scratch on every load (M40)
- [x] ~~BSA v103 (Oblivion) decompression not working~~ — **stale premise, closed via #699**. v103 archive opens AND extracts cleanly: 147 629 / 147 629 vanilla files across all 17 Oblivion BSAs (2026-04-17 + 2026-04-25 sweeps); `nif_stats` round-trips 8032 NIFs through the v103 path. The real Oblivion exterior blocker is TES4 worldspace + LAND wiring (same shape FO3 was) — already covered by the M40 / M41 / "exterior renderer" Tier-1/2 plan, no separate tracker needed.
- [x] Skyrim + FO4 cells not wired through `cell_loader` — **closed M32.5**, both render end-to-end

### Open — Tier 3 / 4 gaps

- [ ] 1 257 FO3 SCPT records parsed; no runtime executes them (M47.0)
- [x] ~~No audio subsystem of any kind (M44)~~ — **closed 2026-05-06**. `byroredux-audio` crate scaffolded on kira `0.10` (Phase 1), BSA → symphonia decode + `SoundCache` (Phase 2), spatial sub-tracks + per-emitter `SpatialTrackHandle` (Phase 3), `play_oneshot` queue + `FootstepEmitter` + XZ-plane stride `footstep_system` (Phase 3.5), looping ambient via `loop_region` + tweened stop on `AudioEmitter` despawn (Phase 4), streaming music via `StreamingSoundData` + `play_music` / `stop_music` (Phase 5), global reverb send + per-cell `set_reverb_send_db` (Phase 6). Pending: FOOT records → per-material lookup (drops dirt hardcode), REGN region-keyed ambient layers, per-cell-load reverb-toggle wiring (API ships, cell loader doesn't call yet), raycast-occlusion attenuation. See M44 row in active milestones.
- [ ] No save/load — playtest iterations require cold cell re-load (M45)
- [ ] `PACK` (AI packages) records have stubs only — no evaluator (#446, M42)

### Open — Risk-reducers (2026-04-22)

- [x] ~~**R1** DrawCommand has ~40 fields + 10 shader-variant payloads — collapse to `material_id` indirection (blocks M38)~~ — **closed 2026-05-01** across 6 phases (`aa48d64`..`22f294a`). `GpuInstance` collapsed 400 → 112 B (72% reduction); per-frame `MaterialBuffer` SSBO with byte-level dedup. M38 unblocked. Two follow-ups: caustic compute set 0 path + `DrawCommand` per-material field cleanup.
- [ ] **R2** ESM sub-record decoder is ad-hoc across 3 000+-line walkers — typed `read_sub::<T>` API (blocks M24.2)
- [x] **R3** NIF `NiUnknown` soft-fail masks per-block regressions — **closed**. `nif_stats --tsv` emits per-type `parsed` vs `unknown`; `crates/nif/tests/per_block_baselines.rs` (opt-in) compares against checked-in 7-game baselines and fails on any unknown growth or parsed shrinkage. Oblivion baseline refreshed 2026-04-26 against the audit-flagged truncation drift; `#687`/`#688`/`#697` track the underlying parser drift sources (R3 surfaces them, doesn't fix them).
- [ ] **R4** SWF/GFx strategic decision needed before M48 — Ruffle+GFx-stubs vs rewrite menus natively
- [ ] **R5** Papyrus full-runtime prototype on one real quest before M47.2 scope commitment
- [x] **R6** `VulkanContext` scratch buffers have no capacity telemetry — **closed**. `ctx.scratch` console command + `ScratchTelemetry` resource cover all 5 persistent scratches; per-frame refresh via `VulkanContext::fill_scratch_telemetry`. Prospector baseline: 337 KB total, 320 B wasted.
- [x] **R6a** Prospector re-bench — **closed**. 192.8 FPS / 5.19 ms at `e6e8091`, wall-clock bench.
- [x] **R6a-stale** Bench-of-record refreshed at `6a6950a` (2026-04-24). Prospector 172.6 FPS / 5.79 ms (was 192.8 / 5.19 — slight regression in compositor-jitter range; fence_ms unchanged at 4.34, GPU still the bottleneck). Skyrim Whiterun 253.3 FPS / 3.95 ms at 1932 entities (was 237 FPS at 1258 entities — entity count up 53% while FPS improved, indicating more REFRs land now without perf cost). FO4 MedTek 92.5 FPS / 10.82 ms (was 90, 7434 entities unchanged).
- [x] **R6a-stale-7** Bench-of-record refresh — **closed 2026-05-11 at HEAD `220e8e1`** (post M41 Phase 2 close-out). Prospector 133.5 FPS / 7.49 ms @ 2562 entities (was 172.6 / 5.79 @ 1200 entities at `6a6950a` — +114% entities, +29% wall_ms; sub-linear scaling consistent with RT cost amortising across the BLAS hierarchy). Skyrim Whiterun 217.3 FPS / 4.60 ms @ 3209 entities (was 253.3 @ 1932 — +66% entities, -14% FPS, sub-linear). FO4 MedTek 68.5 FPS / 14.61 ms @ 10 809 entities (was 92.5 @ 7434 — +45% entities, -26% FPS). Frame still GPU-bound on Prospector (fence=5.81 ms / 78% wall). Two M41-EQUIP changes drove most of the entity inflation: the Phase 2 scaffold spawning NPC inventory roots (`#896` A.0 → B.2) and the REFR Euler→Y-up composition fix (`Rx · Ry · Rz`, was `Rz · Ry · Rx`) which now lands every REFR through the corrected order. **Session 33 Markarth grid diagnostic stays as a separate snapshot, not a bench-of-record candidate** — it's a new workload class (Tier 8 indirect lighting + 1500+ mesh exterior grid) which the three steady-state interior benches don't measure.
- [x] **R7** Scheduler access declarations — **closed**. `Access` builder + `System::access()` opt-in + `Scheduler::add_to_with_access` for closures + `sys.accesses` console command surface a per-stage Conflict / Unknown report. 3 of 12 systems declared so far (fly_camera, spin, log_stats); 4 Unknown pairs remaining. M27 flip is diagnosable now; eliminating the Unknown rows is incremental migration work.

### Closed — Renderer regressions (2026-05-01 / 02 live debug arc)

- [x] **M-NORMALS** ([#783](https://github.com/matiaszanolli/ByroRedux/issues/783)) — **closed 2026-05-02** (commits 91e9011 + 82a4563). Per-vertex tangent decode (`NiBinaryExtraData("Tangent space (binormal & tangent vectors)")`) + `synthesize_tangents` fallback (Rust port of nifly's `CalcTangentSpace` per-triangle accumulator, runs on FO3/FNV/Oblivion content that ships without authored tangents). Vertex stride 84 → 100 B; `triangle.vert/frag`, `ui.vert`, `skin_vertices.comp` updated in lockstep. `perturbNormal` re-enabled with authored-tangent Path 1 + screen-space-derivative Path 2 fallback. See Tier 5 row.
- [x] **LIGHT-N2** ([#784](https://github.com/matiaszanolli/ByroRedux/issues/784)) — **closed 2026-05-02** (commit 18bbeae). Composite fog mix moved from HDR-linear pre-ACES to display space post-ACES; ~10-line `composite.frag` change.
- [x] **Chrome posterized walls** — **closed 2026-05-02** (commit b2354a4). `tex.missing` revealed 39 unique missing textures × 263 entities for FNV `GSDocMitchellHouse` — checker placeholder × valid normal map = "chrome" speckle. Root cause: FNV ships base textures across `Fallout - Textures.bsa` AND `Fallout - Textures2.bsa`; only the former was loaded. Fixed by `open_with_numeric_siblings` (auto-loads `<stem>2.bsa` … `<stem>9.bsa` siblings on disk when the explicit path has no digit before `.bsa` / `.ba2`). `tex.missing` now reports 1 entry (`<no path, no material>` placeholder geometry, legitimate). See Session 27 in [HISTORY.md](HISTORY.md). Permanent diagnostic bit `DBG_BYPASS_NORMAL_MAP = 0x10` retained alongside `DBG_VIZ_NORMALS` / `DBG_VIZ_TANGENT`.
- [x] **Coplanar z-fighting on rugs / decals / desktop clutter** — **closed 2026-05-03** (commits 0f13ff5 / ee3cb13 / 088696e / c515028). New `RenderLayer` ECS component (Architecture / Clutter / Actor / Decal) attached at cell-load time from each REFR's `RecordType`; renderer applies a per-layer `vkCmdSetDepthBias` ladder via `RenderLayer::depth_bias()`. Per-mesh `is_decal` / `alpha_test` escalate to Decal at spawn (alpha-tested rugs, NIF-flagged blood splats); small-STAT meshes (bounding-sphere radius < 50 units ≈ 71 cm) escalate to Clutter so paper piles / folders / clipboards win their desk z-fights. Live verification via the new `BYROREDUX_RENDER_DEBUG=0x40` (`DBG_VIZ_RENDER_LAYER`) tint-by-layer viz. Replaces the ad-hoc `is_decal || alpha_test_func != 0` heuristic — single source of truth, game-invariant Oblivion → Starfield.
- [x] **Cluster-light cull-radius shoulder visible on floors** — **closed 2026-05-03** (commit 78632a6). Replaced `window = 1 - (d/r)²` with the Frostbite smooth-distance attenuation curve `(1 - (d/r)⁴)²` in both point and spot arms of `triangle.frag`. Reference: Lagarde & de Rousiers, "Moving Frostbite to Physically Based Rendering" §3.1.2. Preserves ~65% more energy in the mid-zone and approaches zero with C¹ continuity at the cull radius — no more visible circular boundary on the floor. Cull range stays at `radius * 4.0`; per-light fill stays at 0.02. SPIR-V recompiled.
- [x] **"Chrome cushion" reflective look on dielectric props near lamps** — **closed 2026-05-03** (commit 8038ae7). `Material::classify_pbr` was piping `env_map_scale` straight into `PbrMaterial.metalness`. `env_map_scale` is the legacy BSShaderPPLighting cube-map intensity authoring knob; vinyl cushions, glass, polished wood, plastic armor all author it > 0 *without being conductors*. Routed every dielectric-with-sheen into the metal-reflection branch (`triangle.frag:metalness > 0.3`), which then picked up cell ambient + nearby emissive sconces — the chromy look on FNV medical gurneys. Fix: `env_map_scale > 0.3` now drops roughness only; metalness stays 0. Real conductors caught by texture-path keyword arms above. Power armor (`metal` + `env_map_scale ≈ 2.5`) keeps `metalness=0.9` from the keyword branch.

### Open — Misc

- [ ] `parry3d` panics on nested compound collision shapes (catch_unwind guard in place)
- [x] ~~`--esm` accepts only one plugin~~ — **closed via #561 / M46.0** (repeatable `--master <path>` CLI arg + multi-plugin merge through `EsmIndex::merge_from`).
- [ ] `BSBoneLODExtraData` has no parser — surfaced by R3 baselines: 0/34 on FO4, 0/52 on Skyrim SE, 0/56 on FO76 (no instances on the other four games). Single-fix candidate matching the Session 18 R3-driven pattern.
- [x] ~~`BSClothExtraData` 0/298 on Starfield~~ — **closed via #722**. Parser was reading the NiExtraData `Name` field that nif.xml line 3222 marks `excludeT="BSExtraData"`; consumed 4 bytes of cloth payload as a string-table index, then read garbage as length and tripped EOF. Fix unblocks 1 523 cloth blocks across FO4 (309) / FO76 (365) / SF Meshes01 (298) + SF FaceMeshes (551). Cloth-simulation animation consumer still future work; parser side now correct. Baseline TSVs need a fresh sweep (`BYROREDUX_REGEN_BASELINES=1`) to lock the per-block delta.
- [ ] One Starfield NIF (`meshes\marker_radius.nif`) requests a 318 MB single-buffer allocation at parse time, exceeding `byroredux_nif::stream::MAX_SINGLE_ALLOC_BYTES = 256 MB`. Per-allocation cap is a different trade-off from the BA2 chunk cap bumped in Session 18 — bumping this one weakens defence against attacker-controlled `u32` sizes inside individual NIF blocks. Tracked separately; one file out of 320 483 in the Starfield mesh archive.
- [ ] **`#688`** — 149 Oblivion files truncate at root NiNode "failed to fill whole buffer". Investigation refuted the audit's "v=20.0.0.5 subset" framing (see `.claude/issues/688/INVESTIGATION.md`): all 149 are pre-Gamebryo NetImmerse-vintage content shipped in Oblivion's BSA, dominated by **v=10.1.0.106 / bsver=5 (77 files, 52% of bucket)** plus v=10.0.1.0 (39), v=10.0.1.2 (21), v=10.1.0.101 (8), v=4.0.0.2 (4). Empirical hex dump of `meshes\menus\hud_brackets\a_b_c_d_seq.nif` shows a 4-byte leading zero before the NiObjectNET.name field that neither `nif.xml` nor `nifly` document. Two block-layout hypotheses tested (per-block u32 prefix; one-time block_data_offset shift), both partial — different blocks expect different leading layouts. The audit's recommended `block_size` end-of-block assertion doesn't apply because `block_sizes` is gated `since 20.2.0.5` and these files are < 20.2.0.5. **Deferred** until Gamebryo 2.3 / NetImmerse-era `NiObjectNET::LoadBinary` source is mounted to bisect against. Affected files are non-critical-path (HUD brackets, menu assets, one creature head); interior cells render fine. **Caution for future audit runs**: do NOT re-derive the "v=20.0.0.5 subset" framing — it's been empirically refuted.

---

## Project Stats

Ground-truth as of 2026-05-10, verified by `/session-close`.

| Metric                                  | Value                        |
|-----------------------------------------|------------------------------|
| Rust source lines (non-test)            | ~153 802                     |
| Rust total lines                        | ~160 086                     |
| Source files (non-test)                 | 307                          |
| Workspace members                       | 19                           |
| Tests (last reported by ROADMAP)        | 1879 (Session 32 1827 + Session 33 +52 across the Tier 8 visual fidelity ship — M55 volumetrics + M58 bloom + M-LIGHT v1 + golden frame harness (33f48b5), SpeedTree Phase 1 unblock (9 commits, dedicated TREE parser + TLV walker + importer placeholder + `--tree` CLI), performance audit-bundle close (5 issues #928–#932), NIF `until=X` doctrine flip (#935, ~14 sites across 11 files), BLAS eviction static/total split (#920), Anniversary Edition build-prefix strip (e2409c0, no issue — live `tex.missing` catch on Markarth). Two audits filed (`AUDIT_PERFORMANCE_2026-05-10.md`, `AUDIT_NIF_2026-05-10.md`); ~8 GitHub issues closed end-to-end + 10 new NIF issues filed via `/audit-publish`.) |
| Open issue directories                  | 898 (`.claude/issues/`)       |
| NIFs in per-game integration sweeps     | 184 886                       |
| Per-game NIF clean-parse rate           | 100% on FO3 / FNV / Skyrim SE; Oblivion 96.24%, FO4 96.46%, FO76 97.34%, Starfield 98.6% aggregate (see compat matrix for per-archive breakdown). Recoverable 100% on all except Oblivion 99.99%. Sweep date 2026-04-27. |
| Supported archive formats               | BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8 |

### Repro commands for every bench claim

| Claim                                                                     | Command                                                                                                                                                                                        |
|---------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Prospector Saloon 2562 entities @ 133.5 FPS / 7.49 ms (commit `220e8e1`, 2026-05-11, wall-clock bench) | `cargo run --release -- --esm "Fallout New Vegas/Data/FalloutNV.esm" --cell GSProspectorSaloonInterior --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa" --bench-frames 300` |
| Skyrim SE WhiterunBanneredMare 3209 entities @ 217.3 FPS / 4.60 ms (commit `220e8e1`, 2026-05-11) | `cargo run --release -- --esm "Skyrim Special Edition/Data/Skyrim.esm" --cell WhiterunBanneredMare --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" --textures-bsa "Skyrim - Textures0.bsa" --textures-bsa "Skyrim - Textures1.bsa" --textures-bsa "Skyrim - Textures2.bsa" --bench-frames 300` |
| FO4 MedTekResearch01 10 809 entities @ 68.5 FPS / 14.61 ms (commit `220e8e1`, 2026-05-11) | `cargo run --release -- --esm "Fallout 4/Data/Fallout4.esm" --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2" --textures-bsa "Fallout4 - Textures1.ba2" --textures-bsa "Fallout4 - Textures2.ba2" --textures-bsa "Fallout4 - Textures3.ba2" --textures-bsa "Fallout4 - Textures4.ba2" --textures-bsa "Fallout4 - Textures5.ba2" --textures-bsa "Fallout4 - Textures6.ba2" --textures-bsa "Fallout4 - Textures7.ba2" --textures-bsa "Fallout4 - Textures8.ba2" --textures-bsa "Fallout4 - Textures9.ba2" --textures-bsa "Fallout4 - TexturesPatch.ba2" --materials-ba2 "Fallout4 - Materials.ba2" --bench-frames 300` |
| Skyrim sweetroll single-mesh ~3000-5000 FPS (2026-04-22, RTX 4070 Ti @ 1280×720)        | `cargo run --release -- --bsa "Skyrim Special Edition/Data/Skyrim - Meshes0.bsa" --mesh meshes\\clutter\\ingredients\\sweetroll01.nif --textures-bsa "Skyrim Special Edition/Data/Skyrim - Textures3.bsa"` |
| Megaton interior parse-side 929 REFRs (2026-04-19)                        | `cargo test -p byroredux-plugin --release --test parse_real_esm parse_real_fo3_megaton_cell_baseline -- --ignored`                                                                             |
| Per-game full mesh sweep (clean rates above; recoverable 100% gate)       | `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`                                                                                                          |
| Full ESM record counts (FNV 73 054 structured + 5 625 long-tail / FO3 31 101 — FO3 unreverified post-#808/#809/#810; #901 FNV refresh) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored`                                                                                                                   |

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
