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

**Last verified**: 2026-07-11 (Session 55 closeout — tests 3549, +118; src/ LOC +~6 324; bench-of-record now 584 commits stale, see staleness caveat below). Session 55 landed the M47.2 QUST VMAD fragment-section decoder + runtime population (`8a70b81a`) — vanilla quests now advance stages from decompiled `.pex` fragment bodies instead of test fixtures — plus the VWD flag materialization and the actual root cause of the long-open interior-ghosting artifact (a repeated camera/capsule desync on cell transitions, #1874, not a shader bug). The bulk of the session worked through five `AUDIT_RENDERER`/`AUDIT_NIF`/`AUDIT_TECH_DEBT` reports (2026-07-05 → 2026-07-09), closing 100+ individual low/medium-severity findings via `/fix-issue`. See [HISTORY.md](HISTORY.md) Session 55.
**Bench-of-record** (R6a-stale-14 refresh, HEAD `1c26bc25`, 2026-06-03,
wall-clock bench, 300 frames, RTX 4070 Ti, run from each game's `Data/`
directory — see Repro-command CWD note below):

| Bench | This refresh (`1c26bc25`) | Prior record (`4e2ebe8c`) | Δ |
|---|---|---|---|
| **Prospector Saloon** (FNV) | **76.2 FPS / 13.11 ms / fence=11.12 / brd=0.37 / 3516 ent / 1224 draws** | 71.4 / 14.00 / fence=11.65 / 3507 ent | FPS +6.7% · fence −4.6% |
| **Whiterun BanneredMare** (Skyrim SE) | **362.8 FPS / 2.76 ms / fence=0.98 / 3216 ent / 1299 draws** | 329.8 / 3.03 / fence=1.01 / 3211 ent | **FPS +10.0%** · fence flat |
| **FO4 MedTekResearch01** | **65.2 FPS / 15.34 ms / brd=3.74 / fence=9.03 / 21414 ent / 14535 draws** | 90.7 / 11.02 / brd=2.63 / fence=4.73 / 15546 ent | FPS −28% · **ent +38% · draws +75%** |

**Interpretation.**
Whiterun (control, authored bhk collision) improved +10% FPS — attributable to Session 46 perf wins (dirty-Vec / billboard gate / bone-pool, #1371-#1379). Prospector improved modestly (+6.7% FPS, fence 11.65→11.12 ms): `IsCollisionOnly` reduces TLAS instance count for entities that carry both `MeshHandle` and synthesized `CollisionShape`, but BLAS entries are still built per mesh-handle and the entity count is unchanged (3516 vs 3507) — the fix addresses TLAS, not BLAS. The full pre-collider baseline (161.4 FPS / fence=2.62 ms @ 2564 entities) has not been recovered; the entity count growth origin is still under investigation (see Known Issues). MedTek's −28% FPS and +38% entity count is **entirely from M49 CSG precombined geometry** (Session 45, landed *after* the prior bench) — correctly spawning precombined mesh entities that weren't loaded before. The new MedTek scene is genuinely larger and richer; 65.2 FPS / 21414 ent becomes the new baseline. GPU-bound at fence=9.03 ms; CPU brd=3.74 ms still sub-dominant.

**Repro-command CWD note:** bare `--bsa` / `--textures-bsa` / `--materials-ba2` names resolve against CWD, not the `--esm` folder. Run each bench with CWD set to that game's `Data/` directory. Run from elsewhere → archives silently fail → scene loads near-empty (Prospector: 36 entities / 3 meshes / spurious ~1792 FPS).

**Staleness (2026-07-11, Session 55):** bench-of-record is now **584 commits stale**. Session 55 was scripting-runtime work (M47.2) plus a large audit-driven bug-bash — the renderer touches were doc/comment corrections, one small classifier-keyword narrowing (#1873), and a stale-SPIR-V-version recompile (#1929), none of them hot-path shader/pipeline changes — so no new drift risk was added, but the gap is not closing either. R6a-stale-15 (a fresh 300-frame three-scene GPU bench) still gates any current FPS claim.

---

## Status

**Rendering, today.** Interior cells load and render end-to-end from
unmodified Bethesda game data across the lineage — Oblivion (Anvil
Heinrich Oaken Halls), FNV (Prospector Saloon), FO3 (Megaton at 929
REFRs), Skyrim SE (WhiterunBanneredMare, full cell with 6 named
equipped NPCs), and FO4 (MedTekResearch01) all load through the same
`cell_loader`. Exterior renders 7×7 (radius 3, default) grids from FNV
WastelandNV with landscape terrain (LAND heightmap + LTEX/TXST splat),
and world streaming swaps cells as the player walks (M40). **Starfield
bring-up** went from `no parser` to a walkable Cydonia interior in
5 days this session (#1289 CDB `.mat` wiring, #1291 XCLL canonical-size
split, #1292 BSGeometry `geometries\X.mesh` resolution, #1294
trimesh-fallback gate, #1295 door-teleporter spawn; ESM Phase 0/1 via
the `sf_smoke` baseline tool). Single-mesh sweetroll ~3000-5000 FPS
(2026-04-22, RTX 4070 Ti @ 1280×720).

**RT lighting.** Full pipeline: SSBO multi-light, ray-query shadows
with streaming weighted reservoir sampling (16 reservoirs/fragment, Phase 19;
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
Skyrim SE land at 100% clean; FO4 at 100% (both base mesh archives,
2026-06-14); Oblivion / FO76 in the 95–97%
band (drift-induced truncation; #687/#688 closed). Starfield at 97.19%
clean (recent BA2 v3 LZ4 chunked content the parser doesn't yet
fully cover). Recoverable rate is 100% on all seven games except
Oblivion (99.99%, single hard-fail on a corrupt-by-design debug
marker — #698 closed). ESM parses structured records across ~25 types on
FNV; 73 054 structured records on the latest sweep, plus a 5 625-
record long-tail bucket (sounds / idle / grasses / debris). Archive
readers cover BSA v103/v104/v105 and BA2 v1/v2/v3/v7/v8 (GNRL + DX10
with reconstructed DDS headers, zlib + LZ4).

**Scripting, physics, UI.** Papyrus `.psc` parses end-to-end —
lexer + Pratt expression parser + statement/script parsers + full
AST (M30 Phase 1 → M30.2). Rapier3D physics bridge with a kinematic
character controller (gravity + collide-and-slide + jump + autostep,
vanilla-Skyrim capsule, M28.5). Ruffle/SWF UI overlay renders Skyrim
SE menus; an embedded egui debug overlay (`byroredux-debug-ui`) draws
over the composite output. ECS-native scripting runtime shipped: the
event-hook dispatcher (M47.0) and CTDA condition evaluator (M47.1)
both wire into engine init, validated by the R5 "go ECS-native"
prototype that hand-translated `defaultRumbleOnActivate.psc` into
plain ECS components + dt-driven systems. The per-script transpiler
that closes the loop on the 1 257 parsed FO3 SCPT records is M47.2
(Tier 3). 3D spatial audio (M44) plays footsteps / ambient / music
through `byroredux-audio` (kira 0.10) with a per-cell reverb send.

**NIFAL — the NIF Abstraction Layer (shipped 2026-05-28).** The
canonical translation tier that resolves a raw per-game `ImportedMesh`
into engine-native ECS data exactly once, killing the per-game
branches that used to leak into the renderer and the two duplicated
load paths. First slices landed this session: a single
`material_translate::translate_material` boundary
([`byroredux/src/material_translate.rs`](byroredux/src/material_translate.rs))
that both `cell_loader/spawn` and `scene/nif_loader` now call —
`Material.metalness`/`roughness` resolved to plain `f32` (was
`Option` + per-draw `classify_pbr`), glass classified alpha-aware,
effect/BGSM flags packed; a particle slice decoding authored
`NiPSysEmitter` base params + birth rate (`NiPSysEmitterCtlr`) + size
(`NiPSysGrowFadeModifier`) instead of preset kinematics; and a
collision-audit fix translating the two previously-dropped
`BhkMultiSphereShape` / `BhkConvexListShape` (all 13 parsed
`bhk*Shape` variants now translate). The emissive-scale unification
was measured and resolved as a no-op. Spec at
[`docs/engine/nifal.md`](docs/engine/nifal.md); design history for the
material side at [`docs/engine/material-abstraction.md`](docs/engine/material-abstraction.md).

**What doesn't work yet (as of 2026-05-28).** Skinned rendering and
world streaming are no longer in this list — both shipped: M29 / M29.5
verified the skinning chain end-to-end with GPU bone-palette compute,
and M40 closed the streaming pipeline (`WorldStreamingState` + async
cell pre-parse + LRU BLAS eviction + interior↔exterior cell-swap).
The live gaps: **Oblivion exterior** — the TES4 worldspace + LAND
wiring is implemented and game-agnostic, so the remaining step is an
on-device exterior render bench (same shape FO3 was in the
pre-cell-loader era — the
long-running "BSA v103 decompression" framing is a stale premise
refuted by the 2026-04-17 + 2026-04-25 sweeps; v103 extracts
147 629 / 147 629 vanilla files end-to-end, see #699). **NPC behavior
beyond spawn** (AI packages, animation playback wiring) — M41 spawns
visible T-pose actors with equipment but M42 behavior is Tier 7.
**Actor motion** — NPCs spawn in bind pose; Havok `.hkx` *animation* is
not yet decoded (M41.x animation slice). The FO4 *skeleton* loads fine —
the old "FO4 humanoid meshes wait on a `.hkx` skeleton loader" framing
was a stale premise: `characterassets\skeleton.nif` is a NIF that ships
in `Fallout4 - Meshes.ba2` and resolves through the corrected path table.
**M41.x ragdoll (Havok-baseline physics)** — the FNV slice shipped: the
`bhkRigidBody` + ragdoll/malleable constraint chain parses, threads into
a Rapier **multibody**, and the `ragdoll <id>` console command runs a
Bethesda ragdoll on our solver (18-body Doc Mitchell verified). FO4/FO76/
Starfield ragdolls stay blocked on the `BhkSystemBinary` blob decoder.
**The Papyrus runtime** that executes the 1 257 parsed
FO3 SCPT records is M47.2 — the event-hook (M47.0) + condition (M47.1)
foundations ship, plus the `.pex` recognizer slice (Session 51); the full
transpiler is deferred. **Save/load** (M45 + M45.1) shipped 2026-06-21.
Weather transitions (fade between WTHR states) and cloud
layers 2/3 closed in M33.1 (`2bfb622`).

**Per-fragment normal mapping (2026-05-02).** Re-enabled and shipped:
**M-NORMALS** ([#783](https://github.com/matiaszanolli/ByroRedux/issues/783),
commits 91e9011 + 82a4563) parses Bethesda's
`NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` blob
when present and falls back to a Rust port of nifly's
`NiTriShapeData::CalcTangentSpace` per-triangle accumulator
(`crates/nif/src/import/mesh/tangent.rs::synthesize_tangents`) for FO3 / FNV /
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
on commit 0681fc7 (`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`);
Oblivion / Fallout 4 / Starfield rows refreshed per-row since (dates in
each row), and Fallout 76 refreshed 2026-07-11 (#1900 / NIF-D3-02 — it had
gone stale at 97.34% vs a live 100%).
Clean = no NiUnknown placeholders + no truncation. Recoverable = file
parses end-to-end (counting NiUnknown / truncation as recoverable).
The audit-publish run #684–#688 / #697 / #698 tracked the parse-rate
work for the games where clean < 100% (all four now CLOSED; residual
gaps tracked under git log).

| Game              | Archive       | NIF parse rate (clean / recoverable)         | Cells                                                    |
|-------------------|---------------|----------------------------------------------|----------------------------------------------------------|
| Oblivion          | BSA v103      | **99.93%** (8 026 / 8 032) · recover 100%    | Interior (Anvil Heinrich Oaken Halls). Exterior parse + load ✓ — TES4 worldspace + LAND wiring is implemented and game-agnostic; only an on-device exterior render bench is pending (same shape FO3 was). `#687` closed (NiGeomMorpherController + NiControllerSequence Phase fixes; 83 truncations recovered). `#688` / `#698` closed; remaining 6 truncated NetImmerse-era Oblivion files (pre-Gamebryo v3.3–v4.2 markers, #1611) tracked under git log (live `nif_stats` over `Oblivion - Meshes.bsa`, 2026-07-11 post-#1900: 0 hard failures, recoverable is a clean 100%). |
| Fallout 3         | BSA v104      | 100% (10 989)                                | Interior (Megaton, 929 REFRs). Exterior wired; fresh GPU bench pending (R6a). |
| Fallout New Vegas | BSA v104      | 100% (14 881)                                | Interior (Prospector **3516 entities @ 76.2 FPS / 13.11 ms / fence=11.12** on RTX 4070 Ti, R6a-stale-14 `1c26bc25` 2026-06-03; +6.7% FPS vs R6a-stale-13 (3507 ent / 71.4 FPS). Full fence recovery to pre-collider baseline (161.4 FPS / 2.62 ms @ ~2564 ent) still pending — see R6a-stale-15). Exterior 7×7 (radius 3). |
| Skyrim SE         | BSA v105 LZ4  | 100% (18 862)                                | Interior (WhiterunBanneredMare **3216 entities @ 362.8 FPS / 2.76 ms / 1299 draws / fence=0.98**, R6a-stale-14 `1c26bc25` 2026-06-03; **+10.0% FPS vs R6a-stale-13 (329.8 FPS)** — Session 46 perf wins (#1371–#1379). Whiterun is the steady-state control bench; Skyrim ships real `bhk` collision so entity count is flat. The cell loads 246 unique textures across `Skyrim - Textures0..8.bsa` — as of the M35 sibling-auto-load fix (2026-06-19) passing just `Skyrim - Textures0.bsa` auto-opens `Textures1..8` (and `Meshes0.bsa` auto-opens `Meshes1.bsa`): the asset provider now treats a `…0`-suffixed archive as Skyrim's zero-based series start, so the older "list all 9 explicitly" workaround is no longer required). |
| Fallout 4         | BA2 v1/v7/v8  | **100.00%** (159 866 / 159 866) · recover 100% | Interior (MedTekResearch01 **21414 entities @ 65.2 FPS / 15.34 ms / 14535 draws / brd=3.74 ms / fence=9.03**, R6a-stale-14 refresh `1c26bc25` 2026-06-03; entity/draw growth vs R6a-stale-13 entirely from M49 CSG precombined geometry — scene is larger and richer, not a regression). Both base mesh archives clean, 0 truncated (`Fallout4 - Meshes.ba2` 34 995 + `Fallout4 - MeshesExtra.ba2` 124 871); the former FaceGen truncation tail is gone (2026-06-14 `parse_rate_fo4_all_meshes`). |
| Fallout 76        | BA2 v1        | **100%** (58 469 / 58 469) · recover 100%    | — (2026-07-11 sweep, #1900; was stale at 97.34%)          |
| Starfield         | BA2 v2/v3 LZ4 | **99.64%** aggregate · recover 100% (all 5 archives, 2026-07-03 sweep) | Per-archive: Meshes01 **100%** (31 058 NIFs), Meshes02 **100%** (7 552), MeshesPatch 98.91% (29 849; 325 truncated, 1.09%), LODMeshes **100%** (19 535), FaceMeshes **100%** (1 282). Intervening parser work (≥ #1510 BSShaderType155 tail, #1606 starfield_tail, #754 BSWeakReferenceNode, #722 cloth) cleared the Meshes01/LODMeshes truncation tails; the residual MeshesPatch tail is unchanged (#746/#747). |

---

## Active Roadmap

Priority: **shortest path to a playable cell**, not shortest path to a
shinier frame. The renderer is mature (RT + RIS + SVGF + TAA + POM)
and the content pipeline parses recoverably across every target
(clean rates per the matrix above; #687/#688/#697/#698 closed — see git log);
next bottlenecks are *consumers* — things that make what we
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
| ~~M34~~ | ~~Exterior lighting~~         | **Closed.** Per-frame sun arc from game time in `weather_system`. TOD ambient + fog + directional from WTHR NAM0. Interior fill at 0.6× + `radius=-1` (unshadowed) in `render/lights.rs`; `triangle.frag` line 2747 gates RT shadow on `radius >= 0`. All pieces were complete before this session.                                | —                  |
| ~~M32.5~~ | ~~Per-game cell loader parity~~ | **Closed.** Skyrim SE WhiterunBanneredMare 1258 entities @ 237 FPS. FO4 MedTekResearch01 7434 entities @ 90 FPS. No code changes — session 14 infrastructure was complete. Oblivion exterior: TES4 worldspace + LAND wiring is implemented and game-agnostic (parse + load ✓); only an on-device exterior render bench is pending (same shape FO3 was — *not* BSA v103 decompression; that was a stale framing closed via #699).                                                                     | —                  |
| ~~R6a~~ | ~~Prospector re-bench~~       | **Closed.** 192.8 FPS / 5.19 ms at `e6e8091` with wall-clock bench. Scene is glass-heavy (RT refraction/reflection); representative tough-case FNV interior.                                                                                                                                                | —                  |

### Tier 2 — Actors visible & animated (blocks "cells are populated")

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                        | Depends on         |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------|
| ~~M29~~ | ~~Skinning chain verification~~ | **Closed.** End-to-end skinning chain (`SkinnedMesh` ECS → bone-palette → vertex shader) verified on FNV NiTriShape path via 7 integration tests in `byroredux/tests/skinning_e2e.rs` (4 FNV + 3 SSE). Bones populate, names round-trip, partition-local→global remap correct, palette responds to bone Transform mutations. CPU palette eval shipped; compute-shader dispatch deferred to M29.5 (gated on M41 producing measurable load). Defensive `MAX_TOTAL_BONES` overflow guard added (covered by `render/bone_palette_overflow_tests.rs`, `Once`-gated warn) so the silent truncation past 32 skinned meshes is no longer invisible. SSE BSTriShape per-vertex skin path filed as #638 (separate parser bug, not in M29 scope). | —                  |
| ~~M29.5~~ | ~~Compute-shader palette dispatch~~ | **Closed (Session 40, 2026-05-20).** GPU bone-palette compute pass replaces host-side per-frame upload (`4ac5ee8f`); orphaned `bone_staging_buffers` + `upload_bones` path dropped (`427cdb69`). **M29.6 promoted persistent SSBO with per-entity slot pool** (`5be66790`) so allocation amortises across frames; three hotfix regressions closed in one bundle (`8ea8d61d`): slot-0 init on first dispatch (#1191), pending re-queue on `clear()` (#1192), bounds assert on stale slot pointer (#1193). Bench-of-record at `b5726a18` not invalidated — skinned BLAS refit remains gated by the LRU eviction policy that landed pre-Session-39. | M41, R1            |
| ~~M41.0~~ | ~~FaceGen heads render~~      | **Closed (2026-05-05).** Phases 0–4 shipped in Session 24; #772 closed via FLT_MAX-pose gate; #794 closed via three-layer regression suite (parser diagnostic + 4 synthetic e2e + 1 real-data e2e in `crates/nif/tests/mtidle_motion_diagnostic.rs` and `byroredux/src/systems.rs::animation_system_e2e_tests`). Real FNV `mtidle.kf` → `animation_system` produces a 1.49 component-wise rotation delta on `Bip01 Spine` after 4 ticks — animation pipeline is healthy end-to-end. The remaining "rigid NPC" symptom is the **already-known Phase 1b.x body-skinning artifact** (`npc_spawn.rs:402-431`: long-spike vertex artifact, `0 unresolved` bones, palette composition bug); not in the animation chain. M41.0.5 (GPU per-vertex morph runtime) + M41.x (Havok `.hkx` stub) deferred to Tier 5. | M29, #458          |
| M41    | NPC spawning (Phase 1 + Phase 2 closed) | **Phase 1 closed (2026-05-07).** FNV `GSDocMitchellHouse` renders Doc Mitchell as a coherent T-pose humanoid at the REFR position — skeleton + body + hands + head + FaceGen morphs all compose without artifact. The closure-bar workload defined 2026-05-03 ("at least one Skyrim/FO4/FNV cell renders NPCs visible at REFR positions, even in T-pose") is met. **#841 closes** — the long-spike body-skinning regression no longer reproduces on the canonical FNV repro; root cause appears resolved as a side-effect of #771 (palette ground truth pin) + subsequent animation fixes since `b386eb3`. The runtime `skin <id>` debug command (`InspectSkinnedMesh` extended with per-bone resolved `GlobalTransform` + computed palette + identity-dropout flagging) lands as the regression guard so any future resurgence localizes in one round-trip. **Phase 2 scaffold shipped (2026-05-07/08, #896 Phases A.0 → B.2):** `Inventory` + `EquipmentSlots` ECS components + `ItemInstancePool` resource land in `0a0d652`; `f1b3156` walks NPC inventory and spawns ARMO meshes (concurrent body+armor as deliberate spike); `21ae560` pre-scans for body-slot armor and skips `upperbody.nif` when present (kills z-fight + 2× bone-palette overhead); `121c705` / `24a7bd8` / `4ec9bb6` build the per-game `resolve_armor_mesh` helper (Skyrim+ ARMO → ARMA → worn-mesh chain) and wire it into both spawn paths. **Phase 2 close-out advanced (Session 32, 2026-05-08):** LVLI dispatch landed in `be4663b` via `byroredux_plugin::equip::expand_leveled_form_id` — both `OTFT.items` outfit walks (prebaked path) and `npc.inventory` CNTO entries (kf-era path) flatten leveled-list refs into base ARMO/WEAP form IDs gated on `actor_level`. Pre-fix vanilla Skyrim+ NPCs whose default outfits referenced LVLI silently spawned with no gear (the loop's `index.items.get(&form_id)` failed silently). Single-pick (highest-level eligible) is the Bethesda flag-bit-0-unset default; multi-pick lands all eligible. 8 new resolver tests cover passthrough / level gating / multi-pick / nested recursion / circular cap / unknown id. Plus `--bench-hold` CLI flag (`73adffb`) keeps the engine open after `--bench-frames` so `byro-dbg` can attach against the loaded scene; `Inventory` + `EquipmentSlots` registered with the debug-server (`9b957bb`) so `byro-dbg`'s `entities <Component>` lights them up; runnable smoke at `docs/smoke-tests/m41-equip.sh` (`9b957bb` + `085321d` + `3422884`) with hard / soft pass-fail assertions parsing the bench summary line + byro-dbg output. **First smoke run on FO4 MedTekResearch01 at 10 809 entities / 57.9 FPS** — the engine + LVLI dispatch produce real geometry; visual A/B remains. **Phase 2 closed (2026-05-11).** Smoke gate green on both targets after hoisting `build_npc_equip_state` above skeleton load in `spawn_prebaked_npc_entity`: SSE WhiterunBanneredMare shows 6 named NPCs with `Inventory` + `EquipmentSlots`, `tex.missing=0`, 3209 entities / 2052 draws / ~84 FPS (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda — equipped via OTFT.items + LVLI dispatch). FO4 MedTekResearch01 surfaces 23 NPCs with `Inventory` + `EquipmentSlots` (LvlFeralGhoul / LvlTurretBubble / LvlFeralGhoulAmbush / Loot_CorpseFeralGhoul01 leveled-creature templates; no humanoid named NPCs at cell-load on this dungeon — that's a cell-content property, not an equip-pipeline gap), 10 809 entities / 8162 draws / ~49 FPS. **The equip pipeline is now observable independent of mesh-load success** — even when the FO4 humanoid 3rd-person skeleton.nif is absent (vanilla ships only `_1stperson\skeleton.nif` + `.hkx` on the character path; the SSE-shaped `character assets\skeleton.nif` is not in any Fallout4 BA2, verified by BA2 scan of Meshes / MeshesExtra / Animations / Misc / Startup), the per-NPC `Inventory` + `EquipmentSlots` components still land on the placement root via the early-build hoist. Follow-ups left open: (1) FO4 humanoid skeleton resolution needs a Havok `.hkx` loader or `_1stperson` placeholder before armor *meshes* materialize on FO4 actors (M41.x Havok stub); (2) `spawn_prebaked_npc_entity` returns `Some(placement_root)` on every early-return path, inflating the cell-loader's "NPCs spawned" count to include mesh-less husks; (3) kf-era `spawn_npc_entity` could use the same equip-build hoist for symmetry (FNV/FO3 work today because their skeleton.nif resolves; not urgent). **Renderer-audit moratorium gate cleared** — bench-of-record refresh (R6a-stale-7 / #902), GPU skinning compute (M29.5), and skinned BLAS coverage are now the next steady-state moves. | M24, M29, M41.0    |
| ~~M49~~ | ~~FO4 PreCombined Geometry (CSG reader)~~ | **Closed (Session 45, 2026-06-02, `b93ad7a9..2900de70`).** `BSPackedGeomObject` TLV format cracked from first principles — no external spec required. Pipeline in five commits: `crates/bsa` gains a `.csg` reader (`b93ad7a9`); NIF import decodes CSG geometry to Y-up meshes (`3d665217`); cell-loader spawns precombined entities from the CSG (`067adc34`); LOD selection fixed to one tier per object, not all three (`a30c088a`); texturing wired through the owning REFR's shape slot indices (`2900de70`). Closes #1351 / #1188 Stage A. Sub-items still open: `_precomb.nif` collision, `.uvd` occlusion volumes. | #1188 (Stage A) |
| ~~M40~~ | ~~World streaming~~           | **Closed 2026-05-24** — re-audit of the streaming pipeline confirms Phase 3 was de-facto landed before this row was last refreshed. **Phase 1a/1b** shipped Session 23 (`cdfef07` / `80e2966` / `592e7bf` / `7dc354a`): the `streaming` module with `WorldStreamingState`, `compute_streaming_deltas` (pure-function diff + hysteresis), async cell-pre-parse worker, and shutdown drain. **Phase 2** shipped 2026-05-21 across 3 stages (`f6b9911a` / `1e92a471` / `a7cc9184`): Stage 1 plumbs `DoorTeleport` from REFR XTEL into `PlacedRef` + `cell_for_refr` reverse-lookup; Stage 3a wires the interior↔interior orchestrator (`script.activate <door_id>` tears the source down and loads the destination through `load_cell_with_masters`); Stage 3b extends to interior↔exterior — but Stage 3b's actual implementation **already spawns a fresh `WorldStreamingState` at `DEFAULT_TRANSITION_RADIUS = 3`** ([`byroredux/src/main.rs:1100-1113`](byroredux/src/main.rs)) and calls `stream_initial_radius` to populate the full 7×7 grid around the destination, then `step_streaming` maintains the loaded set as the player walks. **Phase 3 (multi-cell grid + BLAS evict/reload)** is implicitly satisfied: (a) `compute_streaming_deltas` runs on every cell-boundary crossing in [`main.rs:1008`](byroredux/src/main.rs), with `radius_load=3` → 7×7 grid + `radius_unload=4` hysteresis preventing boundary thrash; (b) `cell_loader::unload_cell` calls `accel.drop_blas(mh)` per freed mesh handle at [`cell_loader/unload.rs:185`](byroredux/src/cell_loader/unload.rs) **and** invokes `shrink_blas_scratch_to_fit` (#495) so the BLAS scratch doesn't pin VRAM after streaming-out a peak cell; (c) `AccelerationManager::evict_unused_blas` (LRU at [`acceleration/blas_static.rs:955`](crates/renderer/src/vulkan/acceleration/blas_static.rs)) runs pre-batch + mid-batch (90% threshold via `should_evict_mid_batch`); (d) `MAX_FRAMES_IN_FLIGHT` const_assert (#960) pins the immediate-destroy safety window; (e) #920 split `static_blas_bytes` from `total_blas_bytes` so skinned BLAS no longer thrash eviction. **Open follow-ups** (not blocking close): smoke test against a real multi-cell exterior workspace (FNV WastelandNV / Skyrim Whiterun plains / FO4 Sanctuary Hills) to bench cell-crossing latency; the existing per-cell parse stutter (~50-100 ms on FNV per Phase 1a doc) remains. **Current-state correction (2026-07-15):** the `main.rs:1100-1113` / `main.rs:1008` citations and `radius_load=3` above are the values as of this row's 2026-05-24 close and have since moved — the transition/streaming-tick logic now lives in `byroredux/src/app_step.rs` (`step_streaming`, `step_cell_transition`, per `#1858`/TD1-003), with `DEFAULT_TRANSITION_RADIUS = 5`. The live cell-swap trigger is the `door.teleport <entity_id>` console command, not `script.activate`. See [docs/engine/exterior-grid-streaming.md](docs/engine/exterior-grid-streaming.md) for the current, source-cited walkthrough. | M32, M35           |
| **M44** | Audio (3D spatial)            | **Phases 1–6 shipped (2026-05-05).** Foundation: `byroredux-audio` crate on [`kira`](https://crates.io/crates/kira) `0.10` — `AudioWorld` + ECS components (`AudioListener` / `AudioEmitter` / `OneShotSound`); BSA decode via `StaticSoundData::from_cursor` + `SoundCache`; `audio_system` spatial sub-track model with lazy listener creation, per-emitter `SpatialTrackHandle`, prune-on-Stopped. Phase 3.5: `play_oneshot` queue API + `FootstepEmitter` + `footstep_system` (XZ-plane stride accumulator, vertical motion excluded). `--sounds-bsa <path>` decodes canonical FNV dirt-walk WAV. **Phase 4**: `AudioEmitter.looping = true` applies kira's `StaticSoundData::loop_region(..)`; the prune sweep notices when a looping sound's source entity has lost its `AudioEmitter` (despawn-by-cell-unload, or explicit removal) and issues a tweened `stop()`. **Phase 5**: `load_streaming_sound_from_bytes` / `_from_file` for multi-minute music via kira's `StreamingSoundData`; `AudioWorld::play_music` (single-slot, non-spatial, crossfade) + `stop_music` + `is_music_active`. **Phase 6**: global reverb send. On manager init, the audio crate creates a kira `SendTrackBuilder.with_effect(ReverbBuilder)` at full wet; spatial sub-tracks opt in via `with_send(reverb.id(), reverb_send_db)`. `AudioWorld::set_reverb_send_db` toggles per cell type (`f32::NEG_INFINITY` = silent default, `-12 dB` = subtle interior, `0 dB` = full wet). Already-playing sounds keep their construction-time send level. **Tests**: 12 default + 5 `#[ignore]`d real-data integrations on cpal (BSA decode, full lifecycle, queue-driven lifecycle, looping survives natural duration + stops on AudioEmitter remove, streaming music play/stop on real OGG). Workspace 1680 / 1680 passing. **Phases 3.5b + REGN-driven ambient pending**: FOOT records → per-material lookup (drops dirt hardcode); REGN region-keyed ambient layers; raycast-occlusion attenuation. **Cell-load reverb-toggle wiring closed 2026-05-08 (#846)** — `reverb_zone_system` ([`byroredux/src/systems/audio.rs`](byroredux/src/systems/audio.rs), an ECS system since the Session 34/35 systems split) flips `set_reverb_send_db` to `-12 dB` on interior cells, `f32::NEG_INFINITY` (silent) on exterior. | —                  |
| ~~R6~~ | ~~Scratch-buffer instrumentation~~ | **Closed.** `ScratchTelemetry` resource refreshed per frame from `VulkanContext::fill_scratch_telemetry`, surfaced via the `ctx.scratch` console command. Reports per-Vec `len` / `capacity` / `bytes_used` / `wasted` for all 5 scratches (gpu_instances, batches, indirect_draws, terrain_tile, tlas_instances). On Prospector (1200 ent / 773 draws): 337 KB total, 320 B wasted — well right-sized; M40 cell transitions can now be diffed against this baseline. | —                  |

### Tier 3 — Scripting runtime (unblocks 1 257 FO3 SCPT records)

**Reordered 2026-05-03**: R5 now comes first. Hooks-first sequencing
risks committing M47.0's event-hook shape before validating the
ECS-native-no-VM bet, then having to rework hooks if R5 falls back
to "Papyrus stack-VM as an ECS system." De-risk first.

| #      | Milestone                      | Scope                                                                                                                                                                                                                                                                                                                                                                                                      | Depends on      |
|--------|--------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------|
| ~~R5~~ | ~~Papyrus quest prototype~~    | **Closed 2026-05-16 — verdict: go ECS-native.** Hand-translated `defaultRumbleOnActivate.psc` (50 LOC Papyrus, ships attached to hundreds of vanilla Skyrim references — pressure plates, ritual buttons, shrines) into `crates/scripting/src/papyrus_demo/` (135 LOC production + 200 LOC tests). All three R5 semantic gates (latent `Utility.Wait()`, multi-state dispatch, cross-subsystem call) translate cleanly to plain ECS components + dt-driven systems. No VM, no fibre, no suspendable script frames. **Load-bearing finding**: a Papyrus event handler with a latent wait splits into two systems — code-before-wait runs on the event, code-after-wait runs on whichever frame the dt counter hits zero. That's the entire pattern. M47.0 hook shape (marker components for `OnActivate` / `OnHit` / …) is unchanged; M47.2 proceeds as a per-script transpiler emitting the same components + systems shape this prototype hand-built. The Papyrus-stack-VM-as-ECS-system fallback is parked. Full evaluation at [`docs/r5-evaluation.md`](docs/r5-evaluation.md); reference fixtures at [`docs/r5/source/`](docs/r5/source/). | M30             |
| ~~M47.0~~  | ~~Event hooks runtime~~     | **Closed 2026-05-23** across 6 phases (`6c51af55..03837739`). R5's "go ECS-native" hand-translations (`papyrus_demo`) now wire into engine init: Phase 1 calls `papyrus_demo::register` from `scripting::register` and adds the 8 dispatcher systems to the scheduler (exclusive in Update). Phase 2 introduces `ScriptRegistry: HashMap<String editor_id, ScriptSpawnFn>` with `defaultRumbleOnActivate` as the first registered spawner; the three property-bearing R5 demos defer to VMAD-decode in M47.2. Phase 3a adds `EsmIndex::base_record_script(base_form_id) → Option<u32>` walking ACTI / CONT / TERM / ITEM record maps. Phase 3b changes `spawn_placed_instances` to return `(EntityId, usize)` so the cell loader's per-REFR walk can call `attach_script_for_refr` after each spawn — three-stage lookup chain `base_record_script → index.scripts → ScriptRegistry → spawn_fn`. Phase 4 ships the `script.activate <entity_id>` console command (gameplay-driven use-key + raycast deferred to M28.5 input wiring as out-of-scope). Phase 5 adds `OnCellLoadEvent` (real engine emit at `attach_script_for_refr`), `OnTriggerEnterEvent` (deferred Rapier sensor wiring), `OnEquipEvent` (deferred M41 equip pipeline). Phase 6 lands 5 e2e tests in `papyrus_demo/tests.rs` walking the full Phase 1-5 chain on synthetic entities. Design doc at [`docs/engine/m47-0-design.md`](docs/engine/m47-0-design.md). Test count: 64 → 74 scripting tests + 3 plugin tests. The two structurally-registered-but-not-yet-emitted markers (OnTriggerEnter, OnEquip) are M47.0.x follow-ups that touch separate subsystems. | R5, #443        |
| ~~M47.1~~ | ~~Condition eval~~          | **Closed 2026-05-23** across 2 commits (`ea9d0cfa`, `0a835e3e`). The universal predicate system shipped: Phase 1 added `byroredux_plugin::esm::records::condition` parsing CTDA sub-records (28-byte FO3/FNV + 32-byte Skyrim+) into `Condition { function_index, comparator, comparand, param_1, param_2, run_on, reference_form_id, extra_data_id, or_next }`. Phase 2+3 added `byroredux_scripting::condition` with the `ConditionFunction` enum + OR-precedence evaluator implementing Bethesda's load-bearing quirk: `A AND B OR C AND D` evaluates as `A AND (B OR C) AND D` (consecutive ORs form blocks that bind tighter than AND). 7 representative functions land at canonical indices: GetActorValue (9), GetDistance (36), GetStage (58) **WORKING**, GetStageDone (59) **WORKING**, GetFactionRank (60), GetIsID (71), HasPerk (99) — the 5 stubs trace-log their backing-ECS-component gaps for future expansion. Phase 4 migrated `quest_advance` (the R5 DA10MainDoorScript translation) from bespoke `require_done`/`forbid_done` Vec<u16> fields to a generic `ConditionList`, demonstrating the first consumer. Tests: 11 plugin-side CTDA parser tests + 9 scripting-side evaluator tests (including 2 load-bearing OR-precedence regression pairs). Workspace test count: 74 → 83 scripting + 3 plugin (condition) tests added. The ~300-function catalog grows additively — new functions just add an enum variant + dispatch arm. AI packages / dialogue triggers / terminal branches plug in by constructing their own `ConditionContext` and calling `evaluate(&list, world, &ctx)`. | M47.0           |
| M47.2  | Full scripting runtime         | **`.pex` + quest-advance + trigger vertical slice shipped (Session 51, `65239fec..f1a00e89`).** A Champollion-port `.pex` decompiler (`crates/pex`) turns vanilla *compiled* Papyrus bytecode into the `byroredux_papyrus` AST (99.996% of the shipping corpus, 26 640/26 641, zero panics); `translate_pex` runs it through the M47.0-style recognizer chain — compiled bytecode drives ECS behavior with no VM. The cell loader resolves each scripted REFR's VMAD-named `.pex` from a `--scripts-bsa` archive at attach time (`base_record_script_instance` retains VMAD on ACTI/CONT/NPC). The generic quest-advance recognizer covers the `default*SetStage*` family (guarded / player-gated / unconditional) on both `OnActivate` and `OnTriggerEnter`; the latter is driven by a new `TriggerVolume` detection system spawned from `XPRM` box/sphere primitives. **QUST fragment keystone (Session 55, `8a70b81a`):** `parse_quest_fragments` decodes the QUST record's trailing stage→`Fragment_N` table (layout derived + cross-validated against all 856 fragment-bearing `Skyrim.esm` VMADs); `populate_quest_fragments_from_pex` decompiles + lowers each bound fragment body, wired into the cell loader after `load_references`. `QuestStageFragments` — built and unit-tested earlier but never fed live data — now drives real quest-stage advancement from vanilla `.pex` content (69.5% of the script corpus per the landing commit's own measurement). Design: [`docs/engine/m47-2-design.md`](docs/engine/m47-2-design.md); smoke: [`docs/smoke-tests/m47-triggers.sh`](docs/smoke-tests/m47-triggers.sh). **Remaining**: scale the recognizer catalog (OnEquip/OnHit families need their emit sites), ESM-native 136-event dispatch, perk entry-point composition, Obscript via SCTX. | R5, M30.2, M43  |

### Tier 4 — Save/load (unblocks "it feels like a game")

| #     | Milestone   | Scope                                                                                                                                                                                                                                  | Depends on                                      |
|-------|-------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| ~~M44~~ | Audio (moved to Tier 2) | See Tier 2 row — promoted 2026-05-03.                                                                                                                                                                                                | —                                               |
| M45   | Save/Load   | **Library landed 2026-06-21 (`bd2d0de2`, branch `feat/m45-save-load`).** New `crates/save`: full-ECS-World snapshot (not a delta log) via `SaveRegistry` (type-erased per-component/-resource save/load closures; binary populates the curated game-state set). Versioned binary container — `magic / major+minor / schema-fpr / CRC32 / len` header over a serde_json payload; `decode` rejects bad-magic / version-skew / schema-drift / truncation / CRC corruption before any parse. `save_world(&World)` / `restore_world(&mut World)` drivers; load **preserves entity ids exactly** (new `World::set_next_entity` + `insert_batch` at saved sparse ids → `Parent`/`Children`/`root_entity` refs valid with no remap). Crash-safe atomic write (`tmp → fsync → read-back verify → rename`) + round-robin `SaveRing`. Pre-save `validate_world` refuses poisoned saves (Parent⇄Children, equip indices, clip-handle resolution, dangling refs). StringPool round-trips via `dump`/`from_dump` (symbol-order) + `fixed_string_serde`; `FormIdComponent` persists the **stable `FormIdPair`** (resolved through `FormIdPool`), never the session-local handle. Core `save = ["inspect"]` feature; serde on Name/Parent/Children/FormIdPair/PluginId(hex-string for the u128)/ItemInstancePool; ScriptTimer gains a `save` feature. Binary: `save [slot]` (validate + snapshot + atomic write) + `save.info <slot>` (decode + verify + summarise) console commands. 16 save-crate + 2 World + 2 binary tests (incl. cross-crate ScriptTimer round-trip). **M45.1 live load-apply landed 2026-06-21 (`48e18c4f`, branch `feat/m45.1-live-load`)** via the change-form model: `load <slot>` reloads the saved cell through the existing loader (full GPU/physics/camera setup), then overlays saved game-state deltas keyed by stable `FormIdPair` (`build_form_id_remap` composes saved-entity→pair→live-entity; `apply_deltas` overlays a curated *mutable* column set — Transform/Inventory/EquipmentSlots/Light*/Animation*/ScriptTimer — onto matched live entities; structural columns Name/Parent/Children/form-id-key are not replayed). New `CurrentCellContext` resource (cell + plugins) set at every interior load makes a save self-describing; `restore_resources` replaces `ItemInstancePool` wholesale first so instance ids resolve. Wired as `load` console command + `PendingSaveLoadSlot` + `step_save_loads` between-frames drain. **M45.1 player-pose restore closed 2026-06-21** — new `PlayerPose` save-resource (position + yaw/pitch + character/flycam flag) refreshed each frame post-scheduler by `capture_player_pose`; on `load`, `apply_player_pose` re-places the persisted player body (Character — `camera_follow_system` re-pins the camera next frame, momentum cleared, kinematic Rapier body re-synced) or the camera (FlyCam) at the saved spot, with yaw/pitch restored onto `InputState` (the look-direction source of truth both camera systems rebuild from each frame). `save.info` now prints the pose; +3 binary tests (flycam round-trip, character body-tracking, snapshot survival). Save/Load is feature-complete. | M40 (world streaming dictates what to serialize) |

### Tier 5 — Renderer polish (quality, not capability)

Each of these buys 10–30% visual quality but no new feature. Keep
active for incremental wins; don't let them block Tier 1–4.

| #       | Milestone             | Scope                                                                                                                                                                                                                                                                                                                                                                                         | Depends on |
|---------|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| ~~R1~~  | ~~MaterialTable refactor~~ | **Closed (2026-05-01)** across 6 phases (`aa48d64`..`22f294a`). `GpuMaterial` (272 B std430) + `MaterialTable` with byte-level dedup; per-frame `MaterialBuffer` SSBO at scene set 1 binding 13. Every per-material read in `triangle.frag` + `ui.vert` migrated to `materials[gpuInstance.material_id]`. `GpuInstance` collapsed **400 → 112 B (72% reduction)**, dropping ~30 fields (PBR / texture indices / alpha state / POM / UV transform / NiMaterialProperty diffuse-ambient / Skyrim+ shader-variant payloads / BSEffect falloff). Two intentional deferrals (filed as R1-followup): caustic compute path still reads `avg_albedo` off its own descriptor set (set 0); `DrawCommand` still carries the legacy per-material fields consumed by `to_gpu_material`. M38 is unblocked. | —          |
| M35     | Terrain LOD (Skyrim+/FO4 prebaked LOD landed) | **Object `.bto` shipped** (Session 45, EXAL step 6) + **terrain `.btr` shipped** (2026-06-19, `cell_loader/terrain_lod_btr.rs`): prebaked per-quad distant meshes load for Skyrim+/FO4 as a source upgrade inside the existing streaming ring; heightmap synth stays the universal fallback. Level-4 `.btr` quads align 1:1 with the 4-cell synth blocks — `spawn_lod_block` picks `.btr` (textured per-quad) for fully-distant blocks (`hole_mask == 0`), synth for boundary/missing/older-game blocks (one block per coord → no z-fight). Live-verified Skyrim Tamriel: 544 `.btr` / 30 synth, 0 errors. Real-data finding: `.btr` is quad-local-normalized (not world-absolute like `.bto`) — placement scales the footprint by LOD level + offsets to the SW corner, heights absolute (`btr_local_to_world`, unit-tested). **LOD atlas texturing fixed** (2026-06-19): the object atlas + `.btr` diffuse live in `Textures7.bsa` and the meshes in `Meshes1.bsa`, which the numeric-sibling auto-loader skipped (bailed on any digit suffix) → distant LOD rendered untextured. Fixed by treating a `…0`-suffixed archive as Skyrim's zero-based series start (`asset_provider::numeric_sibling_paths`, unit-tested): passing just `Meshes0` + `Textures0` now auto-opens `Meshes1` + `Textures1..8`, texturing distant terrain + objects with no explicit archive list. **Oblivion/FO3/FNV `_far.nif` distant-object LOD shipped** (Session 52, #1726): `DistantLOD\*.lod` → `_far.nif` placement scheme + full-model-path fallback, real Oblivion distant-terrain LOD textures with the bogus synth-LOD suppressed (#1745), default full-detail radius extended to 12. **Remaining**: distance-based multi-band selection (8/16/32 — both LODs load level 4 only; couples to far-plane extension + reversed-Z depth); `.btr` `_n.dds` normal maps. Gameplay-relevant half is world streaming (M40); pure LOD is quality.                                                                                                                                                                                                                                        | M32        |
| ~~M37~~ | ~~SVGF spatial filter~~ | **Closed (2026-06-18, #1662, `6b061120`).** À-trous wavelet spatial pass (Schied 2017 §4.3) consuming the per-frame moments buffer the temporal-only denoiser never read; adds a spatial variance estimate so converged-but-noisy GI regions also filter. Shipped in the RT denoiser overhaul; `DBG_DISABLE_ATROUS 0x4000` for A/B.                                                                                                                                                | —          |
| M37.3   | ReSTIR-DI              | Full spatiotemporal reservoir reuse. Drops shadow rays to 1/pixel while sampling hundreds of lights. Streaming-RIS already shipped as M31.5. **Phase 1 landed 2026-06-02** (`9abbe510`): reservoir data structs + initial sampling path wired into `triangle.frag` + `VulkanContext`. **Phase 2 temporal landed 2026-06-18** (#1662, `6b061120`): per-pixel reservoir SSBO (scene set bindings 16/17, ping-pong per frame-in-flight, `vulkan::restir`) + EMA accumulation of shaded radiance for direct soft shadows, 4 shadow rays/frame; the interim Phase-1 G-buffer reservoir attachment was removed (`218b425b`). **Spatial resampling pass remains.** | M31.5, M37 |
| ~~M38~~ | ~~Transparency & water~~ | **Water shipped (2026-05-11, `2ee1c68`).** ECS `WaterPlane` / `WaterVolume` / `SubmersionState` components, dedicated `WaterPipeline` (vertex displacement + Fresnel), RT reflection + refraction rays against TLAS, exterior cell loader detects water-plane refs and spawns geometry, camera submersion state writes through `submersion_system`. OIT / depth-peeled transparency for non-water alpha-blend remains future work (filed under M38.2 if/when a real workload demands it). | ~~R1~~     |
| M39     | Texture streaming      | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Memory budget with LRU eviction.                                                                                                                                                                                                                                                                              | —          |
| M29.3   | Pre-skinned raster path | Phase 3 of the GPU pre-skinning arc (`SkinComputePipeline` + per-skinned-entity BLAS refit shipped in `1ae235b`, RT shadows / reflections / GI now see this-frame skinned pose). Migrate `triangle.vert:147-204` to read pre-skinned vertices from the per-skinned-entity `SkinSlot` output buffer rather than doing inline weighted-bone-matrix-sum. The same commit must re-add `VERTEX_BUFFER` to the output buffer's usage mask — dropped in `#681` (`MEM-2-6`) so deferred-Phase-3 doesn't bloat memory-type masks today. Single source of truth, drops ~50 ALU ops per skinned vertex, but adds a critical-path dependency on the compute pass: a failed slot would now break raster too. **Defer-rationale:** the rasterized skinning path is well-understood and tested on real content; the new compute path is not. Ship only after the M41 NPC-spawning rollout proves the compute + BLAS-refit chain stable on visible animated content. | `1ae235b`, M41 stable, `#681` re-add |
| ~~M-NORMALS~~ | ~~Per-vertex tangents~~ | **Closed (2026-05-02)** — commits 91e9011 (decode `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` for Skyrim+/FO4) + 82a4563 (`synthesize_tangents` — Rust port of nifly's `CalcTangentSpace` per-triangle accumulator for FO3/FNV/Oblivion content that ships without authored tangents). Vertex stride 84 → 100 B (`tangent: [f32; 4]` at offset 84 / location 8 / RGBA32_SFLOAT); `triangle.vert/frag`, `ui.vert`, `skin_vertices.comp` updated in lockstep. `perturbNormal` re-enabled with `vertexTangent.xyz`-driven Path 1 (TBN from authored / synthesized tangent + `sign × cross(N, T)` bitangent reconstruction) and screen-space-derivative Path 2 fallback for content with neither authored nor synthesizable tangents. See [#783](https://github.com/matiaszanolli/ByroRedux/issues/783). | NIF parser |
| ~~LIGHT-N2~~ | ~~Display-space fog blend~~ | **Closed (2026-05-02, commit 18bbeae)** — composite fog mix moved from HDR-linear pre-ACES to display space post-ACES, removing the residual interior yellow/sepia distance wash on far interior surfaces. ~10-line `composite.frag` change. See [#784](https://github.com/matiaszanolli/ByroRedux/issues/784). | ~~M-NORMALS~~ |
| ~~EFFECT-LIT~~ | ~~Effect shader intensity control~~ | **Closed (2026-06-03, commit `ea044b68`).** `BSEffectShaderProperty.lighting_influence` (0–255 byte) packed into `GpuMaterial.material_flags` bits 16–23 via `pack_effect_shader_flags`. `triangle.frag` EFFECT_LIT branch: `liScale = float((materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFF) / 255.0` gates the scene-lit contribution (lighting_influence = 0 → fully unlit effect; 255 → prior behavior). Magic auras and power-armor glows with low authored `lighting_influence` no longer over-lit in bright scenes. Constant `EFFECT_LI_SHIFT = 16` propagated via `shader_constants_data.rs` + `build.rs` auto-gen — no hand-sync hazard. Lockstep test + `shader_constants.glsl` updated. | R1 |

### Tier 6 — Engine infrastructure (enablers)

| #       | Milestone                           | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Depends on     |
|---------|-------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------|
| ~~R7~~  | ~~Scheduler access declarations~~   | **Closed.** `Access` builder + `System::access()` opt-in declaration + `Scheduler::add_to_with_access` registration-side override + `access_report()` per-stage conflict analysis (`None` / `Conflict { pairs }` / `Unknown`). Snapshot stored as `SchedulerAccessReport` resource and surfaced via the `sys.accesses` console command. Current state on the engine binary: 12 systems registered, 3 declared (`fly_camera_system` / `spin_system` / `log_stats_system`), 9 undeclared, 0 known conflicts, 4 unknown pairs. M27 can now flip on with diagnosable contention; further system migrations driven by `sys.accesses` output. | —              |
| ~~M27~~ | ~~Parallel system dispatch~~        | **Closed 2026-05-23** (`a9810d40`, `05fe2bac`). Phase 1+2: 12 parallel-stage systems now declare reads/writes via `add_to_with_access` with explicit `Access::new().reads::<T>().writes::<U>()` chains at the registration site. Phase 3: 4 runtime-mutually-exclusive systems re-staged as exclusive (audio + spin + character-mode dispatcher + the new `player_controller_system` that branches PlayerMode → fly_camera vs character_controller). `sys.accesses` console command now reports **0 unknown / 0 conflicts** (was 13 unknown + 4 conflicts before the migration). Parallel-scheduler feature flipped on. R7's `sys.accesses` infrastructure (closed two sessions ago) walked us through the diagnosis; M27 walked the actual migration. | R7             |
| ~~M28.5~~ | ~~Character controller~~          | **Closed 2026-05-22** across 5 commits (`65adad60`, `610b6ae0`, `75474e71`, `a787c0d2`, `826d3cfb` + companion lifecycle `525c690c` / `52b99dda` / `d89bd5aa` + void-fall fix `cfc52e1c`). Rapier3D `KinematicCharacterController` with gravity + collide-and-slide + jump + autostep replaces fly-cam-only on-foot movement. Vanilla-Skyrim capsule size (1.8 m × 0.35 m radius) with dynamic-AABB rescale tracking animation pose; downward ray-cast spawn snaps the player to the first hit at cell-load anchor; #1230 void-fall slip-through closed via 1m cylindrical probe sweep before falling back to fly-cam. Walk/fly toggle on `T`, FlyCam fallback when cell load fails, door-teleporter spawn for un-doored test cells, `TriMesh::FIX_INTERNAL_EDGES` for collision-mesh quality. Companion: `ContactConfig + CharacterKinematic` unifies the three previously-duplicated collider spawn paths; Vulkan 1.3 `synchronization2` enabled; dangling `VARIABLE_DESCRIPTOR_COUNT` flag dropped. | M28, M32       |
| **R2** Phase A+B | ~~ESM sub-record cursor + migration~~ | **Phase A + B closed 2026-05-24.** Phase A (cursor primitive `SubReader`, 437 LOC, strict + lenient reads + `f32_array` + `rgb_color`/`rgba_color`) had landed prior. Phase B (call-site migration) closed today: all 169 legacy `read_*_at` / matching `from_le_bytes` field-read sites across 15 ESM record files migrated to sequential `SubReader::new(&data)` decoders. Files touched (sites): `records/actor.rs` 32, `records/weather.rs` 28, `records/misc/water.rs` 18, `records/misc/world.rs` 14, `records/misc/effects.rs` 12, `records/script.rs` / `records/misc/equipment.rs` / `records/misc/ai.rs` / `records/misc/magic.rs` (35 combined), `records/{container, common, tree, global, climate, outfit, list_record, items}.rs` (long tail). Behavior preserved across the SCHR-flags Oblivion-vs-FO3+ dead-branch (intentionally kept the original truncation semantics). Dead `read_u32_at` / `read_u16_at` / `read_i16_at` / `read_f32_at` helpers dropped from `records/common.rs` (Phase 8). Validation: full workspace `cargo test --workspace` = **2493 passed / 0 failed / 107 ignored**; the two ignored Oblivion real-data parity tests (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`) re-run green against vanilla Oblivion.esm. **Phase C remaining** (not blocking M24.2): typed `read_sub::<T>` schema API with compile-time layouts — `Decode`/`Subrecord` trait + per-record-type struct shapes. The cursor + sequential-read shape that Phase B locks in is the foundation; Phase C is the further layer if M24.2's QUST / DIAL / INFO / PERK / MGEF / SPEL / ENCH / AVIF surface justifies it. **M24.2 unblocked.** | —              |
| M24.2 Phase 1a+1b | ESM Phase 2 — QUST stages + PERK entries | **Started 2026-05-24.** **Phase 1a (QUST):** stage + objective decoding ([`crates/plugin/src/esm/records/misc/ai.rs`](crates/plugin/src/esm/records/misc/ai.rs) `parse_qust`). Block-state walker decodes `INDX`/`QSDT`/`CNAM`/`SCHR` stage blocks (index + flags + log text + `has_script` marker) and `QOBJ`/`CNAM`/`NNAM`/`QSTA` objective blocks (index + text + target form-id list); `start_up_stage` derived from the first stage carrying QSDT bit 0. 4 new tests. **Phase 1b (PERK):** PRKE/PRKF block-state walker ([`crates/plugin/src/esm/records/misc/magic.rs`](crates/plugin/src/esm/records/misc/magic.rs) `parse_perk`) decodes all 3 entry types per `perk_system.md`: `PerkEntryBody::Quest { quest_form_id, stage }`, `Ability { spell_form_id }`, `EntryPoint { entry_point_index, function_type, function_data, formatter, extra_flags }`. PRKE-internal `DATA` dispatches per `entry_type` (Quest=8B, Ability=4B, EntryPoint=4B); EPFT can override the EntryPoint `function_type`; EPFD/EPF2/EPF3 captured raw for the consumer-side function dispatcher. PERK record-level DATA header expanded from byte-0-only to full 5-byte FO3/FNV layout (trait + level + num_ranks + playable + hidden). 6 new tests including multi-entry authoring-order preservation + unclosed-block silent-drop. Plugin suite **399 → 410**. **Phase 2 closed (2026-06-03, `45509f4f`)**: ~~MGEF full effect struct + flags~~, ~~SPEL/ENCH EFID/EFIT chain~~, ~~AVIF PERK list lookup~~, ~~QUST per-stage CTDA~~, INFO records all semantically decoded. **Remaining**: per-`function_type` EPFD decode (f32 / range / FormID / lstring), per-entry CTDA conditions on PRKE blocks, DIAL full conversation tree. | R2             |
| ~~M30.2~~ | ~~Papyrus Phase 2–4~~              | **Closed 2026-05-23** (`ab0eee96`). Filled the gap between M30 Phase 1 (lexer + Pratt expression parser) and full `.psc` parse: statement parser (`parser/stmt.rs`: Return, If/ElseIf/Else/EndIf, While/EndWhile, local VarDecl with speculative-type disambiguator, expr-stmt, assignment with compound operators), top-level item parser (`parser/script.rs`: ScriptName + Extends header, Property short + full forms with six PropertyFlags, Function typed/untyped + four FunctionFlags incl. Native bodyless form, Event, Auto State / State, Struct, CustomEvent, Group, Variable, Import), public `parse_script` driver with per-item error recovery, doc-comment skipping at item boundaries. Load-bearing finding: `Parser::peek()` silently skips Newlines, so Return-with-vs-without-value detection needs `peek_raw()` — fixed across stmt.rs. All four R5 source scripts (`defaultRumbleOnActivate`, `DA10MainDoorScript`, `MG07LabyrinthianDoorScript`, `DLC2TTR4aPlayerScript`) round-trip end-to-end with zero recovered errors; tests in `crates/papyrus/tests/r5_round_trip.rs` assert structural shape (item counts + names + key flags). Test deltas: 12 stmt + 10 script + 4 round-trip = +26 new tests, papyrus crate at 70 tests. FO4 extensions (`Const`, `Hidden`, `Mandatory`, `BetaOnly`, `DebugOnly`) land as flag tokens decorating existing items — no separate grammar. Semantic validation + doc-comment threading on non-doc-aware items deferred to M47.2. **Unblocks M47.2 (Papyrus transpiler).** | M30            |
| ~~M46.0~~ | ~~Multi-plugin CLI~~              | **Closed** via #561. Repeatable `--master <path>` CLI arg + `load_cell_with_masters` (interior) / `build_exterior_world_context` + `load_one_exterior_cell` (exterior) entry points. Each plugin's TES4 master_files header drives a per-plugin `FormIdRemap` so cross-plugin REFRs land in the merged `EsmIndex` under their global FormIDs. Last-write-wins on key collisions (canonical Bethesda load-order semantics). `EsmIndex::merge_from` + `EsmCellIndex::merge_from` carry the merge across the 30+ record-type maps. The unresolved-REFR diagnostic now names the missing plugin instead of silently rendering empty. Usage: `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01`. | #445 (done)    |
| ~~R3~~  | ~~NIF per-block-type parse histogram~~ | **Closed.** `nif_stats --tsv` emits a per-header-type `parsed` vs `NiUnknown` histogram; `crates/nif/tests/per_block_baselines.rs` integration test (opt-in via `cargo test -- --ignored`) compares against checked-in TSV baselines for all 7 games and fails on any `unknown` growth or `parsed` shrinkage. `BYROREDUX_REGEN_BASELINES=1` regenerates after intentional changes. Oblivion baseline refreshed 2026-04-26 to track the post-session-18 truncation drift surfaced by the audit (#687/#688/#697, all now CLOSED); R3's job was to surface the drift, not fix it — the underlying issues were separately resolved. Today the gate runs as a manual `cargo test … -- --ignored` invocation — there is no GitHub Actions pipeline yet, so "fail CI on regression" is the test's *contract* rather than an enforced workflow. | —              |
| **M-EXAL** | EXAL — Exterior Abstraction Layer | NIFAL-mirror for exterior content (terrain / sky / sun / weather / water / LOD / distant objects). Design doc at [`docs/engine/exal.md`](docs/engine/exal.md) (introduced Session 45, `00e38caa`). Mirrors the NIFAL principle: per-game exterior data translates to canonical `ExteriorEnvironment` structs at parse time; no per-game branches leak into the renderer. Key gap: **distant object LOD** — `.btr` terrain LOD mesh + `.bto` object LOD passthrough currently bypasses any canonical layer; the M35 row covers the render side but not the abstraction boundary. Priority order within EXAL: terrain LOD (M35 feeds this) → sky/weather (M33/M34 already done, EXAL captures their outputs) → water (M38, done) → REGN region ambient (M44 pending). **Not blocking Tier 1–4** — existing exterior cells render without it. Positions ByroRedux to accept third-party exterior mods cleanly when M50 ships. | NIFAL, M35, M40 |

### Tier 7 — Deep gameplay systems (deferred until Tier 1–4 proves out)

| #       | Milestone                    | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Depends on                                      |
|---------|------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| M42     | AI packages                  | 30 composable procedures, package stack, Sandbox. Patrol paths from NAVM. Basic wander/follow/travel. **#446 closed** (`90e6b068`) — PACK dispatches with PKDT (flags/procedure type) + PSDT (schedule) + **PLDT (location, `7d5e91db`, 2026-07-14)** decoded, verified against real FalloutNV.esm. `SandboxBehavior`/`Seated` ECS components + `sandbox_seat_system` ship a v0 "sit in nearest free chair, once" behind `BYRO_SANDBOX_SIT`, now using each package's authored PLDT radius (`17d55414`) instead of a blanket guess. **Investigated and deprioritized (2026-07-14):** resolving `PackLocationTarget::NearReference` to a live entity's position for the search *center* — only ~12% of vanilla FNV NearReference packages resolve to anything spawnable (most target either an unloaded cell or the hardcoded XMarker family `cell_loader` never spawns); v0 keeps the actor's-own-position center approximation. Still open: PTDT/PTD2 (target data, the real FO3/FNV tag — ROADMAP previously called this "PKTG", which is a Skyrim+-only name), package CTDA conditions, non-Sandbox procedure runtimes (Follow/Escort/Guard/Patrol/…), sit-enter/loop animation gap (see `fnv_furniture_sit_needs_transition` follow-up).                                                                                                                                                                                                                                                         | M28.5, M41                                      |
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
GPU-bound on RT cost on FNV / Skyrim interiors (`fence=6.12 ms / 75% wall`
on Prospector, `fence=2.09 ms` on Whiterun, post-#1115 refactor 2026-05-17)
but **CPU-bound on FO4 MedTek** (`brd_ms=6.96 ms ≈ fence=6.11 ms` at
10 810 entities — fence and brd now nearly co-dominate; the gap closed
slightly thanks to #1136 + #1115's orchestrator split) —
`build_render_data` walks every entity per frame across 8 sibling
sub-modules (post-#1115 TD9-001 split: `static_meshes` / `skinned`
/ `particles` / `lights` / `sky` / `water` / `camera` + the
orchestrator), scaling linearly with cell complexity. M52 (GPU-driven rendering) is the
explicit ceiling raiser for that regime; not on the active path yet,
but the data point now exists. Split out from Tier 8 (2026-05-08)
once Tier 8 was reframed around visual fidelity — these are honest
ceiling raisers, not beauty work, and conflating the two muddied both.

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
| NIF block dispatch                           | `Box<dyn NiObject>` over ~250 dispatch arms (live count in source) | Enum dispatch would cost more in maintenance than it gains in perf at these branch counts. Keep.                                                                                                                       |
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
- **R2** — ESM typed subrecord decoder. **Phase A + B closed 2026-05-24** (cursor primitive + 169-site migration; legacy `read_*_at` helpers dropped). Phase C (typed `read_sub::<T>` schema layer) deferred — not blocking M24.2.
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

**Tripwire today** (re-measured 2026-06-03): the old `esm/cell.rs`
tripwire is resolved — Session 35 split it into the
`crates/plugin/src/esm/cell/` directory (`mod.rs` / `helpers.rs` /
`support.rs` / `walkers.rs` / `wrld.rs` + per-topic `tests/`), so no
single file in that walker exceeds threshold. The current largest
source file is `crates/renderer/src/vulkan/context/draw.rs` at **3 486
lines** (per-frame command recording) — **over the 3 500 line threshold
and approaching it**; `vulkan/context/mod.rs` (3 064) and
`byroredux/src/asset_provider.rs` (2 781) follow. `draw.rs` grew via
ReSTIR-DI Phase 1 plumbing — **split investigation warranted before M37.3
Phase 2 lands more plumbing there**. R2 (typed subrecord
decoder, Phase A+B closed) shipped before M24.2 started, as planned.

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
N23.10 test infrastructure. **The block dispatcher now carries ~250
match arms (parsed types + Havok skips); the live count is in
`crates/nif/src/blocks/mod.rs` — not frozen here.**

**Asset pipeline**
M9 NIF parser · M10 NIF→ECS import · M11 BSA reader ·
M14 DDS texture loading · M16 ESM parser & cell loading ·
M18 Skyrim SE NIF · M19 full cell loading · M26 BA2 archive
support (v1/v2/v3/v7/v8, zlib + LZ4) · NIFAL (NIF Abstraction Layer,
2026-05-28) — canonical parse-time translation boundary, first slices
material / particle / collision; surface audit complete 2026-06-02
(NiRangeLODData last block, lod_group on ImportedNode) ·
**M49** FO4 precombined geometry (CSG reader, 2026-06-02, closes #1351).
Per-game
clean-parse rates in the compat matrix above; recoverable rate at
100% across all seven games except Oblivion's single hard-fail
(#698, closed).

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
- [x] ~~No world streaming — entire cell re-imported from scratch on every load (M40)~~ — **closed 2026-05-24 via M40 row audit**. `WorldStreamingState` + async cell-pre-parse worker + LRU BLAS eviction + interior↔exterior cell-swap all live; verified by code inspection of [`byroredux/src/streaming.rs`](byroredux/src/streaming.rs), [`byroredux/src/main.rs`](byroredux/src/main.rs), [`byroredux/src/cell_loader/unload.rs`](byroredux/src/cell_loader/unload.rs), and [`crates/renderer/src/vulkan/acceleration/blas_static.rs`](crates/renderer/src/vulkan/acceleration/blas_static.rs). The previous "single-cell-at-a-time" framing on the M40 row was stale wording from before Stage 3b landed.
- [x] ~~BSA v103 (Oblivion) decompression not working~~ — **stale premise, closed via #699**. v103 archive opens AND extracts cleanly: 147 629 / 147 629 vanilla files across all 17 Oblivion BSAs (2026-04-17 + 2026-04-25 sweeps); `nif_stats` round-trips 8032 NIFs through the v103 path. The real Oblivion exterior blocker is TES4 worldspace + LAND wiring (same shape FO3 was) — already covered by the M40 / M41 / "exterior renderer" Tier-1/2 plan, no separate tracker needed.
- [x] Skyrim + FO4 cells not wired through `cell_loader` — **closed M32.5**, both render end-to-end

### Open — Tier 3 / 4 gaps

- [ ] 1 257 FO3 SCPT records parsed; no runtime executes them (M47.0)
- [x] ~~No audio subsystem of any kind (M44)~~ — **closed 2026-05-06**. `byroredux-audio` crate scaffolded on kira `0.10` (Phase 1), BSA → symphonia decode + `SoundCache` (Phase 2), spatial sub-tracks + per-emitter `SpatialTrackHandle` (Phase 3), `play_oneshot` queue + `FootstepEmitter` + XZ-plane stride `footstep_system` (Phase 3.5), looping ambient via `loop_region` + tweened stop on `AudioEmitter` despawn (Phase 4), streaming music via `StreamingSoundData` + `play_music` / `stop_music` (Phase 5), global reverb send + per-cell `set_reverb_send_db` (Phase 6). Pending: FOOT records → per-material lookup (drops dirt hardcode), REGN region-keyed ambient layers, per-cell-load reverb-toggle wiring (API ships, cell loader doesn't call yet), raycast-occlusion attenuation. See M44 row in active milestones.
- [x] ~~No save/load — playtest iterations require cold cell re-load (M45)~~ — **M45 + M45.1 landed 2026-06-21** (`bd2d0de2` library + `48e18c4f` live load, branches `feat/m45-save-load` / `feat/m45.1-live-load`). `crates/save`: full-snapshot save (validate + atomic-write + ring), `save.info` verify, and live `load <slot>` (reload saved cell + overlay FormId-keyed game-state deltas). Save round-trip + delta-reroute headlessly tested. **Open refinement**: precise player/camera-pose restore (load currently lands at cell default); full cosave/original-engine compatibility remains out of scope per design.
- [ ] `PACK` (AI packages) records have stubs only — no evaluator (#446, M42)

### Open — Risk-reducers (2026-04-22)

- [x] ~~**R1** DrawCommand has ~40 fields + 10 shader-variant payloads — collapse to `material_id` indirection (blocks M38)~~ — **closed 2026-05-01** across 6 phases (`aa48d64`..`22f294a`). `GpuInstance` collapsed 400 → 112 B (72% reduction); per-frame `MaterialBuffer` SSBO with byte-level dedup. M38 unblocked. Two follow-ups: caustic compute set 0 path + `DrawCommand` per-material field cleanup.
- [ ] **R2** ESM sub-record decoder is ad-hoc across 3 000+-line walkers — typed `read_sub::<T>` API (blocks M24.2)
- [x] **R3** NIF `NiUnknown` soft-fail masks per-block regressions — **closed**. `nif_stats --tsv` emits per-type `parsed` vs `unknown`; `crates/nif/tests/per_block_baselines.rs` (opt-in) compares against checked-in 7-game baselines and fails on any unknown growth or parsed shrinkage. Oblivion baseline refreshed 2026-04-26 against the audit-flagged truncation drift; `#687`/`#688`/`#697` (all CLOSED) were the underlying parser drift sources — R3 surfaced them, separate fixes resolved them.
- [ ] **R4** SWF/GFx strategic decision needed before M48 — Ruffle+GFx-stubs vs rewrite menus natively
- [ ] **R5** Papyrus full-runtime prototype on one real quest before M47.2 scope commitment
- [x] **R6** `VulkanContext` scratch buffers have no capacity telemetry — **closed**. `ctx.scratch` console command + `ScratchTelemetry` resource cover all 5 persistent scratches; per-frame refresh via `VulkanContext::fill_scratch_telemetry`. Prospector baseline: 337 KB total, 320 B wasted.
- [x] **R6a** Prospector re-bench — **closed**. 192.8 FPS / 5.19 ms at `e6e8091`, wall-clock bench.
- [x] **R6a-stale** Bench-of-record refreshed at `6a6950a` (2026-04-24). Prospector 172.6 FPS / 5.79 ms (was 192.8 / 5.19 — slight regression in compositor-jitter range; fence_ms unchanged at 4.34, GPU still the bottleneck). Skyrim Whiterun 253.3 FPS / 3.95 ms at 1932 entities (was 237 FPS at 1258 entities — entity count up 53% while FPS improved, indicating more REFRs land now without perf cost). FO4 MedTek 92.5 FPS / 10.82 ms (was 90, 7434 entities unchanged).
- [x] **R6a-stale-7** Bench-of-record refresh — **closed 2026-05-11 at HEAD `220e8e1`** (post M41 Phase 2 close-out). Prospector 133.5 FPS / 7.49 ms @ 2562 entities (was 172.6 / 5.79 @ 1200 entities at `6a6950a` — +114% entities, +29% wall_ms; sub-linear scaling consistent with RT cost amortising across the BLAS hierarchy). Skyrim Whiterun 217.3 FPS / 4.60 ms @ 3209 entities (was 253.3 @ 1932 — +66% entities, -14% FPS, sub-linear). FO4 MedTek 68.5 FPS / 14.61 ms @ 10 809 entities (was 92.5 @ 7434 — +45% entities, -26% FPS). Frame still GPU-bound on Prospector (fence=5.81 ms / 78% wall). Two M41-EQUIP changes drove most of the entity inflation: the Phase 2 scaffold spawning NPC inventory roots (`#896` A.0 → B.2) and the REFR Euler→Y-up composition fix (`Rx · Ry · Rz`, was `Rz · Ry · Rx`) which now lands every REFR through the corrected order. **Session 33 Markarth grid diagnostic stays as a separate snapshot, not a bench-of-record candidate** — it's a new workload class (Tier 8 indirect lighting + 1500+ mesh exterior grid) which the three steady-state interior benches don't measure.
- [x] **R6a-stale-8** Bench-of-record refresh — **closed 2026-05-16 at HEAD `c8519082`** (191 commits past `220e8e1`). Prospector 108.8 FPS / 9.19 ms @ 2563 entities, fence=7.04 ms / 77% wall (was 133.5 / 7.49 / fence=5.81 — **-18.5% FPS, +1.23 ms fence on effectively flat entity count**, regression diagnosed and fixed in the same session — see R6a-prospector-regress below). Skyrim Whiterun 205.9 FPS / 4.86 ms @ 3210 entities, 1650 draws (was 217.3 / 4.60 @ 3209 / 2052 draws — -5.2% FPS at the buggy state; post-fix back to 218.4 / 4.58, the apparent regression was pipeline-cache warmup noise). FO4 MedTek 66.2 FPS / 15.10 ms @ 10 810 entities, 7363 draws, brd_ms=8.23 (was 68.5 / 14.61 @ 10 809 / 8162 draws — -3.4% FPS at the buggy state; post-fix 67.1 / 14.91 / brd=8.07, **frame still CPU-bound on `build_render_data`** — that one is genuine and persists across the fix, see Tier 11 narrative). 191-commit window dominated by audit-bundle fixes (Renderer-D / NIF-D / tech-debt) but includes real shader work: TAA YCoCg variance gamma 1.25 → 1.5 (#1108), SVGF `frames_since_creation` per-FIF array (#964), SVGF NEAREST sampler binding 1 (#1085), BSGeometry UDEC3 tangents (#1086), WATR reflection_color propagation to shader (#1069), volumetric scattering=0 for interiors (#1084), volumetric froxel clear-to-(0,1) (#1082), gbuffer/caustic `initialize_layouts` (#1100), TLAS `built_primitive_count` (#1083), bloom doc correction (#1081), memory_barrier helper unification (#1061). Bench-of-record now post-fix HEAD; staleness tracker for the next cycle filed as R6a-stale-9 below.
- [x] **R6a-stale-9** Threshold tripped 2026-05-17 at Session 38 close (HEAD `c265032e`, 34 commits past `1775a7e6` — both threshold limbs exceeded: >30 commits *and* real shader / sync / perf changes landed). Notable post-bench commits since `1775a7e6`: 6 TOP_OF_PIPE → NONE Vulkan-barrier migrations (#1121 / #1122 / `a49eb945`), skin-path scratch cluster reorder + instance-buffer dirty-gate (#1133 / #1134 / `4f55b2f1`), queue MutexGuard held across `vkQueueSubmit` (CONC-D2-NEW-01 / `1608e6a2`), `MAX_BONES_PER_MESH` 128 → 144 to cover FO76 vanilla ceiling (#1135 / `835793c7`), FO4 BGSM/BGEM material-path normalisation drops MedTek `tex.missing` 12 → 6 (`91b03e6b`), `AccelerationManager::destroy` direct skinned-BLAS drain (#1138 / `ec9ef7c1`). FO4 work is mostly MedTek-only, but the queue MutexGuard + scratch cluster + 144-bone alloc and the 6 TOP_OF_PIPE → NONE migrations all hit Prospector's hot path. Re-run deferred to Session 39 ahead of the next workload change (R6a-stale-10 is the next tracker).
- [x] **R6a-stale-10** Bench-of-record refresh — **closed 2026-05-17 at HEAD `b5726a18`** (post #1115 8-step build_render_data refactor). Triggered by the threshold-exceeded condition at Session 38 close (R6a-stale-9) and by the #1115 hot-path refactor itself (per `feedback_speculative_vulkan_fixes.md` gating rule). Prospector 122.7 FPS / 8.15 ms @ 2563 entities, fence=6.12 ms / 75% wall (vs 124.6 / 8.03 / fence=6.17 at `1775a7e6` — **-1.5% FPS, within noise, frame still GPU-bound on RT cost**). Skyrim Whiterun 211.8 FPS / 4.72 ms @ 3210 entities, 1635 draws (vs 218.4 / 4.58 / 1641 draws — -3.0% FPS, within 5% gate, draws within 6 of baseline). FO4 MedTek 68.5 FPS / 14.60 ms @ 10 810 entities, 7371 draws, brd=6.96 ms (vs 67.1 / 14.91 / 7359 draws / brd=8.07 — **+2.1% FPS, slight improvement**, confirms #1136 FX-mesh spawn-time tagging held its win after the refactor). All three benches within the documented `<5%` gate; #1115 refactor passes bench validation. Whiterun is the closest-to-threshold (-3.0%); future investigation could narrow the variance if the gap grows, but for now within compositor-jitter range.
- [x] **R6a-stale-11** Bench-of-record refresh — **closed 2026-05-21 at HEAD `d0b52bd5`** (60 commits past `b5726a18`, post #1194 GPU timer follow-up fix). Triggered by the threshold-exceeded condition at Session 40 close (57 commits + M29.5/6 + spawn-site plumbing + reset_fences reorder + shader-include flag routing). **Refresh surfaced two issues**: (a) the original #1194 implementation hung Whiterun on frame 2 because `get_query_pool_results` with `WAIT` flag blocks indefinitely on unwritten TIMESTAMP queries (Prospector worked because all three brackets fired every frame; Whiterun's BannerredMare cell has six named NPCs that all land in first-sight on frame 1 with no refit, so the BLAS_REFIT bracket never wrote that frame, hanging frame 2's read) — fix at `d0b52bd5` switches to per-bracket reads gated on the matching active_bits bit. (b) The prior Skyrim repro command listed only `Textures0/1/2.bsa` but SE ships through Textures8 — the visual A/B showed magenta-checker on furniture / rugs that the headless bench summary couldn't catch. **Post-fix numbers**: Prospector 120.7 FPS / 8.28 ms / fence=6.37 (-1.6% vs 122.7 / 8.15 / 6.12 baseline, within 5% gate); Skyrim Whiterun 211.0 FPS / 4.74 ms / fence=2.12 / 1648 draws (-0.4% vs 211.8 baseline, within noise); FO4 MedTek 67.9 FPS / 14.72 ms / brd=7.10 / 7364 draws (-0.9% FPS / +0.14 ms brd vs 68.5 / 14.60 / brd=6.96 baseline, frame still CPU-bound on `build_render_data` per Tier 11 narrative). The ~+0.25 ms Prospector fence cost is the per-frame TIMESTAMP-query overhead introduced by #1194's instrumentation — acceptable price for the measurement infrastructure unblocking #1195/#1196/#1197. **First confirmed GPU-pass numbers**: Prospector skin_dispatch=0.029 ms / blas_refit=0.675 ms / taa=0.053 ms; Whiterun blas_refit=0.000 (NPCs in first-sight, no refit pose) / taa=0.053; MedTek same (FO4 humanoid skeleton.nif still unresolved). The 0.7 ms blas_refit cost on Prospector is the measurable baseline for #1196 PERF-DIM7-02 conditional-refit work.
- [x] **R6a-stale-12** Bench-of-record refresh — **closed 2026-05-24 at HEAD `a9bbe8d1`** (113 commits past `d0b52bd5`). Triggered by the threshold-exceeded condition at Session 41 close + the #1195/#1196/#1197 skin-chain dirty-gate work shipping this session. **Headline: all three benches far exceed the 5% gate in the WIN direction — biggest perf bump in months.** Prospector **161.4 FPS / 6.19 ms / fence=2.62** @ 2564 entities / 812 draws (was 120.7 / 8.28 / fence=6.37 — **+33.7% FPS / −25.2% wall_ms / −58.9% fence_ms**). Skyrim Whiterun **287.8 FPS / 3.47 ms / fence=0.71** @ 3211 entities / 1593 draws (was 211.0 / 4.74 / fence=2.12 — **+36.4% FPS / −26.8% wall_ms / −66.5% fence_ms**). FO4 MedTek **102.1 FPS / 9.79 ms / brd=7.81 / fence=0.44** @ 10 913 entities / 7363 draws (was 67.9 / 14.72 / brd=7.10 — **+50.4% FPS / −33.5% wall_ms**, +0.71 ms brd overhead overwhelmed by GPU win). **Per-pass GPU timer confirms the headline cause**: Prospector skin_dispatch 0.029 → **0.000** ms and blas_refit 0.675 → **0.001** ms (NPCs idle this frame; the #1195+#1196 paired bone-pose dirty gate skips both); TAA holds at 0.050 ms (within noise). Whiterun and MedTek skin numbers stay at 0.000 (NPCs first-sight or unresolved skeleton respectively) but still benefit massively from #1259 blend-pipeline pre-pop fast path + #1260 off-frustum flag-assembly skip on the rasterization side. Other contributing landings since `d0b52bd5`: M27 declared-access scheduler re-stage, #1147 PBR/SSS/model-space-normals gating, #1160 DST-side BOTTOM_OF_PIPE → NONE migration, #1159 SVGF nearest-tap bit-31 mask, #1198 MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME 16 → 227, #890 greyscale-to-palette LUT consumer, today's `8b5d77c1` sun-sprite mip 0 force. **Bench-of-record advances to `a9bbe8d1`** — Prospector 161.4 FPS becomes the new R6a baseline.
- [x] **R6a-stale-13** Bench-of-record refresh — **closed 2026-05-28 at HEAD `4e2ebe8c`** (125 commits past `a9bbe8d1`). Numbers + repro in the Bench-of-record section above. **The Session-42-close prediction was half-right and undershot.** Whiterun held *and improved* (287.8 → **329.8 FPS, +14.6%** at flat 3211 entities — the steady-state hot path did not regress). But the #1294 collider-gate prediction ("may shift MedTek slightly") badly understated the effect and missed FNV entirely: **both FNV Prospector and FO4 MedTek grew their entity counts ~40%** (Prospector 2564 → **3507, +37%**; MedTek 10 913 → **15546, +42%**) because FNV/FO4 architecture lacks authored `bhk` collision and now spawns more synthesized static-trimesh colliders (each adding RT BLAS). Prospector FPS fell 161.4 → **71.4 (−56%)** with `fence` 2.62 → **11.65 ms** (super-linear — the synthesized-collider BLAS dominates RT cost on this glass-heavy cell); MedTek fell 102.1 → **90.7 (−11%)** but its `brd_ms` *improved* 7.81 → **2.63** (CPU win held; GPU now pays for the bigger scene). Skyrim, with real `bhk` collision, neither grew nor regressed — the control. **Method gotcha caught: bare `--bsa` names resolve against CWD, not the `--esm` dir — benches must run from each game's `Data/` directory or the scene loads near-empty (Prospector 36 ent / 3 meshes / spurious 1792 FPS). Repro table + CWD note updated.** Bench-of-record advances to `4e2ebe8c`.
- [x] **R6a-stale-13-collider-cost** (follow-up filed by the refresh above): **Closed 2026-06-03.** Root cause confirmed: synthesized static-trimesh colliders (from the F3 fallback gate, commit `15016ee0` / #1294) received `MeshHandle` unconditionally, flowing through `collect_static_mesh_draws` with `in_tlas=true` by default. Each synthesized collider entity added a full RT BLAS entry — invisible physics proxies paying the same RT cost as visible geometry. Fix: added `IsCollisionOnly` marker component to `byroredux/src/components.rs`; tagged in `cell_loader/spawn.rs` after `synthesize_static_trimesh`; `static_meshes.rs` forces `in_tlas = false` for tagged entities (matches the existing `IsLodTerrain` exclusion pattern). Entity count on FNV/FO4 cells is unchanged — synthesized colliders still spawn for physics — but they no longer enter the BLAS, eliminating the super-linear `fence_ms` growth. Bench re-run required to confirm FPS recovery (see R6a-stale-14).
- [x] **R6a-stale-14** (filed 2026-06-01): **Closed 2026-06-03 at HEAD `1c26bc25`.** Three-scene bench run (300 frames each, from each game's `Data/` dir). Results: Prospector 76.2 FPS / fence=11.12 ms / 3516 ent (+6.7% FPS, fence −4.6% vs R6a-stale-13); Whiterun 362.8 FPS / fence=0.98 ms (+10.0% FPS, steady-state hot path confirms no regression); MedTek 65.2 FPS / fence=9.03 ms / 21414 ent (FPS −28%, entity +38%, draw +75% — all from M49 CSG precombined geometry landed Session 45, not a regression). `IsCollisionOnly` fix confirmed to reduce TLAS instances on FNV cells; full recovery of Prospector fence to pre-collider 2.62 ms pending further investigation (entity count unchanged, BLAS still built per mesh-handle). Open follow-up: see new R6a-stale-14-collider-partial below.
- [x] **R6a-stale-14-collider-partial** (filed 2026-06-03): **Closed** (this session). Root cause analysis: the bhk-authored collision path already creates **separate, `MeshHandle`-free ghost entities** (lines 479-487 of `cell_loader/spawn.rs`), so bhk colliders never enter BLAS/TLAS. The synthesized trimesh path (F3 fallback — FO4/Starfield architecture) incorrectly piggybacked `CollisionShape + RigidBodyData + IsCollisionOnly` onto the render entity instead of following the bhk pattern. This gave those entities a BLAS entry and then excluded them from TLAS — wasting GPU memory on unused BLAS builds while also removing visible architecture from RT shadows/GI. Fix: synthesize path now spawns the same ghost-entity shape as the bhk path (`world.spawn()` → `Transform + GlobalTransform + CollisionShape + RigidBodyData`, no `MeshHandle`). Render entity is untouched — enters BLAS+TLAS normally. `entities` command extended with `physics-only (no MeshHandle)` count and `IsCollisionOnly (expect 0)` count so the next bench run can confirm. **R6a-stale-15 required** to measure the fence recovery and verify `IsCollisionOnly=0` in Prospector/MedTek. Note: the 2564→3516 entity count growth origin (the larger gap vs. the pre-collider baseline) remains unconfirmed — strong candidate is M41 Phase 2 NPC mesh additions (hair, eyebrow, eye meshes per actor: `740036f7`, `323e3a9c`, committed just after the 2564-entity baseline `a9bbe8d1`). That growth is legitimate scene content; the synthesized-collider BLAS waste is now resolved.
- [ ] **R6a-stale-15** (filed 2026-06-03): Ghost-entity rerouting for synthesized colliders closed R6a-stale-14-collider-partial — render entities now enter BLAS+TLAS normally. Requires a fresh 300-frame bench to confirm: (a) fence recovery beyond R6a-stale-14's partial result (76.2 FPS / 11.12 ms fence, vs pre-collider target 161.4 FPS / 2.62 ms @ ~2564 ent); (b) `IsCollisionOnly=0` in `entities` output for Prospector and MedTek. FO4 MedTek should also see fence improvement since its architecture uses synthesized colliders. **Now overdue (2026-06-22):** bench-of-record is **332 commits stale**; Session 47 churned the RT hot path heavily (Cornell glass/GI/caustics, camera-relative render origin, SVGF firefly, SSAO, TAA) and Session 49 landed the **RT denoiser overhaul** (#1662 — multi-scatter, SVGF à-trous spatial, ReSTIR-DI temporal shadows, PCG-hash sampling) directly on it — the re-bench should pick up the net effect of both arcs, not just the collider rerouting.
- [x] **REND-#1447** HIGH (filed 2026-06-02, `AUDIT_RENDERER_2026-06-02`): **Closed 2026-06-02** (`e6df0f5b`) — SPIR-V recompiled after DoF CameraUBO extension.
- [x] **REND-#1448** LOW (filed 2026-06-02, `AUDIT_RENDERER_2026-06-02`): **Closed 2026-06-02** (`f8e5daad`) — screenshot extent captured at record time, survives same-frame resize.
- [x] **BUILD-SFMATERIAL** (2026-06-03): **Closed 2026-06-03.** `ee727346` removed `pub use chunk::ChunkType` and broke `crate::StringTable` / `crate::ChunkType` in internal modules + integration test. Fixed: `ChunkType` re-exported from `lib.rs`; internal `reader.rs` and `error.rs` use module-local paths.
- [x] **BUILD-SCRIPTING** (2026-06-03): **Closed 2026-06-03.** `0785661d` referenced `World::try_get`, `FactionMembership`, `BaseFormId`, `PerkList` (none exist), wrong `esm::index::EsmIndex` path, and attempted `u32 → FormId` coercions. All non-working implementations reverted to their original trace-log stubs; the only surviving change is the `world` parameter on `resolve()` (inert but ready for future use).
- [x] **BUILD-RENDERER-TEST** (2026-06-03): **Closed 2026-06-03.** `9abbe510` (ReSTIR-DI Phase 1) added `RESERVOIR_FORMAT` / `GBuffer::reservoir_view()` / extra args to `create_render_pass` + `create_main_framebuffers` without updating `gbuffer.rs` or `helpers.rs`. Fixed: `RESERVOIR_FORMAT = R32G32B32A32_UINT` added to `gbuffer.rs`; `reservoir: Attachment` added to `GBuffer` (alloc / view / destroy / recreate / initialize_layouts); `create_render_pass` now takes `reservoir_format` (attachment slot 6, depth moves to 7); `create_main_framebuffers` now takes `reservoir_views`; `resize.rs` updated to match. Also fixed a companion type mismatch in `nif/import/walk/mod.rs` from `b4c453c7` (`zup_point_to_yup` takes `&NiPoint3` not `&[f32;3]` — inlined as `zup_to_yup_pos`).
- [ ] **BUILD-SFMATERIAL-NIFTEST** (2026-06-03): `byroredux-nif` lib tests previously also broke due to `b4c453c7` (`zup_point_to_yup` type mismatch) — **closed** by the same fix above.
- [~] **REND-#1451** MEDIUM (filed 2026-06-03, Lonesome Road Ulysses Temple + prior cells): Bright near-zone ring around player. **Root cause (confirmed against OpenMW reference 2026-06-04):** the shader used the anti-pop-in cull window as the *entire* attenuation — `atten = pow(clamp(1 − (d/R)², 0, 1), shape)` with `R = authored × LIGHT_RANGE_EXTENSION` — so at the authored radius (`d = R/2` under the 2.0 extension) it read **75%** instead of the intended ~10–30%, a bright disc fading to zero only at `2× authored radius`. The Bethesda/Gamebryo lineage (OpenMW `lcalcIllumination`, `reference/openmw/files/shaders/lib/light/lighting_util.glsl`) is **two multiplied terms**: a physical falloff `1/(c+l·d+q·d²)` (already ~30% at the authored radius — Morrowind's stock `linear = 3/r`) × a soft cull window that fades full→zero from `radius` to `2×radius` purely to kill pop-in. ByroRedux had dropped the physical term. NB the original entry's `2.5` was stale — `83d6a155` already moved it to `2.0` + fixed the compounding AMBIENT_FILL additive→max() (REND-#1452). **Fix landed (2026-06-04, code-side):** `triangle.frag::pointSpotAtten` now implements the two-term model — physical near-zone falloff keyed to the authored radius × soft `smoothstep` cull window to `R` — used by BOTH the pass-1 reservoir loop and `shadowableLightRadiance` (WRS bit-identical, #1369). `LIGHT_RANGE_EXTENSION` stays `2.0` as the cull boundary (OpenMW zeroes at exactly `2×r`). `DBG_LEGACY_LIGHT_ATTEN = 0x1000` restores the old window-only formula for A/B. **Remaining (needs user GPU + FNV data):** the controlled bench — run `--bench-hold`, attach `byro-dbg`, and sweep the live knee with `light.atten knee <0.05..1.0>` (and `light.atten legacy on|off`) on Ulysses Temple / Prospector to pick the final `kneeFrac` (default `0.5` ⇒ ~50% at authored radius for shape 1; expect ~0.3–0.4 to hit the 30% target), then bake it as the shader default. FO4 dense interiors may want a different value than FNV. Filed as #1451.
- [ ] **REND-#1449** LOW / latent (filed 2026-06-02, `AUDIT_RENDERER_2026-06-02`): `evict_unused_blas` immediate-destroy assumes no in-flight TLAS during multi-batch cell load — gated behind a future refactor. Tracked as #1449.
- [ ] **REND-#1450** LOW / low-confidence (filed 2026-06-02, `AUDIT_RENDERER_2026-06-02`): Submersion state has no hysteresis band — design observation, not a confirmed regression. Tracked as #1450.
- [x] **R6a-prospector-regress** — **closed 2026-05-16** in the same session it was filed. -18.5% FPS / +1.23 ms fence on Prospector between `220e8e1` (2026-05-11) and `c8519082` (2026-05-16). First bad commit: `6059e2ab` "Pick off 4 TLAS / acceleration LOWs from bundle #926" (git bisect, 8 steps). Behavioral change: REN-D8-NEW-08 flipped skinned BLAS BUILD+UPDATE flags from `PREFER_FAST_BUILD` to `PREFER_FAST_TRACE`. Commit's reasoning (BLAS refits ~600× between BUILDs → trace cost dominates ~6 orders of magnitude) was theoretically sound and measurement confirmed it: telemetry over 500 frames on Prospector showed 0 BUILDs : 34 refits per frame in steady state across 34 active skinned NPCs (1:289 ratio across the bench window), exactly matching the "refits dominate" model. But the empirical outcome went the other way — at the same workload the FAST_TRACE BVH cost more per frame than FAST_BUILD did, by ~+0.77 ms fence. The likely mechanism (un-confirmed without a deeper driver/RenderDoc dive): for small skinned-mesh BVHs (~5K-15K triangles per FNV body), the FAST_TRACE construction picks a wider, deeper tree that's actually worse for either refit cost or ray traversal cost on NVIDIA RTX 4070 Ti at our ray fan-out — the BUILD-time micro-optimisation cost > traversal-time win. **Fix landed**: split the shared `UPDATABLE_AS_FLAGS` constant into `UPDATABLE_AS_FLAGS` (TLAS, stays `FAST_TRACE`) + `SKINNED_BLAS_FLAGS` (skinned BLAS, reverts to `FAST_BUILD`), updated the three skinned-BLAS call sites in `blas_skinned.rs`. Recovers +15.8 FPS (108.8 → 124.6) on Prospector. Whiterun returns to 218.4 (within noise of 217.3 baseline — confirms Whiterun's apparent regression was pipeline-cache warmup, not skinned-BLAS). MedTek 67.1 (was 66.2, baseline 68.5 — its FO4 humanoid skeleton.nif doesn't resolve so the skinned path was never on the hot path there). 242 renderer tests still pass.
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
- [x] ~~`BSBoneLODExtraData` has no parser — surfaced by R3 baselines: 0/34 on FO4, 0/52 on Skyrim SE, 0/56 on FO76 (no instances on the other four games). Single-fix candidate matching the Session 18 R3-driven pattern.~~ — **closed via #614** (commit `782b7238`, 2026-04-25). Parser landed in `crates/nif/src/blocks/extra_data.rs` (`"BSBoneLODExtraData"` arm). The 0/N counts were zero *instances* in vanilla content, not parse failures — restored Skyrim Meshes0 to 100% clean (D5-01 / #1356).
- [x] ~~`BSClothExtraData` 0/298 on Starfield~~ — **closed via #722**. Parser was reading the NiExtraData `Name` field that nif.xml line 3222 marks `excludeT="BSExtraData"`; consumed 4 bytes of cloth payload as a string-table index, then read garbage as length and tripped EOF. Fix unblocks 1 523 cloth blocks across FO4 (309) / FO76 (365) / SF Meshes01 (298) + SF FaceMeshes (551). Cloth-simulation animation consumer still future work; parser side now correct. Baseline TSVs need a fresh sweep (`BYROREDUX_REGEN_BASELINES=1`) to lock the per-block delta.
- [ ] One Starfield NIF (`meshes\marker_radius.nif`) requests a 318 MB single-buffer allocation at parse time, exceeding `byroredux_nif::stream::MAX_SINGLE_ALLOC_BYTES = 256 MB`. Per-allocation cap is a different trade-off from the BA2 chunk cap bumped in Session 18 — bumping this one weakens defence against attacker-controlled `u32` sizes inside individual NIF blocks. Tracked separately; one file out of 320 483 in the Starfield mesh archive.
- [x] ~~**`#688`**~~ — **CLOSED.** Originally 149 Oblivion files truncated at root NiNode "failed to fill whole buffer" (pre-Gamebryo NetImmerse-vintage HUD brackets, menu assets, one creature head); after the #1506–#1509 family fixes the live count is **6** (the pre-Gamebryo v3.3–v4.2 markers, #1611) after #1543/#1544 also closed the 2 OBL-D1-NEW-01 files (live `nif_stats` over `Oblivion - Meshes.bsa`, 2026-06-15). Investigation (`.claude/issues/688/INVESTIGATION.md`) refuted the audit's "v=20.0.0.5 subset" framing — all 149 are `v=10.x.x.x / bsver=5` NetImmerse content with an undocumented 4-byte leading zero before `NiObjectNET.name`. Parser-side recovery (block_size gate) already handles them as truncated-not-failed; interior cells render fine. **Caution for future audit runs**: do NOT re-derive the "v=20.0.0.5 subset" framing — it has been empirically refuted.

---

## Project Stats

Ground-truth as of 2026-07-11 (Session 55 closeout). Last `/session-close` verification was 2026-07-11 (Session 55).

| Metric                                  | Value                        |
|-----------------------------------------|------------------------------|
| Rust source lines (`src/` dirs)         | ~270 020 (Session 55 measure) |
| Rust total lines (all `.rs`, excl. `target/`) | ~285 245 (updated 2026-07-11)  |
| Source files (`.rs`, excl. `target/`)   | 668 total · 630 outside `tests/` dirs |
| Workspace members                       | 23 (21 crates + `byroredux` binary + `tools/byro-dbg`) |
| Tests (last reported by ROADMAP)        | **3549 passing** (Session 55 closeout, 2026-07-11). +118 vs Session 54. |
| Open issue directories                  | 1825 (`.claude/issues/`)     |
| NIFs in per-game integration sweeps     | 184 886                       |
| Per-game NIF clean-parse rate           | 100% on FO3 / FNV / Skyrim SE / FO4 / FO76 (FO4 both base mesh archives, 159 866 NIFs, 2026-06-14; FO76 58 469 NIFs, 2026-07-11 #1900); Oblivion 99.93% (2026-06-15 sweep, post-#1543/#1544), Starfield 99.64% aggregate (2026-07-03 sweep; see compat matrix for per-archive breakdown). Recoverable 100% on all games, including Oblivion (#698's corrupt-marker hard-failure is closed). Sweep dates: Oblivion 2026-06-15, FO4 2026-06-14, FO76 2026-07-11, Starfield 2026-07-03; FO3/FNV/Skyrim SE unrefreshed since the original integration sweep (still 100%). |
| Supported archive formats               | BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8 |

### Repro commands for every bench claim

> **CWD matters.** Bare `--bsa` / `--textures-bsa` / `--materials-ba2` names
> resolve against the current working directory, not the `--esm` folder. Run
> each command with CWD set to that game's `Data/` directory (e.g.
> `cd "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"`) — and
> drop the `Game/Data/` prefix from `--esm` accordingly. Run from elsewhere and
> the archives silently fail to open; the scene loads near-empty (Prospector
> falls to 36 entities / 3 meshes and reports a spurious ~1792 FPS). FPS numbers
> below are the R6a-stale-13 refresh (`4e2ebe8c`, 2026-05-28).

| Claim                                                                     | Command                                                                                                                                                                                        |
|---------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Prospector Saloon 3516 entities @ **76.2 FPS / 13.11 ms / fence=11.12 ms / brd=0.37 ms** (R6a-stale-14, `1c26bc25`, 2026-06-03; fence −4.6% vs R6a-stale-13 — `IsCollisionOnly` reduces TLAS instance count; entity count unchanged at ~3516; full recovery to pre-collider 161.4 FPS still pending deeper investigation) | (CWD = `.../Fallout New Vegas/Data`) `cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa" --bench-frames 300` |
| Skyrim SE WhiterunBanneredMare 3216 entities @ **362.8 FPS / 2.76 ms / 1299 draws / fence=0.98 ms** (R6a-stale-14, `1c26bc25`, 2026-06-03; **+10.0% FPS vs 329.8 R6a-stale-13** — Session 46 perf wins; control bench confirms no steady-state regression). Must list all 9 texture archives explicitly — numeric-sibling auto-load gates on a non-digit suffix and `Textures0.bsa` ends in a digit. | (CWD = `.../Skyrim Special Edition/Data`) `cargo run --release -- --esm Skyrim.esm --cell WhiterunBanneredMare --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" --textures-bsa "Skyrim - Textures0.bsa" --textures-bsa "Skyrim - Textures1.bsa" --textures-bsa "Skyrim - Textures2.bsa" --textures-bsa "Skyrim - Textures3.bsa" --textures-bsa "Skyrim - Textures4.bsa" --textures-bsa "Skyrim - Textures5.bsa" --textures-bsa "Skyrim - Textures6.bsa" --textures-bsa "Skyrim - Textures7.bsa" --textures-bsa "Skyrim - Textures8.bsa" --bench-frames 300` |
| FO4 MedTekResearch01 21414 entities @ **65.2 FPS / 15.34 ms / 14535 draws / brd_ms=3.74 / fence=9.03** (R6a-stale-14, `1c26bc25`, 2026-06-03; entity +38% / draws +75% vs R6a-stale-13 — **entirely from M49 CSG precombined geometry** landed Session 45; new larger scene; 65.2 FPS is the new baseline) | (CWD = `.../Fallout 4/Data`) `cargo run --release -- --esm Fallout4.esm --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2" --textures-bsa "Fallout4 - Textures1.ba2" --textures-bsa "Fallout4 - Textures2.ba2" --textures-bsa "Fallout4 - Textures3.ba2" --textures-bsa "Fallout4 - Textures4.ba2" --textures-bsa "Fallout4 - Textures5.ba2" --textures-bsa "Fallout4 - Textures6.ba2" --textures-bsa "Fallout4 - Textures7.ba2" --textures-bsa "Fallout4 - Textures8.ba2" --textures-bsa "Fallout4 - Textures9.ba2" --textures-bsa "Fallout4 - TexturesPatch.ba2" --materials-ba2 "Fallout4 - Materials.ba2" --bench-frames 300` |
| Skyrim sweetroll single-mesh ~3000-5000 FPS (2026-04-22, RTX 4070 Ti @ 1280×720)        | `cargo run --release -- --bsa "Skyrim Special Edition/Data/Skyrim - Meshes0.bsa" --mesh meshes\\clutter\\ingredients\\sweetroll01.nif --textures-bsa "Skyrim Special Edition/Data/Skyrim - Textures3.bsa"` |
| Megaton interior parse-side 929 REFRs (2026-04-19)                        | `cargo test -p byroredux-plugin --release --test parse_real_esm parse_real_fo3_megaton_cell_baseline -- --ignored`                                                                             |
| Per-game full mesh sweep (clean rates above; recoverable 100% gate)       | `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`                                                                                                          |
| Full ESM record counts (FNV 77 825 = 73 054 structured + 4 771 NAVMs post-#1272; FO3 44 657 = 37 459 structured + 7 198 NAVMs post-#1272; both re-verified 2026-05-26 against vanilla masters) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored`                                                                                                                   |

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
| `byroredux-nif`               | NIF binary parser (~250 dispatch arms — live count in `crates/nif/src/blocks/mod.rs`), import-to-ECS, animation import |
| `byroredux-bsa`               | BSA (v103/v104/v105) + BA2 (v1/v2/v3/v7/v8, GNRL + DX10) readers                                                 |
| `byroredux-bgsm`              | FO4 / Skyrim SE / FO76 external material files (BGSM / BGEM v1–v22)                                              |
| `byroredux-sfmaterial`        | Starfield `materialsbeta.cdb` component-database reader (`.mat` JSON descriptors resolve through it)            |
| `byroredux-spt`               | SpeedTree `.spt` binary parser (Oblivion 4.x / FO3+FNV 5.x), placeholder-billboard fallback                     |
| `byroredux-facegen`           | FaceGen sidecar parsers — `.egm` geometry morphs, `.egt` texture morphs, `.tri` animated morph targets          |
| `byroredux-physics`           | Rapier3D bridge (M28 Phase 1, kinematic character controller M28.5)                                             |
| `byroredux-scripting`         | ECS-native events + timers + condition evaluator (M47.1) + `papyrus_demo` hand-translations                     |
| `byroredux-papyrus`           | Papyrus `.psc` parser (lexer + Pratt expression parser + statement/script parsers + full AST, M30.2)            |
| `byroredux-ui`                | Scaleform/SWF via Ruffle                                                                                         |
| `byroredux-debug-ui`          | Embedded egui debug overlay (egui-ash-renderer Vulkan pipeline, F-key toggle)                                   |
| `byroredux-debug-protocol`    | Wire types + component registry for debug CLI                                                                    |
| `byroredux-debug-server`      | TCP debug server (Late-stage exclusive system)                                                                   |
| `byroredux-cxx-bridge`        | C++ interop via cxx                                                                                              |
| `byroredux-audio`             | 3D spatial audio via kira 0.10 (spatial sub-tracks, reverb send, streaming music — M44)                          |
| `byroredux` (binary)          | Game loop, cell loader, fly camera, animation system, render data collection, NIFAL translation boundary         |
| `tools/byro-dbg`              | Standalone debug CLI (TCP client, REPL)                                                                          |
