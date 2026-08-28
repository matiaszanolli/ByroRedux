# Shared Audit Protocol — ByroRedux

This file is referenced by all audit skills. Do NOT use as a slash command (prefixed with `_`).

## Project Layout

```
Core ECS:        crates/core/src/ecs/
Components:      crates/core/src/ecs/components/
Animation:       crates/core/src/animation/          (types, player, stack, registry, interpolation, root_motion, text_events, controller)
Resources:       crates/core/src/ecs/resources/       (mod.rs + skin_slot_pool.rs, split under #1869)
Strings:         crates/core/src/string/
Character(CHARAL): crates/core/src/character/         (per-game character RULESET → canonical ActorValues/Level/Perks: mod, ruleset (CharacterRuleset — the per-game Resource seam), attribute, skill, derived (the fixed-layout bilinear DerivedStatFormula: HP/AP/carry-weight + VATS AP formulas), leveling (XpCurve / SkillUse / SkillXp), regen (fixed-60 Hz PoolRegenAccumulator), resistance, reputation, affliction, components, fallout.rs / tes.rs / skyrim.rs (the three family impls). Translates *rules*, not data — single sink, and the single construction site is `build_character_ruleset` in byroredux/src/npc_spawn.rs (FO4 + FO3NV only today; Oblivion/Skyrim rulesets exist but are unwired). Specs: docs/engine/charal.md + charal-{fnv-fo3,fo4,fo76,oblivion,skyrim,starfield}-ruleset.md. Console consumers: commands/actor_value.rs (setav/modav). CHARAL-adjacent, in-scope siblings (CHAR-D6-05, #2962): crates/core/src/combat.rs (classic Oblivion combat-damage math — modified_skill, oblivion_weapon_damage_multiplier, oblivion_hand_to_hand_damage) and crates/core/src/stealth.rs (FO3/FNV sneak-detection — detection_score, classify); both read ActorValues as inputs but evaluate at combat/stealth-resolution time against transient per-hit state that never lives in ActorValues, so they sit outside crates/core/src/character/ as siblings, not submodules — see each module's own docstring. Owner audit: /audit-character.)
NIF Parser:      crates/nif/src/
NIF Blocks:      crates/nif/src/blocks/               (see blocks/mod.rs dispatch; controller/ subdir, tri_shape/ subdir {mod, ni_tri_shape, bs_tri_shape, agd}, collision/ subdir {mod, collision_object, rigid_body, ragdoll, shape_primitive, shape_compound, shape_mesh, compressed_mesh, constraints, phantom_action}, particle.rs (typed NiPSysEmitter/NiPSysEmitterCtlr/NiPSysEmitterCtlrData/NiPSysGrowFadeModifier), shader.rs, skin.rs, properties.rs, interpolator.rs, extra_data.rs, light.rs, multibound.rs, palette.rs, legacy_particle.rs, texture.rs, bs_geometry.rs, node.rs, base.rs, traits.rs; *_tests.rs siblings)
NIF Import:      crates/nif/src/import/               (mod.rs thin dispatch + types.rs + tests.rs; walk/{mod, tests} (mod.rs carries extract_emitter_params/extract_emitter_rate), mesh/{mod, material_path, decode, ni_tri_shape, bs_tri_shape, bs_geometry, tangent, sse_recon, skin, *_tests}, material/{mod, walker, shader_data, *_tests}, transform.rs, coord.rs, collision/{mod, shape, ragdoll} (mod.rs walks the bhk*CollisionObject tree AND — added 2026-08-07 — `summarize_collision_authoring`/`CollisionAuthoringSummary` (classic/new_physics/phantom counts), a scene-level census that lets the cell loader tell "nothing authored" apart from "FO4+ packed Havok authored but its `BhkSystemBinary` payload is still opaque" (`needs_packed_havok_fallback`); shape.rs resolves the bhk shape tree incl. BhkMultiSphereShape + BhkConvexListShape → CollisionShape, split from the original collision.rs under #1876; ragdoll.rs extracts Havok ragdoll articulation), precombine.rs (M49 FO4 precombined PSG slice → renderer-space mesh, paired with CsgArchive))
NIF Animation:   crates/nif/src/anim/                 (mod.rs re-exports; coord, controlled_block, transform, sequence, keys, channel, bspline, entry; types.rs + tests.rs)
BSA Reader:      crates/bsa/src/archive/             (mod.rs, open.rs, extract.rs, hash.rs, tests.rs)
BA2 Reader:      crates/bsa/src/ba2.rs
CSG Reader:      crates/bsa/src/csg.rs               (FO4 precombined geometry; BSPackedGeomObject TLV; M49. Spec: docs/engine/fo4-csg-format.md. Consumed by cell_loader/precombined.rs)
BGSM Materials:  crates/bgsm/src/                     (FO4+ external material parser)
SF Material:     crates/sfmaterial/src/               (Starfield CDB material consumer: chunk, reader, string_table, types, value)
FaceGen (M41):   crates/facegen/src/                  (.tri/.egt morph + texture blend)
Physics (M28):   crates/physics/src/                  (PHYSAL solver end — Rapier3D bridge: world.rs (PhysicsWorld, PHYSICS_DT/MAX_SUBSTEPS/SUBSTEP_TIME_BUDGET fixed-step accumulator, static-scene fast path, query casts, move_character), sync.rs (physics_sync_system — the 4-phase + 2.5-buoyancy tick), convert.rs (collision_shape_to_parts), components.rs (RapierHandles/CharacterController/Ragdoll), config.rs (ContactConfig/TriMeshFlagBits), ragdoll.rs (build_ragdoll + specs), water.rs (the WATAL physics sink: buoyancy_force/current_force/submerged_fraction). Engine side: byroredux/src/ragdoll.rs + byroredux/src/systems/character.rs. Owner audit: /audit-physics.)
Papyrus (M30):   crates/papyrus/src/                  (.psc lexer + Pratt parser → AST: token, lexer, ast, span, error, parser/{mod, expr})
Pex (M47.2):     crates/pex/src/                      (compiled-Papyrus .pex → AST decompiler, Champollion port: opcode, reader, model, decompile/{mod, cfg, lift, control_flow, lower, boolean, node, event_names}. 5-phase: CFG → node-lift+copy-prop → control-flow recon → AST lower+fidelity gate → short-circuit booleans)
Hkx (M47.2):     crates/hkx/src/                      (byroredux-hkx: minimal safe Havok 2010 packfile reader for the MQ101 cinematic slice — packfile.rs, animation.rs (decode_skeleton/decode_spline_animation, hkaSkeleton + static/dynamic hkaSplineCompressedAnimation transform tracks, no behavior-graph execution))
Mod Runtime:     crates/mod-runtime/src/              (byroredux-mod-runtime, added 2026-08-07: sandboxed executable-mod host — the engine-owned boundary between untrusted community code and semantic host services. Guests are WebAssembly Components; each instance gets its own `Principal`/`CapabilitySet`; no WASI is linked by default, so OS access is absent, not merely unused. lib.rs re-exports; bindings.rs, error.rs, identity.rs (`Principal`/`PrincipalId`/`CapabilityId`/`CapabilitySet`), limits.rs (`SandboxConfig`), runtime.rs (`SandboxRuntime`/`ModInstance`/`CompiledMod`/`LifecyclePhase`/`FaultInfo`/`LogEntry`). NO dedicated owner audit skill — see coverage note below.)
Scripting (M12/M47): crates/scripting/src/            (ECS-native scripting runtime: events, timer, cleanup, condition (M47.1 cond eval), trigger (M47.2 TriggerVolume detection), quest_stages, fragment.rs + fragment/, recurring_update.rs + recurring_update/, registry, vm_state, globals, package, equipment, player_control, dialogue, cinematic (the M47.2 MQ101 slice — hkx consumer), scene.rs + scene/{playback, quest_alias} (SCEN playback + the M47.3 quest-alias substrate); translate/ holds the AST→ECS recognizer chain {mod, source, archetype, compose, effects, tables, recognizers/{mod, quest_stage_gate, rumble, two_state_activator}}; papyrus_demo/ holds hand-verified reference scripts)
Save (M45):      crates/save/src/                     (full-ECS-snapshot save/load: snapshot, registry, disk, validate, driver; M45.1 live load-apply = reload cell + FormId-keyed deltas + player-pose restore)
Audio (M44):     crates/audio/src/lib.rs + tests.rs   (byroredux-audio: kira backend, AudioWorld resource, AudioListener/AudioEmitter/OneShotSound components, audio_system, SoundCache, streaming music, global reverb send)
SpeedTree (S1):  crates/spt/src/                      (byroredux-spt: TLV walker for FNV/FO3/Oblivion .spt; placeholder-billboard import fallback)
Debug Protocol:  crates/debug-protocol/src/           (wire types, component registry)
Debug Server:    crates/debug-server/src/             (TCP server + DebugDrainSystem)
Debug UI (egui): crates/debug-ui/src/                 (lib.rs, panels.rs — egui overlay)
Renderer:        crates/renderer/src/vulkan/
VulkanContext:   crates/renderer/src/vulkan/context/  (mod.rs, draw.rs, resize.rs, resources.rs, helpers.rs, screenshot.rs, geometry_pass.rs, post_passes.rs, skinned_blas_refit.rs — the last three split out under #1857/the FSR3 frame-tail work)
Accel (RT):      crates/renderer/src/vulkan/acceleration/  (mod.rs struct + new()/destroy(); constants, types, predicates, blas_static, blas_skinned, tlas, memory; tests.rs)
G-Buffer:        crates/renderer/src/vulkan/gbuffer.rs
SVGF Denoiser:   crates/renderer/src/vulkan/svgf.rs
TAA (M37.5):     crates/renderer/src/vulkan/taa.rs
Composite:       crates/renderer/src/vulkan/composite.rs
SSAO:            crates/renderer/src/vulkan/ssao.rs
Caustics (M22):  crates/renderer/src/vulkan/caustic.rs       (#321 Option A: per-frame compute splat into R32_UINT accumulator)
Water Caustic:   crates/renderer/src/vulkan/water_caustic.rs (#1210/#1255 Phase C: per-FIF R32_UINT accumulator for water-side caustics)
GPU Timers:      crates/renderer/src/vulkan/gpu_timers.rs
egui Pass:       crates/renderer/src/vulkan/egui_pass.rs      (egui overlay render pass; feeds debug-ui)
Volumetrics(M55):crates/renderer/src/vulkan/volumetrics.rs + volumetrics/ (noise.rs)  (render-extent-derived froxel grid configured by `VolumetricsConfig`; size the live per-FIF image set from `VolumetricsPipeline::new`; inject + integrate compute, single-ray TLAS shadow, HG phase)
ReSTIR-DI:       crates/renderer/src/vulkan/restir.rs      (ReservoirBuffers — screen-sized per-FIF reservoir SSBOs, RESERVOIR_STRIDE = 32 B, set-1 bindings 16/17 ping-pong; the shadow-reservoir half of Dimension 1/2 of /audit-renderer)
Bloom (M58):     crates/renderer/src/vulkan/bloom.rs        (5-mip down + 4-mip up pyramid, B10G11R11_UFLOAT, 4-tap bilinear)
Water (M38):     crates/renderer/src/vulkan/water.rs        (WaterPipeline: vertex displacement + Fresnel, RT reflection/refraction against TLAS)
GPU Skin (M29):  crates/renderer/src/vulkan/skin_compute.rs
Material (R1):   crates/renderer/src/vulkan/material.rs   (MaterialBuffer SSBO, GpuMaterial dedup; replaces per-instance fields)
FSR3 Upscaler:   crates/fsr3-sys/src/lib.rs (vendored FidelityFX SDK FFI, added 2026-07-22) + crates/renderer/src/vulkan/{frame_upscaler,presentation,exposure}.rs
SPIR-V Reflect:  crates/renderer/src/vulkan/reflect.rs    (descriptor layout reflection from SPIR-V)
Scene Buffers:   crates/renderer/src/vulkan/scene_buffer/  (mod, constants, gpu_types, buffers, upload, descriptors; gpu_instance_layout_tests + instance_hash_tests + material_hash_tests + scene_descriptor_reflection_tests)
Descriptors:     crates/renderer/src/vulkan/descriptors.rs
Vk Debug Util:   crates/renderer/src/vulkan/debug.rs
Vk Instance:     crates/renderer/src/vulkan/instance.rs
Vk Surface:      crates/renderer/src/vulkan/surface.rs
Mesh:            crates/renderer/src/mesh.rs
Vertex:          crates/renderer/src/vertex.rs
Tex Registry:    crates/renderer/src/texture_registry.rs (+ texture_registry_tests.rs)
Shaders:         crates/renderer/shaders/             (22 GLSL sources: triangle.vert/frag, svgf_temporal.comp, svgf_atrous.comp, taa.comp, composite.vert/frag, presentation.frag, ssao.comp, cluster_cull.comp, skin_palette.comp, skin_vertices.comp, caustic_splat.comp, volumetrics_inject.comp, volumetrics_integrate.comp, bloom_downsample.comp, bloom_upsample.comp, bloom_apply.comp, water.vert/frag, ui.vert/frag — full per-pass roles and G-buffer layout in docs/engine/shader-pipeline.md)
Shader Includes: crates/renderer/shaders/include/     (re-count via `ls crates/renderer/shaders/include/` — don't trust a hand-typed list here, see #3047: it drifted from 9/12 to missing 5/14 in the week between filing and fixing. Two entries carry meaning worth calling out by name rather than leaving to the directory listing: bindings.glsl holds the GpuInstance/GpuMaterial GLSL mirrors; shader_constants.glsl is GENERATED from crates/renderer/src/shader_constants_data.rs by crates/renderer/build.rs, never hand-edited. Any Rust-side #[repr(C)] change must land in bindings.glsl in lockstep — this is the #1 source of silent GPU-struct desync. `affected_shaders_include_constants_header` (shader_constants.rs) pins which entry-point shaders must #include shader_constants.glsl — see #2984.)
Plugin/ESM:      crates/plugin/src/                   (esm/{mod, reader, sub_reader, strings_table}, esm/cell/{mod, walkers, support, helpers, wrld, tests/}, esm/records/{mod (parse_esm + GRUP label dispatch), index (EsmIndex), the eight dispatch_*.rs group routers, grup_walker, common, condition, actor/{mod,tests} (#2055), actor_value_derive, climate, container, global, items, list_record, misc/{water, character, world (incl. the #2738 packed NVNM navmesh decode), pack, quest, scene, dialogue, imagespace, magic, effects, equipment} (#2054 split misc/ai.rs into these), movs, mswp, outfit, pkin, scol, script, script_instance, tree, weather, …}, equip.rs (xEdit-derived biped-slot constants), record.rs generic dispatch, datastore/manifest/resolver (the Redux-native tier); legacy/ holds the LegacyFormId/LoadOrder bridge — per-game stubs were removed under #390. Owner audit: /audit-esm.)
Platform:        crates/platform/src/
UI (Ruffle/M48): crates/ui/src/                       (lib.rs UiManager + player.rs SwfPlayer (Ruffle wrapper, offscreen wgpu + pixel readback); R4/M48 host layer added Session 61: profile.rs ScaleformProfile::{SkyrimAvm1, Fallout4Avm2}, host.rs + host/ ScaleformHostBridge (bidirectional ExternalInterface: AS→engine queue, engine→AS via call_internal_interface, records unknown methods + callback registrations), avm2_host.rs (FO4 BGSCodeObj lifecycle + generated AVM2 forwarding adapter), navigator.rs (archive-backed BSA/BA2 resource resolution + local-executor pump), input.rs, catalog.rs (menu catalog). Engine side: byroredux/src/ui_input.rs (winit→UiInputEvent translation + focus release) and the per-frame tick/drain/render/upload block in main.rs. Doc: docs/engine/ui.md. Owner audit: /audit-ui.)
CXX Bridge:      crates/cxx-bridge/
Binary:          byroredux/src/main.rs (App struct + construction + the debug-UI snapshot bridge; 834 LOC. The >2k-LOC monolith WAS split under #2731 — the winit ApplicationHandler moved to app_events.rs and the per-frame render driver to app_frame.rs. Skill text telling you to "route findings against the live main.rs, not a remembered split" is stale: the split is real, so route render-loop findings to app_frame.rs and window/device/input-event findings to app_events.rs.)
                 · app_events.rs — the winit ApplicationHandler (resumed / window_event / device_event / about_to_wait). Owns input dispatch into interaction.rs + the game-menu/pause routing.
                 · app_frame.rs — App::render_one_frame, the per-frame render driver. Owns the draw_frame call, the skin_dispatch_ran rollback gate (#1791/#1796, moved here from main.rs) and its skin_dispatch_ran_rollback_scope_tests guard.
Binary modules:  byroredux/src/ — the top-level files no other row below covers.
                 · boot.rs — scheduler construction: every add_system / add_exclusive registration and its declared resource access. The authority for "which stage does X run in".
                 · app_step.rs — per-tick streaming / debug-load / save / cell-transition steppers.
                 · cli_args.rs + game_profiles.rs — argument parsing + per-game launch profiles.
                 · save_io.rs + save_io/ (*_tests.rs only) — the ENGINE side of M45: command queue, live reload, registry completeness, round-trip, serde-default guard, validation gate. crates/save is the crate half; both are owned by /audit-save.
                 · streaming.rs + streaming_helpers.rs + streaming_tests.rs — M40 cell lifecycle.
                 · fog.rs — fog-volume assembly (EXAL consumer, feeds render/fog_volumes.rs).
                 · interaction.rs (1356 LOC) + ui_input.rs — winit→UI input routing (/audit-ui Dim 7) AND, since 2026-08-15, the canonical player-interaction producer: InputAction/ActionState, camera_ray, activation/pick, and the hold/look action edges every OnActivate consumer reads. Its interaction_system is the first exclusive in Stage::Update (boot.rs). It has outgrown the "/audit-ui Dim 7" framing — the UI-routing half is /audit-ui's, the action/ray/activation half belongs to the gameplay slice below.
                 · studio_host.rs (252 LOC, `21a840d5` 2026-08-25) — the engine-side host adapter for `crates/sdk`: snapshots ECS state for the renderer-independent Studio tooling. Landed without a layout row; see #3457.
 · debug_load.rs, list_cells.rs, name_lookup.rs, parsed_nif_cache.rs, scene_import_cache.rs, ownership_sample.rs, groundcover_translate.rs, anim_convert.rs, helpers.rs, bench.rs, bench_camera.rs, scheduler_access_tests.rs, groundcover_translate_tests.rs, ownership_sample_tests.rs.
Gameplay Slice:  byroredux/src/{combat,inventory,settings_io}.rs + the action half of interaction.rs — the P2 playable-vertical-slice runtime, added 2026-08-15/16. **NO owner audit skill** (see coverage note below); this is the project's active execution focus, so an audit that ignores it is auditing last month's engine.
                 · combat.rs (407 LOC) — first melee vertical slice. MELEE_REACH_BU = 180.0, MELEE_COOLDOWN_SECONDS = 0.45, UNARMED_DAMAGE = 8.0. combat_input_system + combat_damage_system are both Stage::Update exclusives (boot.rs); CombatState is a Resource holding cooldown + the CombatTraceEntry smoke evidence. Casts from the active camera, resolves a ragdoll bone through ActorColliderOwner, emits the canonical scripting HitEvent, applies damage to the Health ActorValue and owns the alive→dead transition (Dead + the AI-behavior teardown). Transient HitEvent cleanup stays in the scripting Late stage — do not report that as a leak.
                 · inventory.rs — native inventory presentation + player-facing equipment mutation. Canonical state stays Inventory + EquipmentSlots (crates/core); this module only carries immutable item metadata (InventoryCatalog / InventoryItemDefinition, FxHashMap-keyed by form id) and seeds the player from base NPC_ 0x00000007. Form 0x00000014 is the placed player reference, not the base record. A finding that it duplicates canonical inventory state is a real NIFAL-style boundary violation; a finding that it *caches* record metadata is by design.
                 · settings_io.rs (334 LOC) — settings persistence behind the game menu.
                 · Gates: docs/smoke-tests/{p0-door-interaction,p1-character-traversal,p2-melee-core}.sh, specs docs/engine/playable-vertical-slice.md + docs/engine/p2-combat-fixture.md (P2 combat core passing 2026-08-16; corpse loot / authored response anim / save-reload continuity still open).
Systems:         byroredux/src/systems.rs (module index) → systems/{animation, audio, billboard, bounds, camera, character, cinematic, debug, escort, follow, guard, light_anim, locomotion, metrics, particle, patrol, sandbox, travel, wander, water, weather}.rs (character.rs is the PLAYER/character controller — physics, not CHARAL; cinematic.rs drives the M47.2 scripted-camera slice) (particle.rs carries apply_emitter_params, fed by the typed NIF emitter pipeline; sandbox/wander/travel/follow/escort/guard/patrol.rs are the M42 AI-package procedure runtimes — see "Sandbox AI" row below; locomotion.rs is their shared `step_toward` walk-to-point primitive)
Scene Setup:     byroredux/src/scene.rs (thin) → scene/{nif_loader, world_setup}.rs (+ *_tests.rs siblings: climate_tod_hours, cloud_tile_scale, procedural_fallback, radius_parse)
Render Data:     byroredux/src/render/ (mod.rs carries build_render_data + draw enumeration) → render/{camera, lights, fog_volumes, skinned, static_meshes, particles, sky, water}.rs (+ *_tests.rs siblings). **`fire_lights.rs` was deleted 2026-08-17 (`2325c1de`)** — an audit finding written against *derive_fire_light* / *fire_lights_enabled* / *BYRO_FIRE_LIGHTS* is stale. Fire no longer derives a light from the analytic primitive; surface illumination is reduced from the transported combustion field by `Volumetrics::append_combustion_surface_lights` (crates/renderer/src/vulkan/volumetrics.rs), called unconditionally from context/draw.rs. `render/lights.rs` owns the single `gpu_light_from_emitter` encoder every authored-LIGH light goes through.
Cell Loader:     byroredux/src/cell_loader.rs (thin dispatch; also owns `pack_imported_material_flags`, the ImportedMaterial → GPU flag-bit packer) → cell_loader/{load, unload, exterior, spawn.rs + spawn/, partial, euler, refr, terrain, terrain_lod, terrain_lod_btr, object_lod, lod_bands, lod_support, placement_lod, water, work_budget, load_order, index, precombined, transition, nif_import_registry}.rs + cell_loader/references/ (a directory, not references.rs: attach, complete, import, synth_child + *_tests.rs) (+ *_tests.rs siblings)
Commands:        byroredux/src/commands/ (per-domain split #1323/TD9-NEW-03: mod.rs registry + world_info (help/stats/entities/systems/sys.accesses/mem.frag/ctx.scratch) + assets (tex.*/mesh.*/skin.*) + view (prid/cam.*/near/pick) + scene (light.*/door.teleport/script.activate/mat.*/ragdoll) + actor_value (setav/modav, CHARAL consumer) + condition (cond — live CTDA eval, M47.1 consumer) + time (time.show/time.set/time.scale/time.pause/time.resume/time.advance — GameTimeRes live console control, added 2026-08-07) + water (WATAL submersion/contact inspection) + quest (M47.3 quest/alias state) + env_health (+ env_health_tests.rs) + physics (phys.stats/phys.census — PHYSAL solver + collider inspection; #3495) + shared helpers) + byroredux/src/commands_tests.rs
SDK / Studio:    crates/sdk/src/ (lib.rs + studio.rs, 282 LOC, `21a840d5` 2026-08-25) — renderer-independent tooling surface; `StudioSession` is a Resource. Its engine-side adapter is byroredux/src/studio_host.rs (Binary modules row). **No owner audit skill** — see the un-owned-subsystems table below; `/audit-ecs` and `/audit-concurrency` have both reached it incidentally (#3445).
NIFAL Translate: byroredux/src/material_translate.rs (translate_material — the SINGLE raw material → ECS Material boundary; per-game material classification happens here, never in the shader) + crates/core/src/ecs/components/material.rs (Material::resolve_pbr; canonical metalness/roughness are plain f32 fields, resolve-once). Spec: docs/engine/nifal.md. See also /audit-nifal.
                 POST-REFACTOR SHAPE (2026-07-27, `05d68926` + `c8c8a834`) — audits written against the older flat layout are stale:
                 · `ImportedMaterial` (crates/nif/src/import/types.rs, ~55 fields) is now a struct reached as `ImportedMesh.material`, NOT flat fields on ImportedMesh. Any skill text saying "merged into ImportedMesh" means "merged into ImportedMesh.material".
                 · `MaterialTextureSet<T>` (same file) replaces per-game texture slot numbers with 18 named source-agnostic roles + `decals: [T; 4]`. Generic over pipeline stage: `Option<FixedString>` imported → `Option<String>` resolved → bindless index, via `map_ref`. NiTexturingProperty / BSShaderTextureSet / BGSM / BGEM / BSEffectShaderProperty all populate the SAME roles — game-specific slot numbers must not survive past the NIF import boundary.
                 · `merge_external_material` (byroredux/src/asset_provider/material.rs) REPLACES the former *merge_bgsm_into_mesh*. It takes `&mut ImportedMaterial`, not `&mut ImportedMesh` — deliberately narrowed so external BGSM/BGEM/.mat sidecars can patch material semantics but CANNOT mutate geometry, transforms, skinning, or scene ownership. Treat a widened signature here as a NIFAL boundary violation.
                 · `pack_imported_material_flags` (byroredux/src/cell_loader.rs) REPLACES the former *pack_material_flags* / *pack_bgsm_material_flags*.
                 · `GpuMaterial` grew 300 → 348 B for the twelve common supplemental role indices, then 348→364→396→432 B (animated shader color/float fields, BGEM glass optics, soft/rim/back Bethesda lighting response) — current size is pinned by `gpu_material_size_is_432_bytes`, not `_348_`. The GLSL mirror in `crates/renderer/shaders/include/bindings.glsl` must match field-for-field, not just in size.
EXAL Translate:  byroredux/src/env_translate.rs (EXAL exterior-environment translation boundary: terrain/sky/sun/weather/water/LOD). Spec: docs/engine/exal.md.
Ragdoll:         byroredux/src/ragdoll.rs (M41.x ragdoll activation + writeback; PHYSAL consumer). Spec: docs/engine/physal.md.
Cornell Harness: byroredux/src/cornell.rs (--cornell self-contained RT material/lighting reference scene; no on-disk game data)
Asset Provider:  byroredux/src/asset_provider/ (mod.rs TextureProvider + resolve_texture + strip_build_prefix for AE pipeline-path paths; archive.rs GameArchive BSA/BA2 wrapper + `<stem>N` sibling auto-load; texture.rs file-data lookup; material.rs MaterialProvider (bgsm_cache/bgem_cache/csg_cache/failed_paths) + the free fn `merge_external_material`; script.rs .pex lookup; animation.rs — the byroredux-hkx consumer (decode_skeleton / decode_spline_animation) and the ONLY caller of that crate; tests/)
Components:      byroredux/src/components.rs (binary-local markers + app resources: Spinning, AlphaBlend, TwoSided, DoorTeleport, IsFxMesh, IsLodTerrain, FootstepEmitter/Config/Scratch, CellLightingRes, SkyParamsRes, WeatherDataRes, LightTuning, …) + components/game_time.rs (`GameTimeRes` — persistent canonical game clock, day+hour+time_scale, feeds weather/AI schedules/console/save-load; M34 day-night cycle, 2026-08-07). Shared ECS components (WaterPlane/WaterVolume/SubmersionState) live in crates/core/src/ecs/components/water.rs; SelectedRef is a resource in crates/core/src/ecs/resources/mod.rs)
NPC Spawn:       byroredux/src/npc_spawn.rs + npc_spawn/{ai_package,tests}.rs (M41 actor instantiation; M42.2 adds CTDA package-condition gating (`package_conditions_pass`, fail-open on unimplemented condition functions); the spawn-tail makes a single `active_package` resolve (`crates/plugin/src/esm/records/misc/pack.rs`) and classifies it with `PackRecord::is_*` — #2031 collapsed the seven `active_package_is_*`/`active_*_location`/`active_*_target` selector pairs into that one resolve and #3042 deleted them — inserting at most one Behavior component per actor — an NPC's active package is always a single winning `PackRecord`, so Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol selection is mutually exclusive by construction)
Sandbox AI:      byroredux/src/systems/{sandbox,wander,travel,follow,escort,guard,patrol}.rs — the seven M42 procedure runtimes (of ~17 in the FO3/FNV enum; the other ten are parse-only, blocked on unbuilt item-use/combat/magic/dialogue subsystems, not just missing dispatch). `sandbox_seat_system` (M42) does nearest-free-seat assignment, per-marker reservation via SeatReservations keyed (furniture, marker index). `wander_system`/`travel_system`/`follow_system`/`escort_system`/`guard_system`/`patrol_system` (M42.3–M42.8) share a `step_toward` walk-to-point primitive (`systems/locomotion.rs`); `patrol_system` additionally reuses `wander_system`'s whole phase-transition core (`step_oscillating_wander`) rather than duplicating it, since no patrol-route data is decoded anywhere in this codebase. All seven are opt-in, gated one env var each (`BYRO_SANDBOX_SIT`/`BYRO_WANDER`/`BYRO_TRAVEL`/`BYRO_FOLLOW`/`BYRO_ESCORT`/`BYRO_GUARD`/`BYRO_PATROL`), none in the default scheduler. Components: `crates/core/src/ecs/components/{sandbox,furniture,wander,travel,follow,escort,guard,patrol}.rs` (SandboxBehavior/Seated, WanderBehavior/WanderState/WanderPhase, TravelBehavior/TravelState/Traveled, FollowBehavior/FollowState, EscortBehavior/EscortState/Escorted, GuardBehavior/GuardState, PatrolBehavior/PatrolState — all `SparseSetStorage`). v0 scope throughout: no pathing/NAVM, no animation-clip swap, no per-frame package re-evaluation (selection is spawn-time-only). Doc: docs/engine/npc-spawn-ai-packages.md.
World Stream:    byroredux/src/streaming.rs           (M40 cell lifecycle) + streaming_tests.rs
SF Smoke:        byroredux/src/sf_smoke.rs            (Starfield ESM resolve-rate harness, --sf-smoke CLI)
Golden Frames:   byroredux/tests/golden_frames.rs     (cube-demo frame-60 regression PNG; opts into --ignored)
Tools:           tools/byro-dbg/ (TCP debug REPL, port 9876 — src/main.rs client + src/display.rs pretty-print), tools/texture-upscale/ (workspace member added Session 61), tools/nifskope/ (vendored reference viewer, NOT a workspace member — do not audit as first-party code)
Bench Harness:   scripts/fsr-bench-matrix.sh + scripts/fsr_bench_report.py (the 5-scene × 5-config × 3-run upscaler matrix; byte-stability of BOTH files is what makes cross-commit bench comparisons valid — flag any edit that isn't itself benched)
Legacy Ref:      docs/legacy/
```

## Key Reference Docs

These docs are the authoritative, code-verified reference for their domain.
Prefer them over re-deriving facts from source during an audit.

| Doc | What it documents |
|-----|------------------|
| `docs/engine/shader-pipeline.md` | All 21 shaders (re-count via `ls crates/renderer/shaders/*.{vert,frag,comp}` — don't trust this prose count, see #2421), G-buffer attachment formats, `GpuCamera`/`GpuInstance`/`GpuMaterial`/`GpuLight` exact byte layouts, descriptor set bindings (Set 0–2), per-frame submission order, pipeline cache |
| `docs/engine/memory-budget.md` | VRAM/RAM ceilings, SSBO sizes, LRU eviction thresholds (`AccelerationManager`, `TextureRegistry`, BGSM cache, `MeshRegistry`), deferred-destroy countdown depth |
| `docs/engine/nifal.md` | NIFAL three-tier canonical translation spec (Imported* → translate() → Canonical); single-boundary / no-fabrication / no-render-time-fallback rules |
| `docs/engine/plugin-loading.md` | `PluginManifest` TOML schema, `DataStore`, `DependencyResolver` algorithm, Form ID three-layer design, ESM parser entry points, conflict resolution |
| `docs/engine/pipeline-overview.md` | Cross-cutting trace #1: a single interior cell load end-to-end, ESM record → ECS spawn → GPU draw |
| `docs/engine/exterior-grid-streaming.md` | Cross-cutting trace #2: exterior worldspace grid streaming — background pre-parse worker, cell-boundary crossing, door teleport scene swaps |
| `docs/engine/save-load-roundtrip.md` | Cross-cutting trace #3: M45/M45.1 save — what a snapshot captures, atomic disk write, live load-apply onto a *running* engine (no process restart) |
| `docs/engine/npc-spawn-ai-packages.md` | Cross-cutting trace #4: NPC_ spawn → AI package selection (CTDA gating, M42.2) → per-procedure runtime (Sandbox M41.5/M42, Wander M42.3, Travel M42.4, Follow M42.5, Escort M42.6, Guard M42.7, Patrol M42.8). States plainly which of the ~17 FO3/FNV package procedures actually execute at runtime (currently: seven — Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol; the other ten are blocked on unbuilt subsystems, not just missing dispatch) |
| `docs/feature-matrix.md` | What works at runtime per game — cell loading, rendering, NPCs, audio, scripting, physics, UI. Living status document. (NOTE: the "Scripting (M47)" + "Save / load (M45)" rows lag the code — M45/M45.1 + the M47.2 .pex slice shipped; treat the matrix as a floor, not ceiling, and flag the doc-rot.) |
| `docs/engine/scripting.md` | ECS-native scripting model (Papyrus VM → ECS), recognizer-chain design, what `.pex`/recognizers translate vs. defer. Paired with `docs/engine/papyrus-parser.md` (`.psc` AST), `docs/engine/m47-0-design.md`, `docs/engine/m47-2-design.md`, `docs/engine/m47-2-recognizer-scaling.md`. Owner audit: `/audit-scripting`. |
| `docs/engine/charal.md` | CHARAL — per-game character ruleset → canonical ActorValues/Level/Perks. Paired with the six per-game rulesets: `docs/engine/charal-fnv-fo3-ruleset.md`, `charal-oblivion-ruleset.md`, `charal-skyrim-ruleset.md`, `charal-fo4-ruleset.md`, `charal-fo76-ruleset.md`, `charal-starfield-ruleset.md` — **these six are the authority for every constant**. Owner audit: `/audit-character`. |
| `docs/engine/exal.md` | EXAL — exterior abstraction layer (terrain/sky/sun/weather/water/LOD). Boundary is `byroredux/src/env_translate.rs`. Paired with `docs/engine/exal-groundcover.md`. |
| `docs/engine/physal.md` | PHYSAL — double-ended physics layer (source game + Rapier solver). Per-game seam is ONLY the constraint CInfo decode. Paired with `docs/engine/physics.md`. Owner audit: `/audit-physics`. |
| `docs/engine/watal.md` | WATAL — double-ended water layer (render + physics). Skyrim-modelled canonical. **Status refreshed 2026-08-20**: the physics half IS built (`WaterContact`, buoyancy, submerged damping, bounded current drag through `crates/physics/src/water.rs`), and character swimming + bounded drowning damage went live in `c7561d74` (2026-08-19) — skill text calling either "unbuilt" is stale. The genuinely open items are water-walking, freezing, the exact Skyrim DNAM tail decode, and the cross-game visual smoke matrix; `docs/engine/watal.md:415-425` is the authority, not this row. Render half → `/audit-renderer` Dim 15; physics half → `/audit-physics` Dim 6. |
| `docs/engine/ui.md` | Scaleform/SWF UI, Ruffle host bridge, `ScaleformProfile` split (Skyrim AVM1 / FO4 AVM2), GameDelegate + BGSCodeObj contracts. Owner audit: `/audit-ui`. |
| `docs/engine/plugin-loading.md` companion → | the ESM/ESP parser itself (GRUP walk, sub-record accounting, per-record schemas, FormID remap) is owned by `/audit-esm`. |
| `docs/engine/fsr3-upscaler-integration-plan.md` | FSR 3.1 integration plan, all 7 phases + the SSIM quality matrix. Paired with `docs/engine/fsr3-troubleshooting.md`. **No owner audit skill** — see `/audit-renderer` Dimension 22. |
| `docs/contributing.md` | Prerequisites, build, test tiers (unit/integration/Vulkan/smoke), shader recompile, game data paths, CI jobs |

Crate count: 25 under `crates/` — audio, bgsm, bsa, core, cxx-bridge,
debug-protocol, debug-server, debug-ui, facegen, fsr3-sys, hkx, mod-runtime,
nif, papyrus, pex, physics, platform, plugin, renderer, save, scripting, sdk,
sfmaterial, spt, ui.
Use this as a coverage sanity check: an audit that never touches a relevant
crate here is incomplete.

Crate → owner audit map (refreshed 2026-08-16):

| Crate | Owner audit |
|---|---|
| `crates/audio` | `/audit-audio` |
| `crates/facegen` | `/audit-skyrim` (pre-baked FaceGen head path) + `/audit-fo3` — no dedicated owner |
| `crates/debug-ui` | `/audit-renderer` (egui_pass) — command/panel surface itself is un-owned |
| `crates/bgsm`, `crates/sfmaterial` | `/audit-fo4`, `/audit-starfield` (+ `/audit-nifal` for the material boundary) |
| `crates/bsa` | `/audit-nif` (archive feed) + per-game |
| `crates/core` (ECS half) | `/audit-ecs` |
| `crates/core/src/character` (CHARAL) | `/audit-character` |
| `crates/nif` | `/audit-nif`, `/audit-nifal` |
| `crates/papyrus`, `crates/pex`, `crates/scripting` | `/audit-scripting` |
| `crates/physics` | `/audit-physics` |
| `crates/plugin` | `/audit-esm` |
| `crates/renderer` | `/audit-renderer` |
| `crates/save` | `/audit-save` |
| `crates/sdk` | no dedicated owner; use the owner for each exposed domain and `/audit-ecs` for shared world contracts |
| `crates/spt` | `/audit-speedtree` |
| `crates/ui` | `/audit-ui` |

### Un-owned subsystems (coverage gaps — read before claiming a sweep is complete)

Seven subsystems still have **no owner audit skill** (refreshed 2026-08-26 — the
list grew with the renderer-independent SDK surface). An audit that touches them
does so incidentally, so nothing guarantees they are ever examined. Do not
report "full coverage" without saying which of these you skipped:

| Subsystem | Code | Nearest owner today | Why it matters now |
|---|---|---|---|
| **Gameplay slice (P2)** | `byroredux/src/combat.rs`, `byroredux/src/inventory.rs`, `byroredux/src/settings_io.rs`, the action half of `byroredux/src/interaction.rs` | `/audit-ecs` (system/resource shape) + `/audit-runtime` (the p0/p1/p2 smoke gates) | **The project's active execution focus.** ~2.6k LOC landed 2026-08-15/16 with three Stage::Update exclusives and a Resource, and nothing owns its damage/equip/activation invariants. Highest-value gap on this list |
| ByroRedux SDK | `crates/sdk/src/` | Per-domain owner + `/audit-ecs` for shared world contracts | Public renderer/UI-independent document, snapshot, selection, and typed-command contracts are the first tooling API surface; an executable-only audit can miss breaking host-facing changes |
| FaceGen | `crates/facegen/src/` | `/audit-skyrim` (incidental, via the NPC head path) | `.tri`/`.egt` morph + texture blend on untrusted archive input, with no parser-discipline dimension of its own |
| Mod Runtime (sandboxed mods) | `crates/mod-runtime/src/` | `/audit-safety` Dimension 11 (added 2026-08-13) | A trust boundary between untrusted WASM guest code and the host. Still has **no consumer in the engine** — audit it as a contract, not as a live path |
| FSR3 upscaler + FFI | `crates/fsr3-sys/`, `crates/renderer/src/vulkan/{frame_upscaler,upscaling,presentation,exposure}.rs` | `/audit-renderer` Dim 23 + `/audit-safety` Dim 1 | Engine-default render path since phase 7; the only live FFI crossing in the workspace |
| Havok packfile reader | `crates/hkx/src/` | `/audit-scripting` Dim 8 (cinematic slice) | Untrusted binary input with no parser-discipline dimension of its own; sole consumer is `byroredux/src/asset_provider/animation.rs` |
| Debug server / protocol | `crates/debug-server/src/`, `crates/debug-protocol/src/` | `/audit-concurrency` Dim 7 (worker threads only) | A TCP listener that evaluates queries against the live `World`; nothing audits its command surface |

`crates/cxx-bridge` (36 LOC) and `crates/platform` (60 LOC) are placeholders —
no owner needed, but do not cite `cxx-bridge` as a live FFI boundary
(`crates/fsr3-sys` is the real one; see `/audit-safety` Dim 1).

When one of these is the subject of a session's work, run the closest generic
audit (`/audit-safety`, `/audit-ecs`, `/audit-concurrency`, `/audit-tech-debt`)
with an explicit instruction to treat that crate as in-scope — and say so in
the report's scope line.

## Game Data Locations

```
Oblivion:      /mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/
Fallout 3:     /mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/
Fallout NV:    /mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/
Skyrim SE:     /mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/
Fallout 4:     /mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/
Fallout 76:    /mnt/data/SteamLibrary/steamapps/common/Fallout76/Data/
Starfield:     /mnt/data/SteamLibrary/steamapps/common/Starfield/Data/
Gamebryo 2.3:  /media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/
```

## Legacy Source (for compatibility audits)

```
CoreLibs/NiMain/       Scene graph, rendering, materials
CoreLibs/NiAnimation/  Controllers, interpolators, keyframes
CoreLibs/NiCollision/  OBB trees, raycasting
CoreLibs/NiSystem/     Memory, threading, I/O
SDK/Win32/Include/     1,592 public headers
```

## Severity Definitions

See `.claude/commands/_audit-severity.md` for the unified severity scale (CRITICAL / HIGH / MEDIUM / LOW).

## Methodology

- Be skeptical. Assume there are bugs even if the code "looks fine."
- For each claim, re-read the code path to confirm before including it.
- Prefer evidence from concrete code paths (call sites, data structures, configs) over assumptions.
- After making a finding, attempt to disprove it. Only include findings you cannot disprove.

## Rust-Specific Context Rules

- **Unsafe blocks**: Always read surrounding code and safety comment. Every unsafe MUST have justification.
- **Lifetimes**: When reading function signatures, trace caller lifetimes through borrows.
- **Trait bounds**: Check Send + Sync requirements on Component/Resource types.
- **Drop ordering**: Validate destroy-before-parent relationships (Vulkan objects).
- **Vulkan validation**: Reference Khronos spec for behavior guarantees.
- **Lock ordering**: Verify TypeId-sorted acquisition for multi-component queries.
- **Hot-path hashing (#2923, 2026-08-15)**: `rustc-hash` is a workspace dep
  (`crates/core`, `crates/renderer`, `byroredux`). The per-frame render/skinning
  path is `FxHashMap`/`FxHashSet` end-to-end and must stay that way across the
  crate boundary — every collection on `SkinSlotPool`
  (`crates/core/src/ecs/resources/skin_slot_pool.rs`), the `pose_dirty` set it
  hands the renderer, `FrameInputs.pose_dirty`, and the `skin_offsets` map
  threaded through `byroredux/src/render/`. A reintroduced
  `std::collections::HashMap`/`HashSet` on any of those is the regression
  (SipHash on a per-frame per-entity keyspace); the guard is the
  `"{what} must stay \`FxHashSet\` (#2923)"` assertion in
  `crates/renderer/src/vulkan/context/mod.rs`. This is a *hot-path* rule, not a
  blanket one — std hashing in load-time or parser code is fine, and DoS-facing
  maps should stay std.

## Context Management Rules

- **Max 1500 lines per Read** — use `offset` and `limit` to paginate larger files.
- **Grep before Read** — search for the specific pattern first, then read only relevant sections.
- **Incremental writes** — append findings to the report as you go; do not hold everything in memory.
- **One dimension at a time** — complete and write up one dimension before starting the next.

## Path-Reference Convention (post-`#1114`)

Backticked file/dir paths in any audit-*.md skill (or this file)
**must resolve against the live repository tree**. The validate gate
at `.claude/commands/_audit-validate.sh` enforces this and is the
structural fix for the recurring TD7-* stale-path findings.

- Backticks = "this path exists right now". The gate fails CI / the
  audit if it doesn't.
- Forward-looking refs (a file that doesn't yet exist) or
  backwards-looking refs (a file that was deleted) **must not** use
  backticks — write them as plain text or italics.
- Run `.claude/commands/_audit-validate.sh` before committing edits
  to any audit skill.

**The same rule applies to symbols** (added 2026-07-27). A backticked
`snake_case` function / test / field name asserts it exists right now;
a renamed or deliberately-absent one must be *italicised*, not
backticked. The validate script now emits an **advisory** list of
backticked symbols found in no tracked `.rs` file — advisory, not
fatal, because the noise floor includes baseline TSV columns, git
hashes and lint names. It is not decoration: it is what caught
`GpuMaterial` still being documented at 300 B after it grew to 348 B,
which is a wrong number in a GPU layout contract, not a typo. Clear
the advisory list rather than learning to scroll past it.

**Never write an instruction to not look** (added 2026-08-20). A skill file may
record a **"known-open, do NOT re-litigate"** fact — dated, and pointing at the
doc that owns it — because a fact that rots becomes a false premise an auditor
can check and correct. It must **never** tell an auditor to *"confirm absence
rather than reporting it"*: that rots into a blindfold over exactly the newest,
least-reviewed code, and it suppresses the evidence that would reveal the rot.
No gate can catch this class — the path gate checks paths and the symbol
advisory checks symbols; neither can evaluate an instruction. `audit-physics`
carried such a line over the shipped swim/drown core for one day and came within
one auditor's skepticism of losing two real findings (#3119, #3125); see #3199.

## Deduplication (MANDATORY)

Before reporting ANY finding:

1. Run: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels` and save to `/tmp/audit/issues.json`
2. Search for keywords from your finding in existing issue titles
3. Scan `docs/audits/` for prior reports covering the same issue
4. If OPEN: note as "Existing: #NNN" and skip
5. If CLOSED: verify fix is in place. If regressed, report as "Regression of #NNN"
6. If no match: report as NEW

## Base Per-Finding Format

```
### <ID>: <Short Title>
- **Severity**: CRITICAL | HIGH | MEDIUM | LOW
- **Dimension**: <audit area>
- **Location**: `<file-path>:<line-range>`
- **Status**: NEW | Existing: #NNN | Regression of #NNN
- **Description**: What is wrong and why
- **Evidence**: Code snippet or exact call path demonstrating the issue
- **Impact**: What breaks, when, blast radius
- **Related**: Links to related findings or issues
- **Suggested Fix**: Brief direction (1-3 sentences)
```

Deep audit commands add extra fields (e.g., `Trigger Conditions`, `Flow`, `Changed File`) — see each command for details.

## Issue Labels

These are the labels that actually exist in the repo (verify drift with
`gh label list --repo matiaszanolli/ByroRedux`). `/audit-publish` must only
apply labels from this set — `gh issue create` rejects unknown labels. Labels
are a deliberate repo decision: never `gh label create` from an audit.

Four independent axes. A finding carries one **severity**, one **type**, one or
more **domain** labels, and a **game** label only when the finding is specific
to a title.

**Severity** (exactly one): `critical` · `high` · `medium` · `low` · `info`

**Type** (exactly one): `bug` · `enhancement` · `documentation`

**Domain** (one or more):
`ecs` · `renderer` · `vulkan` · `pipeline` · `shaders` · `memory` · `sync` ·
`concurrency` · `cxx` · `nif` · `nif-parser` · `nifal` · `import-pipeline` ·
`esm-plugin` · `animation` · `physics` · `character` · `water` ·
`terrain-exterior` · `speedtree` · `audio` · `ui` · `save-load` · `scripting` ·
`gameplay` · `ai` · `combat` · `dialogue` · `inventory` · `quests` ·
`legacy-compat` · `performance` · `safety` · `tech-debt` · `doc-rot` ·
`test-gap` · `info`

**Game** (zero or more — only when the finding is specific to a title):
`game:fnv` · `game:fo3` · `game:fo4` · `game:fo76` · `game:skyrim` ·
`game:oblivion` · `game:starfield`

`sync` vs `concurrency`: `sync` is GPU-side (Vulkan semaphores, fences,
barriers, queue submission); `concurrency` is CPU-side (ECS lock ordering,
scheduler access declarations, `RwLock` scopes, data races).

**Added 2026-08-21** — every `game:*`, plus `water` `terrain-exterior`
`shaders` `nifal` `esm-plugin` `save-load` `ui` `physics` `speedtree`
`concurrency` `character` `audio` `doc-rot` `test-gap`. These closed the old
"no label for this subsystem" gaps: the prior guidance to fold them into
`import-pipeline` / `legacy-compat` / `tech-debt` is **obsolete** — label the
subsystem directly.

Still without a label of their own — map to the closest domain and flag the gap
in the publish summary: BSA/BA2/CSG archive readers → `import-pipeline`;
platform/windowing, debug-server / `byro-dbg`, and audit infrastructure
(`.claude/commands/`, `_audit-validate.sh`) → `tech-debt`; FaceGen →
`import-pipeline`. There is no `bsa`, `platform`, `debug-ui`, or `maintenance`
label — do not apply them.

## Report Finalization

1. Save your report to: `docs/audits/AUDIT_<TYPE>_<TODAY>.md` (YYYY-MM-DD format)
2. Do NOT create GitHub issues directly
3. Inform the user the report is ready and suggest:
   ```
   /audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md
   ```
