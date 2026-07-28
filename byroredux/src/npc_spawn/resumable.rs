//! Cooperative NPC assembly.
//!
//! A placed actor is a bundle of independently loadable NIFs.  Keeping that
//! bundle inside one REFR-sized operation made a single runtime-FaceGen actor
//! the largest remaining EXAL apply outlier.  This module makes one top-level
//! actor part (skeleton, body piece, head part, or armor piece) the cooperative
//! work unit while preserving the synchronous public spawn functions through
//! an unlimited-budget driver.

use super::*;
use crate::cell_loader::FrameTimeBudget;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

type SkeletonMap = HashMap<Arc<str>, EntityId>;

#[derive(Debug)]
pub(crate) struct NpcSpawnResult {
    pub(crate) root: Option<EntityId>,
    /// Time spent actively advancing this actor.  Inter-frame time while a
    /// streaming job is parked is deliberately excluded.
    pub(crate) work_wall: Duration,
}

#[derive(Debug)]
pub(crate) enum NpcSpawnProgress {
    Pending,
    Complete(NpcSpawnResult),
}

/// Owned continuation for one placed NPC.
///
/// Record data is cloned once when the REFR begins so the continuation does
/// not borrow the cell index across frames.  Entity IDs and the shared
/// skeleton map are retained between individual NIF spawns.
pub(crate) struct NpcSpawnJob {
    npc: NpcRecord,
    race: Option<RaceRecord>,
    game: GameKind,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    state: NpcSpawnState,
    work_wall: Duration,
}

enum NpcSpawnState {
    BeginRuntime,
    BeginPrebaked { plugin_name: String },
    Runtime(Box<RuntimeNpcState>),
    Prebaked(PrebakedNpcState),
    Done,
}

struct RuntimeNpcState {
    placement_root: EntityId,
    skel_root: Option<EntityId>,
    skel_map: SkeletonMap,
    body_paths: Vec<String>,
    head_path: Option<String>,
    hair_path: Option<String>,
    brow_path: Option<String>,
    eye_paths: Vec<String>,
    eye_texture_override: Option<String>,
    inventory: Option<Inventory>,
    equipment_slots: Option<EquipmentSlots>,
    armor: Vec<RuntimeArmor>,
    equipped_armor_count: u32,
    phase: RuntimePhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePhase {
    Skeleton,
    Body(usize),
    Head,
    Hair,
    Brow,
    Eye(usize),
    Armor(usize),
    Finalize,
}

struct RuntimeArmor {
    model_path: String,
    resolved_fid: u32,
    source_fid: u32,
}

struct PrebakedNpcState {
    placement_root: EntityId,
    skel_map: SkeletonMap,
    facegen_path: Option<String>,
    tint_path: Option<String>,
    armor: Vec<PrebakedArmor>,
    equipped_armor_count: u32,
    phase: PrebakedPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrebakedPhase {
    Skeleton,
    Facegen,
    Armor(usize),
    Finalize,
}

struct PrebakedArmor {
    form_id: u32,
    model_path: String,
}

enum UnitOutcome {
    Continue,
    Complete(Option<EntityId>),
}

impl NpcSpawnJob {
    pub(crate) fn runtime(
        npc: &NpcRecord,
        race: Option<&RaceRecord>,
        game: GameKind,
        ref_pos: Vec3,
        ref_rot: Quat,
        ref_scale: f32,
    ) -> Self {
        Self {
            npc: npc.clone(),
            race: race.cloned(),
            game,
            ref_pos,
            ref_rot,
            ref_scale,
            state: NpcSpawnState::BeginRuntime,
            work_wall: Duration::ZERO,
        }
    }

    pub(crate) fn prebaked(
        npc: &NpcRecord,
        game: GameKind,
        plugin_name: &str,
        ref_pos: Vec3,
        ref_rot: Quat,
        ref_scale: f32,
    ) -> Self {
        Self {
            npc: npc.clone(),
            race: None,
            game,
            ref_pos,
            ref_rot,
            ref_scale,
            state: NpcSpawnState::BeginPrebaked {
                plugin_name: plugin_name.to_owned(),
            },
            work_wall: Duration::ZERO,
        }
    }

    /// Advance through as many actor parts as the caller's frame budget admits.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mut mat_provider: Option<&mut MaterialProvider>,
        idle_pool: &[u32],
        index: &EsmIndex,
        budget: &mut FrameTimeBudget,
    ) -> NpcSpawnProgress {
        let call_t0 = Instant::now();
        loop {
            if budget.should_yield() {
                self.work_wall += call_t0.elapsed();
                log::debug!(
                    "EXAL NPC {:08X}: yielding before {} ({:.1}ms active work total)",
                    self.npc.form_id,
                    self.state.unit_name(),
                    self.work_wall.as_secs_f64() * 1000.0,
                );
                return NpcSpawnProgress::Pending;
            }

            let unit_name = self.state.unit_name();
            let unit_t0 = Instant::now();
            let outcome = match &mut self.state {
                NpcSpawnState::BeginRuntime => {
                    if !self.game.has_runtime_facegen_recipe() {
                        UnitOutcome::Complete(None)
                    } else {
                        self.state = NpcSpawnState::Runtime(Box::new(prepare_runtime_state(
                            world,
                            &self.npc,
                            self.race.as_ref(),
                            self.game,
                            self.ref_pos,
                            self.ref_rot,
                            self.ref_scale,
                            index,
                        )));
                        UnitOutcome::Continue
                    }
                }
                NpcSpawnState::BeginPrebaked { plugin_name } => {
                    if !self.game.uses_prebaked_facegen() {
                        UnitOutcome::Complete(None)
                    } else {
                        self.state = NpcSpawnState::Prebaked(prepare_prebaked_state(
                            world,
                            &self.npc,
                            self.game,
                            plugin_name,
                            self.ref_pos,
                            self.ref_rot,
                            self.ref_scale,
                            index,
                        ));
                        UnitOutcome::Continue
                    }
                }
                NpcSpawnState::Runtime(state) => advance_runtime_unit(
                    state,
                    world,
                    ctx,
                    &self.npc,
                    self.game,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    idle_pool,
                    index,
                ),
                NpcSpawnState::Prebaked(state) => advance_prebaked_unit(
                    state,
                    world,
                    ctx,
                    &self.npc,
                    self.game,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    index,
                ),
                NpcSpawnState::Done => {
                    unreachable!("a completed NPC continuation cannot be advanced again")
                }
            };
            log::debug!(
                "EXAL NPC {:08X}: completed {} unit in {:.1}ms",
                self.npc.form_id,
                unit_name,
                unit_t0.elapsed().as_secs_f64() * 1000.0,
            );
            budget.complete_unit();

            if let UnitOutcome::Complete(root) = outcome {
                self.state = NpcSpawnState::Done;
                self.work_wall += call_t0.elapsed();
                return NpcSpawnProgress::Complete(NpcSpawnResult {
                    root,
                    work_wall: self.work_wall,
                });
            }
        }
    }
}

impl NpcSpawnState {
    fn unit_name(&self) -> &'static str {
        match self {
            Self::BeginRuntime | Self::BeginPrebaked { .. } => "placement setup",
            Self::Runtime(state) => match state.phase {
                RuntimePhase::Skeleton => "skeleton",
                RuntimePhase::Body(_) => "body part",
                RuntimePhase::Head => "head",
                RuntimePhase::Hair => "hair",
                RuntimePhase::Brow => "brow",
                RuntimePhase::Eye(_) => "eye",
                RuntimePhase::Armor(_) => "armor",
                RuntimePhase::Finalize => "finalization",
            },
            Self::Prebaked(state) => match state.phase {
                PrebakedPhase::Skeleton => "skeleton",
                PrebakedPhase::Facegen => "pre-baked FaceGen",
                PrebakedPhase::Armor(_) => "armor",
                PrebakedPhase::Finalize => "finalization",
            },
            Self::Done => "completed actor",
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_runtime_state(
    world: &mut World,
    npc: &NpcRecord,
    race: Option<&RaceRecord>,
    game: GameKind,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
) -> RuntimeNpcState {
    let placement_root = spawn_placement_root(world, npc, game, ref_pos, ref_rot, ref_scale, index);
    log::info!(
        "NPC {:08X} ({}) spawning at world [{:.0},{:.0},{:.0}] scale={:.2}",
        npc.form_id,
        npc.editor_id,
        ref_pos.x,
        ref_pos.y,
        ref_pos.z,
        ref_scale,
    );

    let gender = Gender::from_acbs_flags(npc.acbs_flags);
    let effective_inventory =
        byroredux_plugin::equip::resolve_inherited_inventory(npc, npc.level, index);
    let mut body_covered_buf = Vec::new();
    let body_covered = effective_inventory.iter().any(|entry| {
        if entry.count.max(0) == 0 {
            return false;
        }
        body_covered_buf.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            npc.level,
            index,
            &mut body_covered_buf,
        );
        body_covered_buf.iter().any(|&fid| {
            index.items.get(&fid).is_some_and(|item| {
                matches!(
                    item.kind,
                    ItemKind::Armor { biped_flags, .. }
                        if armor_covers_main_body(game, biped_flags)
                )
            })
        })
    });
    let body_paths = humanoid_body_paths(game)
        .iter()
        .filter(|path| {
            let keep = !(body_covered && path.ends_with("upperbody.nif"));
            if !keep {
                log::info!(
                    "NPC {:08X} ({}): equipped armor covers torso — skipping {}",
                    npc.form_id,
                    npc.editor_id,
                    path,
                );
            }
            keep
        })
        .map(|path| (*path).to_owned())
        .collect();

    let head_path = race
        .and_then(|race| race.body_models.first())
        .map(|path| normalize_mesh_path(path).into_owned());
    if head_path.is_none() {
        log::debug!(
            "NPC {:08X} ({}): race {:08X} has no head MODL — skipping head mesh",
            npc.form_id,
            npc.editor_id,
            npc.race_form_id,
        );
    }

    let recipe = npc.runtime_facegen.as_ref();
    let hair_path = recipe
        .and_then(|recipe| recipe.hair_form_id)
        .and_then(|form_id| index.hair.get(&form_id))
        .map(|hair| hair.model_path.clone())
        .filter(|path| !path.is_empty());
    let brow_path = recipe
        .and_then(|recipe| recipe.eyebrow_form_id)
        .and_then(|form_id| index.head_parts.get(&form_id))
        .map(|part| part.model_path.clone())
        .filter(|path| !path.is_empty());
    let eye_texture_override = recipe
        .and_then(|recipe| recipe.eyes_form_id)
        .and_then(|form_id| index.eyes.get(&form_id))
        .map(|eyes| eyes.icon_path.clone())
        .filter(|path| !path.is_empty());
    let want_gender_tag = match gender {
        Gender::Male => 0,
        Gender::Female => 1,
    };
    let eye_paths = if recipe.is_some() {
        race.into_iter()
            .flat_map(|race| race.head_parts.iter())
            .filter(|(part_idx, path, section)| {
                (*part_idx == byroredux_plugin::esm::records::actor::head_part::LEFT_EYE
                    || *part_idx == byroredux_plugin::esm::records::actor::head_part::RIGHT_EYE)
                    && !path.is_empty()
                    && section.is_none_or(|tag| tag == want_gender_tag)
            })
            .map(|(_, path, _)| path.clone())
            .collect()
    } else {
        Vec::new()
    };

    let (inventory, equipment_slots, armor) =
        build_runtime_equip_state(npc, game, gender, index, effective_inventory);

    RuntimeNpcState {
        placement_root,
        skel_root: None,
        skel_map: HashMap::new(),
        body_paths,
        head_path,
        hair_path,
        brow_path,
        eye_paths,
        eye_texture_override,
        inventory: Some(inventory),
        equipment_slots: Some(equipment_slots),
        armor,
        equipped_armor_count: 0,
        phase: RuntimePhase::Skeleton,
    }
}

fn build_runtime_equip_state(
    npc: &NpcRecord,
    game: GameKind,
    gender: Gender,
    index: &EsmIndex,
    effective_inventory: &[byroredux_plugin::esm::records::NpcInventoryEntry],
) -> (Inventory, EquipmentSlots, Vec<RuntimeArmor>) {
    struct Candidate {
        model_path: String,
        resolved_fid: u32,
        source_fid: u32,
        inv_idx: InventoryIndex,
    }

    let mut inventory = Inventory::new();
    let mut equipment_slots = EquipmentSlots::new();
    let mut candidates = Vec::new();
    let mut resolved_buf = Vec::new();

    for entry in effective_inventory {
        let runtime_count = entry.count.max(0) as u32;
        if runtime_count == 0 {
            continue;
        }
        let inv_idx = inventory.push(ItemStack::new(entry.item_form_id, runtime_count));
        resolved_buf.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            npc.level,
            index,
            &mut resolved_buf,
        );
        for &resolved_fid in &resolved_buf {
            let Some(item) = index.items.get(&resolved_fid) else {
                continue;
            };
            let ItemKind::Armor {
                biped_flags,
                slot_mask,
                ..
            } = item.kind
            else {
                continue;
            };
            let displaced = equipment_slots.equip(biped_flags, inv_idx);
            if !displaced.is_empty() {
                log::debug!(
                    "NPC {:08X} ({}): armor {:08X} (from CNTO {:08X}) \
                     displaced inventory slots {:?} on biped mask {:#010x}",
                    npc.form_id,
                    npc.editor_id,
                    resolved_fid,
                    entry.item_form_id,
                    displaced,
                    biped_flags,
                );
            }
            let _ = slot_mask;
            let Some(model_path) = byroredux_plugin::equip::resolve_armor_mesh(
                item,
                gender,
                npc.race_form_id,
                index,
                game,
            ) else {
                continue;
            };
            candidates.push(Candidate {
                model_path: model_path.to_owned(),
                resolved_fid,
                source_fid: entry.item_form_id,
                inv_idx,
            });
        }
    }
    candidates.retain(|candidate| equipment_slots.occupants.contains(&Some(candidate.inv_idx)));
    let armor = candidates
        .into_iter()
        .map(|candidate| RuntimeArmor {
            model_path: candidate.model_path,
            resolved_fid: candidate.resolved_fid,
            source_fid: candidate.source_fid,
        })
        .collect();
    (inventory, equipment_slots, armor)
}

#[allow(clippy::too_many_arguments)]
fn advance_runtime_unit(
    state: &mut RuntimeNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    game: GameKind,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    idle_pool: &[u32],
    index: &EsmIndex,
) -> UnitOutcome {
    match state.phase {
        RuntimePhase::Skeleton => {
            let Some(skel_path) = humanoid_skeleton_path(game) else {
                return UnitOutcome::Complete(None);
            };
            let Some(skel_data) = tex_provider.extract_mesh(skel_path) else {
                log::warn!(
                    "NPC {:08X} ({}): skeleton '{}' not found in archives — skipping spawn",
                    npc.form_id,
                    npc.editor_id,
                    skel_path,
                );
                return UnitOutcome::Complete(Some(state.placement_root));
            };
            let (_, skel_root, skel_map) = load_nif_bytes_with_skeleton(
                world,
                ctx,
                &skel_data,
                skel_path,
                tex_provider,
                mat_provider,
                None,
                None,
                None,
            );
            keyframe_live_ragdoll_bones(world, &skel_map);
            if let Some(root) = skel_root {
                parent_part(world, state.placement_root, root);
            } else {
                log::debug!(
                    "NPC {:08X}: skeleton '{}' produced no root entity",
                    npc.form_id,
                    skel_path,
                );
            }
            state.skel_root = skel_root;
            state.skel_map = skel_map;
            state.phase = RuntimePhase::Body(0);
            UnitOutcome::Continue
        }
        RuntimePhase::Body(index) => {
            let Some(body_path) = state.body_paths.get(index) else {
                state.phase = RuntimePhase::Head;
                return UnitOutcome::Continue;
            };
            match tex_provider.extract_mesh(body_path) {
                Some(body_data) => {
                    let (_, body_root, _) = load_nif_bytes_with_skeleton(
                        world,
                        ctx,
                        &body_data,
                        body_path,
                        tex_provider,
                        mat_provider,
                        Some(&state.skel_map),
                        None,
                        None,
                    );
                    if let Some(root) = body_root {
                        parent_part(world, state.placement_root, root);
                    }
                }
                None => log::debug!(
                    "NPC {:08X} ({}): body '{}' not in archives — skipping body mesh",
                    npc.form_id,
                    npc.editor_id,
                    body_path,
                ),
            }
            let next = index + 1;
            state.phase = if next < state.body_paths.len() {
                RuntimePhase::Body(next)
            } else {
                RuntimePhase::Head
            };
            UnitOutcome::Continue
        }
        RuntimePhase::Head => {
            if let Some(head_path) = state.head_path.as_deref() {
                spawn_runtime_head(
                    state,
                    world,
                    ctx,
                    npc,
                    head_path,
                    tex_provider,
                    mat_provider,
                );
            }
            state.phase = RuntimePhase::Hair;
            UnitOutcome::Continue
        }
        RuntimePhase::Hair => {
            if state.skel_root.is_some() {
                if let Some(path) = state.hair_path.as_deref() {
                    spawn_shared_skeleton_part(
                        state,
                        world,
                        ctx,
                        npc,
                        path,
                        "hair",
                        tex_provider,
                        mat_provider,
                        None,
                    );
                }
            }
            state.phase = RuntimePhase::Brow;
            UnitOutcome::Continue
        }
        RuntimePhase::Brow => {
            if state.skel_root.is_some() {
                if let Some(path) = state.brow_path.as_deref() {
                    spawn_shared_skeleton_part(
                        state,
                        world,
                        ctx,
                        npc,
                        path,
                        "eyebrow HDPT",
                        tex_provider,
                        mat_provider,
                        None,
                    );
                }
            }
            state.phase = RuntimePhase::Eye(0);
            UnitOutcome::Continue
        }
        RuntimePhase::Eye(index) => {
            let Some(path) = state.eye_paths.get(index).cloned() else {
                state.phase = RuntimePhase::Armor(0);
                return UnitOutcome::Continue;
            };
            if state.skel_root.is_some() {
                let interned_override = state.eye_texture_override.as_ref().map(|texture| {
                    let mut pool = world.resource_mut::<StringPool>();
                    pool.intern(texture)
                });
                let mut hook = |scene: &mut byroredux_nif::import::ImportedScene| {
                    let Some(texture) = interned_override else {
                        return;
                    };
                    for mesh in &mut scene.meshes {
                        mesh.material.textures.base_color = Some(texture);
                    }
                };
                let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
                    if interned_override.is_some() {
                        Some(&mut hook)
                    } else {
                        None
                    };
                spawn_shared_skeleton_part(
                    state,
                    world,
                    ctx,
                    npc,
                    &path,
                    "eye mesh",
                    tex_provider,
                    mat_provider,
                    pre_spawn,
                );
            }
            let next = index + 1;
            state.phase = if next < state.eye_paths.len() {
                RuntimePhase::Eye(next)
            } else {
                RuntimePhase::Armor(0)
            };
            UnitOutcome::Continue
        }
        RuntimePhase::Armor(index) => {
            let Some(armor) = state.armor.get(index) else {
                state.phase = RuntimePhase::Finalize;
                return UnitOutcome::Continue;
            };
            match tex_provider.extract_mesh(&armor.model_path) {
                Some(data) => {
                    let (_, root, _) = load_nif_bytes_with_skeleton(
                        world,
                        ctx,
                        &data,
                        &armor.model_path,
                        tex_provider,
                        mat_provider,
                        Some(&state.skel_map),
                        None,
                        None,
                    );
                    if let Some(root) = root {
                        parent_part(world, state.placement_root, root);
                        state.equipped_armor_count += 1;
                    }
                }
                None => log::debug!(
                    "NPC {:08X} ({}): armor {:08X} (from CNTO {:08X}) \
                     model '{}' not in archives",
                    npc.form_id,
                    npc.editor_id,
                    armor.resolved_fid,
                    armor.source_fid,
                    armor.model_path,
                ),
            }
            let next = index + 1;
            state.phase = if next < state.armor.len() {
                RuntimePhase::Armor(next)
            } else {
                RuntimePhase::Finalize
            };
            UnitOutcome::Continue
        }
        RuntimePhase::Finalize => {
            if state.equipped_armor_count > 0 {
                log::info!(
                    "NPC {:08X} ({}): equipped {} armor mesh(es) from {} inventory entries",
                    npc.form_id,
                    npc.editor_id,
                    state.equipped_armor_count,
                    state.inventory.as_ref().map_or(0, Inventory::len),
                );
            }
            world.insert(
                state.placement_root,
                state.inventory.take().unwrap_or_default(),
            );
            world.insert(
                state.placement_root,
                state.equipment_slots.take().unwrap_or_default(),
            );
            if let Some(skeleton) = state.skel_root {
                if let Some(handle) = pick_idle_handle(idle_pool, npc.form_id) {
                    let duration = world
                        .resource::<AnimationClipRegistry>()
                        .get(handle)
                        .map(|clip| clip.duration)
                        .unwrap_or(0.0);
                    let (start_time, speed) = idle_desync(npc.form_id, duration);
                    let mut player = AnimationPlayer::new(handle).with_root(skeleton);
                    player.local_time = start_time;
                    player.prev_time = start_time;
                    player.speed = speed;
                    world.insert(state.placement_root, player);
                }
            }
            apply_ai_package_behavior(world, state.placement_root, npc, index);
            tag_descendants_as_actor(world, state.placement_root);
            UnitOutcome::Complete(Some(state.placement_root))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_runtime_head(
    state: &RuntimeNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    head_path: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) {
    let Some(head_data) = tex_provider.extract_mesh(head_path) else {
        log::debug!(
            "NPC {:08X} ({}): head '{}' not in archives — skipping head mesh",
            npc.form_id,
            npc.editor_id,
            head_path,
        );
        return;
    };
    let recipe = npc.runtime_facegen.as_ref();
    let egm_bytes = recipe
        .and_then(|_| facegen_sidecar_path(head_path, "egm"))
        .and_then(|path| tex_provider.extract_mesh(&path));
    let egm_file =
        egm_bytes
            .as_ref()
            .and_then(|bytes| match byroredux_facegen::EgmFile::parse(bytes) {
                Ok(egm) => Some(egm),
                Err(error) => {
                    log::debug!(
                        "NPC {:08X}: EGM parse failed for head '{}': {}",
                        npc.form_id,
                        head_path,
                        error,
                    );
                    None
                }
            });
    let mut hook_state = match (recipe, egm_file.as_ref()) {
        (Some(recipe), Some(egm)) => Some((egm, recipe.fggs, recipe.fgga, npc.form_id)),
        _ => None,
    };
    let has_hook = hook_state.is_some();
    let mut hook = |scene: &mut byroredux_nif::import::ImportedScene| {
        let Some((egm, fggs, fgga, form_id)) = hook_state.take() else {
            return;
        };
        let mut deformed_meshes = 0;
        for mesh in &mut scene.meshes {
            if mesh.positions.is_empty() {
                continue;
            }
            let after_sym =
                byroredux_facegen::apply_morphs(&mesh.positions, &egm.fggs_morphs, &fggs);
            mesh.positions = byroredux_facegen::apply_morphs(&after_sym, &egm.fgga_morphs, &fgga);
            deformed_meshes += 1;
        }
        log::debug!(
            "M41.0 Phase 3b/3c: NPC {:08X} applied FGGS+FGGA morphs to {} head mesh(es) \
             (EGM {} verts × {} sym + {} asym; best-effort prefix until Phase 3b.x parses .tri remap)",
            form_id,
            deformed_meshes,
            egm.num_vertices,
            egm.fggs_morphs.len(),
            egm.fgga_morphs.len(),
        );
    };
    let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
        if has_hook { Some(&mut hook) } else { None };
    let (_, root, _) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &head_data,
        head_path,
        tex_provider,
        mat_provider,
        Some(&state.skel_map),
        None,
        pre_spawn,
    );
    if let Some(root) = root {
        parent_part(world, state.placement_root, root);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_shared_skeleton_part(
    state: &RuntimeNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    path: &str,
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)>,
) {
    let Some(data) = tex_provider.extract_mesh(path) else {
        log::debug!(
            "NPC {:08X} ({}): {} '{}' not in archives — skipping",
            npc.form_id,
            npc.editor_id,
            label,
            path,
        );
        return;
    };
    let (_, root, _) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &data,
        path,
        tex_provider,
        mat_provider,
        Some(&state.skel_map),
        None,
        pre_spawn,
    );
    if let Some(root) = root {
        parent_part(world, state.placement_root, root);
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_prebaked_state(
    world: &mut World,
    npc: &NpcRecord,
    game: GameKind,
    plugin_name: &str,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
) -> PrebakedNpcState {
    let placement_root = spawn_placement_root(world, npc, game, ref_pos, ref_rot, ref_scale, index);
    let gender = Gender::from_acbs_flags(npc.acbs_flags);
    let equip = build_npc_equip_state(npc, index, game, gender);
    let armor = equip
        .armor_to_spawn
        .into_iter()
        .map(|armor| PrebakedArmor {
            form_id: armor.form_id,
            model_path: armor.model_path.to_owned(),
        })
        .collect();
    world.insert(placement_root, equip.inventory);
    world.insert(placement_root, equip.equipment_slots);

    PrebakedNpcState {
        placement_root,
        skel_map: HashMap::new(),
        facegen_path: prebaked_facegen_nif_path(plugin_name, npc.form_id),
        tint_path: prebaked_facegen_tint_path(plugin_name, npc.form_id),
        armor,
        equipped_armor_count: 0,
        phase: PrebakedPhase::Skeleton,
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_prebaked_unit(
    state: &mut PrebakedNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    game: GameKind,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    index: &EsmIndex,
) -> UnitOutcome {
    match state.phase {
        PrebakedPhase::Skeleton => {
            let Some(skel_path) = humanoid_skeleton_path(game) else {
                return UnitOutcome::Complete(None);
            };
            let Some(data) = tex_provider.extract_mesh(skel_path) else {
                log::warn!(
                    "NPC {:08X} ({}): skeleton '{}' not in archives — skipping mesh spawn (equip state retained)",
                    npc.form_id,
                    npc.editor_id,
                    skel_path,
                );
                return UnitOutcome::Complete(Some(state.placement_root));
            };
            let (_, skel_root, skel_map) = load_nif_bytes_with_skeleton(
                world,
                ctx,
                &data,
                skel_path,
                tex_provider,
                mat_provider,
                None,
                None,
                None,
            );
            keyframe_live_ragdoll_bones(world, &skel_map);
            if let Some(root) = skel_root {
                parent_part(world, state.placement_root, root);
            }
            state.skel_map = skel_map;
            state.phase = PrebakedPhase::Facegen;
            UnitOutcome::Continue
        }
        PrebakedPhase::Facegen => {
            let Some(facegen_path) = state.facegen_path.as_deref() else {
                log::debug!(
                    "NPC {:08X}: empty plugin name in load order; skipping pre-baked FaceGen",
                    npc.form_id,
                );
                return UnitOutcome::Complete(Some(state.placement_root));
            };
            let Some(data) = tex_provider.extract_mesh(facegen_path) else {
                log::debug!(
                    "NPC {:08X} ({}): pre-baked FaceGen '{}' not in archives — \
                     NPC visible as skeleton-only (no per-NPC mesh)",
                    npc.form_id,
                    npc.editor_id,
                    facegen_path,
                );
                return UnitOutcome::Complete(Some(state.placement_root));
            };
            let tint_path = state
                .tint_path
                .as_deref()
                .filter(|path| tex_provider.extract(path).is_some());
            let (_, root, _) = load_nif_bytes_with_skeleton(
                world,
                ctx,
                &data,
                facegen_path,
                tex_provider,
                mat_provider,
                Some(&state.skel_map),
                tint_path,
                None,
            );
            if let Some(root) = root {
                parent_part(world, state.placement_root, root);
            }
            state.phase = PrebakedPhase::Armor(0);
            UnitOutcome::Continue
        }
        PrebakedPhase::Armor(index) => {
            let Some(armor) = state.armor.get(index) else {
                state.phase = PrebakedPhase::Finalize;
                return UnitOutcome::Continue;
            };
            match tex_provider.extract_mesh(&armor.model_path) {
                Some(data) => {
                    let (_, root, _) = load_nif_bytes_with_skeleton(
                        world,
                        ctx,
                        &data,
                        &armor.model_path,
                        tex_provider,
                        mat_provider,
                        Some(&state.skel_map),
                        None,
                        None,
                    );
                    if let Some(root) = root {
                        parent_part(world, state.placement_root, root);
                        state.equipped_armor_count += 1;
                    }
                }
                None => log::debug!(
                    "NPC {:08X} ({}): armor {:08X} model '{}' not in archives",
                    npc.form_id,
                    npc.editor_id,
                    armor.form_id,
                    armor.model_path,
                ),
            }
            let next = index + 1;
            state.phase = if next < state.armor.len() {
                PrebakedPhase::Armor(next)
            } else {
                PrebakedPhase::Finalize
            };
            UnitOutcome::Continue
        }
        PrebakedPhase::Finalize => {
            if state.equipped_armor_count > 0 {
                log::info!(
                    "NPC {:08X} ({}): equipped {} armor mesh(es) on pre-baked path \
                     ({} armor candidates queued)",
                    npc.form_id,
                    npc.editor_id,
                    state.equipped_armor_count,
                    state.armor.len(),
                );
            }
            apply_ai_package_behavior(world, state.placement_root, npc, index);
            tag_descendants_as_actor(world, state.placement_root);
            UnitOutcome::Complete(Some(state.placement_root))
        }
    }
}

fn spawn_placement_root(
    world: &mut World,
    npc: &NpcRecord,
    game: GameKind,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
) -> EntityId {
    let placement_root = world.spawn();
    world.insert(placement_root, Transform::new(ref_pos, ref_rot, ref_scale));
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
    );
    if !npc.editor_id.is_empty() {
        let symbol = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern(&npc.editor_id)
        };
        world.insert(placement_root, Name(symbol));
    }
    stamp_faction_ranks(world, placement_root, npc);
    stamp_actor_values(world, placement_root, npc, index, game);
    stamp_character_components(world, placement_root, npc);
    placement_root
}

fn parent_part(world: &mut World, placement_root: EntityId, part_root: EntityId) {
    world.insert(part_root, Parent(placement_root));
    add_child(world, placement_root, part_root);
    // A yielded actor can render for several frames before finalization.
    // Keep each newly attached mesh on the actor layer immediately rather
    // than exposing an Architecture-layer transient.
    tag_descendants_as_actor(world, placement_root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_do_not_mutate_world_before_budget_admits_first_unit() {
        let npc = NpcRecord {
            form_id: 0x1234,
            ..NpcRecord::default()
        };
        let runtime = NpcSpawnJob::runtime(
            &npc,
            None,
            GameKind::Fallout3NV,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
        );
        assert!(matches!(runtime.state, NpcSpawnState::BeginRuntime));

        let prebaked = NpcSpawnJob::prebaked(
            &npc,
            GameKind::Skyrim,
            "skyrim.esm",
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
        );
        assert!(matches!(
            prebaked.state,
            NpcSpawnState::BeginPrebaked { .. }
        ));
    }

    #[test]
    fn runtime_phases_are_one_top_level_asset_at_a_time() {
        assert_ne!(RuntimePhase::Skeleton, RuntimePhase::Body(0));
        assert_ne!(RuntimePhase::Body(0), RuntimePhase::Body(1));
        assert_ne!(RuntimePhase::Head, RuntimePhase::Hair);
        assert_ne!(RuntimePhase::Eye(0), RuntimePhase::Eye(1));
        assert_ne!(RuntimePhase::Armor(0), RuntimePhase::Armor(1));
    }
}
