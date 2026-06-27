# Feature Matrix

What works at runtime, per game. This is not parse rates — those live in
[Game Compatibility](engine/game-compatibility.md). This is: load a cell,
run the engine, what do you see?

**Legend:** ✓ Working · ~ Partial / known gaps · ✗ Not started · — Not applicable

**Bench staleness:** Numbers in the *Cells* row reference the R6a-stale-13 refresh
(`4e2ebe8c`, 2026-05-28). See [ROADMAP.md](../ROADMAP.md) for the repro commands.

---

## Cell Loading

| | Oblivion | FO3 | FNV | Skyrim SE | FO4 | FO76 | Starfield |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Interior cells** | ✓ | ✓ | ✓ | ✓ | ✓ | parse only | ✓ |
| **Exterior grid (7×7)** | bench pending | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **LAND heightmap + splatting** | parse ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **World streaming (M40)** | — | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **Confirmed bench** | 379 ent · ~1 600 FPS | 929 ent | 3 507 ent · 71 FPS | 3 211 ent · 330 FPS | 15 546 ent · 91 FPS | — | Cydonia walkable |

**Oblivion exterior**: TES4 worldspace + LAND parse + load ✓ — the wiring
is implemented and game-agnostic; only an on-device exterior render bench
is pending. (BSA v103 extracts fine — that framing was a stale premise,
closed by #699.)

---

## Rendering

| Feature | Status | Notes |
|---|---|---|
| **RT shadows** | ✓ All games | Streaming WRS, 16 reservoirs/fragment, weight clamp 64× |
| **RT reflections** | ✓ All games | Per-mesh BLAS compaction + LRU eviction |
| **1-bounce GI** | ✓ All games | Ray-query; denoised by SVGF |
| **SVGF temporal denoiser** | ✓ All games | Motion-vector reprojection, mesh-ID disocclusion |
| **TAA** | ✓ All games | Halton(2,3) jitter, YCoCg variance clamp |
| **ACES tone mapping** | ✓ All games | Post-ACES fog blend (LIGHT-N2, #784) |
| **Normal mapping** | ✓ All games | Authored tangents (Skyrim+/FO4) or synthesized (FO3/FNV/Oblivion) |
| **Terrain splatting** | ✓ FO3/FNV/Skyrim/FO4/Starfield | LTEX/TXST splat; `INSTANCE_FLAG_TERRAIN_SPLAT` path |
| **Water + RT caustics** | ✓ All games | Vertex displacement, Fresnel, RT reflection/refraction; caustic splat compute (M38) |
| **Bloom** | ✓ All games | Dual-pass pyramid (downsample + upsample) |
| **SSAO** | ✓ All games | Screen-space, sampled in `triangle.frag` |
| **Volumetrics** | ~ Scaffold | Froxel injection + integration shaders shipped; content-driven density not wired |
| **Depth of field** | ~ TAA-accumulated | Aperture disk jitter via TAA history; no explicit CoC pass |
| **Disney BSDF** | ✓ FO4/Starfield/BGSM | `MAT_FLAG_PBR_BSDF`; subsurface/sheen/anisotropic |
| **Glass RT refraction** | ✓ All games | `MATERIAL_KIND_GLASS` triggers RT refraction ray budget |
| **Terrain LOD (M35)** | ~ Partial | `.btr` (Skyrim+/FO4) + `.bto` + `_far.nif` (Oblivion/FO3/FNV) shipped; distance-based multi-band selection + `.btr` normal maps deferred |

### FO4 Precombined Geometry (M49 — closed 2026-06-02)

| Sub-feature | Status |
|---|---|
| `.csg` reader | ✓ Shipped |
| Precombined mesh decode + Y-up convert | ✓ Shipped |
| Cell-loader spawn from XCRI hash list | ✓ Shipped |
| LOD tier selection (one tier, not all three) | ✓ Fixed |
| Texture wiring from owning REFR | ✓ Shipped |
| `_precomb.nif` collision | ✗ Deferred |
| `.uvd` occlusion volumes | ✗ Deferred |

---

## NPC Spawning (M41 — Phases 1+2 closed)

| Feature | FO3 | FNV | Skyrim SE | FO4 |
|---|:---:|:---:|:---:|:---:|
| Visible T-pose spawn at REFR | ✓ | ✓ | ✓ | ~ |
| Skeleton + body + head composition | ✓ | ✓ | ✓ | ~ |
| FaceGen morphs (FGGS+FGGA) | ✓ | ✓ | ✓ | ✓ |
| Equipment via OTFT + LVLI dispatch | ✓ | ✓ | ✓ | ~ |
| `Inventory` + `EquipmentSlots` components | ✓ | ✓ | ✓ | ✓ |
| Skinned GPU rendering | ✓ M29.5 | ✓ M29.5 | ✓ M29.5 | ~ |
| AI / behavior | ✗ | ✗ | ✗ | ✗ |

FO4 humanoid actors are `~` because `character assets\skeleton.nif` is absent
from vanilla FO4 BA2s (only `_1stperson` skeleton exists). `Inventory` +
`EquipmentSlots` components still land; visible skinned geometry awaits a
Havok `.hkx` loader (M41.x, Tier 5).

---

## Animation

| Feature | Status | Games |
|---|---|---|
| Keyframe (`.kf`) playback | ✓ | All |
| Linear / Hermite / TBC interpolation | ✓ | All |
| B-spline compressed (NiBSplineCompTransformInterpolator) | ✓ | FNV / FO3 and later |
| Per-frame GPU bone-palette compute (M29.5) | ✓ | All |
| Per-entity skinned BLAS refit (zero-lag RT pose) | ✓ | All |
| Embedded ambient controllers (UV scroll, alpha fade, vis flicker) | ✓ | All |
| Inline transform controllers in embedded path | ✓ | All (#1440) |
| Particle animation (birth rate, grow/fade size) | ✓ | All |
| Runtime morph updates (FaceGen) | ✗ | — spawn-time only |
| UV scrolling (animated UV offset) | ✗ | — parsed, not rendered |
| Havok `.hkx` skeleton loader | ✗ | — M41.x Tier 5 |

---

## Audio (M44 — Phases 1–6 complete)

| Feature | Status |
|---|---|
| 3D spatial audio (kira 0.10) | ✓ |
| BSA WAV decode + cache | ✓ |
| One-shot sounds + footstep system | ✓ |
| Looping ambient (tweened stop on despawn) | ✓ |
| Streaming music (OGG, crossfade) | ✓ |
| Per-cell reverb send (`-12 dB` interior / silent exterior) | ✓ |
| Per-material footsteps (FOOT records) | ✗ |
| Region ambient (REGN) | ✗ |

---

## Physics (M28 + M28.5)

| Feature | Status |
|---|---|
| Rapier3D bridge (NIF collision → ECS → stepper) | ✓ |
| Kinematic character controller (gravity, collide-and-slide, jump, autostep) | ✓ |
| NPC / creature physics | ✗ |
| Weapon / item physics | ✗ |
| Ragdoll (Havok constraint mapping) | ~ Classic constraint chain (Oblivion/FO3/FNV/Skyrim) on Rapier; FO4+ blocked on BhkSystemBinary |

---

## Scripting (M47)

| Feature | Status |
|---|---|
| ESM SCPT record parse (FO3/FNV, 1 257 records) | ✓ |
| Papyrus `.psc` → full AST (M30.2) | ✓ |
| ECS-native event hooks (M47.0) — `OnCellLoad`, `OnActivate`, `OnHit` | ✓ |
| CTDA condition evaluation with OR-precedence (M47.1) | ✓ 7 functions |
| `script.activate` console command wired | ✓ |
| Full Papyrus transpiler (M47.2) | ✓ `.pex` recognizer slice (CFG→lift→control-flow→lower→short-circuit); full transpiler deferred |

---

## UI

| Feature | Status |
|---|---|
| Static SWF menus via Ruffle (Skyrim SE) | ✓ M20 |
| GFx extensions (`_global.gfx`, text replacement, Papyrus callbacks) | ✗ |
| `byroredux-debug-ui` egui overlay (F-key toggle) | ✓ |
| Full menu reimplementation | ✗ R4 decision pending (Tier 7) |

---

## Starfield-Specific

| Feature | Status |
|---|---|
| ESM parse (CELL / REFR / record types) | ✓ 99.9% vanilla records |
| BSGeometry `.mesh` external reference resolution | ✓ |
| Starfield CDB material system (`materialsbeta.cdb`) | ✓ Phase 1 |
| XCLL 108-byte interior lighting (volumetric height-fog model) | ✓ |
| Static-trimesh collider synthesize from render geometry | ✓ |
| `.hkx` animation skeleton | ✗ |

---

## What Doesn't Work Yet (live gaps as of 2026-06-25)

<!-- TD3-002: Save/load (M45/M45.1) removed — shipped 2026-06-21. The M47.2
     row below is the *full* transpiler, which is genuinely still deferred;
     the `.pex` recognizer slice that shipped is annotated inline. -->


| Gap | Blocking what | Milestone |
|---|---|---|
| Oblivion exterior (TES4 worldspace + LAND) | Oblivion exterior render | M32.5 follow-up |
| Havok `.hkx` loader | FO4 humanoid actors; Starfield animation | M41.x (Tier 5) |
| Terrain LOD multi-band selection | distance-based 8/16/32 LOD-band selection + `.btr` normal maps (the `.btr`/`.bto`/`_far.nif` parsers ship) | M35 |
| NPC behavior / AI packages | NPCs animate + navigate | M42 (Tier 7) |
| Full Papyrus transpiler (M47.2) | Arbitrary script execution on real content (`.pex` recognizer slice shipped Session 51) | M47.2 (Tier 3) |
| Full Scaleform menus | In-game UI | M48 / R4 decision |
| UV scroll animated materials | Animated terminals / displays | audited, not prioritised |
| Per-material footsteps (FOOT) | Correct surface audio | M44 follow-up |
