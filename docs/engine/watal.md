# WATAL — Water Abstraction Layer

**WATAL** (Water Abstraction Layer; pronounced "WAH-tal") is the canonical
translation tier for **water** — the surfaces, volumes, flow, and submersion the
engine renders *and* simulates: the per-game WATR records and cell/worldspace
water planes that today feed only the shader, plus the buoyancy, currents,
swimming, and drowning that no game's data authors at all. It is the sibling of
[`nifal.md`](nifal.md), [`exal.md`](exal.md), and [`physal.md`](physal.md): where
NIFAL translates per-game **NIF geometry/material** data, EXAL translates per-game
**ESM environment** data, and PHYSAL translates per-game **Havok physics** data,
WATAL translates per-game **water authoring** into one canonical water state that
the renderer *and* the physics solver consume identically for every game.

"Abstraction" is the brand; the mechanism is **canonical translation** (per-game
`Imported*` → one resolved, game-agnostic representation). The verbs stay
`translate` / `canonical` / `resolve`; **WATAL** names the layer as a whole.

**Status**: PROPOSED (design, 2026-06-19). Carves the water concern out of EXAL §2
(which covered water *rendering* only) into a first-class **double-ended** layer —
render **and** physics — modelled on Skyrim, with Oblivion/FO3/FNV translating up.
Implementation rolls out per §7. The crash-fix (§0) is the precondition and folds
into the exterior reroute.

**Goal**: every supported engine version (Oblivion / FO3 / FNV / Skyrim LE/SE /
FO4 / FO76 / Starfield) translates its native, per-game water authoring into **one
canonical, fully-resolved representation** through a single explicit `translate()`
boundary. The renderer (RT reflection/refraction, Fresnel, flow, foam, caustics,
underwater fog) **and** the physics solver (buoyancy, flow currents, swimming,
drowning) consume the canonical representation **identically for every game** — no
per-game branches downstream, no `Option` "resolve-it-later" fallbacks, no
render-time heuristics.

This is the same doctrine NIFAL formalises (`feedback_format_translation.md`:
"never per-game branches in the shader; translate at the parser→Material
boundary"; `format_abstraction.md`: the GameVariant pattern), now applied to the
water pipeline. WATAL is **double-ended** like PHYSAL: water authoring feeds two
distinct sinks (renderer + solver), so the boundary resolves both concerns into
one canonical spec.

---

## 0. Render-hardening step-0 — the "near water" device-loss was TWO bugs, both *not* water

The reported crash ("the renderer crashes when I get near water") turned out to be
**two independent exterior-streaming bugs stacked on the same symptom** — neither a
water fault. Water only *appeared* implicated because Skyrim's lake/mill/coastal
cells are dense streaming cells (the confirming cell was `BleakfallsBarrowPath`,
grid 2,-10). WATAL records both here because water cannot be exercised in an
exterior cell until streaming stops losing the device. **Both are fixed and
GPU-confirmed** (2026-06-20, RTX 4070 Ti): a fly-through of the cells that always
crashed now loads `(2,-10)` clean and holds ~100 fps with **0 device-loss**.

Method note: the first two written-up theories (RT-cost TDR; static-BLAS
use-after-free) were **both wrong** — refuted by measurement, see §0.3. The real
causes were found only after adding *unconditional per-frame* CPU/GPU phase
instrumentation (`CpuFrameTimings` → `systems/debug.rs` `cpu_ms:` line) and reading
the decisive `cpu_ms` of an actual slow frame. The lesson is in
`feedback_speculative_vulkan_fixes`: instrument and read the failing frame; do not
theorise a Vulkan cause you can't see.

### 0.1 Bug A — physics freeze (the multi-second stall, *not* a GPU fault at all)

The decisive log: a `dt=3170 ms` frame with `cpu_ms: fence_wait=0 …
atw_scheduler=3005`. `fence_wait=0` means the GPU was **not** the bottleneck — the
3 s was pure CPU inside the scheduler. Cause: every streamed Skyrim NPC attaches a
`RagdollTemplate` (18 bodies, each `motion=Dynamic`), spawned **awake** with no
terrain collider, so they free-fall forever. After flying through a few cells,
`physics_sync_system` was stepping **~3000 awake dynamic bodies** every frame →
multi-second CPU stall. That stall starved frame submission and ultimately
manifested as a device-loss downstream.

**Fix (landed, `crates/physics/src/sync.rs`):** in `register_newcomers`, spawn
dynamic bodies **asleep** (`body_builder.sleeping(true)`) and replace the
`pw.wake()` on registration with `pw.update_query_pipeline()` — newcomers no longer
force the whole island awake. Regression test
`sleeping_dynamic_newcomer_does_not_fall_or_pin_sim` (`world.rs`). **Confirmed:**
`atw_scheduler` dropped from **3005 ms → 1 ms** across the full flight. (The proper
*complement* — a terrain/water-plane collider so genuinely-woken bodies rest on the
ground instead of falling — is WATAL §7 Phase 2 + a PHYSAL item, not done yet.)

### 0.2 Bug B — stale RT-geometry descriptor (bindings 8/9 use-after-free)

The signature that isolated it: device-loss on streaming `(2,-10)`, with the last
*healthy* frame showing **cheap GPU passes** (`main_render=8.7 ms`) and a ~5 s GPU
hang before TDR — a GPU **page fault**, not a cost overrun. Confirmed by a 4-reader
+ adversarial-verify investigation (`device-loss-ssbo-blas-uaf` workflow, HIGH
confidence, verify pass could not refute it). Mechanism:

1. Crossing into a denser cell grows the **global geometry SSBO**
   (`MeshRegistry::rebuild_geometry_ssbo`, `mesh.rs`) — it allocates a **new**
   `VkBuffer` and defers-destroys the old one (`DEFAULT_COUNTDOWN ==
   MAX_FRAMES_IN_FLIGHT == 2`). E.g. 1351207 → 1363605 verts.
2. The RT-shading descriptor **bindings 8/9** (`GlobalVertices`/`GlobalIndices`,
   `scene_buffer/descriptors.rs::write_geometry_buffers`) were written **only once**
   at scene setup (`scene.rs`). Nothing re-pointed them after the realloc — the
   comment in `mesh.rs` *claimed* they were updated "in the same frame," but the
   verify pass proved that update was never wired up.
3. So bindings 8/9 kept naming the **old** buffer. At frame N+2 the deferred-destroy
   tick frees that `VkBuffer` while the descriptor still points at it; the next RT
   ray-query hit-fetch (`getHitUV`/`getHitTriNormal`, reflection/refraction/GI
   paths in `triangle.frag` via `raytrace.glsl`) dereferences freed device memory →
   page fault → ~5 s TDR → `VK_ERROR_DEVICE_LOST`. The 2-frame countdown is exactly
   why the loss lands a couple of frames *after* the healthy cell-stream frame.

The **raster** path never hit this — it re-fetches the global buffer fresh every
frame (`draw.rs` `cmd_bind_vertex_buffers`). Only the once-bound RT descriptor
dangled, which is why it surfaced only in the RT era and stayed invisible to
`cargo test` + sync-validation (a stale-descriptor UAF is not a missing-barrier
hazard).

**Fix (landed):** re-point bindings 8/9 for the **current** frame **every frame**
inside `draw_frame`, immediately after the deferred-destroy tick (i.e. *after* the
fence wait, so `descriptor_sets[frame]` is provably idle) — mirroring `write_tlas`
(binding 2, already re-pointed per frame) and the raster path's per-frame rebind.
Self-healing: any realloc is picked up within the 2-frame window, before the old
buffer is freed. Site: `crates/renderer/src/vulkan/context/draw.rs` (the
`write_geometry_buffers(&device, frame, …)` call after `tick_deferred_destroy`); the
stale `mesh.rs` comment was corrected to document why the rebind lives in
`draw_frame` and not at the realloc site (re-pointing at the realloc site, before
the fence wait, would be a descriptor-update-while-in-use hazard — bindings 8/9 are
not `UPDATE_AFTER_BIND`). **No new barriers, no `device_wait_idle`.**

**GPU-confirmed:** `(2,-10) BleakfallsBarrowPath` loads clean ("92 entities, terrain
BLAS 1/1"), 0 device-loss, recovers to 97–106 fps. The only residual hitch is a
single ~200–290 ms `atw_post`-dominated frame on cell-load (synchronous per-cell
upload cost, `MAX_CELLS_SPAWNED_PER_FRAME`) — GPU idle (`fence_wait=0`), nowhere
near the ~2 s watchdog; a separate perf item, not a fault.

### 0.3 Refuted alternatives (recorded so they aren't re-investigated)

- **RT-cost TDR from the denoiser overhaul (commit `6b061120`)** — *refuted* by the
  ~7 ms GPU measurement (≈280× under the ~2 s watchdog). The `dt` values are the
  device-loss *host-block symptom*, not a 2 s GPU frame. The RT overhaul is fine.
- **Static-BLAS use-after-free in `evict_unused_blas`** — *refuted as the active
  crash*. The original §0 confidently blamed this; the deferred-destroy fix to
  `evict_unused_blas` was landed and **did not stop the crash** (tell: no "BLAS
  eviction: freed" log lines — eviction never even fired during the burst). The
  change is **kept as latent `MEM-01` hardening** (the invariant is still correct;
  it just wasn't the bug). The real geometry UAF was bindings 8/9 (§0.2), which the
  later investigation proved BLAS do **not** touch (BLAS reference **per-mesh**
  buffers by device address, never the global SSBO).
- **Geometry-SSBO realloc dangling the BLAS device address** — *refuted*; BLAS bake
  the **per-mesh** `vertex_buffer`/`index_buffer` address, which `rebuild_geometry_
  ssbo` never touches. (The realloc *did* dangle a descriptor — bindings 8/9 — but
  that is a shading-read UAF, §0.2, not a BLAS/TLAS address fault.)
- **Water render fault** — *refuted*; water draw is RT-gated, a 4-vertex quad, every
  `water.frag` RT helper early-outs. The mill-pond scene renders correctly post-fix.
- **VRAM exhaustion** — *refuted*; an OOM surfaces as a caught `anyhow::Err`, never
  `DEVICE_LOST`.

> **Shared dependency:** both fixes share the Rapier/GPU lifecycle discipline with
> the exterior-reroute and the WATAL Phase 2 buoyancy work — "free a resource (or
> re-point its descriptor) only with respect to the frame that referenced it" is the
> same rule as "wake/sleep a body only when the sim genuinely changed." §7 Phase 2
> co-designs them — and Bug A's missing terrain collider is literally the WATAL
> water-plane-collider precursor.

---

## 1. The three-tier model (double-ended)

```
                 parse                    translate()                consume
  ESM/NIF  ───────────────▶  Imported*  ────────────────▶  Canonical  ──────┬───▶  renderer
  (WATR DATA/DNAM/GNAM/      (raw, per-game     (one site:      (resolved,    │     (water.frag:
   NAM2/XCLW/XCWT,            WatrRecord,       resolve_water_  game-agnostic │      reflect / refract /
   BSWaterShaderProperty)     WorldspaceRecord) material folds  components +  │      foam / caustics)
                                              in every quirk)   a Resource)   │
                                                                              └───▶  physics solver
                                                                                    (buoyancy / flow /
                                                                                     swim / drown)
```

| Tier | What it is | Where it lives | Rule |
|---|---|---|---|
| **Raw / `Imported*`** | A faithful, per-game decode of the WATR wire format (Oblivion DATA ~102 B, FO3/FNV DATA 196 B, Skyrim DNAM 252 B+) + cell `XCLW`/`XCWT` + worldspace `NAM2`/`DNAM`. May carry `Option`s and undecoded `raw_data`/`raw_dnam` tails. **Allowed to be messy.** | `crates/plugin/src/esm/records/misc/water.rs`, `crates/plugin/src/esm/cell/wrld.rs` | Decode only; never the engine's source of truth. |
| **`translate()` boundary** | The single function that resolves a raw `WatrRecord` into the canonical tier, folding every per-game quirk into one convention. | `byroredux/src/env_translate.rs` (`resolve_water_material`, `default_water_for_worldspace`) — WATAL is the water *arm* of EXAL's boundary module; **extend it, do not duplicate**. | Exactly **one** site. No duplicate construction. |
| **Canonical** | The resolved, game-agnostic components/resources the renderer + solver consume. No `Option` "resolve-later" fields. | ECS components in `crates/core/src/ecs/components/water.rs` + a `PhysicsWaterConstants` resource. | The single source of truth. |

### The canonical-type rule (inherited from NIFAL)

> **Where an ECS component already serves the game-agnostic, engine-facing role,
> that component IS the canonical type.** Introduce a *new* canonical type only
> where none exists.

The render-facing components already exist and are mature:
`WaterKind` / `WaterMaterial` / `WaterPlane` / `WaterFlow` / `WaterVolume` /
`SubmersionState`. WATAL does **not** fabricate a parallel `CanonicalWater` struct
these copy from. It reaches the canonical tier by **(a)** promoting the missing
Skyrim-authored *render* fields onto `WaterMaterial`, **(b)** adding the missing
*physics* state (the one place no component exists), and **(c)** routing every
per-game decision through the single `resolve_water_material` site.

The genuinely **new** canonical types (no existing ECS role) are: `WaterLod`
(distant water — the same LOD gap EXAL §5 flags), `PhysicsWaterConstants`
(engine-defined buoyancy params — *no game authors these*), `WaterContact`
(generalises camera-only `SubmersionState` to all bodies), and the transient
`SplashEvent` / `RippleEvent` markers.

---

## 2. Per-category maturity inventory (2026-06-19)

How close each water concern is to the canonical contract today.

### Components — **mostly canonical (render), nonexistent (physics)**

`crates/core/src/ecs/components/water.rs` is well-designed and *richer than
Oblivion/FO3/FNV already* on the render side — exactly the "Skyrim as canonical
base" target. `WaterMaterial` carries 24+ shading fields; `WaterFlow`,
`WaterVolume`, `SubmersionState` are clean. **Missing:** every physics field
(density/drag/buoyancy), `submerged_fraction` (only a scalar `depth` exists —
insufficient for Archimedes), and the Skyrim-authored render fields dropped at the
boundary (`sun_power`, scatter, `wave_amplitude/frequency`, GNAM noise layers,
below-water fog).

### Decode + translate — **good (render), drops Skyrim-only fields**

`water.rs` decodes the shared 60-byte DATA prefix (Oblivion/FO3/FNV) and a
best-effort Skyrim DNAM prefix; `NNAM`/`TNAM` texture and `GNAM` noise FormIDs are
parsed; `raw_data`/`raw_dnam` tails preserved. `resolve_water_material`
(`env_translate.rs:89-176`) captures colors/fog/fresnel/reflectivity/flow.
**Dropped at the boundary:** `wave_amplitude`/`wave_frequency` (parsed, never
copied), `GNAM` noise refs (parsed, never consumed), `sun_power` (skipped). The
`WaterKind` classification is a fragile EDID-substring heuristic
(`rapid`/`waterfall`/`falls`/`river`/`stream`), English-only, with `waterfall`
deliberately demoted to `River` for cell planes.

### Spawn — **functional, coarse**

`cell_loader/water.rs` spawns one flat quad per cell (interior: centred,
`HALF_EXTENT = 256`; exterior: full 4096-unit tile). Per-game default height via
`default_water_for_worldspace`. **Gaps:** no shoreline-fit mesh, one plane per
cell (can't represent multiple bodies at different heights, or rivers spanning
cells), and — critically — **no Rapier collider** (the `WaterVolume` AABB is
ECS-only; the solver never learns water exists).

### Render — **mature, with documented fragilities**

RT reflection/refraction (Schlick Fresnel, Snell, Beer-Lambert, TIR guard),
shoreline + flow-aligned foam, dual scrolling normal layers, procedural normal
fallback, underwater fog, water-side caustic splat. Battle-tested across many
closed bug IDs, correctly RT-gated. **Fragilities (all in-code):** `WaterPush` is
at the exact 128-byte Vulkan 1.1 max (no headroom — adding canonical fields forces
a UBO move); procedural-noise hash bands past ~176k world units (#1502, visual
only); reflection ray returns a per-WATR tint constant, not real hit albedo; waves
are normal-perturbation only (no displacement; `wave_amplitude/frequency` unused).

### Physics / gameplay — **stub (detection only) / nonexistent (everything else)**

`systems/water.rs` writes `SubmersionState` onto the **active camera only**
(actors explicitly deferred). That is the *entire* gameplay surface.
`crates/physics` has **zero** water code, exposes **no** force API beyond
`set_linvel` (no `add_force`/`apply_impulse`/`reset`), and `CharacterController`
has **zero** `SubmersionState` references. `WaterFlow` is decoded, rendered, and
**never reaches the solver**. No buoyancy, no currents, no swimming, no drowning,
no splashes, no water-walking, no freezing.

---

## 3. The single boundary (proposed)

WATAL extends the existing EXAL water arm rather than adding a module. Convention:
**AUTHORED** = copied from the parsed record; **SENTINEL** = explicit canonical
game-default (`WaterParams::default` / `WaterMaterial::default`), **never** a
render-time guess.

```rust
// byroredux/src/env_translate.rs  (WATAL is EXAL's water arm)

pub(crate) fn resolve_water_material(
    waters: &HashMap<u32, WatrRecord>,
    xcwt_form: Option<u32>,
) -> (WaterMaterial, WaterKind, Option<WaterFlow>, Option<WaterLod>, Option<String>);
//                                                ^^^^^^^^^^^^^^^^^^^ added: NAM3/NAM4 LOD (None for pre-Skyrim = sentinel)

pub(crate) fn default_water_for_worldspace(wrld: &WorldspaceRecord, game: GameKind) -> f32;
//   the ONE genuine per-game leak — the §4 GameVariant arm
```

**Contract for every function above:**

1. **Single site.** Both the bulk `--grid` loader and the streaming bootstrap call
   these — no second construction of `WaterMaterial`/`WaterFlow` anywhere.
2. **No render-time fallback.** A missing/absent WATR field is resolved *here* by
   returning the documented canonical SENTINEL, not by a branch in `water.frag` or
   the physics systems.
3. **No `Option` resolve-later leaks** on the canonical output beyond ones that
   encode a real game distinction (`WaterFlow` is `None` for calm water;
   `WaterLod` is `None` for pre-Skyrim — these are real, not deferred work).
4. **Physics is game-invariant.** No game's WATR authors buoyancy/density/swim —
   those are `PhysicsWaterConstants` engine constants. `GameKind` never enters the
   physics path; older games contribute a coarser *synthesized* `WaterFlow`
   through the same boundary (see §4).

---

## 4. The GameVariant doctrine for water

Per-game quirks route through a **single `GameKind`-keyed decision**, not scattered
`if game == X` checks. For water there is exactly **one** genuine per-game leak —
the default water height — already prototyped in `default_water_for_worldspace`.
Everything else is a SENTINEL the older game leaves unset, identical across games.

| Concern | Oblivion | FO3 / FNV | Skyrim (canonical) | Source field |
|---|---|---|---|---|
| WATR appearance payload | DATA ~102 B | DATA 196 B (60 B shared prefix) | DNAM 252 B+ | `water.rs:30-61` |
| shallow/deep color, reflectivity, fresnel | AUTHORED | AUTHORED | AUTHORED | DATA/DNAM RGBA |
| `fog_near`/`fog_far` | **SENTINEL** 80/600 (DATA omits 28..36) | AUTHORED | AUTHORED | DATA[28..36] |
| diffuse/normal texture | **SENTINEL** `u32::MAX` → procedural | AUTHORED (`NNAM`) | AUTHORED (`TNAM`) | NNAM/TNAM |
| noise layers (`GNAM`) | **SENTINEL** `[u32::MAX;3]` | **SENTINEL** | AUTHORED (3 slots) | GNAM |
| below-water fog split | **SENTINEL** (reuse above) | **SENTINEL** | AUTHORED (DNAM tail) | DNAM tail (undecoded) |
| `wave_amplitude/frequency` | AUTHORED | AUTHORED | AUTHORED | DATA[8..16] |
| `sun_power` | AUTHORED (was skipped) | AUTHORED | AUTHORED | DATA[16] |
| `WaterFlow` | SYNTHESIZED from wind | SYNTHESIZED from wind | AUTHORED flow | wind / DNAM flow |
| `ior` 1.33, `shoreline_width` 32, foam-by-kind | SENTINEL | SENTINEL | SENTINEL | engine-invariant |
| **default plane height** | **GameVariant:** Z=0 if `NAM2` (no WRLD DNAM, 0/84 in Oblivion.esm) | WRLD `DNAM[1]` (WastelandNV −2300) | WRLD `DNAM[1]` (Tamriel −14000) | `wrld.rs:120-132` |
| **physics** (buoyancy/swim/drown) | engine constant | engine constant | engine constant | **not in any WATR** |

The bottom row is the load-bearing finding: **physics translate-up is zero-cost and
game-invariant.** The older games are not "harder" for physics — they simply
contribute a coarser synthesized `WaterFlow` through the same channel.
Radioactive/dirty FO3/FNV water is **not** a distinct format (ordinary authored
color/fog/fresnel) — no special-case arm.

---

## 5. New canonical types

Per the NIFAL "promote, don't add a 3rd type" rule, most fields promote onto
existing components. Four genuinely new types fill gaps where no ECS role exists.

### 5.1 Promote onto `WaterMaterial` (render gaps; sentinel for older games)

`sun_power` + `spec_hardness`/`spec_brightness` (A7, currently skipped) ·
`scatter_color`/`scatter_amount`/`scatter_extinction` (A8 sunlight sub-surface
glow) · `wave_amplitude`/`wave_frequency` (A11, parsed but dropped — drive
normal-only perturbation now, optional displacement later) · `normal_octaves: u8`
(A1, allow ≥4-6 to match Skyrim chop; sentinel 2 = today's `scroll_a/b`) ·
`noise_layers: [u32;3]` (Skyrim GNAM; sentinel `[u32::MAX;3]`) ·
`caustic_intensity`/`caustic_scale` (B4) · `reflection_source: { Rt, Cubemap,
Procedural }` (A13) · `below_water_fog: { near, far, color }` (Skyrim DNAM split;
sentinel = reuse above-water fog).

### 5.2 `WaterLod` — distant water (the EXAL §5 LOD gap)

```rust
pub struct WaterLod { over_color: [f32;3] /*NAM3*/, under_color: [f32;3] /*NAM4*/, lod_mesh: Option<MeshHandle> }
```
`None` for Oblivion/FO3/FNV. Per-worldspace. Its LOD mesh/colors depend on the
worldspace bounds EXAL owns — sequence after the exterior-reroute terrain work.

### 5.3 `PhysicsWaterConstants` — engine-defined buoyancy (ECS Resource)

```rust
pub struct PhysicsWaterConstants {
    density, linear_damping_in/out, angular_damping_in/out,
    swim_speed_mult, buoyancy_scale, current_drag,
    f_swim_height_scale, hold_breath_time, drown_dps,
}
```
**Game-invariant** — Bethesda's buoyancy is engine-defined, *not* in any WATR
(verified: no game authors physics params). Identical for all games.

### 5.4 `WaterContact` — per-body submersion (generalises camera-only `SubmersionState`)

```rust
pub struct WaterContact { depth, submerged_fraction /*NEW*/, head_submerged, flow: Option<WaterFlow>, material: Option<WaterMaterial> }
```
`submerged_fraction` (displaced-volume estimate from collider-AABB vs
`WaterVolume` overlap) is the field buoyancy needs that the scalar `depth` cannot
provide.

### 5.5 `SplashEvent` / `RippleEvent` — transient markers

Modelled on `crates/scripting` `ActivateEvent`/`HitEvent`. Emitted on surface
crossing / submerged movement, feeding *both* the render ripple-normal injection
(A10) and audio — unifying render ripple and physics splash into one event.

---

## 6. What stays out of scope

- **Shader passes.** Like NIFAL/EXAL, no WATAL slice rewrites the Vulkan
  render-pass / pipeline. `water.frag` already consumes canonical inputs; WATAL
  changes only what *produces* them (and the §0 crash-fix bounds RT *cost*, not
  water sync). The one render plumbing change is moving water params off the full
  128-byte `WaterPush` to a UBO so the richer canonical material fits.
- **OpenMW-style RTT planar reflection cameras.** The flat-mesh + RT-ray
  substitution is the canonical realisation of reflection/refraction; keep it. The
  canonical *type* still carries DISPLACEMENT/LOD/DEPTH/REFLECTIONS/REFRACTIONS
  flags so the per-game translate can disable rays for opaque waterfalls
  (`WaterKind::refracts`) without a shader per-game branch.
- **Per-frame BLAS water displacement.** Waves stay normal-only
  (`water.rs:27` "do not displace BLAS per frame") — important given the crash was
  RT-cost-driven. Amplitude/frequency are *represented* canonically for a future
  optional displacement path.

---

## 7. Rollout order

Each phase ships independently behind `cargo test` (pure translate + system unit
tests); the render/physics-device parts are validated by the smoke-test pattern
(`docs/smoke-tests/README.md`) since they need a Vulkan device + game data.

1. **Phase 0 — CRASH-FIX FIRST** (§0). Blocks all exterior water testing; folds
   into the exterior reroute. Confirm via the §0.1 A/B repro (zero code), then ship
   the view-context RT gate + GPU-time circuit breaker + graceful device-lost
   handling. Test gate: validation-clean exterior fly at grid 8,-10 holding under
   the watchdog. **Shared with the exterior-reroute view-distance work.**

2. **Phase 1 — CANONICAL TYPE + TRANSLATE** (pure, fully cargo-testable, no
   device). Promote the §5.1 `WaterMaterial` fields; add `WaterLod`; extend
   `resolve_water_material`; decode the FO3/FNV DATA 136-byte tail + Skyrim DNAM
   tail into `below_water_fog` + noise layers; generalise
   `default_water_for_worldspace` into the §4 `GameKind` arm. Add the per-game
   translate-up regression test: feed a representative Oblivion / FNV / Skyrim WATR
   and assert the canonical `WaterMaterial` differs **only in AUTHORED fields**,
   all SENTINELs identical. **`WaterLod` sequences after the exterior-reroute
   terrain work exposes worldspace bounds.**

3. **Phase 2 — PHYSICS FORCE API + BUOYANCY / FLOW.** Add
   `add_force`/`apply_impulse`/`reset_external_forces` + a **pre-substep hook**
   (`world.rs:189`, immediately before the `while accumulator >= PHYSICS_DT` loop)
   to `crates/physics`; register a Rapier static water-plane collider from
   `WaterVolume`/`WaterPlane`; generalise submersion to `WaterContact` for all
   bodies (swim-level `= waterLevel − halfExtentsZ·fSwimHeightScale`, reuse the
   proven `WATERLINE_HYSTERESIS = 4` band, #1450); implement buoyancy
   (`F_up = density·g·displaced_volume`) + flow (`F = flow.dir·speed·current_drag·
   submerged_fraction`, finally consuming `WaterFlow`) + drowning (ECS timer on
   `head_submerged`). **Critical shared dependency:** buoyancy needs the *same*
   Rapier wake/sleep discipline as the exterior-reroute physics-freeze fix
   (`world.rs:182` step-skip fast path, `:225-244` kill-plane). A buoyant body
   must wake the sim and must **not** free-fall past the kill plane; buoyancy is
   what lets dropped clutter *rest* on water instead of being frozen by the
   kill-plane as the only sink. **Co-design the force hook with the freeze fix.**
   Test gate: drop a body into a `WaterVolume`, assert it rises and settles; apply
   `WaterFlow`, assert downstream drift; + an m41-style smoke test.

4. **Phase 3 — RENDER-FIDELITY + GAMEPLAY POLISH.** Move water params to a UBO
   (prerequisite for the richer material on GPU; keep the shader-struct lockstep
   per `feedback_shader_struct_sync`); wire swimming into `CharacterController`
   (OpenMW swimlevel model — gravity off below swimlevel, clamp to waterline,
   swim-up, inert float-to-surface); `SplashEvent`/`RippleEvent` → particle +
   audio + ripple normal injection (A10); underwater god-rays via the M55
   volumetrics gated on `WaterContact`; replace the EDID `WaterKind` heuristic with
   data-driven classification. Each sub-item independently shippable.

---

## 8. Tooling (proposed)

- `water.dump` debug-server command — print the live `WaterMaterial`/`WaterFlow`/
  `WaterLod` resolved for the cell the camera is in.
- `water.contacts` — list every body with a `WaterContact` (depth,
  submerged_fraction, swim state) — the physics analog of `tex.missing`.
- A per-game translate-up unit harness (Phase 1) asserting SENTINEL-identity across
  Oblivion / FNV / Skyrim WATR inputs.

---

## 9. Resolved questions (research pass, 2026-06-19)

Sources: OpenMW (`/mnt/data/src/reference/openmw/`), nif.xml
(`/mnt/data/src/reference/nifxml/nif.xml`), Havok 2013
(`/mnt/data/src/reference/havok-2013`), and the in-repo decode/translate code.

### Q1 — Is Skyrim's WATR really a superset of the older games? → **Yes.**

Oblivion DATA (~102 B) ⊂ FO3/FNV DATA (196 B) ⊂ Skyrim DNAM (252 B+) share a
60-byte wind/wave/reflect/fresnel/fog/color prefix (`water.rs:135-209`). Skyrim
only *adds* fields (GNAM noise refs, NAM3/NAM4 LOD colors, displacement/LOD/depth
flags, linear-velocity flow). Skyrim is the natural canonical schema; older games
leave Skyrim-only fields at sentinel exactly as `decode_data` already does.
nif.xml gates `WaterShaderProperty` as `#FO3_AND_LATER#` (`:6322`) and
`BSWaterShaderProperty` as `#SKY_AND_LATER#` (`:6695`) — confirming the era split.

### Q2 — Does any game author water *physics* params? → **No.**

No game's WATR authors buoyancy, density, swim speed, or current drag — in
Bethesda these are engine-defined. Authoritative buoyancy semantics come from
Havok `hkpBuoyancyAction` (Havok 2013) and the Skyrim havok water material
(nif.xml `:596` `SKY_HAV_MAT_WATER`, `:773` `SKYL_CLUTTER` "float on water
surface", `:780` `SKYL_WATER`). So `PhysicsWaterConstants` is engine-canonical and
game-invariant (§4 bottom row, §5.3).

### Q3 — What is the reference swim / submersion model? → **OpenMW movementsolver.**

`swimlevel = waterLevel − halfExtentsZ·fSwimHeightScale`
(`movementsolver.cpp:149`, `worldimp.cpp:2111-2134`); below it, gravity off + full
3D movement + clamp to surface (`:149-380`); inert/dead bodies float to surface
(`:155-161` — the closest OpenMW gets to buoyancy; true Archimedes is the one
feature OpenMW lacks, sourced from Havok per Q2). Drowning: `fHoldBreathTime` then
3 dmg/sec (`actors.cpp:963-1010`). Water plane: `btStaticPlaneShape(+Z, height)`
on `CollisionType_Water` (`physicssystem.cpp:811-847`). Ripples: per-actor
emitters on movement > 10 u (`ripplesimulation.cpp:148-195`).

### Q4 — Skyrim surface-render feature set? → **multi-octave normals + depth-aware refraction + scattering.**

6-octave animated normals with wave-weight blending (OpenMW `water.frag:60-134`,
`WAVE_SCALE=75`), Fresnel reflection mix (`:144`), depth-aware refraction with
shore suppression + Beer-Lambert tint (`:147-202`), sun specular (`:160-176`),
sunlight scattering (`:202-212`), wobbly-shore + foam (`:217-233`), rain/disturbance
ripple injection (`:82-133`). The engine's current 2-layer scroll is the floor;
`normal_octaves` (§5.1) lets the canonical reach Skyrim chop.

### Q5 — Open item: exact byte offsets of the undecoded tails. → **MEDIUM confidence; verify before relying.**

The FO3/FNV DATA 136-byte tail and the Skyrim DNAM tail (`below_water_fog`,
displacement layers) are currently `raw_data`/`raw_dnam` and **best-effort**
across Skyrim 1.5/1.6. Before Phase 1 carries `below_water_fog`/noise offsets,
byte-decode a vanilla record via the extract→trace method
([[nif_v10x_stride_drift_resolved]]). Until then they stay SENTINEL — correctness
is not blocked, only fidelity.
