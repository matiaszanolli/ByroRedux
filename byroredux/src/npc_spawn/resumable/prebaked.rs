//! Pre-baked spawn arm (#5091 split of `resumable.rs`): the Skyrim+
//! state machine that composes authored face/body NIFs rather than
//! building a head through runtime FaceGen.
use super::*;

pub(super) struct PrebakedNpcState {
    pub(super) appearance: NpcLootAppearance,
    pub(super) placement_root: EntityId,
    pub(super) skel_root: Option<EntityId>,
    pub(super) skel_map: SkeletonMap,
    pub(super) skeleton_path: Option<String>,
    pub(super) facegen_path: Option<String>,
    pub(super) tint_path: Option<String>,
    pub(super) armor: Vec<PrebakedArmor>,
    /// #3409 — biped bits an equipped item took from the FaceGen head, in
    /// `hide_skin_partitions` format. See `NpcEquipState::facegen_hidden_mask`.
    pub(super) facegen_hidden_mask: u32,
    pub(super) equipped_armor_count: u32,
    /// #4700 — [`RuntimeNpcState::combat_anim_draugr`]'s pre-baked twin,
    /// from the same `Use Traits`-resolved race.
    pub(super) combat_anim_draugr: bool,
    /// Player-body attach — same finalize-skip contract as the runtime twin.
    pub(super) player_body: bool,
    pub(super) phase: PrebakedPhase,
}

impl PrebakedNpcState {
    /// Missing per-NPC FaceGen is a visual degradation, not the end of the
    /// spawn job. Armor, animation/ragdoll targeting, AI, and descendant
    /// tagging still have to finalize on the already-loaded skeleton.
    pub(super) fn skip_missing_facegen(&mut self) -> UnitOutcome {
        self.phase = PrebakedPhase::Armor(0);
        UnitOutcome::Continue
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrebakedPhase {
    Skeleton,
    Facegen,
    Armor(usize),
    Finalize,
}

pub(super) struct PrebakedArmor {
    pub(super) form_id: u32,
    pub(super) model_path: String,
    pub(super) hidden_biped_mask: u32,
    pub(super) ownership: NpcEquipmentPart,
}


#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_prebaked_state(
    world: &mut World,
    npc: &NpcRecord,
    game: GameKind,
    plugin_name: &str,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
    player_body: bool,
) -> PrebakedNpcState {
    let (placement_root, resolved) =
        spawn_placement_root(world, npc, ref_pos, ref_rot, ref_scale, index, player_body);
    // #4092 (D5-01) — the "Use Traits" terminal off the spawn boundary's
    // one resolution (#4457).
    let traits = resolved.r#traits;
    let gender = Gender::from_acbs_flags(traits.acbs_flags);
    // P3 mid-life gear import — the body class the gear-mesh resolution
    // below uses, retained so a mid-life equip resolves the same meshes.
    world.insert(
        placement_root,
        crate::npc_spawn::ActorBodyClass {
            gender,
            race_form_id: traits.race_form_id,
        },
    );
    let skeleton_path = npc_skeleton_path(game, traits, index);
    let equip = build_npc_equip_state(&resolved, index, game, gender);
    let facegen_hidden_mask = equip.facegen_hidden_mask;
    let mut appearance = NpcLootAppearance::default();
    appearance.parts = equip
        .restore_skin_paths
        .iter()
        .map(|path| RestorePart::body(path))
        .collect();
    let armor = equip
        .armor_to_spawn
        .into_iter()
        .map(|armor| PrebakedArmor {
            ownership: NpcEquipmentPart {
                actor: placement_root,
                inventory_index: armor.inv_idx,
                form_id: armor.form_id,
                intrinsic_skin: armor.intrinsic_skin,
                hidden_biped_mask: armor.hidden_biped_mask,
            },
            form_id: armor.form_id,
            model_path: armor.model_path.to_owned(),
            hidden_biped_mask: armor.hidden_biped_mask,
        })
        .collect();
    // The player body does not insert these: the player entity already owns
    // its Inventory/EquipmentSlots/EquippedWeapon (stamped from the same
    // resolved `NPC_ 0x7` by `inventory::attach_to_player`), and its equip
    // indices reference the shared `ItemInstancePool` handles — overwriting
    // them with a second resolution's rows would orphan the player's real
    // instances.
    if !player_body {
        world.insert(placement_root, equip.inventory);
        let mut equipment_slots = equip.equipment_slots;
        // #3112 — same weapon-slot mirror as the resumable finalize path above.
        if let Some(weapon) = equip.equipped_weapon {
            equipment_slots.equip_weapon(weapon.inventory_index);
        }
        world.insert(placement_root, equipment_slots);
        if let Some(weapon) = equip.equipped_weapon {
            world.insert(placement_root, weapon);
        }
    }

    PrebakedNpcState {
        skeleton_path,
        appearance,
        placement_root,
        skel_root: None,
        skel_map: HashMap::new(),
        facegen_path: prebaked_facegen_nif_path(plugin_name, npc.form_id),
        tint_path: prebaked_facegen_tint_path(plugin_name, npc.form_id),
        armor,
        facegen_hidden_mask,
        equipped_armor_count: 0,
        combat_anim_draugr: is_draugr_race(index.races.get(&traits.race_form_id)),
        player_body,
        phase: PrebakedPhase::Skeleton,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_prebaked_unit(
    state: &mut PrebakedNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    index: &EsmIndex,
) -> UnitOutcome {
    match state.phase {
        PrebakedPhase::Skeleton => {
            let Some(skel_path) = state.skeleton_path.as_deref() else {
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
            // Same no-bone-collider contract as the runtime Skeleton phase:
            // the player's rays exclude only its capsule, so a player-body
            // skeleton must not register bone bodies beside it.
            let fallback_collider = if state.player_body {
                None
            } else {
                keyframe_live_ragdoll_bones(world, state.placement_root, &skel_map)
            };
            if let Some(root) = skel_root {
                parent_part(world, state.placement_root, root);
                if let Some(collider) = fallback_collider {
                    super::install_fallback_ragdoll_template(world, root, collider);
                }
            }
            state.skel_root = skel_root;
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
                return state.skip_missing_facegen();
            };
            let Some(data) = tex_provider.extract_mesh(facegen_path) else {
                log::debug!(
                    "NPC {:08X} ({}): pre-baked FaceGen '{}' not in archives — \
                     NPC visible as skeleton-only (no per-NPC mesh)",
                    npc.form_id,
                    npc.editor_id,
                    facegen_path,
                );
                return state.skip_missing_facegen();
            };
            let tint_path = state
                .tint_path
                .as_deref()
                .filter(|path| tex_provider.extract(path).is_some());
            // #3409 — the head is a multi-region mesh source exactly like the
            // race skin (partitions 130 head+beard / 131 hair / 143 ears /
            // 132 neck), so it needs the SAME pre-spawn hook the armour phase
            // below uses. Passing `None` here is why hair rendered through
            // every helmet: the mask, the biped→partition mapping and the
            // hook all existed and all worked; nothing wired them together.
            let hidden_biped_mask = state.facegen_hidden_mask;
            let mut hide_displaced_head = |scene: &mut byroredux_nif::import::ImportedScene| {
                hide_skin_partitions(scene, hidden_biped_mask);
            };
            let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
                (hidden_biped_mask != 0).then_some(&mut hide_displaced_head);
            let (_, root, _) = load_nif_bytes_with_skeleton(
                world,
                ctx,
                &data,
                facegen_path,
                tex_provider,
                mat_provider,
                Some(&state.skel_map),
                tint_path,
                pre_spawn,
            );
            if let Some(root) = root {
                parent_part(world, state.placement_root, root);
                if hidden_biped_mask != 0 {
                    state.appearance.original_roots.push(root);
                    state.appearance.parts.push(RestorePart {
                        path: facegen_path.to_owned(),
                        tint: tint_path.map(str::to_owned),
                    });
                }
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
                    let hidden_biped_mask = armor.hidden_biped_mask;
                    let mut hide_displaced_skin =
                        |scene: &mut byroredux_nif::import::ImportedScene| {
                            hide_skin_partitions(scene, hidden_biped_mask);
                        };
                    let pre_spawn: Option<
                        &mut dyn FnMut(&mut byroredux_nif::import::ImportedScene),
                    > = (hidden_biped_mask != 0).then_some(&mut hide_displaced_skin);
                    let (_, root, _) = load_nif_bytes_with_skeleton(
                        world,
                        ctx,
                        &data,
                        &armor.model_path,
                        tex_provider,
                        mat_provider,
                        Some(&state.skel_map),
                        None,
                        pre_spawn,
                    );
                    if let Some(root) = root {
                        parent_equipment_part(world, root, armor.ownership);
                        if !armor.ownership.intrinsic_skin || armor.hidden_biped_mask != 0 {
                            state.appearance.original_roots.push(root);
                        }
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
        PrebakedPhase::Finalize => finalize_prebaked(state, world, npc, index),
    }
}

/// The pre-baked path's finalize unit: animation targeting, the Draugr
/// combat marker, AI, actor tagging and loot appearance on the assembled
/// skeleton. Split from [`advance_prebaked_unit`] because it needs no GPU
/// context, so a test can drive it through the real spawn state (#4700).
pub(super) fn finalize_prebaked(
    state: &mut PrebakedNpcState,
    world: &mut World,
    npc: &NpcRecord,
    index: &EsmIndex,
) -> UnitOutcome {
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
    if let Some(skeleton_root) = state.skel_root {
        world.insert(
            state.placement_root,
            crate::components::AnimationTarget {
                skeleton_root,
                consumed_idle_serial: 0,
            },
        );
        // P3 mid-life gear import — retain the assembled bone map so an
        // equip of a never-worn item can import its mesh against the living
        // actor's skeleton (the corpse path retains its own copy at death).
        world.insert(
            state.placement_root,
            crate::npc_spawn::NpcSkeletonBones(state.skel_map.clone()),
        );
        // #4700 — the Draugr family marker, beside `AnimationTarget` as on
        // the runtime path. Without it every take and impact/death sound
        // `combat_feedback_system` gates on it stayed silent on Skyrim,
        // the only game with Draugr.
        if state.combat_anim_draugr && !state.player_body {
            world.insert(
                state.placement_root,
                crate::components::DraugrCombatAnim::default(),
            );
        }
        // M42.10/M42.11 — prebaked-path actors (Skyrim+) resolve
        // the same decoded HKX walk clip the runtime path uses,
        // with the same authored-stride speed derivation. The player
        // body takes neither from the job — `player_body::` attaches
        // its walk/idle playback beside the assembled root (see the
        // runtime Finalize twin).
        if !state.player_body {
            if let Some(handle) = world
                .try_resource::<crate::components::SkyrimWalkClip>()
                .and_then(|r| r.0)
            {
                let walk_speed = walk_speed_for(world, handle);
                let last_pos = world
                    .query::<Transform>()
                    .and_then(|q| q.get(state.placement_root).map(|t| t.translation))
                    .unwrap_or_default();
                world.insert(
                    state.placement_root,
                    crate::components::WalkAnimation {
                        walk_handle: handle,
                        walking: false,
                        last_pos,
                        captured: None,
                        transition_secs: 0.0,
                    },
                );
                world.insert(
                    state.placement_root,
                    crate::components::WalkSpeed(walk_speed),
                );
            }
        }
    }
    if !state.player_body {
        let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(npc, index);
        apply_ai_package_behavior(world, state.placement_root, &resolved, index);
        arm_combat_disposition(world, state.placement_root, &resolved);
    }
    // The caller restores eviction state after stamping the placed
    // ACHR identity, shared with the runtime-mesh spawn path above.
    tag_descendants_as_actor(world, state.placement_root);
    if !state.player_body {
        super::loot_appearance::install(
            world,
            state.placement_root,
            std::mem::take(&mut state.appearance),
            &state.skel_map,
        );
    }
    UnitOutcome::Complete(Some(state.placement_root))
}
