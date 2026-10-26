# Feature Matrix

What works at runtime, per game. This is not parse rates — those live in
[Game Compatibility](engine/game-compatibility.md). This is: load a cell,
run the engine, what do you see?

**Legend:** ✓ Working · ~ Partial / known gaps · ✗ Not started · — Not applicable

**Bench record:** Numbers in the *Cells* row reference the stepped-camera
75-run matrix (`2da754e7`, 2026-09-03): all 75 state-gated runs accepted. See
[ROADMAP.md](../ROADMAP.md) for the full preset table and repro commands.

---

## Cell Loading

| | Oblivion | FO3 | FNV | Skyrim SE | FO4 | FO76 | Starfield |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Interior cells** | ✓ | ✓ | ✓ | ✓ | ✓ | parse only | ✓ |
| **Exterior grid (7×7)** | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **LAND heightmap + splatting** | parse ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **World streaming (M40)** | — | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **Confirmed bench** | 6 043 ent · 2 355 draws (#2368) | device check pending | 2 359 ent · 83.8 FPS TAA | 5 853 ent · 83.7 FPS TAA | 24 923 ent · 35.4 FPS TAA (MedTek; bench of record `d9a201c71`) | — | Cydonia walkable |

**Oblivion exterior**: TES4 worldspace + LAND parse + load ✓ — closed by
#2368 (2026-08-12): Tamriel `(0,0)` radius 1 on-device, 6,043 entities /
2,355 draws, image-health + environment-value gates both clean. (BSA v103
extracts fine — that framing was a stale premise, closed by #699.)

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
| **Distant terrain LOD (M35)** | ~ Partial | Per-family schemes, all shipped: `.btr` quads (Skyrim SE/FO4), `meshes\landscape\lod\<world>\` quads (FO3/FNV), synthesized heightmap ring (Oblivion — no baked-quad ladder is wired for it; `LodBandLadder::for_terrain_game` returns `None`). `.btr` normal maps deferred |
| **Distant object LOD** | ~ Partial | `.bto` quads (Skyrim SE/FO4), `landscape\lod\<world>\blocks\` quads (FO3/FNV, #3321), `DistantLOD\*.lod` + `_far.nif` placement lists (**Oblivion only** — `placement_lod_supported` gates on `GameKind::Oblivion`; FO3 ships 2 `_far.nif` and zero `distantlod\`, FNV zero of both). Distance-based multi-band selection shipped for every quad-based scheme (#2371, four-level ladder); FO76/Starfield unexercised |

### FO4 Precombined Geometry (M49 — closed 2026-06-02)

| Sub-feature | Status |
|---|---|
| `.csg` reader | ✓ Shipped |
| Precombined mesh decode + Y-up convert | ✓ Shipped |
| Cell-loader spawn from XCRI hash list | ✓ Shipped |
| LOD tier selection | ✓ Shipped — every populated band decoded (#4234; the bands are disjoint sub-meshes, and picking one dropped 12.45 % of MeshesExtra's baked triangles) |
| Texture wiring | ✓ Shipped — material from the owning shape + BGSM merge; no REFR overlay (deliberately empty, #4290) |
| `<cell>_physics.nif` collision | ✗ Deferred (the collision sibling is `_physics.nif`, not `_precomb.nif` — 6,803 files across the 8 FO4 mesh archives; census in AUDIT_FO4_2026-10-05) |
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
complete vanilla skeleton asset. The `crates/hkx` reader covers Skyrim's
Havok 2010 packfiles (the 2011 release's 32-bit packfiles and Special
Edition's 64-bit ones) and FO4's Havok 2014 packfiles, including skeletons,
clips, and behavior graphs. The FO4 real-data census is pinned by
`fallout_4_behavior_graph_census_is_exact` in `crates/hkx/src/corpus_tests.rs`.

AI / behavior is `~` (M42, Tier 7) — 10 of ~17 `PACK` procedures have a
runtime, and since M42.10 (2026-09-18) they run **by default** (single
kill-switch: `BYRO_NO_AI_LOCOMOTION=1`): Sandbox, Wander, Travel, Follow,
Escort, Guard, and Patrol (aliases Wander's algorithm — no patrol-route
data is decoded anywhere in this codebase), plus Eat/Sleep v0
(`eat_sleep_system`, walk to the PLDT anchor then seat at the nearest
furniture marker) and Dialogue (the force-greet bridge, #5367 Phase F).
Walking NPCs are animated
(authored per-body-class walk clips — KF games, Skyrim's humanoid HKX path,
and FO4's Havok 2014 behavior-graph path; FO4 ambient NPC walking is not
wired to this animation path yet) and their steps are physics-backed through
Rapier's kinematic character controller, at each actor's authored stride
speed (M42.11). v0 scope limits still apply: package selection (schedule
+ priority + CTDA conditions) runs at spawn and is re-evaluated once per
in-game minute by `ambient_ai_package_system` (M42.9 / #2652) — not per
frame — and movement remains straight-line within a single resident NAVM
tile. Their shared walk-to-point step routes through NAVM A*
(`byroredux/src/systems/navmesh_path.rs`, #2372). The remaining 7
procedures (Find/Accompany/UseItemAt/Ambush/FleeNotCombat/
CastMagic/UseWeapon) are parse-only — each blocked on a subsystem
(item use beyond Sandbox's seat-snap, magic) that doesn't
exist in the engine yet, or (UseWeapon) on `PACK` not yet driving combat:
both the P2 melee slice and the MQ101 `npc_combat_ai_system` are
player- or script-initiated, never package-selected (see Gameplay/Combat
below). See
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
| UV scrolling (animated UV offset) | ✓ | `AnimatedUvTransform` written by `animation_system`, consumed in `render/static_meshes.rs` (#525) |
| Havok `.hkx` skeleton + clip loader | ~ | Skyrim LE + SE and FO4 — see note |

The `.hkx` row is `~` because Starfield's layout remains unread. `crates/hkx`
reads Skyrim's Havok 2010 packfiles — Special Edition's 64-bit layout and,
since `fb8173fe` (2026-09-14), the 2011 release's 32-bit one — plus FO4's
Havok 2014 packfiles. It decodes `hkaSkeleton`, static / spline-compressed
clip tracks, and FO4 behavior graphs; the FO4 census is pinned by
`fallout_4_behavior_graph_census_is_exact` in
`crates/hkx/src/corpus_tests.rs`. Skyrim's reader is wired into the animation
asset provider for the MQ101 cart-idle catalog; FO4's graph runtime is wired
for authored behavior-graph animation, while ambient NPC walking remains a
separate integration gap. See the gaps table below.

---

## Audio (M44 — Phases 1–6 complete, plus the WATAL water consumer)

| Feature | Status |
|---|---|
| 3D spatial audio (kira 0.12) | ✓ |
| BSA sound decode (WAV, OGG Vorbis, MP3, FLAC) | ✓ (combat one-shots use the engine-wide `SoundCache`, which caches decoded paths and misses; voice and script sounds use the byte-budgeted `VoiceSoundCache`) |
| One-shot sounds + footstep system | ✓ |
| Looping ambient (tweened stop on despawn) | ✓ |
| Streaming music (OGG / MP3 / WAV, crossfade) | ✓ |
| Scripted `PlayMusic` (FO3/FNV `MUSC` track layered over the cell music; loose `Data\Music` files or archive) | ✓ (#6226) |
| NPC dialogue voice playback (Phase V; archive-backed segmented lines) | ✓ (Skyrim / FO4 `.fuz` voice needs an optional host `ffmpeg` to decode the xWMA payload; without one the line stays silent on the subtitle estimate) |
| Script sound plays (Papyrus `Sound.Play`, ObScript `PlaySound` / `PlaySound3D`, graph `SoundPlay.*`) | ◐ played from the source through the bounded `VoiceSoundCache`, with a `SOUN`'s authored `SNDD` attenuation and `2D` / `Menu Sound` flags; Skyrim `SNDR` output-model attenuation is not decoded, so those use the generic range; looping descriptors are not played (#6281) |
| Contact-force impact sounds (Wave 4) | ◐ config-driven dispatch mechanism shipped; default/per-material sound content is not populated |
| Per-cell reverb send (`-12 dB` interior / silent exterior) | ✓ |
| Underwater low-pass (submersion-driven, 900 Hz wet / dry bypass) | ✓ |
| Water-surface splash + ripple one-shots (WATAL events) | ✓ |
| Per-material footsteps (FOOT records) | ◐ mechanism shipped (`select_footstep_sound` + surface-material probe); no authored data yet — the FOOT→IPDS material table awaits decode |
| Region ambient (REGN) — background music | ◐ dispatch mechanism ✓; no supported game's `music_form` currently resolves to a decodable SOUN path; playback awaits #3816 (MUSC/MUST/MSET/RDMD decode) |
| Region ambient (REGN) — incidental/loop sounds | ✗ |

---

## Physics (M28 + M28.5)

| Feature | Status |
|---|---|
| Rapier3D bridge (NIF collision → ECS → stepper) | ✓ |
| Kinematic character controller (gravity, collide-and-slide, jump, autostep) | ✓ |
| NPC / creature physics | ~ NPC locomotion through the Rapier KCC (M42.10/M42.11); no NPC dynamics |
| Weapon / item physics | ✗ |
| Ragdoll (Havok constraint mapping) | ~ Classic constraint chain (Oblivion/FO3/FNV/Skyrim) on Rapier; FO4 decoded `bhkRagdollSystem` articulation (#6070); FO76/SF tagfile payloads open |

---

## Scripting (M47)

| Feature | Status |
|---|---|
| ESM SCPT record parse (FO3, 1 257 records; FNV, 2 576 records) | ✓ |
| Papyrus `.psc` → full AST (M30.2) | ✓ |
| ECS-native event hooks (M47.0) — `OnCellLoad`, `OnActivate`, `OnHit` | ✓ |
| CTDA condition evaluation with OR-precedence (M47.1) | ✓ 77 functions |
| `script.activate` console command wired | ✓ |
| Full Papyrus transpiler (M47.2) | ✓ `.pex` recognizer slice (CFG→lift→short-circuit→control-flow→lower); full transpiler deferred | 
| Papyrus VM (bounded interpreter, decline-fallback) | ✓ reference/alias/update/event + quest-fragment scripts run what the static lowering declines, under a per-frame budget (#5763); structs + array methods |
| Script-extender compatibility layer (SKSE-family) | ◐ **vertical slice, unaudited** — six built-in provider families registered in the Papyrus provider catalog (Game, Input, UI, StorageUtil, JContainers, ModEvent), plus an ObScript runtime. ~23.9k LOC across [`crates/sdk/src/`](../crates/sdk/src/) (14 245), [`crates/scripting/src/papyrus_provider/`](../crates/scripting/src/papyrus_provider/) (6 377) and `compatibility.rs` + `obscript*.rs` (3 294), measured 2026-09-10. No dedicated audit pass has run over it — treat the status as "exists and is tested in-crate", not "verified against real mod content". Design: [`docs/engine/sdk-v0.1-development-plan.md`](engine/sdk-v0.1-development-plan.md). |

---

## Quests (M43)

| Feature | Status |
|---|---|
| QUST record parse — stages, log entries, objectives, targets (version-aware) | ✓ |
| Stage lifecycle — start-up / shut-down stages, repeated-stage policy, initial active/completed/failed flags | ✓ |
| Conditional per-log transitions (complete / fail / next-quest) | ✓ |
| Papyrus quest effects — `Start`/`Stop`/`Complete`/`Reset`/`SetActive`/`FailAllObjectives` | ✓ |
| QUST VMAD stage→fragment dispatch from vanilla `.pex` | ✓ M47.2 |
| **Legacy ObScript quest scripts (Oblivion/FO3/FNV) — compiled SCDA execution** | ✓ M47.3 (2026-09-18): each running quest's `GameMode` bytecode executes on the vanilla 5 s cadence, encodings/command ids derived from vanilla `Oblivion.esm`; 120 auto-started quests ticked live (the 2026-09-18 note that script-driven `SetStage`s advanced MS23 to stage 90 is withdrawn: it predates the 2026-10-18 fix to calls on references inside expressions). Object scripts, item scripts, reference/remote variables, ~60 effect-bridge commands and the actor-value codes shipped 2026-10-18 (84 % / 83 % / 81 % of FNV / FO3 / Oblivion call-site lines modelled); animation (`PlayGroup`), caster-runtime (`ScriptEffect*`, `Cast`), crime/detection and presentation commands stay declined — see `docs/engine/scripting.md` |
| Save-persistent quest progress | ✓ M45 |
| Alias fill — direct / unique / condition / XLRT / external / near / closest / force-into (loaded refs) | ✓ |
| Alias reservations + quest-lifetime semantics | ✓ |
| Faction + inventory injection from alias data | ✓ |
| Authored alias metadata without an owning subsystem | ~ exposed as runtime overlays, not fabricated results |
| Scene (`SCEN`) playback + PACK scene-package actions | ✓ MQ101 vertical slice |
| Console observability — `quest.show`, `quest.aliases`, `quest.start`/`stop`/`setstage` | ✓ M43.1 |
| Story Manager event payload / search | ✓ Phases 0–4 (SMBN/SMEN/SMQN decode, dispatcher, R1/R2/L1/L2 payload slots, policy bits, KILL/CLOC/AHEL producers, `sm1-story-manager.sh`); SCPT/LEVL/SKIL producers remain |
| Reference collections filled by scripts (`AddRef` / `AddArray` / `RemoveRef` / `GetCount` / `GetAt` / `Find`) | ✓ `crates/scripting/src/alias_collections.rs` |
| Reference collections filled by conditions or a fill type; true LCTN / unloaded-world alias queries | ✗ |
| Created-object spawning from aliases ("Create Reference to Object" At another alias; `PlaceAtMe`) | ✓ `crates/scripting/src/created_references.rs` (`eeb6793b7`, `3299c7dcb`) |
| Dialogue tree + dialogue UI integration | ~ single-level topic selection + native response surface (P4, ab31cfefe/766e1746e — live in MarkarthWarrens); full tree + dialogue UI open |

Smoke test: [`docs/smoke-tests/m43-quest-runtime.sh`](smoke-tests/m43-quest-runtime.sh)
drives the production ESM → runtime → TCP command path against installed
Skyrim data.

---

## Gameplay / Combat (Playable Vertical Slice — P0-P5)

Execution plan: [`docs/engine/playable-vertical-slice.md`](engine/playable-vertical-slice.md).
Phases run P0 input/interaction → P1 reliable traversal → P2 combat → P3 game
UI/inventory → P4 authored objective/dialogue → P5 persistence/soak;
capability on this route takes precedence over renderer polish.

| Feature | Status | Notes |
|---|---|---|
| P0 — door / `[E]` interaction | ✓ Closed 2026-08-10 | Camera-forward XTEL target → native `[E] Open` prompt → one bound E-key edge → canonical `ActivateEvent` → deferred cell-transition arrival. Smoke: [`p0-door-interaction.sh`](smoke-tests/p0-door-interaction.sh) |
| P1 — reliable character control | ✓ Closed 2026-10-01 | Movement/jump/sprint consumers share one once-per-frame `ActionState` snapshot; native Escape menu owns pause/focus/cursor transfer; settings-backed key rebinding. Closed by the live traversal gates; gamepad physical sources are a named follow-on (ROADMAP), not a closure blocker. Smoke: [`p1-character-traversal.sh`](smoke-tests/p1-character-traversal.sh) |
| P2 — melee combat core | ✓ Landed 2026-08-16, closed 2026-10-01 | Skyrim race Health + signed ACBS offset → actor-owned bone ray hit → bound Attack edge → canonical `HitEvent` → layered Health damage → one `Dead`/AI-disable transition → the existing 18-body ragdoll. Deterministic weapon selection (highest authored damage, FormID tie-break); explicit 8-damage unarmed fallback. Closed by the live combat gates; the Skyrim-route rapier multi-SAP panic is tracked as #5161, not a P2 blocker. Smoke: [`p2-melee-core.sh`](smoke-tests/p2-melee-core.sh) |
| NPC combat AI (MQ101 slice) | ~ Landed 2026-09-13 | `Faction.SetEnemy` / `Actor.StartCombat` arm `AiCombatState`; `npc_combat_ai_system` chases its target in a straight line and strikes on cooldown through the same `HitEvent` → `combat_damage_system` pipeline as the player, with creature damage from `CreatureAttack`. Ambient hostility (#4414, `systems/faction_hostility.rs`): an NPC with decoded `AIDT` aggression starts combat against the nearest actor it sees within `fSneakMaxDistance` whose faction reaction (scripted `SetEnemy` override, then authored `XNAM`, then Neutral) its aggression attacks; `FactionRelations` is directed, `FormRef`-keyed and saved. Not yet: detection meter (sound/light/sneak), warn radii, crime hostility, Oblivion's 0–100 aggression, ranged/magic attacks, attack animation |
| Authored attack/hit/death animation + sound | ~ Draugr family | `byroredux/src/systems/combat_anim.rs` (`ec3a18d2f`): attack / hit / death one-shot takes for Draugr-family actors plus the spatial swing / impact / death-voice sound family (fixture `docs/engine/p2-combat-anim-sound-fixture.md`). Other races and creatures play no combat takes yet |
| Corpse interaction / loot transfer | ✓ Closed 2026-09-30 | Corpse = container: lethal-combat-then-physical activation loots a corpse through its collider (`lethal_combat_then_physical_activation_loots_corpse_through_its_collider`), `is_loot_source` gates container counts, loot-appearance import carries the outfit (`npc_spawn/loot_appearance.rs`); live Skyrim corpse-loot validation 2026-09-17 |
| P3 — inventory and native game UI | ✓ Closed 2026-10-01 | Native HUD bars / crosshair / interaction prompts / notifications / objective text from canonical ECS state; container / corpse / pickup interaction with an inventory screen; equip through `Inventory` + `EquipmentSlots` + mesh attachment; pause / menu routing and settings-backed rebinding. Smoke: [`p3-hud.sh`](smoke-tests/p3-hud.sh), [`p3-player-body.sh`](smoke-tests/p3-player-body.sh) |
| P4 — authored objective and dialogue loop | ✓ Closed 2026-09-30 | MS01 "The Forsworn Conspiracy" in MarkarthWarrens: NPC activation → dialogue selection → presented response → objective-chain transitions over real QUST/DIAL/INFO/PACK data, with TIF_ fragments and journal announcements. Smoke: [`p4-quest-route.sh`](smoke-tests/p4-quest-route.sh) |
| P5 — persistence and session hardening | ✓ Closed 2026-10-01 | F5/F9 bound-input quicksave/quickload, quest-state revert/restore, cross-cell door saves, process-restart `--load` continuity, graceful quit, `BYRO_VALIDATION=1` clean leg, 30-minute transition/save soak. Smoke: [`p5-f5-f9-quicksave.sh`](smoke-tests/p5-f5-f9-quicksave.sh), [`p5-quest-persistence.sh`](smoke-tests/p5-quest-persistence.sh), [`p5-door-transition.sh`](smoke-tests/p5-door-transition.sh), [`p5-save-restart.sh`](smoke-tests/p5-save-restart.sh), [`p5-soak.sh`](smoke-tests/p5-soak.sh) |
| Save → exit → reload continuity | ✓ | M45/M45.1 — atomic rotating slots, interior/exterior live reload, stable FormID delta overlay, player pose, typed preflight, corrupt-slot fallback and player notifications |

Implementation: `byroredux/src/combat.rs` (melee vertical slice), `byroredux/src/systems/combat_ai.rs` (NPC combat AI), `byroredux/src/interaction.rs` (Activate/E-key), `byroredux/src/systems.rs` + action-state consumers (movement).

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
| Native game menu (Pause / Settings / Inventory) | ✓ Shipped 2026-08-15/16 — `byroredux-debug-ui`'s egui `GameMenuPage::{Pause,Settings,Inventory}` (`crates/debug-ui/src/panels.rs`); native `InventorySnapshot`/`InventoryAction` bridge over the canonical `Inventory`/`EquipmentSlots` components (`byroredux/src/inventory.rs`); validated TOML-persisted settings with stale-entry recovery (`byroredux/src/settings_io.rs`). Runs alongside Scaleform, not a replacement. Save/load toasts are live. Container/corpse transfer shipped (`container_loot_system` — activation take-all + corpse looting, #4712), and the visible player mesh shipped (a070baaad body + view toggle, db8351587 walk/idle, 0182fc5e8 gear import; `p3-player-body.sh`); general HUD bars and quest-objective presentation remain open. |

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
| Ruleset wired (`CharacterRuleset`: derived-stat formulas + leveling model) | ~ built, unwired | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ |
| NPC actor-value population at spawn | ✗ | ✓ class auto-calc | ✓ class auto-calc | ✓ pools + 18 skills | ✓ stored `PRPS`+`DNAM` | ✗ wire layout uncaptured (#4453) | ✗ wire layout uncaptured (#4453) |
| Creature (`CREA`) actor-value population at spawn | ✗ `CREA.DATA` layout unsourced | ✓ SPECIAL + Health | ✓ SPECIAL + Health | n/a (no `CREA`) | n/a | n/a | n/a |
| Runtime leveling (XP grant / level-up) | ✗ | ✓ | ✓ | ~ | ~ | ✗ | ✗ |
| Pool regen tick (Fatigue/Magicka — Health deliberately unmodelled, Stamina has no row; #4463) | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert |
| Affliction tick (radiation/disease/addiction) | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert | ✗ inert |

`oblivion_ruleset()` (`crates/core/src/character/tes.rs`) builds a real
`CharacterRuleset`, but `CharacterRulesProfile::OBLIVION` still maps to
`RulesetBuilder::None`, so it never reaches a live actor. Skyrim's
`skyrim_ruleset()` was wired by #3848 (`RulesetBuilder::Skyrim`), which also
makes it the first family whose leveling model reads authored GMSTs at load. FO76/Starfield have data captures
(`charal-fo76-ruleset.md`, `charal-starfield-ruleset.md`) but no ruleset
builder at all; Starfield's is additionally blocked on its XP/level curve and
category-spend thresholds being unpublished research (charal.md §9).

`CREA` records get their own stat model (`NpcStatModel::CreatureData`,
#3390): the 7 SPECIAL and Health straight off the record's own `DATA`, for
1,578 FNV and 533 FO3 creatures. It deliberately omits `DATA`'s three
aggregate skills (Combat / Magic / Stealth) because FO3/FNV publish no `AVIF`
they map onto and inventing one would be a guess. `DATA.Damage` is likewise
not an actor value — it reaches the spawned entity as the dedicated
`CreatureAttack` component instead (#3762), which `combat::attack_damage`
reads in place of its flat unarmed baseline when a creature is the
aggressor. Since `f61ea044` that can happen: `npc_combat_ai_system` is a
second `HitEvent` producer beside the player's melee, so a creature that a
script arms with `Actor.StartCombat` strikes for its authored damage.
Nothing arms combat ambiently, though, so most creatures still never attack.

Skyrim's NPC population derives Health, Magicka and Stamina, each
independently from its own `RACE.DATA` starting value plus its own signed
`ACBS` offset, and all 18 skills by the UESP `CLAS` formula
(`derive_skyrim_actor_values` + `derive_skyrim_skills`, `7c740eca0`;
a non-auto-calc NPC keeps its authored `DNAM` skill values). FO4 is the
only game whose "stored" mechanism populates actors (`NPC_` `PRPS` property
pairs pass through verbatim, plus baked `DNAM` Health/Action Points),
exercised against real content. FO76/Starfield still run the `PRPS`/`DNAM`
decoders (`crates/plugin/src/esm/reader.rs`, the `DNAM` tail flagged
unconfirmed), but since #4453 their profiles pin `NpcStatModel::None` — the
parse reaches no actor until a wire-layout capture lands (#4453).

Runtime leveling is wired end to end for FO3/FNV and partially for FO4/Skyrim;
Oblivion, FO76 and Starfield still have no live leveling path. `grant_experience`
increments `CharacterLevel.xp`, rolls levels to the active model's cap and
banks modeled rewards. Kill XP, `RewardPlayerXP`, `levelup xp` and Skyrim's
`Game.AdvanceSkill` can reach it; see the Coverage Matrix above for each game's
leveling coverage. Pool regen remains inert: `pool_regen_tick_system` is
registered in `Stage::Update` every frame but early-returns forever because
its required `PoolRegenConfig` resource is inserted only inside unit tests,
never in `byroredux/src/boot/`. `affliction_tick_system` is not registered in
the scheduler at all.

---

## Starfield-Specific

| Feature | Status |
|---|---|
| ESM parse (CELL / REFR / record types) | ✓ 99.9% vanilla records |
| BSGeometry `.mesh` external reference resolution | ✓ |
| Starfield CDB material system (`materialsbeta.cdb`) | ✓ Phase 1 |
| XCLL 108-byte interior lighting (volumetric height-fog model) | ✓ |
| Static-trimesh collider synthesize from render geometry | ✓ |
| `.hkx` animation skeleton | ✗ — `crates/hkx` reads Skyrim's Havok 2010 and FO4's Havok 2014 packfiles, not Starfield's layout |

---

## What Doesn't Work Yet (live gaps as of 2026-10-22)

<!-- TD3-002: Save/load (M45/M45.1) removed — shipped 2026-06-21. The M47.2
     row below is the *full* transpiler, which is genuinely still deferred;
     the `.pex` recognizer slice that shipped is annotated inline. -->


| Gap | Blocking what | Milestone |
|---|---|---|
| Starfield Havok `.hkx` loader | Starfield animation | M41.x follow-up |
| General NPC locomotion from `.hkx` / behavior graphs | Skyrim actors animating outside the MQ101 cart-idle catalog; FO4 ambient NPC walking | M41.x follow-up |
| `.btr` terrain normal maps | distant-terrain normal detail on Skyrim SE/FO4. Distance-based multi-band selection is **no longer a gap** — the four-level ladder shipped in #2371 and runs on every quad-based scheme, FO3/FNV included (#3508) | M35 |
| Remaining `PACK` procedures (Find/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/UseWeapon) | NPCs perform item-use/combat/magic behaviors | M42 (Tier 7) |
| Full Papyrus transpiler (M47.2) + VM | Arbitrary script execution on real content — the recognizer slice ships the static path and the bounded VM (`fragment/vm.rs`) runs the scripts the lowering declines | M47.2 (Tier 3) |
| Script-extender compatibility layer verified against real mods | Confidence that SKSE-family mod scripts (StorageUtil / JContainers / ModEvent consumers) actually run. The layer *exists* — ~23.9k LOC, six provider families, tested in-crate — but no audit pass has exercised it against shipped mod content, so its real-world coverage is unmeasured, not zero and not proven (#3953) | M47.2 follow-up |
| Full Scaleform menus | In-game UI (method behavior / `_global.gfx`; native menu covers Pause/Settings/Inventory in parallel) | M48 / R4 decision |
| Per-material footsteps (FOOT) | Correct surface audio | M44 follow-up |
| CHARAL: player actor-value seed | The player body carries `ActorValues` seeded from the base Player `NPC_` (#4458), with the ruleset's PlayerOnly rows (FO4/FO3/FNV Health + AP) evaluated for the player at stamping (#4674). Games: FO3/FNV/FO4/Skyrim yes, Oblivion no, FO76/Starfield no (NPC stat wire layout uncaptured, #4453) | CHARAL (charal.md §7) |
| CHARAL: Oblivion ruleset built but unwired; regen + affliction ticks inert everywhere | Derived Health/leveling formulas on Oblivion; passive Fatigue/Magicka regen (the built tick's only pools — Health deliberately unmodelled per charal.md §4.7, Stamina rates unsourced/with no row, #4463) and radiation/disease/addiction on all seven games | CHARAL (charal.md §8) |
