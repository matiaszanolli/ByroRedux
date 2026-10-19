**HEAD**: 75b921cf4 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-12.md (HEAD 5d31fd637) · **Audited**: Dims 1, 4, 5, 6 (area half), 8 (cell_loader/NPC-hook half), 9 (spawner/boundary inventory); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 2 (no commits in `import/mesh`, `rotation.rs` beyond doc-only d81f7501f / 89a0b6ccd), Dim 3 (lights/skin: only the `scene_satellites` consolidation touched the area), Dim 7 (call sites in the area only; guards run)

# NIFAL Audit — 2026-10-19 (AREA-SCOPED: streaming-deep)

**This report is AREA-SCOPED, not a full-codebase NIFAL audit.** Scope was `byroredux/src/{streaming,npc_spawn,cell_loader}/` and the data flows they touch, read through the NIFAL lens (Imported -> translate -> Canonical, single boundary, no render-time fallback). The next full run must not treat this as a baseline; the 2026-10-12 report remains the full baseline. Out of area and not re-audited: `crates/nif/src/import/{coord,units,transform}`, the exterior/env translators (`/audit-exterior`), BGSM/CDB parse (`/audit-parsers`), `translation_completeness` (`--ignored`, needs game data, not reachable from this area).

Scratch notes: `/tmp/audit/streaming-deep/nifal/dim_*.md`.

Phase-1 guards, run this session:
- `cargo test -p byroredux --bin byroredux -- material_translate canonical_animation_completeness_harness every_hkx_sample_field every_exterior_spawner every_source_derived npc_spawn seam_blend graph_nodes node_collision`: 233 passed, 0 failed, 10 ignored.
- `cargo test -p byroredux-nif --lib -- dispatch_coverage_tests light_dispatch_coverage_tests the_scene_graph_walkers roles_covers_every_field values_covers_every_field hide_skin_partitions`: 8 passed.
- Not run: the engine, smoke tests, `translation_completeness`.

Dedup: forwarded sibling IDs (EXT-D6-01..04, EXT-D2-05, #6053, CONC-SD-01..03, PERF-SD-01..05, #5715, #5999, GAME-SD-01..04, ECS-SD-01..04, SAVE-SD-01..04, CHAR-SD-01..03) and the late ESM forward (ESM-SD-01..03) were checked. None overlaps the findings below. ECS-SD-04 is confirmed and extended (section after the findings).

## Executive Summary

**New findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 3 LOW.** No violation of the material boundary itself.

| Category | Status | Note |
|---|---|---|
| Material | converged | `translate_material` still takes `&ImportedMaterial`; the only `pub` delta on the canonical type since 10-12 is `source_shader_type` (2266f7778). All four area callers route through the boundary; `GameKind` appears nowhere in `spawn/`, `material_translate.rs` or `nif_loader.rs`'s material section. |
| Particles | converged | `apply_emitter_overlays(` has exactly the two spawn sites. |
| Animation | converged | Every area producer of a core clip goes through `convert_nif_clip` (`object_sequences.rs`, `spawn.rs` graph sequences, `partial.rs`). |
| Collision | converged | `spawn_packed_collision_proxy` stays renderer-free and applies scale once (pinned by `packed_proxy_placement_scale_is_applied_once_end_to_end`). |
| Nodes | triaged | Parked fields have zero canonical consumers; ledger gap in NIFAL-SD-02. The new `graph_nodes` path is a node-keeping spawn for behaviour-graph props only. |
| NPC part assembly (spawn-time hooks) | pending | NIFAL-SD-01, -03, -04. |

Violations by invariant: single-boundary 2 (SD-01, SD-03), no-leak/parked 1 (SD-02), harness-gap 1 (SD-04), no-fabrication 0, no-render-time-fallback 0.

Delta since the baseline (`5d31fd637..HEAD`) touching the area: 2266f7778 (#4256), 2a8663f39 (#5408), 2ba5c8c12 (#5794 shared finalize tail), the M47.2 behaviour-graph work (6db3805a5, fa1473e25: graph_nodes, node colliders, `scene_satellites` / `from_scene` consolidation, #5795), a10b5d578 (#5523 SCOL CM), e32304ec9 (#5489 PKIN template cells), bb57270c3 (#5496 LOD source worldspace), a9384f9c0 (#4913 tree LOD).

## Findings

### NIFAL-SD-01: Restoring a stripped NPC body part re-imports it without the spawn-time hooks, so an Oblivion body loses its race skin
- **Severity**: MEDIUM
- **Dimension**: Nodes / NPC part assembly (spawn-time translation hooks)
- **Tier Violated**: `single-boundary`
- **Game Affected**: Oblivion (race skin texture); FO3/FNV (hand seam blend)
- **Location**: `byroredux/src/npc_spawn/loot_appearance.rs:780-790` (restore import), `byroredux/src/npc_spawn/resumable/runtime.rs:240,266` (parts registered for restore), `runtime.rs:581-640` (the hooks the restore skips)
- **Status**: NEW (checked #5487, closed: it fixed the spawn path only)
- **Description**: The NPC part appearance is not a function of the part path. It depends on per-call-site `pre_spawn` closures: the Oblivion race body-section `ICON` base-texture override (#5487, `body_textures`), and the FO3/FNV hand seam blend. The initial spawn applies them (`runtime.rs:581-640`). A body piece covered by worn armour is skipped at spawn and registered as `RestorePart::body(path)` (`runtime.rs:266`). When the outfit is removed (corpse take-all, `equipment_appearance_system`), the part is re-imported by `load_nif_bytes_with_skeleton(.., part.tint.as_deref(), None)` at `loot_appearance.rs:780`. `RestorePart` carries only `path` and `tint` (`loot_appearance.rs:14-17`) and `pre_spawn` is `None`.
- **Evidence**: `humanoid_body_path_icon_index` returns `Some` only for Oblivion (`npc_spawn.rs:688-703`), and its doc says the bodies "enter with the race's body-section ICON as their spawn-time base texture" because the NIF default is Imperial (`runtime.rs:581-583`, `runtime.rs:1217-1220`). `RestorePart` has no texture field and `loot_appearance.rs` has no `body_textures`/`race_body_texture` reference, so the restored body gets the NIF's own Imperial skin while the head keeps the race head texture.
- **Impact**: A non-Imperial Oblivion NPC (Dunmer, Orc, ...) whose body armour is taken shows an Imperial-toned torso, legs or hands under a race-toned head, the exact mismatch #5487 fixed at spawn. On FO3/FNV the restored hands lose the wrist-cut normal/tone blend (minor). No crash and no data loss. Reachable on any corpse with covered body parts.
- **Related**: #5487 (closed), #5794 (shared finalize tail; its dedup did not cover the restore re-import), seam_blend.
- **Suggested Fix**: Make the part's appearance a value, not a closure: store the resolved `body_texture` on `RestorePart` and apply it through the same imported-tier override helper the spawn uses (extract that closure body into one function both sites call). Add a test that an Oblivion restore carries the race texture.

### NIFAL-SD-02: `bs_sub_index` is listed as raw-parked with no consumer, but equip-time partition hiding reads it
- **Severity**: LOW
- **Dimension**: Nodes
- **Tier Violated**: `parked-not-leak` (ledger inaccuracy; not a leak)
- **Game Affected**: Skyrim SE, FO4 (`BSSubIndexTriShape`); FO3/FNV/Oblivion (`triangle_body_parts`)
- **Location**: `docs/engine/nifal.md:848`; `crates/nif/src/import/types.rs:1471-1559` (`hide_skin_partitions`), `:1455` (`is_dismemberment_cap`); consumers `byroredux/src/npc_spawn/resumable/mod.rs:322,460`, `prebaked.rs:308,354`, `scene/nif_loader.rs:967`, `npc_spawn/seam_blend.rs:148`
- **Status**: NEW (documentation)
- **Description**: The passthrough table says `bs_sub_index` is "raw-parked — dismemberment / locational-damage system" with no consumer, and the audit skill's Dim 4 first step expects zero consumers. In fact `ImportedMesh::hide_skin_partitions` reads `bs_sub_index` segments (and `skin.triangle_body_parts`) to remove triangles covered by worn armour, driven from the NPC `pre_spawn` hooks. `is_dismemberment_cap` is used to drop FO3/FNV gore caps at spawn (`nif_loader.rs:967`) and to exclude them as seam neighbours. None of `triangle_body_parts`, `hide_skin_partitions` or the cap drop appears in `nifal.md` (grep for `dismember`/`partition`/`triangle_body_parts` finds only line 848).
- **Evidence**: This is a raw-tier to raw-tier transform before `translate_material` and feeds no canonical component, so it is not a leak. The skill's first-step grep only searches `byroredux/`, `renderer/`, `core/` and so stays empty even though the nif crate itself consumes the field.
- **Impact**: A future auditor reads "zero consumers" and may flag the legitimate consumer, or delete the field as dead. The dropped gore caps are a deliberate "until dismemberment exists" translation drop that the ledger does not record.
- **Related**: `nifal.md` §2 "Passthroughs".
- **Suggested Fix**: Change the `bs_sub_index` row to "consumed by `hide_skin_partitions` (equip-time skin hiding); the dismemberment system itself remains parked", and add a ledger row for `triangle_body_parts` / gore-cap drop.

### NIFAL-SD-03: FO3/FNV `HairTint` is resolved by a `GameKind` branch plus a vertex-colour rewrite at NPC spawn
- **Severity**: LOW
- **Dimension**: Material / Shader-flags
- **Tier Violated**: `single-boundary`
- **Game Affected**: FO3, FNV (Oblivion takes a different branch of the same closure)
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:382` (`hair_uses_fallout_tint_mask: matches!(game, GameKind::Fallout3NV)`), `:687-700` (hook), `:1433-1447` (`bake_fallout_hair_tint`), `:1421-1423` (`fallout_hair_tint_factor`)
- **Status**: NEW
- **Description**: The tint formula is a faithful decode of the shipped `HairTint` pixel shader (`runtime.rs:1404-1420`), so the output is right. But the choice to apply it is `GameKind`, not authored data, and the result overwrites the imported `mesh.colors` in place (the tint mask becomes a final modulation). Skyrim's hair tint travels as `ShaderTypeData::HairTint` into `Material.shader_type_fields.hair_tint_color`; Oblivion hair multiplies `material.diffuse_color`. Three carriers for one concept, chosen at spawn. FO3/FNV author the `Hair` bit (`F3SF1` bit 18, nif.xml `BSShaderFlags`), which `ImportedMaterial` does not carry (grep of `crates/nif/src/import` finds no consumer of that bit), so the only discriminator available at the boundary is absent.
- **Evidence**: `bake_fallout_hair_tint` runs on every mesh of the hair NIF, not on meshes that author the flag. The neutral tint `0.5` for an unauthored `HCLR` is documented as "an engine choice" (`runtime.rs:1430`), which is acceptable under no-fabrication.
- **Impact**: No wrong pixels today. A hair NIF reached by another path (a cell-placed FO3 mannequin or hat, a modded hair mesh) renders its tint mask as teal vertex colour, and the mask has no canonical home. A cost for the next hair-tint consumer, not a live defect.
- **Related**: ECS-SD-04 (`source_shader_type`).
- **Suggested Fix**: Surface the authored `Hair` flag on `ImportedMaterial` and let the NPC hook key on it instead of `GameKind`; longer term give the vertex-colour tint-mask a canonical field.

### NIFAL-SD-04: NPC texture hooks override `base_color` but leave the provenance label at the NIF slot
- **Severity**: LOW
- **Dimension**: Shader-flags / texture roles
- **Tier Violated**: `harness-gap`
- **Game Affected**: Oblivion, FO3, FNV
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:601` (body), `:750` (eyes), `:1029` (head); compare `byroredux/src/scene/nif_loader.rs:1250-1257`
- **Status**: NEW
- **Description**: The FaceGen tint path stamps `MaterialTextureSource::RuntimeOverride` when it replaces the base colour (`nif_loader.rs:1256`). The three legacy hooks replace `mesh.material.textures.base_color` on the imported tier but do not touch `texture_sources`, which `spawn_nif_mesh` derives from the mesh's own `ImportedTextureSource` (`nif_loader.rs:1196-1210`). `tex.*` / `mat.dump` therefore report `nif-texture-set` for a texture that came from the RACE record.
- **Evidence**: `ImportedTextureSource` has no runtime variant (`types.rs:326-334`), so a hook cannot stamp it; only `diffuse_override` can.
- **Impact**: Diagnostics only. It misleads the documented "chrome means missing textures" triage on NPC heads and bodies.
- **Related**: #4421.
- **Suggested Fix**: Route the hooks through the same override channel as `diffuse_override` (a per-mesh runtime override carried into `spawn_nif_mesh`), which also fixes the provenance label.

## ECS-SD-04 confirmed and extended (existing sibling finding, not re-counted)

Confirmed: `rg source_shader_type` finds the writer at `material_translate.rs:732` and the field at `material.rs:240`, and no reader. Extension from the NIFAL lens:

- The absence is structural, not an oversight. Every consumer in the area that needs the authored shader type runs before `translate_material`, on the imported tier, so it cannot read a canonical field that does not exist yet: `select_facegen_diffuse(.., mesh.material.material_kind)` (`nif_loader.rs:1250`; safe because the glass classifier has not run), the REFR overlay's `TextureSlotContext`, and NIFAL-SD-03's hair branch. Downstream of the boundary the only consumer of the authored type is the shader, which reads `material_kind`.
- The field is not in `nifal.md`, so the ledger does not say "carried, no reader". Suggest recording it as parked-not-leak with the first planned reader, or reverting if none exists.
- The audit skill is stale on this: `.claude/commands/audit-nifal/SKILL.md:90` still says "Known-open: the shader-type discriminator never reaches the canonical `Material` (#4256)". #4256 was closed 2026-10-19 by 2266f7778. Route to `/audit-sync`.

## Verified clean (area-scoped; the next streaming-deep run can skim)

- **Boundary callers.** `translate_material_with_provenance` at `mesh_instance.rs:1140` and `nif_loader.rs:1276`; `translate_material` at `placement_lod.rs:549` and `object_lod.rs:376`; texture-only variants at terrain, terrain LOD, `.btr`, water, tree LOD. `every_exterior_spawner_inserts_a_boundary_material` is green and its needle (`, MeshHandle(`) finds the only non-scan form, `cornell/builders.rs:407`, which is a documented exemption. `tree_lod.rs` (new, #4913) joined the provenance guard list.
- **Raw-tier re-reads after the boundary** (`source_material.alpha_test` / `is_decal` / `material_kind` at `mesh_instance.rs:1469,1532`) equal the canonical values (`material_translate.rs:728` copies `alpha_test` verbatim; `is_decal` is a marker, not a `Material` field). Documented, no divergence.
- **Dim 4 first step**: all seven parked fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) return zero consumers outside the nif crate.
- **Dim 5**: `apply_emitter_overlays(` outside `systems/particle.rs` appears at `scene/nif_loader.rs:1713` and `cell_loader/spawn.rs:1320` only; the cell site rotates by `ref_rot * nif_rot` (#4398 holds).
- **Streaming pool rebinding**: `reintern_imported_meshes` (`partial.rs:183`) covers every `FixedString` field a worker import can carry (`textures` via `map_ref`, `material_path`). The only other `FixedString` field, `ImportedTextureEffect.texture_path`, is never produced (dead extractor, `nifal.md` Passthroughs). `graph_nodes::build` runs on a throwaway `StringPool`, but it retains only `Arc<str>` names.
- **Behaviour-graph props (new M47.2 path)**: `graph_nodes` keeps the flat `GlobalTransform` and only re-parents; `build` declines on a mesh-count or name mismatch or a non-identity root. `spawn_node_collisions` rewrites `Static` to `Keyframed` at the spawn site (`spawn.rs:1448`); judged intentional (the graph is attached later by `populate_object_graphs`, so spawn cannot know which nodes move) and equivalent to the packed-proxy choice, so not a finding.
- **Magic-shader emissive tint** (`systems/magic_shader.rs`) mutates the saved `Material` column, while `ShaderTintRestore` / `PlayingShaders` are session-local. Disproved as a bug: `restore_world` has no production caller (tests only) and the live-load path re-imports materials from the NIF, so a tint cannot persist across a load today. Becomes live if `restore_world` is ever wired as a production load path.
- **Pre-spawn hooks mutate a private copy**: `apply_spawn_hook` `Arc::make_mut`s the cached scene only when a hook exists (`nif_loader.rs:237-247`), so one NPC's face or seam cannot leak into the shared `SceneImportCache`.
- **Seam blend skin test** (`is_skin_texture`, `characters\` prefix): the spawn-time path heuristic is grounded in measurement for FNV (`upperbody.nif` kind 0 on every mesh, torso skin under `textures\characters\male`, underwear arms under `textures\armor\`), and Oblivion has no shader type to key on. The two tuning constants are documented as engine tuning set by eye, with the measured widest gap (2.4 units) behind `SEAM_MATCH_RADIUS`.

## Documented-limitation ledger (restated so it is not re-reported)

Parked-not-leak: node/mesh passthroughs (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`; `bs_sub_index` only for the dismemberment system itself, see SD-02); FO4+ `bhkNPCollisionObject` blob, phantoms, size-over-life particle curves, per-light ambient and morph-weight animation channels, affected-node light scoping (incl. `NiAmbientLight`), CDB parked slots and the Starfield additive-blend carry-over (#5509), the remaining texture-set-slot half of #5502, the FO4 streaming partial keeping `attach_points` / `child_attach_connections` `None` (#1594, documented in `from_scene`).

## Could not check

Engine and smoke legs (rule 4); `translation_completeness` (opt-in, game data, out of area); whether FO3/FNV hair NIFs author the `Hair` bit (no dump tool for shader flags was run; the nif.xml bit is cited, not measured); the end-to-end Oblivion take-all restore (static reading only, no Vulkan device).

Suggest `/audit-publish docs/audits/AUDIT_NIFAL_2026-10-19.md` (domain label `nifal`; add `game:oblivion` for NIFAL-SD-01, `game:fnv` / `game:fo3` for NIFAL-SD-03, `doc-rot` for NIFAL-SD-02).
