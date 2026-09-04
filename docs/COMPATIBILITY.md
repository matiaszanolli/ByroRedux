# ByroRedux — Compatibility Chart vs. Gamebryo / Creation Engine

**What this is.** A feature-by-feature checklist of what ByroRedux does today
measured against the original Gamebryo → Creation Engine lineage
(Oblivion 2006 → Starfield 2023). One row per capability, marked from the
live code, not from intent.

**Legend**

| Mark | Meaning |
|:----:|---------|
| `[x]` | **Complete** — works end-to-end on real game data, or exceeds the original |
| `[~]` | **Partial** — a real, exercised slice ships; known gaps remain |
| `[ ]` | **Pending** — not implemented (parsing-only counts as pending for a runtime row) |

**Baseline column** = what the original engine did, so "complete" means
*parity with that*, not parity with an imagined ideal. Rows marked 🔺 are
places where ByroRedux is already **ahead** of every shipping Bethesda title.

**Last synced:** 2026-07-27 · against `ROADMAP.md` (verified 2026-07-26,
Session 61) and a live source sweep at HEAD `1d94eb24`.

---

## Scorecard at a glance

| Domain | Complete | Partial | Pending | Feel |
|---|---:|---:|---:|---|
| File formats & data | 15 | 6 | 3 | ████████░░ Strongest area — 7 games parse |
| Rendering — geometry | 11 | 4 | 3 | ███████░░░ |
| Rendering — lighting | 12 | 3 | 3 | ████████░░ Ahead of baseline 🔺 |
| Post-process & AA | 8 | 2 | 3 | ███████░░░ |
| Sky, weather, atmosphere | 8 | 3 | 4 | ██████░░░░ |
| World & streaming | 8 | 4 | 4 | ██████░░░░ |
| Actors & animation | 6 | 5 | 6 | ████░░░░░░ Weakest visible gap |
| Physics & collision | 7 | 3 | 5 | █████░░░░░ |
| Scripting & logic | 8 | 5 | 4 | ██████░░░░ |
| Quests, dialogue, AI | 4 | 6 | 8 | ███░░░░░░░ Biggest surface left |
| Character / RPG rules | 6 | 4 | 6 | ████░░░░░░ |
| UI & menus | 4 | 4 | 7 | ███░░░░░░░ |
| Audio | 7 | 2 | 4 | ██████░░░░ |
| Save / load | 6 | 1 | 3 | ███████░░░ |
| Modding & plugins | 5 | 3 | 3 | ██████░░░░ |
| Tooling & diagnostics | 8 | 1 | 3 | ████████░░ Ahead of baseline 🔺 |

Bars are eyeballed weight, not a computed metric — read the rows, not the bars.

---

## 1. File formats & data

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **NIF mesh parser** | Gamebryo binary, v10.x → 34.1.1.3 | 184 886 files sweep across 7 games; 100 % recoverable on all seven |
| `[x]` | NIF — Oblivion / NetImmerse v10.x–v20.0.0.5 | TES4 | 99.93 % clean (8 026/8 032); 6 pre-Gamebryo-era truncations remain |
| `[x]` | NIF — FO3 / FNV v20.2.0.7 | — | 100 % clean (10 989 / 14 881) |
| `[x]` | NIF — Skyrim SE `BSTriShape` packed half | — | 100 % clean (18 862) |
| `[x]` | NIF — FO4 half-float + `BSSubIndexTriShape` | — | 100.00 % clean (159 866) |
| `[x]` | NIF — FO76 | — | 100 % clean (58 469) |
| `[x]` | NIF — Starfield `BSGeometry` + external `.mesh` | — | 99.99 % aggregate (89 270/89 276); 6-file residual in `MeshesPatch` |
| `[x]` | **BSA archives** v103 / v104 / v105 | Oblivion → Skyrim SE | zlib + LZ4 frame; sibling auto-load (`Textures0` → `1..8`) |
| `[x]` | **BA2 archives** v1/v2/v3/v7/v8 | FO4 → Starfield | GNRL + DX10 with reconstructed DDS headers |
| `[x]` | **DDS textures** BC1/BC3/BC5, FourCC + DX10 | — | Full mip chain, staging + layout transitions |
| `[x]` | **ESM/ESP/ESL** record walker | TES4 → Starfield | ~25 structured record types; 77 828 records on `FalloutNV.esm` |
| `[x]` | `.kf` / `.kfm` animation containers | Gamebryo 1.2.0.0 → 2.2.0.0 | 8 controller types, TBC + Hermite + B-spline decompression |
| `[x]` | `.bgsm` / `.bgem` materials | FO4 | Dedicated `byroredux-bgsm` crate |
| `[x]` | Starfield `materialsbeta.cdb` | Starfield | `byroredux-sfmaterial` crate |
| `[x]` | `.pex` compiled Papyrus | Skyrim+ | Champollion port; 26 640/26 641 of corpus, zero panics |
| `[~]` | `.psc` Papyrus source | Skyrim+ | Lexer + Pratt parser + full AST; no compiler back-end |
| `[~]` | FO4 `.csg` precombined geometry | FO4 | Format cracked from first principles; `_precomb.nif` collision + `.uvd` occlusion still open |
| `[~]` | `.spt` SpeedTree | Oblivion/FO3/FNV/Skyrim | TLV walker + placeholder billboard fallback; no real tree geometry |
| `[~]` | FaceGen `.egm` / `.tri` morphs | — | Dedicated crate; head morphs compose, GPU morph runtime deferred |
| `[~]` | `.swf` Scaleform / GFx | — | Pinned Ruffle VM; AVM1 (Skyrim) + AVM2 (FO4) host profiles |
| `[~]` | Havok `.hkx` | — | **Skeletons load** (they're NIFs); *animation* decode not started |
| `[ ]` | Havok `BhkSystemBinary` blob | FO4+ | Blocks FO4/FO76/Starfield ragdolls |
| `[ ]` | `.lod` / `.btr` / `.bto` object LOD | Skyrim/FO4 | Terrain LOD landed; **object** LOD is the big outdoor gap |
| `[ ]` | Morrowind NIF v3.x/v4.x | TES3 | Explicit anti-scope — different format family |

## 2. Rendering — geometry & materials

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | Static mesh rendering | `NiTriShape`/`NiTriStrips`/`BSTriShape` | All shape families → one unified vertex format (104 B) |
| `[x]` | Instanced draw batching | Creation Engine batching | Indirect draw + material-dedup SSBO |
| `[x]` | Texture slots — diffuse/normal/spec/glow/env | Gamebryo `BSShaderTextureSet` | Cross-game roles unified at parse time (`1d94eb24`) |
| `[x]` | Per-fragment normal mapping | — | Authored `NiBinaryExtraData` tangents + Mikkelsen synthesis fallback |
| `[x]` | PBR metalness/roughness translation | ✖ (original is Blinn-Phong-ish) 🔺 | Canonical `Material` at the NIFAL boundary; no per-game branches in shader |
| `[x]` | Alpha test / alpha blend / two-sided | `NiAlphaProperty` | Sort key + render-layer components |
| `[x]` | Vertex colors | `NiVertexColorProperty` | Widened `vec3`→`vec4` |
| `[x]` | Cubemap environment mapping | `BSShaderPPLightingProperty` | |
| `[x]` | Skinned mesh rendering | Creation Engine GPU skinning | 4 096-slot bone palette, GPU compute pre-skin (M29.5/M29.6) |
| `[x]` | Particle systems | `NiPSys*` | Authored emitter params + birth rate + grow/fade decoded, not preset kinematics |
| `[x]` | Billboards / `NiBillboardNode` | — | All 4 billboard modes, camera-change gated |
| `[~]` | Landscape terrain | LAND heightmap + LTEX splat | Renders; texture blend quality not yet A/B'd against original |
| `[~]` | Terrain LOD | Skyrim/FO4 prebaked LOD | Prebaked tiers land; dynamic stitching partial (M35) |
| `[~]` | FO4 precombined geometry | FO4 | CSG spawns + textures via owning REFR slot indices; LOD tier selection fixed |
| `[~]` | Decals | Creation Engine decals | ECS component + render layer exist; projection pass is M59 (pending) |
| `[ ]` | Distant object LOD / impostors | Skyrim/FO4 LOD meshes | **Known must-have gap** — tracked under EXAL |
| `[ ]` | Grass (GRAS records) | Oblivion+ grass shader | Record type dispatched; no instanced grass renderer |
| `[ ]` | Pre-skinned raster path (M29.3) | — | Raster still re-skins inline; compute output only feeds RT |

## 3. Rendering — lighting & shadows

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **Ray-traced shadows** | ✖ (shadow maps) 🔺 | Ray queries; no cascades, no peter-panning, no PCF |
| `[x]` | **Ray-traced reflections** | ✖ (cubemaps / SSR in FO4) 🔺 | |
| `[x]` | **1-bounce ray-traced GI** | ✖ (baked/none) 🔺 | |
| `[x]` | Multi-light SSBO pipeline | Fixed 4-light forward | Effectively unbounded light count |
| `[x]` | Streaming RIS / ReSTIR-DI | ✖ 🔺 | 16 reservoirs/fragment, weight clamped 64× |
| `[x]` | **SSAO** (ambient occlusion) | ✖ on Oblivion/FO3/FNV; SSAO in FO4 | Compute pipeline, noise texture + hemisphere kernel |
| `[x]` | SVGF temporal denoiser | ✖ 🔺 | Motion-vector reprojection + mesh-ID disocclusion |
| `[x]` | SVGF spatial à-trous filter | ✖ 🔺 | |
| `[x]` | Clustered light culling | Creation Engine (FO4+) | Compute froxel assignment |
| `[x]` | Interior lighting from LGTM/CELL | Lighting templates | Per-field inheritance honored |
| `[x]` | Exterior sun + TOD ambient/fog | Creation Engine | Per-frame sun arc from game time, WTHR NAM0 |
| `[x]` | Light animation controllers | `NiLightController` | Flicker/pulse via `AnimatedColor` |
| `[~]` | **Volumetric lighting / god rays** | ✖ on ≤Skyrim; FO4 has volumetric | Froxel inject + integrate compute passes ship (M55 shaders live); REGN-driven density pending |
| `[~]` | BLAS/TLAS management at scale | ✖ 🔺 | Compaction + LRU eviction + refit; skinned BLAS per-entity |
| `[~]` | Emissive / glow materials | `BSEffectShaderProperty` | Intensity control shipped (EFFECT-LIT); layering pending |
| `[ ]` | IBL / per-cell HDR sky probe | ✖ | M-LIGHT |
| `[ ]` | Multi-bounce GI (≥2) | ✖ | M51 path-tracing mode |
| `[ ]` | Soft shadow penumbra from RT samples | ✖ | M-LIGHT |

## 4. Post-processing & anti-aliasing

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | ACES tone mapping | Reinhard-ish / none | Composite pass |
| `[x]` | **TAA** | FXAA (Skyrim) / TAA (FO4) | Halton(2,3) jitter + YCoCg variance clamp + mesh-ID disocclusion |
| `[x]` | **FSR 3.1.4 upscaling** | ✖ 🔺 | Engine default at Quality; 4 presets, reactive + T&C masks, runtime `r.upscaler` switch |
| `[x]` | FSR dispatch-failure fallback | ✖ | Falls back cleanly, telemetry-reported |
| `[x]` | Bloom | Creation Engine bloom | Dual-filter downsample/upsample pyramid |
| `[x]` | Fog — display-space blend | HDR-space fog | Fixed post-ACES (LIGHT-N2); removes interior yellow wash |
| `[x]` | Motion vectors / G-buffer | — | Normal, motion, mesh-ID, albedo, raw indirect |
| `[x]` | SSIM quality fence | ✖ 🔺 | 5 deterministic camera paths gate every upscaler preset |
| `[~]` | Imagespace modifiers (IMGS) | Creation Engine IMAD/IMGS | Record dispatched; runtime application partial |
| `[~]` | Water rendering | Skyrim/FO4 water | Vertex displacement + RT reflection/refraction + caustic splat; **physics half absent** (WATAL) |
| `[ ]` | Depth of field | Creation Engine DOF | M58 |
| `[ ]` | Color grading / 3D LUT | Creation Engine | M58 |
| `[ ]` | Per-object motion blur | Creation Engine | M58 — motion vectors already exist |

## 5. Sky, weather & atmosphere

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **Sky gradient** | Gamebryo skydome | 10 color groups × 6 TOD slots interpolated |
| `[x]` | Sun disc with game-time arc | — | Per-frame arc from world time |
| `[x]` | Procedural cloud body | WTHR cloud deck | Continuous wind-advected multi-octave field; WTHR classification drives coverage and PNAM/JNAM tint it, while authored DDS layers supply game-specific detail |
| `[x]` | Cloud layers 1–4 (DNAM/CNAM/ANAM/BNAM) | 4 layers | Parallax scroll on all four |
| `[x]` | Weather fade transitions | 8 s crossfade | `WeatherTransitionRes` post-TOD-sample blend |
| `[x]` | Horizon fog | — | |
| `[x]` | Time-of-day interpolation | — | |
| `[x]` | CLMT climate records | — | Sunrise/sunset/moon-phase hours parsed |
| `[x]` | Procedural sky fallback | ✖ | For cells with no WTHR |
| `[~]` | WTHR record coverage | Full weather | DATA timing/frequency/lightning/wind controls, PNAM/JNAM cloud tint/motion, Skyrim extra sky tables and 32 cloud paths parsed; four portable cloud layers are rendered across all games |
| `[~]` | REGN region records | Region weather/sound/objects | Dispatched; region-driven ambient + fog density pending |
| `[~]` | Interior fill lighting split | — | 0.6× unshadowed fill; heuristic, not data-driven |
| `[x]` | Moons & stars | Skydome moon/star planes | Procedural moon disc and hashed star field use the live TOD palette and WTHR star colour |
| `[x]` | Precipitation (rain / snow particles) | Particle weather | Exterior screen-space rain/snow overlay with separate streak/flake behaviour; close impacts remain authored particle work |
| `[x]` | Aurora (Skyrim) | — | Procedural animated aurora gated by Skyrim WTHR aurora flags |
| `[x]` | Lightning strikes | — | Deterministic weather-frequency flashes with authored DATA lightning colour |

## 6. World, cells & streaming

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **Interior cell loading** | — | Verified on Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield — one `cell_loader` |
| `[x]` | Exterior grid loading | 5×5 loaded grid | 7×7 (radius 3) default, radius 1–7 |
| `[x]` | **World streaming** | Cell-buffer streaming | `WorldStreamingState` + async pre-parse + hysteresis + LRU BLAS eviction |
| `[x]` | Interior ↔ exterior transitions | Door teleports | XTEL → `DoorTeleport` → full grid respawn at destination |
| `[x]` | WRLD worldspace hierarchy | Parent worldspaces | Selective data inheritance |
| `[x]` | CELL 4-group structure | — | Persistent / temporary / VWD split |
| `[x]` | Interior spawn point | `coc` behavior | First door's own placement (vanilla has no auto spawn-point logic either) |
| `[x]` | Multi-master load order | Skyrim+ DLC | Repeatable `--master`, DLC interiors verified |
| `[~]` | Oblivion exterior | TES4 worldspace | Parse + load ✓, game-agnostic wiring; **on-device render bench pending** |
| `[~]` | XCLL / lighting-template inheritance | Per-field inheritance | Canonical-size split landed for Starfield |
| `[~]` | SCOL static collections | Oblivion/FNV | 98 records parsed on FNV; render path not proven |
| `[~]` | Occlusion culling | Occlusion planes / `.uvd` | Frustum culling ships; portal/occlusion volumes pending |
| `[ ]` | Object LOD across cell distance | LOD meshes + tree LOD | The headline outdoor gap (EXAL) |
| `[ ]` | Water physics volumes | Swim / drown / buoyancy | WATAL physics half is nonexistent |
| `[ ]` | Fast travel / map markers | — | Records referenced only |
| `[ ]` | Havok precomputed cell physics | — | |

## 7. Actors, skeletons & animation

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | NPC spawning at REFR positions | — | FNV `GSDocMitchellHouse` (Doc Mitchell), Skyrim Bannered Mare (6 named NPCs) |
| `[x]` | Skeleton loading | `skeleton.nif` | All games incl. FO4 — it's a NIF, not an `.hkx` |
| `[x]` | Body + head + hands composition | — | No long-spike artifact; verified via `skin <id>` |
| `[x]` | FaceGen head morphs | FaceGen | Heads render composed |
| `[x]` | Equipment / outfit equip | ARMO/ARMA, biped slots | M41 verified end-to-end on Skyrim+ / FO4 via smoke test |
| `[x]` | `.kf` animation playback | — | Real `mtidle.kf` produces measured joint deltas |
| `[~]` | Animation blending stack | Behavior-graph blending | Layer stack + blended sampling ships; no behavior graph |
| `[~]` | Root motion | — | `split_root_motion` exists; not driving locomotion |
| `[~]` | Text-key / anim-note events | `BSAnimNote` | Collected with IK hints; consumers thin |
| `[~]` | Inventory & equip slots | — | `Inventory` + `EquipmentSlots` + `ItemInstancePool` ship; no equip *gameplay* |
| `[~]` | Furniture / idle markers | Sit/sleep markers | `sandbox_seat_system` v0 behind `BYRO_SANDBOX_SIT`; sit-enter transition missing so actors float |
| `[ ]` | **Havok `.hkx` animation decode** | The actual animation format for Skyrim+ | NPCs stand in bind pose — single biggest visible gap |
| `[ ]` | Behavior graphs (`.hkb`) | Skyrim+ behavior system | |
| `[ ]` | IK — foot placement / look-at | — | |
| `[ ]` | Facial animation / lip sync | FaceFX / `.lip` | |
| `[ ]` | GPU morph runtime | — | M41.0.5 |
| `[ ]` | Creature / non-humanoid rigs | — | Not exercised |

## 8. Physics & collision

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | Physics backend | Havok | **Rapier3D** — clean-room, no Havok linkage |
| `[x]` | `bhk*Shape` parsing — all 13 variants | — | Sphere/MultiSphere/Box/Capsule/Cylinder/Convex/List/Transform/Mopp/ConvexList/TriStrips/Packed/CompressedMesh |
| `[x]` | Static collision from NIF | — | 416 colliders on Skyrim architecture after the mass=0 reclassification fix |
| `[x]` | Character controller | Havok character proxy | Capsule + gravity + collide-and-slide + jump + autostep, vanilla-Skyrim dimensions |
| `[x]` | Rigid body dynamics | — | `bhkRigidBody` → Rapier |
| `[x]` | **Ragdoll (FNV slice)** | Havok ragdoll | Constraint chain → Rapier **multibody**; 18-body Doc Mitchell verified |
| `[x]` | Trigger volumes | XPRM primitives | Box + sphere, drives `OnTriggerEnter` |
| `[~]` | Ragdoll — Oblivion / FO3 / Skyrim | — | Constraint CInfo decode converged; not all verified on device |
| `[~]` | Havok material → footstep/impact | FOOT records | Material read; footstep sound still hardcoded to dirt |
| `[~]` | PHYSAL abstraction layer | ✖ 🔺 | Double-ended design; per-game seam isolated to constraint CInfo decode |
| `[ ]` | Ragdoll — FO4 / FO76 / Starfield | — | Blocked on `BhkSystemBinary` |
| `[ ]` | Havok cloth / hair physics | Skyrim+ | Parser arm exists; no sim |
| `[ ]` | Destruction (DEST records) | — | |
| `[~]` | Navmesh pathfinding | NAVM/NAVI | Geometry + cross-cell links decoded for Oblivion-era typed `NVVX`/`NVTR`/`NVEX` **and** the Skyrim-era packed `NVNM` (#2738); FO4 keeps its blob (body layout diverges) though every tile still locates. **No path graph, no A\*, no actor consumer yet** |
| `[ ]` | Water buoyancy / swimming | — | |

## 9. Scripting & game logic

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **ECS-native scripting model** | Papyrus stack VM 🔺 | No VM, no fibres, no suspendable frames — validated by the R5 prototype |
| `[x]` | Papyrus `.psc` parser | — | Lexer + Pratt + full AST |
| `[x]` | `.pex` decompiler | — | Champollion port, 99.996 % of shipping corpus |
| `[x]` | Event-hook dispatcher | Papyrus events | Marker components: `OnActivate`, `OnHit`, `OnTriggerEnter`, `OnCellLoad`, … |
| `[x]` | **CTDA condition evaluator** | ~300 condition functions | OR-precedence quirk reproduced exactly (`A AND (B OR C) AND D`) |
| `[x]` | Script timers | `Utility.Wait` / `RegisterForSingleUpdate` | dt-driven `ScriptTimer` + `TimerExpired` |
| `[x]` | Global variables (GLOB) | — | |
| `[x]` | VMAD script attachment | — | Per-REFR resolution from `--scripts-bsa` at attach time |
| `[~]` | Quest-stage advance recognizer | `SetStage` | Trigger families plus compiled QUST/SCEN fragment dispatch; arbitrary Papyrus remains decline-by-default |
| `[~]` | QUST/SCEN fragment lowering | Quest/scene fragments | Stage/phase bindings decoded from VMAD and `.pex`; conservative catalog includes objectives, conditionals, globals, lifecycle, object/reference, package and cinematic effects |
| `[~]` | Condition function coverage | Hundreds across the lineage | 19 modeled variants plus safe-default `Unknown`; actor/quest/cell/faction/level/equipment/perk/reputation/scene/script-variable consumers are live |
| `[~]` | Papyrus → ECS transpiler | — | Recognizer chain, not a general transpiler; 1 257 FO3 SCPT records still not driven |
| `[~]` | Script effects | — | `AddItem`, `MoveTo`, `Disable`, `SetValue`, conditional branches, quest/scene lifecycle, package re-evaluation and cinematic/control effects are live; unsupported shapes decline safely |
| `[ ]` | Full script-object API (101 types) | ScriptObject tree | |
| `[ ]` | Custom events / `RegisterForCustomEvent` | — | |
| `[ ]` | States (`GoToState`) | Papyrus states | Multi-state dispatch proven in R5; not generalized |
| `[ ]` | Console command parity | ~200 console commands | Custom debug commands exist instead |

## 10. Quests, dialogue & AI

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | QUST record parsing | — | 436 quests on FNV |
| `[x]` | DIAL/INFO record parsing | — | 18 215 dialogue records on FNV |
| `[x]` | PACK package records | — | PKDT + PSDT + PLDT decoded, verified on real `FalloutNV.esm` |
| `[x]` | Quest stage storage | Journal stages | `quest_stages.rs` |
| `[~]` | Quest alias system | 6 ref fill types | Parser ships incl. the ALFI "Force Into Alias" spec fix; runtime fill thin |
| `[~]` | AI package procedures | 30 composable procedures | Systems exist for wander / follow / travel / patrol / guard / escort / sandbox |
| `[~]` | Sandbox behavior | Sandbox package | v0 "sit in nearest free chair, once", gated behind an env var |
| `[~]` | Package schedule (PSDT) | Time-of-day scheduling | Parsed; scheduler wiring thin |
| `[~]` | Faction records (FACT) | — | Parsed; rank/relation runtime pending |
| `[~]` | Perk records (PERK) | 3 entry types, ~120 entry points | Catalogued + parsed; modifier pipeline not applied |
| `[ ]` | **Dialogue trees / conversation UI** | Full dialogue system | The single biggest unbuilt surface |
| `[ ]` | Voice lines / `.fuz` audio | — | |
| `[ ]` | Story Manager (~25 events) | Radiant Story | |
| `[ ]` | Quest objectives / journal UI | — | |
| `[ ]` | Package priority stack | Package stack evaluation | |
| `[ ]` | Combat AI | — | |
| `[ ]` | Crime / bounty / detection | — | |
| `[ ]` | Radiant quest generation | — | |

## 11. Character & RPG rules (CHARAL)

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **ActorValue system** | AVIF records | Typed (attribute/skill/resistance/resource), base + mods − damage composition |
| `[x]` | Fallout ruleset (SPECIAL + skills) | FO3/FNV | `character/fallout.rs` |
| `[x]` | TES ruleset | Oblivion/Skyrim | `character/tes.rs` + `skyrim.rs` |
| `[x]` | Derived stats | HP / AP / carry weight | `character/derived.rs` — VATS AP formulas match the wiki |
| `[x]` | Class records (CLAS) auto-calc | `skill = 2 + 2×SPECIAL + ceil(Luck/2)` | SPECIAL lives in `ATTR`, not `DATA` |
| `[x]` | Live ActorValue editing | `setav`/`modav` | Console commands |
| `[~]` | Leveling & XP | — | `character/leveling.rs`; tag-skill per-level formula undocumented → deferred |
| `[~]` | Regeneration | — | `character/regen.rs` |
| `[~]` | Reputation | FNV reputation | `character/reputation.rs` |
| `[~]` | Afflictions / diseases | — | `character/affliction.rs` |
| `[ ]` | **Combat resolution** | — | Damage formulas exist; no combat loop |
| `[ ]` | **VATS runtime** | FO3/FNV/FO4 | Formulas match; no AP pool, no time-pause, no limb health, no kill-cam |
| `[ ]` | Perk application pipeline | ~120 entry points | |
| `[ ]` | Spells / magic effects (SPEL/MGEF) | — | Records stubbed |
| `[ ]` | Skill checks (lockpick / speech / hack) | — | |
| `[ ]` | Character creation | — | |

## 12. UI & menus

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | SWF/Flash rendering | Scaleform GFx | Pinned Ruffle, offscreen wgpu → pixel readback → UI overlay pass |
| `[x]` | Skyrim `GameDelegate` contract | — | 74 SkyUI methods + 12 request contracts, re-entrant response routing |
| `[x]` | FO4 `BGSCodeObj` contract | — | 138 installed-corpus methods, generated AVM2 forwarding adapter |
| `[x]` | Archive-backed SWF resource resolution | — | Relative resources through BSA/BA2 |
| `[~]` | HUD menu | HUDMenu.swf | Loads + readiness/destruction checks pass; not driven by game state |
| `[~]` | Pip-Boy menu (FO4) | PipboyMenu.swf | Loads; lifecycle ownership verified |
| `[~]` | Input layer routing | Menu input stack | `ui_input.rs` + `input.rs` exist |
| `[~]` | Menu catalog | 34 Scaleform menus | `catalog.rs` enumerates; most unbacked |
| `[ ]` | Inventory / container / barter menus | — | |
| `[ ]` | Dialogue menu | — | |
| `[ ]` | Journal / quest log UI | — | |
| `[ ]` | World map / local map | — | |
| `[ ]` | Text replacement + book/terminal markup | Alias/Global tags, HTML-like markup | Catalogued in memory; unbuilt |
| `[ ]` | Font system (FontConfig) | — | |
| `[ ]` | FO3/FNV XML menus | Legacy non-Scaleform UI | Separate track, not started |

## 13. Audio

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | Audio backend | Bethesda/XAudio | **kira 0.10** via `byroredux-audio` |
| `[x]` | 3D spatial audio | — | Per-emitter spatial sub-tracks, lazy listener, prune-on-Stopped |
| `[x]` | BSA-decoded sound assets | — | `StaticSoundData::from_cursor` + `SoundCache` |
| `[x]` | Footstep audio | — | XZ-plane stride accumulator, vertical motion excluded |
| `[x]` | Looping emitters | — | Stops on `AudioEmitter` removal / cell unload, tweened |
| `[x]` | Streaming music | — | Multi-minute OGG, single-slot, crossfade |
| `[x]` | Reverb send per cell type | — | −12 dB interior, silent exterior, auto-flipped by `reverb_zone_system` |
| `[~]` | FOOT material lookup | Per-material footsteps | Havok material available; still hardcoded to dirt |
| `[~]` | REGN region ambient layers | Region sound | Records parsed; no ambient layer driver |
| `[ ]` | Occlusion / raycast attenuation | — | |
| `[ ]` | Voice / `.fuz` dialogue audio | — | |
| `[ ]` | MUSC music-type switching | — | |
| `[ ]` | Combat / stealth music states | — | |

## 14. Save & load

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **Full-ECS-World snapshot** | Delta-log save 🔺 | Not a delta log — structurally immune to Bethesda's slow-corruption tail |
| `[x]` | Versioned container | — | magic / major+minor / schema-fingerprint / CRC32 / len |
| `[x]` | Corruption rejection before parse | ✖ 🔺 | Bad magic / version skew / schema drift / truncation / CRC all rejected |
| `[x]` | **Pre-save validation gates** | ✖ 🔺 | Refuses to write a poisoned save: Parent⇄Children, equip indices, clip handles, dangling refs |
| `[x]` | Crash-safe atomic write + ring | ✖ 🔺 | tmp → fsync → read-back verify → rename; round-robin `SaveRing` |
| `[x]` | Stable Form ID persistence | Load-order-fragile IDs 🔺 | Persists the `FormIdPair`, never the session-local handle |
| `[~]` | Live load-apply | — | M45.1: cell reload + FormId-keyed deltas + player-pose restore |
| `[ ]` | Cosave compatibility with original saves | — | Explicit anti-scope (speculative) |
| `[ ]` | Save thumbnails / metadata UI | — | |
| `[ ]` | Autosave / quicksave policy | — | |

## 15. Modding & plugin system

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | Legacy plugin loading (ESM/ESP/ESL) | — | Multi-master CLI wiring (M46.0) |
| `[x]` | Form ID resolution | 8-bit load-order prefix | `FormId` / `PluginId` / `LocalFormId` / `FormIdPair` |
| `[x]` | **Content-addressed stable Form IDs** | ✖ 🔺 | No LOOT sorting, no 255-slot limit, no ID renumbering |
| `[x]` | Dependency DAG resolver | LOOT (external tool) 🔺 | In-engine, `plugin/resolver.rs` |
| `[x]` | Conflict resolution model | Last-wins overrides | `DataStore` + `ResolvedRecord` + `Conflict` |
| `[~]` | Native plugin manifests (TOML) | ✖ 🔺 | Manifest + `Record` component bundles defined; ecosystem unproven |
| `[~]` | Full load-order merge | — | M46 — discovery/sort/merge across a real load order |
| `[~]` | Archive-invalidation equivalent | — | Loose-file override path exists implicitly |
| `[ ]` | Script-extender equivalent (SKSE/F4SE) | Community-built | Native plugin system should subsume it |
| `[ ]` | Mod distribution (M72) | Nexus / Bethesda.net | Content-addressed hosting is Tier 10 |
| `[ ]` | Creation Kit equivalent (M50) | Creation Kit | The single largest user-visible win still unbuilt |

## 16. Tooling & diagnostics

| ✔ | Feature | Original baseline | Notes |
|:-:|---------|-------------------|-------|
| `[x]` | **Live ECS inspection over TCP** | ✖ 🔺 | `byro-dbg` on port 9876 |
| `[x]` | Papyrus-expression query language | ✖ 🔺 | `find`, `entities(Component)` against the live world |
| `[x]` | Console command set | ~200 vanilla commands | Different set: `tex.missing`, `skin`, `setav`, `cond`, `ragdoll`, `mat.*`, … |
| `[x]` | In-engine egui debug overlay | ✖ 🔺 | Draws over composite output |
| `[x]` | Screenshot capture over protocol | — | |
| `[x]` | Deterministic bench harness | ✖ 🔺 | `--bench-frames` / `--bench-hold`, wall-clock + FrameTimings sub-phases |
| `[x]` | Parse-rate regression tests | ✖ 🔺 | Per-game NIF + ESM floor tests over real archives |
| `[x]` | SPIR-V reflection cross-check | ✖ 🔺 | Descriptor layouts validated against shader declarations at pipeline create |
| `[~]` | Runtime telemetry baselines | ✖ | `audit-runtime` diffs against checked-in baselines; some baselines stale |
| `[ ]` | Profiler integration (Tracy/RenderDoc capture) | — | |
| `[ ]` | Asset preprocessing pipeline (M82) | Archive tools | |
| `[ ]` | In-engine world editor (M50) | Creation Kit | |

## 17. Beyond the original engine 🔺

Capabilities with **no counterpart** in any shipping Bethesda title.
These are the reason the rebuild exists.

| ✔ | Capability | Why it's only possible here |
|:-:|-----------|------------------------------|
| `[x]` | RT-first renderer (shadows + reflections + GI + denoise) | No forward-renderer legacy to preserve |
| `[x]` | Unbounded dynamic light count | SSBO + clustered culling instead of a fixed 4-light forward path |
| `[x]` | Stable content-addressed Form IDs | Kills load-order sorting and the 255-plugin ceiling outright |
| `[x]` | Validated, atomic, full-snapshot saves | ECS state is a first-class value, not a VM heap dump |
| `[x]` | Cross-game canonical material pipeline (NIFAL) | Per-game translation at the parse boundary; the shader sees one `Material` |
| `[x]` | Live debug protocol into a running engine | Wire shape already covers ~60 % of what an editor needs |
| `[x]` | FSR 3.1 + SSIM-fenced quality matrix | Reconstruction quality is regression-tested, not eyeballed |
| `[~]` | Abstraction layers: NIFAL / EXAL / PHYSAL / WATAL / CHARAL | One canonical model per domain; per-game code confined to translation |
| `[x]` | Parallel system dispatch (M27) | Declared access sets (R7) → deadlock-free multi-threaded ECS |
| `[ ]` | Path-traced reference mode (M51) | |
| `[ ]` | In-engine editor with hot reload (M50) | |
| `[ ]` | Deterministic P2P co-op (M60) | Impossible on Papyrus stack-VM semantics |
| `[ ]` | Local-LLM dialogue plugin (M62) | ECS event hooks make it plugin-space, not engine-space |
| `[ ]` | OpenXR / VR (M63) | |
| `[ ]` | Procedural exterior cells (M64) | Cells are ECS state, not save blobs |

---

## Per-game rollup

| Game | Parse | Interior renders | Exterior renders | NPCs | Ragdoll | Overall |
|---|:---:|:---:|:---:|:---:|:---:|---|
| **Oblivion** (TES4) | `[x]` 99.93 % | `[x]` Anvil Oaken Halls | `[~]` wired, bench pending | `[~]` | `[~]` | Solid interior, exterior unproven |
| **Fallout 3** | `[x]` 100 % | `[x]` Megaton, 929 REFRs | `[~]` wired, bench pending | `[~]` | `[~]` | |
| **Fallout: New Vegas** | `[x]` 100 % | `[x]` Prospector Saloon | `[x]` 7×7 WastelandNV | `[x]` Doc Mitchell | `[x]` 18-body verified | **Reference title** |
| **Skyrim SE** | `[x]` 100 % | `[x]` Bannered Mare, 6 NPCs | `[~]` | `[x]` equipped | `[~]` | Strongest after FNV |
| **Fallout 4** | `[x]` 100 % | `[x]` MedTek + Dugout Inn | `[~]` | `[x]` equipped | `[ ]` blocked | CSG precombines land |
| **Fallout 76** | `[x]` 100 % | `[ ]` | `[ ]` | `[ ]` | `[ ]` | Parser only |
| **Starfield** | `[x]` 99.99 % | `[~]` walkable Cydonia | `[ ]` | `[ ]` | `[ ]` blocked | Bring-up stage |

---

## The honest summary

**Where ByroRedux is already past the original:** lighting, denoising,
upscaling, save integrity, plugin identity, and live diagnostics. Those aren't
close calls — RT shadows and validated atomic saves have no Gamebryo analogue.

**Where it reaches parity:** file-format coverage (seven games, 100 %
recoverable), interior cell rendering, streaming, archives, physics collision,
spatial audio.

**Where the real gaps are, in priority order:**

1. **Havok `.hkx` animation** — NPCs spawn correct, equipped, and motionless.
   Everything downstream of "actors move" is gated on this one decoder.
2. **Dialogue & quest runtime** — 18 215 DIAL records and 436 quests parse;
   nothing drives them. Largest unbuilt surface in the engine.
3. **Distant object LOD** — the outdoor silhouette gap. Terrain streams;
   the buildings on the horizon don't.
4. **Combat & VATS runtime** — the formulas are in, the loop isn't.
5. **Gameplay menus** — inventory, map, journal, barter. The SWF host is
   ready; nothing feeds it game state.

**Maintenance note.** This chart is a snapshot. `ROADMAP.md` is the live source
of truth for milestones and parse rates; when the two disagree, ROADMAP wins.
Refresh this file at `/session-close` alongside it.
