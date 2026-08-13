# Issues 2643, 2660, 2661, 2663

Four unrelated issues spanning different domains/crates.

## #2643 — SF-D9-2026-08-07-04: BGEM palette bits mutually exclusive losing color variant; envmap fill ignores env_mapping_enabled
- **Severity**: LOW
- **Domain**: binary (`byroredux/src/asset_provider/material.rs`, `byroredux/src/cell_loader.rs`)
- **Bug 1**: `byroredux/src/asset_provider/material.rs:1173-1200` — `bgsm_greyscale_lut_is_alpha`/`_enabled` derivation makes `PALETTE_ALPHA`/`PALETTE_COLOR` mutually exclusive; a BGEM authoring both independent bits (format permits it) yields `PALETTE_ALPHA` only, dropping color. The inline NIF effect-shader path (`pack_effect_shader_flags`) derives the two bits independently and can set both — divergence between the two "documented mirror" paths.
- **Bug 2**: `byroredux/src/cell_loader.rs:272-278` — envmap texture fill (`textures.environment`/`environment_mask`) fills unconditionally from `bgem.envmap_texture`/`envmap_mask_texture` without consulting `BgemFile::env_mapping_enabled()` (the #2358 accessor), while `bgem_uses_glass_behavior` *does* consult it — same authored bit honoured for classification, ignored for texture binding.
- **Fix**: (1) preserve both palette bits or pick a documented precedence; (2) gate the envmap fill on `env_mapping_enabled()`.
- **Tests**: fixture with both palette bits set; fixture with `env_mapping_enabled()==false`.

## #2660 — SCR-D6-NEW11-03: #2539's lock isolation is partial — hold scope still nests 6 resource acquisitions (3 writes) and 12 component acquisitions
- **Severity**: MEDIUM
- **Domain**: scripting (`crates/scripting/src/fragment.rs`)
- **Bug**: `6ad64ef6` (closing #2539) scoped `QuestDefinitionRegistry` snapshot-clone and `mark_scene_actor_bindings_dirty` deferral, but `SceneActorBindings` is still read-acquired inside the `(QuestStageState, QuestObjectiveState)` hold scope via `resolve_object` (`:246-248`), plus `PlayerControlState` (3 writes) and 3 other resources, plus 12 component acquisitions (`Inventory` for `AddItem`, `GlobalTransform`+`Transform` for `MoveTo`) remain nested.
- **Impact**: No live deadlock today (`add_exclusive` registration only), but the surface any future parallelization must sweep is much larger than #2539's closure implied.
- **Fix**: Resolve alias lookups (the `resolve_object` results) before the guards are taken — they're knowable from the queue up front. Record residual nesting in house-rule doc #2270. Add a structural test: `apply_effect` takes no `&World` resource handle not already on the deferred struct.

## #2661 — SCR-D6-NEW11-04: ALCS collection aliases are not excluded from the single-entity fill loop
- **Severity**: MEDIUM
- **Domain**: scripting (`crates/scripting/src/scene.rs` `resolve_alias_bindings`) + plugin/esm (`crates/plugin/src/esm/records/misc/quest.rs:712,722` ALCS/ALMI decode)
- **Bug**: `docs/engine/m47-3-quest-alias-design.md` says reference-collection aliases (ALCS) are a Phase 4+ deferral — should decline with `ReferenceCollectionRuntimeUnavailable`. Instead an ALCS alias falls through the ordinary single-entity fill path: binds exactly one candidate, which receives the whole collection's injected factions/inventory, and diagnoses as `Bound` (false success). `ALMI` (collection fill limit) is parsed but never read by any consumer.
- **Fix**: Detect `ALCS` in the fill loop and decline with `ReferenceCollectionRuntimeUnavailable` per the design doc. Either read `ALMI` or drop it explicitly with a comment.
- **Tests**: regression pinning the decline behavior for ALCS aliases.

## #2663 — SCR-D7-NEW11-02: World-placement base-record family and TERM decode VMAD as presence-only flag
- **Severity**: MEDIUM
- **Domain**: esm (`crates/plugin/src/esm/cell/support.rs:74`, `crates/plugin/src/esm/records/dispatch_world_placement.rs:25-27`, `crates/plugin/src/esm/records/misc/world.rs:383`, `crates/plugin/src/esm/records/index.rs:605-629`)
- **Bug**: Sibling of closed #2189 (which fixed `CommonItemFields`). Two other record populations still drop VMAD payload:
  1. MODL-only world-placement family (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT) parsed by `parse_modl_group` → `build_static_object_from_subs`: `VMAD` arm sets a bool and drops the payload. `StaticObject` has no field to store `ScriptInstanceData`; `EsmIndex` has no typed map for `base_record_script_instance` to consult.
  2. `TERM` is parsed through `CommonNamedFields` (decodes VMAD fully) but `parse_term` only copies `editor_id`/`full_name`/`model_path`/`script_form_id`, discarding `script_instance`; `base_record_script_instance` has no `terminals` arm. The justifying comment ("TERM is FO3/FNV-only") is factually wrong — FO4 ships 207 VMAD-bearing TERM records.
- **Impact**: Corpus census: Skyrim.esm 42 unreachable scripted base records (FURN/DOOR/FLOR); Fallout4.esm 442 unreachable (FURN/TERM/MSTT/FLOR/DOOR/LIGH/TACT/STAT). Silent decline, no corrupted state, but scripted crafting stations/planters/workshop bars/jail doors/elevator doors/FO4 terminals attach nothing.
- **Fix**: Add `script_instance: Option<ScriptInstanceData>` to `StaticObject`, populate in `build_static_object_from_subs`'s VMAD arm, add a `statics` lookup arm at the END of `base_record_script_instance` (typed maps keep priority). Add `script_instance` to `TermRecord`, wire from `common.script_instance`, add a `terminals` arm, delete the incorrect FO3/FNV-only comment.
- **Tests**: mirror `base_record_script_instance_resolves_an_item_records_vmad`.

## Domains
- #2643 → binary (`byroredux`)
- #2660 → scripting (`byroredux-scripting`)
- #2661 → scripting (`byroredux-scripting`) + esm (`byroredux-plugin`)
- #2663 → esm (`byroredux-plugin`)
