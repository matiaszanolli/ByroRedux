# Feature Matrix

What works at runtime, per game. This is not parse rates — those live in
[Game Compatibility](engine/game-compatibility.md). This is: load a cell,
run the engine, what do you see?

**Legend:** ✓ Working · ~ Partial / known gaps · ✗ Not started · — Not applicable

**Bench record:** Numbers in the *Cells* row reference the stepped-camera
75-run matrix (`34074b93`, 2026-08-14). It is intentionally a dated record and
is currently beyond the 30-commit freshness gate; see
[ROADMAP.md](../ROADMAP.md) for the live staleness warning and repro commands.

---

## Cell Loading

| | Oblivion | FO3 | FNV | Skyrim SE | FO4 | FO76 | Starfield |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Interior cells** | ✓ | ✓ | ✓ | ✓ | ✓ | parse only | ✓ |
| **Exterior grid (7×7)** | bench pending | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **LAND heightmap + splatting** | parse ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **World streaming (M40)** | — | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **Confirmed bench** | device check pending | device check pending | 3 757 ent · 71.0 FPS TAA | 5 183 ent · 89.9 FPS TAA | 32 920 ent · 42.5 FPS TAA | — | Cydonia walkable |

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
| **Upscaling** | ✓ All games | FSR 3.1 Quality (default, `5c7acfe2`) / TAA native (`--upscaler taa` fallback); four FSR presets (Quality/Balanced/Performance/Ultra Performance), see ROADMAP.md Session 60 |
| **TAA** | ✓ All games | Halton(2,3) jitter, YCoCg variance clamp |
| **ACES tone mapping** | ✓ All games | Runs in the output-resolution presentation pass (`presentation.frag`), after upscaling; fog blend (LIGHT-N2, #784) happens earlier, in `composite.frag` |
| **Normal mapping** | ✓ All games | Authored tangents (Skyrim+/FO4) or synthesized (FO3/FNV/Oblivion) |
| **Terrain splatting** | ✓ FO3/FNV/Skyrim/FO4/Starfield | LTEX/TXST splat; `INSTANCE_FLAG_TERRAIN_SPLAT` path |
| **Water + RT caustics** | ✓ All games | Vertex displacement, Fresnel, RT reflection/refraction; caustic splat compute (M38) |
| **Bloom** | ✓ All games | Dual-pass pyramid (downsample + upsample) |
| **SSAO** | ✓ All games | Screen-space, sampled in `triangle.frag` |
| **Volumetrics** | ~ Partial | Procedural froxel-grid fog + clustered local fog volumes shipped (Session 62); authored CELL/WTHR extinction/chromaticity/peak-radiance/coverage now consumed; REGN-driven per-cell density and god-ray light shafts remain open (M55) |
| **Depth of field** | ~ TAA-accumulated | Aperture disk jitter via TAA history; no explicit CoC pass |
| **Disney BSDF** | ✓ FO4/Starfield/BGSM | `MAT_FLAG_PBR_BSDF`; subsurface/sheen/anisotropic |
| **Glass RT refraction** | ✓ All games | `MATERIAL_KIND_GLASS` triggers RT refraction ray budget |
| **Fire refraction** | ~ Partial | `MATERIAL_KIND_FIRE_REFRACTION` (103, Session 62) — normal-driven heat-haze distortion proxy; the three consistency gaps found this audit cycle (shadow masking #2224, G-buffer normal overwrite #2236, composition sort order #2237) are all fixed |
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
| AI / behavior | ~ | ~ | ~ | ~ |

FO4 humanoid actors are `~` because `character assets\skeleton.nif` is absent
from vanilla FO4 BA2s (only `_1stperson` skeleton exists). `Inventory` +
`EquipmentSlots` components still land; visible skinned geometry awaits a
Havok `.hkx` loader for FO4's packfile layout (M41.x, Tier 5) — the
`crates/hkx` reader that shipped covers Skyrim SE only.

AI / behavior is `~` (M42, Tier 7) — 7 of ~17 `PACK` procedures have a
runtime, each opt-in behind its own `BYRO_*` env flag: Sandbox
(`BYRO_SANDBOX_SIT`), Wander (`BYRO_WANDER`), Travel (`BYRO_TRAVEL`), Follow
(`BYRO_FOLLOW`), Escort (`BYRO_ESCORT`), Guard (`BYRO_GUARD`), and Patrol
(`BYRO_PATROL`, aliases Wander's algorithm — no patrol-route data is decoded
anywhere in this codebase). v0 scope limits apply across all seven: package
selection is spawn-time-only (schedule + priority + CTDA conditions,
evaluated once, no per-frame re-evaluation as game time advances), and none
swap animation clips for locomotion (straight-line walk, ground-snapped,
no NAVM pathing). The remaining 10 procedures (Find/Eat/Sleep/Accompany/
UseItemAt/Ambush/FleeNotCombat/CastMagic/Dialogue/UseWeapon) are parse-only —
each blocked on a subsystem (item/furniture-use beyond Sandbox's seat-snap,
magic, dialogue) that doesn't exist in the engine yet, or (UseWeapon) on
`PACK` not yet driving the player-only P2 melee vertical slice (see
Gameplay/Combat below). See
[docs/engine/npc-spawn-ai-packages.md](engine/npc-spawn-ai-packages.md) for
the full trace.

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
| Havok `.hkx` skeleton + clip loader | ~ | Skyrim SE — see note |

The `.hkx` row is `~` because `crates/hkx` (shipped 2026-08-01) reads
Skyrim SE's 64-bit Havok 2010 packfiles: it decodes `hkaSkeleton` and
expands static / spline-compressed `hkaSplineCompressedAnimation`
transform tracks, with no behavior-graph loading or execution. It is wired
into the animation asset provider to install the MQ101 cart-idle catalog
from real game data — a deliberate vertical slice, not general NPC
locomotion. FO4 and Starfield `.hkx` remain unread (different packfile
layouts); see the gaps table below.

---

## Audio (M44 — Phases 1–6 complete, plus the WATAL water consumer)

| Feature | Status |
|---|---|
| 3D spatial audio (kira 0.10) | ✓ |
| BSA WAV decode + cache | ✓ |
| One-shot sounds + footstep system | ✓ |
| Looping ambient (tweened stop on despawn) | ✓ |
| Streaming music (OGG, crossfade) | ✓ |
| Per-cell reverb send (`-12 dB` interior / silent exterior) | ✓ |
| Underwater low-pass (submersion-driven, 900 Hz wet / dry bypass) | ✓ |
| Water-surface splash + ripple one-shots (WATAL events) | ✓ |
| Per-material footsteps (FOOT records) | ✗ |
| Region ambient (REGN) — background music | ✓ |
| Region ambient (REGN) — incidental/loop sounds | ✗ |

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
| ESM SCPT record parse (FO3, 1 257 records; FNV, 2 576 records) | ✓ |
| Papyrus `.psc` → full AST (M30.2) | ✓ |
| ECS-native event hooks (M47.0) — `OnCellLoad`, `OnActivate`, `OnHit` | ✓ |
| CTDA condition evaluation with OR-precedence (M47.1) | ✓ 13 functions |
| `script.activate` console command wired | ✓ |
| Full Papyrus transpiler (M47.2) | ✓ `.pex` recognizer slice (CFG→lift→short-circuit→control-flow→lower); full transpiler deferred |

---

## Quests (M43)

| Feature | Status |
|---|---|
| QUST record parse — stages, log entries, objectives, targets (version-aware) | ✓ |
| Stage lifecycle — start-up / shut-down stages, repeated-stage policy, initial active/completed/failed flags | ✓ |
| Conditional per-log transitions (complete / fail / next-quest) | ✓ |
| Papyrus quest effects — `Start`/`Stop`/`Complete`/`Reset`/`SetActive`/`FailAllObjectives` | ✓ |
| QUST VMAD stage→fragment dispatch from vanilla `.pex` | ✓ M47.2 |
| Save-persistent quest progress | ✓ M45 |
| Alias fill — direct / unique / condition / XLRT / external / near / closest / force-into (loaded refs) | ✓ |
| Alias reservations + quest-lifetime semantics | ✓ |
| Faction + inventory injection from alias data | ✓ |
| Authored alias metadata without an owning subsystem | ~ exposed as runtime overlays, not fabricated results |
| Scene (`SCEN`) playback + PACK scene-package actions | ✓ MQ101 vertical slice |
| Console observability — `quest.show`, `quest.aliases`, `quest.start`/`stop`/`setstage` | ✓ M43.1 |
| Story Manager event payload / search | ✗ |
| Reference collections; true LCTN / unloaded-world alias queries | ✗ |
| Created-object spawning from aliases | ✗ |
| Dialogue tree + dialogue UI integration | ✗ M43 remainder |

Smoke test: [`docs/smoke-tests/m43-quest-runtime.sh`](smoke-tests/m43-quest-runtime.sh)
drives the production ESM → runtime → TCP command path against installed
Skyrim data.

---

## Gameplay / Combat (Playable Vertical Slice — P0-P2)

Execution plan: [`docs/engine/playable-vertical-slice.md`](engine/playable-vertical-slice.md).
Phases run P0 input/interaction → P1 reliable traversal → P2 combat → P3 game
UI/inventory → P4 authored objective/dialogue → P5 persistence/soak;
capability on this route takes precedence over renderer polish.

| Feature | Status | Notes |
|---|---|---|
| P0 — door / `[E]` interaction | ✓ Closed 2026-08-10 | Camera-forward XTEL target → native `[E] Open` prompt → one bound E-key edge → canonical `ActivateEvent` → deferred cell-transition arrival. Smoke: [`p0-door-interaction.sh`](smoke-tests/p0-door-interaction.sh) |
| P1 — reliable character control | ~ Core traversal gate passes; not closed | Movement/jump/sprint consumers share one once-per-frame `ActionState` snapshot; native Escape menu owns pause/focus/cursor transfer; settings-backed key rebinding. Gamepad physical sources remain open. Smoke: [`p1-character-traversal.sh`](smoke-tests/p1-character-traversal.sh) |
| P2 — melee combat core | ✓ Landed 2026-08-16 | Skyrim race Health + signed ACBS offset → actor-owned bone ray hit → bound Attack edge → canonical `HitEvent` → layered Health damage → one `Dead`/AI-disable transition → the existing 18-body ragdoll. Deterministic weapon selection (highest authored damage, FormID tie-break); explicit 8-damage unarmed fallback. Core checkpoint, not P2 closure. Smoke: [`p2-melee-core.sh`](smoke-tests/p2-melee-core.sh) |
| Authored attack/hit/death animation + sound | ✗ | P2 remainder |
| Corpse interaction / loot transfer | ✗ | P2 remainder |
| Save → exit → reload continuity | ✓ | M45/M45.1 — atomic rotating slots, interior/exterior live reload, stable FormID delta overlay, player pose, typed preflight, corrupt-slot fallback and player notifications |

Implementation: `byroredux/src/combat.rs` (melee vertical slice), `byroredux/src/interaction.rs` (Activate/E-key), `byroredux/src/systems.rs` + action-state consumers (movement).

---

## UI

| Feature | Status |
|---|---|
| Static SWF menus via Ruffle (Skyrim SE) | ✓ M20 |
| Ruffle ExternalInterface host bridge (Skyrim AVM1 + Fallout 4 AVM2) | ✓ R4 |
| Skyrim GFx host methods (`GameDelegate`, `_global.gfx`, text replacement, Papyrus callbacks) | ◐ 74-method catalog + request routing shipped; method behavior pending M48 |
| Fallout 4 Scaleform host objects | ◐ `BGSCodeObj` lifecycle + 138-method installed-corpus catalog + injected ABC dispatch + BA2-backed `ImportAssets` shipped; HUD/Pip-Boy readiness and destruction plus Atomic Command inventory asserted, method behavior pending M48 |
| Scaleform menu input routing + modal focus | ✓ M48 (`3ea5e275`) — winit → `UiInputEvent` translation (`crates/ui/src/input.rs`, `byroredux/src/ui_input.rs`), cursor position + modifier state, focus transfer and modal capture ahead of world controls, window→movie coordinate scaling |
| `byroredux-debug-ui` egui overlay (F-key toggle) | ✓ |
| Native game menu (Pause / Settings / Inventory) | ✓ Shipped 2026-08-15/16 — `byroredux-debug-ui`'s egui `GameMenuPage::{Pause,Settings,Inventory}` (`crates/debug-ui/src/panels.rs`); native `InventorySnapshot`/`InventoryAction` bridge over the canonical `Inventory`/`EquipmentSlots` components (`byroredux/src/inventory.rs`); validated TOML-persisted settings with stale-entry recovery (`byroredux/src/settings_io.rs`). Runs alongside Scaleform, not a replacement. Save/load toasts are live; container/corpse transfer, visible player-mesh attachment, general HUD bars, and quest-objective presentation remain open. |

---

## Character / Progression (CHARAL)

Per-game ruleset → canonical `ActorValues`/`CharacterLevel`/`Perks`/`Background`
translation layer. Design: [`docs/engine/charal.md`](engine/charal.md) §8
(rollout order); per-game data captures: `docs/engine/charal-*-ruleset.md`.
"Wired" below means reachable from the live spawn/tick path
(`CharacterRulesProfile::build_ruleset` / `derive_npc_actor_values` /
scheduler registration), not just present as a buildable function.

| Feature | Oblivion | FO3 | FNV | Skyrim SE | FO4 | FO76 | Starfield |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Ruleset wired (`CharacterRuleset`: derived-stat formulas + leveling model) | ~ built, unwired | ✓ | ✓ | ~ built, unwired | ✓ | ✗ | ✗ |
| NPC actor-value population at spawn | ✗ | ✓ class auto-calc | ✓ class auto-calc | ~ Health only | ✓ stored `PRPS`+`DNAM` | ~ stored, unverified | ~ stored, unverified |
| Runtime leveling (XP grant / level-up) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Pool regen tick (Health/Magicka/Stamina) | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert |
| Affliction tick (radiation/disease/addiction) | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert |

`oblivion_ruleset()` (`crates/core/src/character/tes.rs`) and `skyrim_ruleset()`
(`crates/core/src/character/skyrim.rs`) both build a real `CharacterRuleset`,
but `CharacterRulesProfile::build_ruleset`'s `RulesetBuilder` enum has no
Oblivion/Skyrim arm — both profiles map to `RulesetBuilder::None`, so neither
ever reaches a live actor. FO76/Starfield have data captures
(`charal-fo76-ruleset.md`, `charal-starfield-ruleset.md`) but no ruleset
builder at all; Starfield's is additionally blocked on its XP/level curve and
category-spend thresholds being unpublished research (charal.md §9).

Skyrim's NPC population derives Health only (`race.starting_health +
NPC_.ACBS.health_offset`) — no skills or other actor values. FO4/FO76/
Starfield share one "stored" mechanism (`NPC_` `PRPS` property pairs
pass through verbatim, plus baked `DNAM` Health/Action Points); FO4 is
exercised against real content, FO76/Starfield inherit the same decoder "by
lineage" with the `DNAM` tail explicitly flagged unconfirmed
(`crates/plugin/src/esm/reader.rs`).

Leveling, pool regen, and affliction are uniformly `✗` across every game:
`CharacterLevel.xp` is stamped `0` at spawn and nothing increments it;
`pool_regen_tick_system` is registered in `Stage::Update` every frame but
early-returns forever because its required `PoolRegenConfig` resource is
inserted only inside unit tests, never in `boot.rs`; `affliction_tick_system`
is not registered in the scheduler at all.

---

## Starfield-Specific

| Feature | Status |
|---|---|
| ESM parse (CELL / REFR / record types) | ✓ 99.9% vanilla records |
| BSGeometry `.mesh` external reference resolution | ✓ |
| Starfield CDB material system (`materialsbeta.cdb`) | ✓ Phase 1 |
| XCLL 108-byte interior lighting (volumetric height-fog model) | ✓ |
| Static-trimesh collider synthesize from render geometry | ✓ |
| `.hkx` animation skeleton | ✗ — `crates/hkx` reads Skyrim SE's Havok 2010 packfiles only, not Starfield's layout |

---

## What Doesn't Work Yet (live gaps as of 2026-08-19)

<!-- TD3-002: Save/load (M45/M45.1) removed — shipped 2026-06-21. The M47.2
     row below is the *full* transpiler, which is genuinely still deferred;
     the `.pex` recognizer slice that shipped is annotated inline. -->


| Gap | Blocking what | Milestone |
|---|---|---|
| Oblivion exterior (TES4 worldspace + LAND) | Oblivion exterior render | M32.5 follow-up |
| Havok `.hkx` loader (FO4 / Starfield layouts) | FO4 humanoid actors; Starfield animation | M41.x (Tier 5) |
| General NPC locomotion from `.hkx` | Skyrim actors animating outside the MQ101 cart-idle catalog | M41.x (Tier 5) |
| Terrain LOD multi-band selection | distance-based 8/16/32 LOD-band selection + `.btr` normal maps (the `.btr`/`.bto`/`_far.nif` parsers ship) | M35 |
| Remaining `PACK` procedures (Find/Eat/Sleep/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/Dialogue/UseWeapon) + per-frame package re-evaluation | NPCs perform item-use/combat/magic/dialogue behaviors; packages react to game-time changes | M42 (Tier 7) |
| Full Papyrus transpiler (M47.2) | Arbitrary script execution on real content (`.pex` recognizer slice shipped Session 51) | M47.2 (Tier 3) |
| Full Scaleform menus | In-game UI (method behavior / `_global.gfx`; native menu covers Pause/Settings/Inventory in parallel) | M48 / R4 decision |
| UV scroll animated materials | Animated terminals / displays | audited, not prioritised |
| Per-material footsteps (FOOT) | Correct surface audio | M44 follow-up |
| CHARAL: Oblivion/Skyrim rulesets built but unwired; regen + affliction ticks inert everywhere | Derived Health/AP/leveling formulas on Oblivion + Skyrim; passive Health/Magicka/Stamina regen and radiation/disease/addiction on all seven games | CHARAL (charal.md §8) |
