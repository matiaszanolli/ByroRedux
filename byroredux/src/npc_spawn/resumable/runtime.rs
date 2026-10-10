//! Runtime-FaceGen spawn arm (#5091 split of `resumable.rs`): the
//! skeleton → body → head-parts → armor state machine for games without
//! pre-baked character meshes, its head/hair/morph assembly, and the
//! creature shortcut that reuses the same machine.
use super::*;
use super::super::loot_appearance::NpcLootAppearance;

pub(super) struct RuntimeNpcState {
    pub(super) appearance: NpcLootAppearance,
    pub(super) placement_root: EntityId,
    pub(super) skel_root: Option<EntityId>,
    pub(super) skel_map: SkeletonMap,
    /// Skeleton NIF for this actor. Per-game canonical path for `NPC_`;
    /// the record's own MODL for `CREA`, whose skeleton is per-creature
    /// (#2567). A field rather than a `humanoid_skeleton_path(game)` call in
    /// the Skeleton phase precisely so the two can differ.
    pub(super) skeleton_path: String,
    /// Actor-specific idle clip, when the actor doesn't animate off the
    /// shared per-cell humanoid idle pool. `Some` for creatures — a rat's
    /// skeleton shares no bone names with the humanoid rig, so the pooled
    /// clip drives nothing (#2567).
    pub(super) idle_kf_path: Option<String>,
    /// M42.10 — the creature's own walk-forward clip, beside its skeleton
    /// (same per-directory convention as `idle_kf_path`). `None` for
    /// humanoids, which resolve the shared per-cell clip by path at
    /// finalize.
    pub(super) walk_kf_path: Option<String>,
    /// M42.11 — body class for the gendered humanoid walk-clip lookup at
    /// finalize (`humanoid_walk_kf_path`). Creatures carry their resolved
    /// ACBS gender (unused by the per-directory creature path).
    pub(super) gender: Gender,
    pub(super) is_child: bool,
    pub(super) body_paths: Vec<String>,
    /// #5487 — RACE body-section `ICON` skin per entry of `body_paths`
    /// (parallel, `None` = keep the NIF's own texture). Oblivion authors
    /// per-race/per-gender body skins there; the NIFs all ship the
    /// Imperial skin, so without the override every beast and mer torso
    /// renders human and the neck seam blends against the wrong tone.
    pub(super) body_textures: Vec<Option<String>>,
    pub(super) head_path: Option<String>,
    /// The race / gender head `ICON` (e.g. FO3 `Characters\Female\HeadHuman.dds`),
    /// replacing the head NIF's own (male default) base texture.
    pub(super) head_texture: Option<String>,
    pub(super) hair_path: Option<String>,
    /// Per-NPC HCLR colour in the renderer's normalized RGB convention.
    /// Classic hair textures are palettes rather than a final actor colour.
    pub(super) hair_tint: Option<[f32; 3]>,
    /// Fallout 3 / New Vegas loose hair meshes are authored at the actor
    /// origin and need the shared head-bone translation. Oblivion uses the
    /// same FaceGen record fields but its hair NIFs already carry their own
    /// actor-space placement; mounting those below `Bip01 Head` applies the
    /// head's rotated basis a second time.
    pub(super) head_parts_use_head_bone_mount: bool,
    /// Fallout 3 / New Vegas hair is lit by the `HairTint` lighting-shader
    /// variants, whose vertex colour is a tint mask rather than a colour
    /// (see [`fallout_hair_tint_factor`]).
    pub(super) hair_uses_fallout_tint_mask: bool,
    pub(super) brow_path: Option<String>,
    pub(super) eye_paths: Vec<String>,
    /// Mouth / teeth / tongue (and, on Oblivion, ears) — the head
    /// sub-meshes that are *not* eyes, so they take no eye texture
    /// override. #3420.
    pub(super) head_sub_paths: Vec<String>,
    pub(super) eye_texture_override: Option<String>,
    pub(super) inventory: Option<Inventory>,
    pub(super) equipment_slots: Option<EquipmentSlots>,
    pub(super) equipped_weapon: Option<EquippedWeapon>,
    pub(super) armor: Vec<RuntimeArmor>,
    pub(super) equipped_armor_count: u32,
    /// Blend the head / hand cuts into the neighbouring skin at spawn
    /// (Oblivion / FO3 / FNV runtime-assembled actors — see `seam_blend`).
    pub(super) blend_skin_seams: bool,
    /// Built once after the skeleton phase when `blend_skin_seams`.
    pub(super) seam_context: Option<std::sync::Arc<super::seam_blend::SeamContext>>,
    pub(super) phase: RuntimePhase,
    /// P2 combat tail — this actor's race resolves to the Draugr family,
    /// so finalize inserts `DraugrCombatAnim` and the combat-feedback
    /// system plays the attack/hit/death takes on it
    /// (`docs/engine/p2-combat-anim-sound-fixture.md`). Keyed on the
    /// RACE editor id (`DraugrRace*`), the same discriminator the body
    /// meshes follow; humans never set it.
    pub(super) combat_anim_draugr: bool,
    /// Player-body attach (`NpcSpawnJob::player_body`) — carried so the
    /// finalize unit can skip the player entity's already-owned state.
    pub(super) player_body: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimePhase {
    Skeleton,
    Body(usize),
    Head,
    Hair,
    Brow,
    Eye(usize),
    HeadSubPart(usize),
    Armor(usize),
    Finalize,
}

pub(super) struct RuntimeArmor {
    pub(super) model_path: String,
    pub(super) resolved_fid: u32,
    pub(super) source_fid: u32,
    pub(super) hidden_biped_mask: u32,
    pub(super) ownership: NpcEquipmentPart,
}

/// Every RACE head-part mesh matching one of `roles`, in role order,
/// filtered to the section this actor's gender may wear.
///
/// The `section.is_none_or(...)` rule is the one the eye selector has
/// used since #3037: an untagged entry is shared (that is how Oblivion
/// authors its whole head section), a tagged one only applies to its
/// own gender. Roles a game does not author simply contribute nothing.
pub(super) fn head_part_paths(
    race: &RaceRecord,
    game: GameKind,
    roles: &[head_part::Role],
    want_gender_tag: u8,
) -> Vec<String> {
    head_part_entries(&race.head_parts, game, roles, want_gender_tag)
}

/// The race / gender base texture (`ICON`) for head-part `role`.
pub(super) fn head_part_texture(
    race: &RaceRecord,
    game: GameKind,
    role: head_part::Role,
    want_gender_tag: u8,
) -> Option<String> {
    head_part_entries(&race.head_part_textures, game, &[role], want_gender_tag)
        .into_iter()
        .next()
}

/// Shared filter behind [`head_part_paths`] / [`head_part_texture`]: the
/// entries of an `(INDX, path, gender section)` list matching `roles`, in
/// role order.
pub(super) fn head_part_entries(
    entries: &[(u32, String, Option<u8>)],
    game: GameKind,
    roles: &[head_part::Role],
    want_gender_tag: u8,
) -> Vec<String> {
    roles
        .iter()
        .filter_map(|role| head_part::index_of(game, *role))
        .flat_map(|want_idx| {
            entries
                .iter()
                .filter(move |(part_idx, path, section)| {
                    *part_idx == want_idx
                        && !path.is_empty()
                        && section.is_none_or(|tag| tag == want_gender_tag)
                })
                .map(|(_, path, _)| path.clone())
        })
        .collect()
}

/// The single RACE head-part mesh for `role`, for this actor's gender.
pub(super) fn head_part_path(
    race: &RaceRecord,
    game: GameKind,
    role: head_part::Role,
    want_gender_tag: u8,
) -> Option<String> {
    head_part_paths(race, game, &[role], want_gender_tag)
        .into_iter()
        .next()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_runtime_state(
    world: &mut World,
    npc: &NpcRecord,
    race: Option<&RaceRecord>,
    game: GameKind,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
    player_body: bool,
) -> RuntimeNpcState {
    let (placement_root, resolved) =
        spawn_placement_root(world, npc, ref_pos, ref_rot, ref_scale, index, player_body);
    log::info!(
        "NPC {:08X} ({}) spawning at world [{:.0},{:.0},{:.0}] scale={:.2}",
        npc.form_id,
        npc.editor_id,
        ref_pos.x,
        ref_pos.y,
        ref_pos.z,
        ref_scale,
    );

    // #2567 (OBL-D3-01) — creatures reuse this whole state machine (skeleton
    // → parts → armor → finalize) but fill it from a different source. Their
    // MODL *is* the skeleton and their meshes come from NIFZ beside it, so
    // none of the humanoid recipe below (canonical body paths, RACE head,
    // hair / brow / eye head-parts) applies: a `CREA` references no RACE at
    // all — Oblivion's `CREA` `RNAM` is a 1-byte attack reach, not a FormID.
    // Return early with the creature shape rather than threading `if
    // is_creature` through every lookup.
    if npc.is_creature {
        return prepare_creature_state(world, npc, game, index, placement_root, &resolved);
    }

    let gender = Gender::from_acbs_flags(npc.acbs_flags);
    // P3 mid-life gear import — the body class the gear-mesh resolution
    // below uses, retained so a mid-life equip resolves the same meshes.
    world.insert(
        placement_root,
        crate::npc_spawn::ActorBodyClass {
            gender,
            race_form_id: resolved.r#traits.race_form_id,
        },
    );
    // #4092 (D5-01) — the "Use Traits" terminal off the spawn boundary's
    // one resolution (#4457), the same source `stamp_character_components`'s
    // `Background` uses.
    let equip = build_npc_equip_state(&resolved, index, game, gender);
    let mut appearance = NpcLootAppearance::default();
    appearance.parts = equip
        .restore_skin_paths
        .iter()
        .map(|path| RestorePart::body(path))
        .collect();
    let is_child = crate::npc_spawn::is_child_race(game, race.map(|race| race.race_flags));
    let want_gender_tag = match gender {
        Gender::Male => 0,
        Gender::Female => 1,
    };
    // #5487 — pair each mesh with the race's body-section ICON skin for
    // its slot (Oblivion only; FO3/FNV body skins ride the head table).
    let body_skin = |path: &str| -> Option<String> {
        let icon_idx = crate::npc_spawn::humanoid_body_path_icon_index(game, path)?;
        race.and_then(|race| {
            race.body_part_textures
                .iter()
                .find(|(idx, texture, section)| {
                    *idx == icon_idx
                        && !texture.is_empty()
                        && section.is_none_or(|tag| tag == want_gender_tag)
                })
                .map(|(_, texture, _)| texture.clone())
        })
        .filter(|texture| !texture.is_empty())
    };
    let (body_paths, body_textures): (Vec<String>, Vec<Option<String>>) =
        humanoid_body_paths(game, gender, is_child)
            .iter()
            .map(|path| ((*path).to_owned(), body_skin(path)))
            .filter(|(path, _)| {
                let body_piece_mask = humanoid_body_path_biped_mask(game, path);
                let covered = if path.ends_with("upperbody.nif") {
                    equip.main_body_covered(game)
                } else {
                    equip.covers_biped_mask(body_piece_mask)
                };
                let keep = !covered;
                if !keep {
                    appearance.parts.push(RestorePart::body(path));
                    log::info!(
                        "NPC {:08X} ({}): equipped armor covers body mask {body_piece_mask:#06X} — skipping {}",
                        npc.form_id,
                        npc.editor_id,
                        path,
                    );
                }
                keep
            })
            .unzip();

    // #3418 — the head is the `head_part::Role::Head` entry of the RACE
    // head section, picked for this actor's gender. It used to be
    // `body_models.first()`, an append-ordered list of *every* `MODL`
    // in the record: on FNV that is always the male head, because the
    // record opens `NAM0` → `MNAM` → `INDX 0` → `MODL`. All 22 vanilla
    // FNV races author a distinct female head in the `FNAM` half, so
    // every one of the 987 female NPCs got the male base mesh — and
    // then had its female-authored FGGS/FGGA morph deltas applied to
    // it. `body_models.first()` stays as the fallback for records with
    // no head-part table (Skyrim+, or a malformed head section).
    let head_path = race
        .and_then(|race| {
            head_part_path(race, game, head_part::Role::Head, want_gender_tag)
                .or_else(|| race.body_models.first().cloned())
        })
        .map(|path| normalize_mesh_path(&path).into_owned());
    let head_texture = race
        .and_then(|race| head_part_texture(race, game, head_part::Role::Head, want_gender_tag))
        .filter(|path| !path.is_empty());
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
    let hair_tint = recipe
        .and_then(|recipe| recipe.hair_color_rgb)
        .map(|color| color.map(|channel| channel as f32 / 255.0));
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
    // The eye indices are per-game: 6/7 on FO3 / FNV but 7/8 on
    // Oblivion, where hard-coding the Fallout pair selected the tongue
    // and the left eye — and then painted the eye texture over both
    // (#3420).
    let eye_paths = if recipe.is_some() {
        race.map(|race| {
            head_part_paths(
                race,
                game,
                &[head_part::Role::LeftEye, head_part::Role::RightEye],
                want_gender_tag,
            )
        })
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    // #3420 — mouth, both teeth rows and the tongue are separate NIFs
    // in every Oblivion / FO3 / FNV race; the head mesh models the lips
    // but not the oral cavity, so without these the jaw opens onto a
    // hole. Ears are a mesh on Oblivion and an `ICON`-only slot on
    // FNV, and their gender split is by index rather than by section,
    // so they are resolved separately.
    let head_sub_paths = if recipe.is_some() {
        race.map(|race| {
            let ear_role = match gender {
                Gender::Male => head_part::Role::EarMale,
                Gender::Female => head_part::Role::EarFemale,
            };
            let mut roles = head_part::ORAL_ROLES.to_vec();
            roles.push(ear_role);
            head_part_paths(race, game, &roles, want_gender_tag)
        })
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    let armor = equip
        .armor_to_spawn
        .into_iter()
        .map(|armor| RuntimeArmor {
            ownership: NpcEquipmentPart {
                actor: placement_root,
                form_id: armor.form_id,
                intrinsic_skin: armor.intrinsic_skin,
                hidden_biped_mask: armor.hidden_biped_mask,
            },
            model_path: armor.model_path.to_owned(),
            resolved_fid: armor.form_id,
            source_fid: armor.source_form_id,
            hidden_biped_mask: armor.hidden_biped_mask,
        })
        .collect();

    RuntimeNpcState {
        appearance,
        placement_root,
        skel_root: None,
        skel_map: HashMap::new(),
        gender,
        is_child,
        skeleton_path: humanoid_skeleton_path(game).unwrap_or_default().to_owned(),
        idle_kf_path: None,
        walk_kf_path: None,
        body_paths,
        body_textures,
        head_path,
        head_texture,
        hair_path,
        hair_tint,
        head_parts_use_head_bone_mount: head_parts_use_head_bone_mount(game),
        hair_uses_fallout_tint_mask: matches!(game, GameKind::Fallout3NV),
        brow_path,
        eye_paths,
        head_sub_paths,
        eye_texture_override,
        inventory: Some(equip.inventory),
        equipment_slots: Some(equip.equipment_slots),
        equipped_weapon: equip.equipped_weapon,
        armor,
        equipped_armor_count: 0,
        blend_skin_seams: matches!(game, GameKind::Oblivion | GameKind::Fallout3NV),
        seam_context: None,
        combat_anim_draugr: is_draugr_race(race),
        player_body,
        phase: RuntimePhase::Skeleton,
    }
}

/// The `CREA` counterpart of [`prepare_runtime_state`]'s humanoid recipe
/// (#2567). Everything a creature needs is authored in one directory keyed
/// off its MODL — skeleton, `NIFZ` part meshes, and `idle.kf` — so this is a
/// path derivation plus the shared inventory build, and the same
/// [`RuntimePhase`] machine runs it from there.
///
/// Head / hair / brow / eye stay `None`: those are head-*part* records
/// reached through RACE, and a creature has no RACE. Its face, where it has
/// one, is simply another `NIFZ` entry (`Head.NIF` beside `Rat.NIF`).
pub(super) fn prepare_creature_state(
    world: &mut World,
    npc: &NpcRecord,
    game: GameKind,
    index: &EsmIndex,
    placement_root: EntityId,
    resolved: &byroredux_plugin::equip::ResolvedNpc<'_>,
) -> RuntimeNpcState {
    let (skeleton_path, dir) = creature_skeleton_and_dir(&npc.model_path).unwrap_or_default();
    let body_paths = creature_body_paths(&dir, &npc.body_part_models);
    if skeleton_path.is_empty() {
        log::debug!(
            "Creature {:08X} ({}): no MODL — cannot locate a skeleton, spawning identity only",
            npc.form_id,
            npc.editor_id,
        );
    } else if body_paths.is_empty() {
        // The skeleton NIF carries no geometry of its own, so a creature
        // with no NIFZ is invisible. Worth a line: it means either an
        // unparsed sub-record or genuinely mesh-less content.
        log::debug!(
            "Creature {:08X} ({}): skeleton '{}' has no NIFZ body parts — no visible mesh",
            npc.form_id,
            npc.editor_id,
            skeleton_path,
        );
    }
    let idle_kf_path = (!dir.is_empty()).then(|| creature_idle_kf_path(&dir));
    let walk_kf_path = (!dir.is_empty()).then(|| creature_walk_kf_path(&dir));

    let gender = Gender::from_acbs_flags(npc.acbs_flags);
    // #4092 (D5-01) — the "Use Traits" terminal off the spawn boundary's
    // one resolution, handed in by `prepare_runtime_state` (#4457); a
    // no-op for creatures in practice (`CREA` references no `RACE`) but
    // keeps this call site correct without an is_creature special case.
    let equip = build_npc_equip_state(resolved, index, game, gender);
    let armor = equip
        .armor_to_spawn
        .into_iter()
        .map(|armor| RuntimeArmor {
            ownership: NpcEquipmentPart {
                actor: placement_root,
                form_id: armor.form_id,
                intrinsic_skin: armor.intrinsic_skin,
                hidden_biped_mask: armor.hidden_biped_mask,
            },
            model_path: armor.model_path.to_owned(),
            resolved_fid: armor.form_id,
            source_fid: armor.source_form_id,
            hidden_biped_mask: armor.hidden_biped_mask,
        })
        .collect();

    let _ = world;
    RuntimeNpcState {
        appearance: NpcLootAppearance::default(),
        placement_root,
        skel_root: None,
        skel_map: HashMap::new(),
        skeleton_path,
        idle_kf_path,
        walk_kf_path,
        gender,
        // Creatures have no child race split (the FO3/FNV RACE flag gate is
        // humanoid-only; `is_child` is computed against the resolved race).
        is_child: false,
        // Creatures take no body-section ICON override; compute before
        // the `body_paths` move below.
        body_textures: vec![None; body_paths.len()],
        body_paths,
        head_path: None,
        head_texture: None,
        hair_path: None,
        hair_tint: None,
        head_parts_use_head_bone_mount: false,
        hair_uses_fallout_tint_mask: false,
        brow_path: None,
        eye_paths: Vec::new(),
        head_sub_paths: Vec::new(),
        eye_texture_override: None,
        inventory: Some(equip.inventory),
        equipment_slots: Some(equip.equipment_slots),
        equipped_weapon: equip.equipped_weapon,
        armor,
        equipped_armor_count: 0,
        blend_skin_seams: false,
        seam_context: None,
        // CREA creatures don't use the humanoid Draugr clip family.
        combat_anim_draugr: false,
        // A creature cannot be the player's body (NPC_ 0x7 is never CREA).
        player_body: false,
        phase: RuntimePhase::Skeleton,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_runtime_unit(
    state: &mut RuntimeNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    // #2567 — `game` used to select the skeleton path here; that moved onto
    // `RuntimeNpcState::skeleton_path` at prepare time so creatures can carry
    // their own, and nothing else in this function needed it.
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    idle_pool: &[u32],
    index: &EsmIndex,
) -> UnitOutcome {
    match state.phase {
        RuntimePhase::Skeleton => {
            // #2567 — was `humanoid_skeleton_path(game)`; now whatever the
            // prepare step resolved, so a creature loads its own per-species
            // skeleton instead of the humanoid rig.
            if state.skeleton_path.is_empty() {
                return UnitOutcome::Complete(None);
            }
            let skel_path = state.skeleton_path.clone();
            let skel_path = skel_path.as_str();
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
                mat_provider.as_deref_mut(),
                None,
                None,
                None,
            );
            // Player bodies get no bone colliders: the bone bodies are
            // separate Rapier bodies mapped to the actor through
            // `ActorColliderOwner`, and every interaction/occlusion/combat
            // ray excludes exactly one body — the player's capsule. Keyframed
            // bone bodies beside it would intercept those rays and make the
            // player target/occlude against itself. The post-spawn pass in
            // `crate::player_body` strips any bhk-derived collision
            // components the skeleton import created.
            let fallback_collider = if state.player_body {
                None
            } else {
                keyframe_live_ragdoll_bones(world, state.placement_root, &skel_map)
            };
            if let Some(root) = skel_root {
                parent_part(world, state.placement_root, root);
                if let Some(fallback) = fallback_collider {
                    super::install_fallback_ragdoll_template(world, root, fallback);
                }
            } else {
                log::debug!(
                    "NPC {:08X}: skeleton '{}' produced no root entity",
                    npc.form_id,
                    skel_path,
                );
            }
            state.skel_root = skel_root;
            state.skel_map = skel_map;
            if state.blend_skin_seams && state.skel_root.is_some() {
                state.seam_context = Some(std::sync::Arc::new(build_seam_context(
                    state,
                    world,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                )));
            }
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
                    // #5487 — the race's body-section ICON replaces the
                    // NIF's own (Imperial-default) base texture. Hands
                    // additionally carry the wrist cut; they are the only
                    // body part blended on a private copy of the cached
                    // import. The torso and legs stay shared and untouched
                    // apart from the texture swap, which happens at spawn
                    // time, not in the shared cache.
                    let body_texture = state.body_textures.get(index).cloned().flatten();
                    let body_texture_id = body_texture.as_ref().map(|texture| {
                        let mut pool = world.resource_mut::<StringPool>();
                        pool.intern(texture)
                    });
                    let hand_seams = state
                        .seam_context
                        .clone()
                        .filter(|_| is_hand_part(body_path));
                    let mut tone_sampler = super::seam_blend::ToneSampler::new(world, tex_provider);
                    let mut blend_hand = |scene: &mut byroredux_nif::import::ImportedScene| {
                        if let Some(texture) = body_texture_id {
                            for mesh in &mut scene.meshes {
                                mesh.material.textures.base_color = Some(texture);
                            }
                        }
                        let Some(context) = hand_seams.as_deref() else {
                            return;
                        };
                        let source = body_path.to_ascii_lowercase();
                        for (index, mesh) in scene.meshes.iter_mut().enumerate() {
                            let own_texture = body_texture
                                .clone()
                                .or_else(|| context.textures.get(&(source.clone(), index)).cloned());
                            let stats = super::seam_blend::blend_part_seams(
                                mesh,
                                &source,
                                own_texture.as_deref(),
                                context,
                                &mut |texture, uv| tone_sampler.sample(texture, uv),
                            );
                            log::debug!(
                                "NPC {:08X}: '{}' seam blend matched {} edge vertices, toned {}",
                                npc.form_id,
                                source,
                                stats.matched_vertices,
                                stats.toned_vertices,
                            );
                        }
                    };
                    let pre_spawn: Option<
                        &mut dyn FnMut(&mut byroredux_nif::import::ImportedScene),
                    > = if body_texture.is_some()
                        || (state.seam_context.is_some() && is_hand_part(body_path))
                    {
                        Some(&mut blend_hand)
                    } else {
                        None
                    };
                    let (_, body_root, _) = load_nif_bytes_with_skeleton(
                        world,
                        ctx,
                        &body_data,
                        body_path,
                        tex_provider,
                        mat_provider,
                        Some(&state.skel_map),
                        None,
                        pre_spawn,
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
                    let tint = state.hair_tint;
                    let fallout_mask = state.hair_uses_fallout_tint_mask;
                    let mut apply_hair_tint = |scene: &mut byroredux_nif::import::ImportedScene| {
                        if fallout_mask {
                            bake_fallout_hair_tint(&mut scene.meshes, tint);
                        } else if let Some(tint) = tint {
                            for mesh in &mut scene.meshes {
                                for (channel, tint_channel) in
                                    mesh.material.diffuse_color.iter_mut().zip(tint)
                                {
                                    *channel *= tint_channel;
                                }
                            }
                        }
                    };
                    spawn_shared_skeleton_part(
                        state,
                        world,
                        ctx,
                        npc,
                        path,
                        "hair",
                        tex_provider,
                        mat_provider,
                        (fallout_mask || tint.is_some()).then_some(&mut apply_hair_tint),
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
                state.phase = RuntimePhase::HeadSubPart(0);
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
                RuntimePhase::HeadSubPart(0)
            };
            UnitOutcome::Continue
        }
        // Mouth / teeth / tongue / ears. Same mount as the eyes — one
        // shared-skeleton part each — but deliberately outside the eye
        // loop: the `ENAM` eye-texture override belongs to the eyes
        // alone, and painting it over the tongue is exactly the
        // Oblivion misfire #3420 fixed on the selector side.
        RuntimePhase::HeadSubPart(index) => {
            let Some(path) = state.head_sub_paths.get(index).cloned() else {
                state.phase = RuntimePhase::Armor(0);
                return UnitOutcome::Continue;
            };
            if state.skel_root.is_some() {
                spawn_shared_skeleton_part(
                    state,
                    world,
                    ctx,
                    npc,
                    &path,
                    "head sub-part",
                    tex_provider,
                    mat_provider,
                    None,
                );
            }
            let next = index + 1;
            state.phase = if next < state.head_sub_paths.len() {
                RuntimePhase::HeadSubPart(next)
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
            // Player bodies keep the player entity's own inventory state
            // (see `prepare_prebaked_state`'s mirrored note): only the mesh
            // assembly and `NpcEquipmentPart` ownership stamps are wanted.
            if !state.player_body {
                world.insert(
                    state.placement_root,
                    state.inventory.take().unwrap_or_default(),
                );
                let mut equipment_slots = state.equipment_slots.take().unwrap_or_default();
                let weapon = state.equipped_weapon.take();
                // #3112 — mirror the wielded weapon into `EquipmentSlots::weapon`
                // so "is this equipped?" consumers (CTDA `GetEquipped`) see it.
                // The equip pipeline only fills `equipped_weapon`; the weapon slot
                // is deliberately outside the biped occupancy array, so nothing
                // else writes it.
                if let Some(weapon) = weapon {
                    equipment_slots.equip_weapon(weapon.inventory_index);
                }
                world.insert(state.placement_root, equipment_slots);
                if let Some(weapon) = weapon {
                    world.insert(state.placement_root, weapon);
                }
            }
            if let Some(skeleton) = state.skel_root {
                world.insert(
                    state.placement_root,
                    crate::components::AnimationTarget {
                        skeleton_root: skeleton,
                        consumed_idle_serial: 0,
                    },
                );
                // P3 mid-life gear import — retain the assembled bone map so
                // an equip of a never-worn item can import its mesh against
                // the living actor's skeleton (the corpse path retains its
                // own copy at death).
                world.insert(
                    state.placement_root,
                    crate::npc_spawn::NpcSkeletonBones(state.skel_map.clone()),
                );
                // P2 combat tail — Draugr-race actors play the Draugr
                // combat clip family (attack/hit/death takes) through
                // `systems::combat_anim`; presence of this component is
                // the family marker. Inserted only with a skeleton —
                // without one there is nothing for a take to animate
                // (`docs/engine/p2-combat-anim-sound-fixture.md`).
                if state.combat_anim_draugr && !state.player_body {
                    world.insert(
                        state.placement_root,
                        crate::components::DraugrCombatAnim::default(),
                    );
                }
                // #2567 — a creature animates off its own `idle.kf`, beside
                // its skeleton. The shared per-cell pool holds the humanoid
                // clip, whose bone names a creature rig doesn't have, so
                // falling back to it would play nothing. Load-on-finalize
                // (rather than at prepare) because this is where the
                // `TextureProvider` is in hand.
                let idle_handle = if state.player_body {
                    // The job attaches no idle pool for the player body —
                    // `player_body::attach_player_locomotion_animation`
                    // resolves the idle itself (and first-person keeps the
                    // body's meshes hidden either way).
                    None
                } else if let Some(path) = state.idle_kf_path.as_deref() {
                    let handle = load_kf_clip_by_path(world, tex_provider, path);
                    if handle.is_none() {
                        log::debug!(
                            "Creature {:08X} ({}): no idle clip at '{}' — spawning unanimated",
                            npc.form_id,
                            npc.editor_id,
                            path,
                        );
                    }
                    handle
                } else {
                    pick_idle_handle(idle_pool, npc.form_id)
                };
                if let Some(handle) = idle_handle {
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
            // M42.10/M42.11 — resolve the walk clip: creatures use their
            // own directory clip, KF-game humanoids the per-body-class clip
            // (`load_references` already registered every variant — a path
            // lookup, no I/O), and Skyrim+ the decoded HKX walk staged into
            // `SkyrimWalkClip` by `populate_skyrim_walk_clip`. Playback is
            // left to `npc_walk_animation_system`, which swaps it in only
            // while the actor is actually moving. The clip's authored
            // stride (accum-root travel per loop) becomes the actor's
            // `WalkSpeed`, so the step length matches the animation.
            let walk_handle = if state.player_body {
                // No `WalkAnimation`/`WalkSpeed` from the job: the capsule
                // controller drives movement, and the walk system watches
                // the entity it is attached to — which must be the capsule,
                // not this body root. `player_body::` attaches both beside
                // the assembled root after the job completes.
                None
            } else if let Some(path) = state.walk_kf_path.as_deref() {
                load_kf_clip_by_path(world, tex_provider, path)
            } else if index.game.has_kf_animations() {
                crate::npc_spawn::humanoid_walk_kf_path(index.game, state.gender, state.is_child)
                    .and_then(|path| world.resource::<AnimationClipRegistry>().get_by_path(path))
            } else {
                world
                    .try_resource::<crate::components::SkyrimWalkClip>()
                    .and_then(|r| r.0)
            };
            if let Some(handle) = walk_handle {
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
                world.insert(state.placement_root, crate::components::WalkSpeed(walk_speed));
            }
            if !state.player_body {
                let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(npc, index);
                apply_ai_package_behavior(world, state.placement_root, &resolved, index);
                arm_combat_disposition(world, state.placement_root, &resolved);
            }
            // Eviction state restores in stamp_quest_reference after the caller
            // assigns the placed ACHR identity. npc.form_id is only the shared
            // base record and cannot identify a particular actor's snapshot.
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
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_runtime_head(
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
    let mut egm_file =
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
    // The EGM deltas are in Gamebyro's Z-up frame, but the importer converts
    // every NiTriShape vertex to Y-up (`zup_point_to_yup`) before this hook
    // sees it. Adding them raw applied forward/back deltas as up/down — most
    // visibly across the nose, the most heavily morphed region.
    if let Some(egm) = egm_file.as_mut() {
        yup_egm_morphs(&mut egm.fggs_morphs);
        yup_egm_morphs(&mut egm.fgga_morphs);
    }
    let mut hook_state = match (recipe, egm_file.as_ref()) {
        (Some(recipe), Some(egm)) => Some((egm, recipe.fggs, recipe.fgga, npc.form_id)),
        _ => None,
    };
    // The race / gender head texture replaces the head NIF's own base
    // texture, which is the male default: FO3 / FNV female heads otherwise
    // rendered with male skin against a female body, the worst of the neck
    // seams.
    let head_texture = state.head_texture.as_ref().map(|texture| {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern(texture)
    });
    // Seam blending runs after the morph, on the head as it will render.
    let seam_context = state.seam_context.clone();
    let own_head_texture = state.head_texture.clone();
    let mut tone_sampler = super::seam_blend::ToneSampler::new(world, tex_provider);
    let has_hook = hook_state.is_some() || head_texture.is_some() || seam_context.is_some();
    let mut hook = |scene: &mut byroredux_nif::import::ImportedScene| {
        if let Some(texture) = head_texture {
            for mesh in &mut scene.meshes {
                mesh.material.textures.base_color = Some(texture);
            }
        }
        apply_head_morphs(scene, hook_state.take());
        if let Some(context) = seam_context.as_deref() {
            for mesh in &mut scene.meshes {
                let stats = super::seam_blend::blend_part_seams(
                    mesh,
                    head_path,
                    own_head_texture.as_deref(),
                    context,
                    &mut |texture, uv| tone_sampler.sample(texture, uv),
                );
                log::debug!(
                    "NPC {:08X}: head seam blend matched {} edge vertices, toned {}",
                    npc.form_id,
                    stats.matched_vertices,
                    stats.toned_vertices,
                );
            }
        }
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

/// Apply the NPC's FaceGen FGGS / FGGA morphs to every head mesh.
pub(super) fn apply_head_morphs(
    scene: &mut byroredux_nif::import::ImportedScene,
    morphs: Option<(
        &byroredux_facegen::EgmFile,
        [f32; 50],
        [f32; 30],
        u32,
    )>,
) {
    let Some((egm, fggs, fgga, form_id)) = morphs else {
        return;
    };
    let mut deformed_meshes = 0;
    for mesh in &mut scene.meshes {
        if mesh.positions.is_empty() {
            continue;
        }
        let after_sym = byroredux_facegen::apply_morphs(&mesh.positions, &egm.fggs_morphs, &fggs);
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
}

/// Whether a body-part NIF is a hand (`lefthand.nif`, `femalerighthand.nif`,
/// …), whose wrist cut the seam pass blends.
pub(super) fn is_hand_part(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|file| file.to_ascii_lowercase().contains("hand"))
}

/// Resolve the shared humanoid head node used by the classic runtime-hair
/// assets. `Bip01 Head` is the vanilla FO3/FNV spelling; the alternate name
/// keeps the mount compatible with later Creation skeleton conventions.
pub(super) fn shared_head_mount(
    skeleton: &std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) -> Option<EntityId> {
    ["Bip01 Head", "NPC Head [Head]"]
        .into_iter()
        .find_map(|name| crate::name_lookup::get_case_insensitive(skeleton, name).copied())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_shared_skeleton_part(
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
        // Fallout 3 / New Vegas head parts — hair, brows, eyes, mouth,
        // teeth, tongue — are unskinned meshes whose vertices sit around the
        // head pivot in the *actor's* axes (measured: `hairmessy02.nif`
        // X/Z ±8, Y 0..16; `eyelefthuman.nif` 6-8 above the pivot, in front).
        // They mount beneath the shared head bone, which supplies the head
        // position and follows head animation. Oblivion head parts carry
        // their own actor-space placement and stay on the placement root, as
        // do skinned parts (body, head), whose palettes already resolve
        // against the shared skeleton.
        let head = (state.head_parts_use_head_bone_mount
            && HEAD_MOUNTED_PART_LABELS.contains(&label)
            && !subtree_has_skinned_mesh(world, root))
        .then(|| shared_head_mount(&state.skel_map))
        .flatten();
        match head {
            Some(head) => {
                // `Bip01 Head`'s bind basis is rotated 90° (its local X points
                // up). Eye / mouth / teeth roots author the inverse of that
                // basis themselves; male hair and brow roots are identity;
                // female / child / ghoul hair authors a 180° root and the
                // same 180° on every shape (net identity, #5157). The vertices
                // are in the actor's axes in all of them, so the root is set
                // to the inverse head bind rotation (plus the inverse of any
                // shape rotation that cancels against the authored root),
                // which keeps every part in the actor's axes at bind pose
                // while still inheriting head animation.
                if let Some(bind) = bind_transform_relative_to(world, head, state.placement_root) {
                    align_part_root_to_actor_axes(world, root, bind);
                }
                parent_part(world, head, root);
            }
            None => parent_part(world, state.placement_root, root),
        }
    }
}

/// Resolve everything the head / hand seam passes need while the world is
/// still reachable: the skeleton's bind transforms (relative to the
/// placement root, read before any animation attaches) and the skin meshes
/// of every body and armour NIF this actor will wear, with their resolved
/// diffuse textures. Neighbour scenes come from the shared import cache, or
/// a fully material-resolved cache fill, so spawn order —
/// the outfit loads after the head — does not matter.
pub(super) fn build_seam_context(
    state: &RuntimeNpcState,
    world: &mut World,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
) -> super::seam_blend::SeamContext {
    let mut context = super::seam_blend::SeamContext::default();
    for (name, &bone) in &state.skel_map {
        if let Some(bind) = bind_transform_relative_to(world, bone, state.placement_root) {
            context.bone_binds.insert(
                name.to_ascii_lowercase(),
                byroredux_core::math::Mat4::from_scale_rotation_translation(
                    Vec3::splat(bind.scale),
                    bind.rotation,
                    bind.translation,
                ),
            );
        }
    }
    // #5487 — body meshes enter with the race's body-section ICON as
    // their spawn-time base texture, so the neighbour lookup must see
    // that skin, not the NIF's own Imperial default (the head seam's
    // tone ratio is computed against these textures).
    let owned = state
        .body_paths
        .iter()
        .zip(state.body_textures.iter())
        .map(|(path, texture)| (path.to_owned(), texture.clone()));
    let sources = owned
        .chain(state.armor.iter().map(|armor| {
            (armor.model_path.to_owned(), None::<String>)
        }));
    for (path, override_texture) in sources {
        let Some(scene) = crate::scene::peek_or_parse_scene(world, &path, tex_provider, mat_provider.as_deref_mut()) else {
            continue;
        };
        let source = path.to_ascii_lowercase();
        let pool = world.resource::<StringPool>();
        for (index, mesh) in scene.meshes.iter().enumerate() {
            let Some(texture) = override_texture.clone().or_else(|| {
                mesh.material
                    .textures
                    .base_color
                    .and_then(|symbol| pool.resolve(symbol))
                    .map(str::to_owned)
            })
            else {
                continue;
            };
            if let Some(neighbor) =
                super::seam_blend::neighbor_from_mesh(mesh, &source, &texture, &context.bone_binds)
            {
                context.neighbors.push(neighbor);
            }
            context.textures.insert((source.clone(), index), texture);
        }
    }
    log::debug!(
        "seam context: {} of {} skeleton bones bound, {} skin neighbour mesh(es): {:?}",
        context.bone_binds.len(),
        state.skel_map.len(),
        context.neighbors.len(),
        context
            .neighbors
            .iter()
            .map(|n| (
                n.source.rsplit('\\').next().unwrap_or(""),
                n.positions.len()
            ))
            .collect::<Vec<_>>(),
    );
    context
}

/// `spawn_shared_skeleton_part` labels for the unskinned FO3 / FNV head parts
/// that mount beneath the shared head bone.
const HEAD_MOUNTED_PART_LABELS: [&str; 4] = ["hair", "eyebrow HDPT", "eye mesh", "head sub-part"];

/// Whether any entity in `root`'s subtree carries a skinned mesh.
pub(super) fn subtree_has_skinned_mesh(world: &World, root: EntityId) -> bool {
    let (Some(skinned), children) = (
        world.query::<byroredux_core::ecs::SkinnedMesh>(),
        world.query::<byroredux_core::ecs::Children>(),
    ) else {
        return false;
    };
    let mut guard =
        byroredux_core::ecs::HierarchyTraversalGuard::new(world.next_entity_id() as usize, 0);
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if !guard.step() {
            break;
        }
        if skinned.get(entity).is_some() {
            return true;
        }
        if let Some(kids) = children.as_ref().and_then(|q| q.get(entity)) {
            stack.extend(kids.0.iter().copied());
        }
    }
    false
}

/// `entity`'s bind-pose transform expressed in `ancestor`'s space, composed
/// from the local `Transform`s along the `Parent` chain. `None` if `ancestor`
/// is not reached or a link has no `Transform`. Read during spawn, before the
/// actor's animation player attaches, so the locals are still the skeleton
/// NIF's rest pose.
pub(super) fn bind_transform_relative_to(
    world: &World,
    entity: EntityId,
    ancestor: EntityId,
) -> Option<Transform> {
    let transforms = world.query::<Transform>()?;
    let parents = world.query::<Parent>()?;
    let mut guard =
        byroredux_core::ecs::HierarchyTraversalGuard::new(world.next_entity_id() as usize, 0);
    let mut relative = *transforms.get(entity)?;
    let mut cursor = parents.get(entity)?.0;
    while cursor != ancestor {
        if !guard.step() {
            return None;
        }
        let parent = transforms.get(cursor)?;
        relative = Transform::new(
            parent.translation + parent.rotation * (relative.translation * parent.scale),
            parent.rotation * relative.rotation,
            parent.scale * relative.scale,
        );
        cursor = parents.get(cursor)?.0;
    }
    Some(relative)
}

/// Rotation (relative to `part_root`) that every renderable shape in its
/// subtree authors, or `None` when there is no shape below the root or the
/// shapes disagree.
fn common_shape_rotation(world: &World, part_root: EntityId) -> Option<Quat> {
    let meshes = world.query::<byroredux_core::ecs::MeshHandle>()?;
    let children = world.query::<byroredux_core::ecs::Children>()?;
    let mut guard =
        byroredux_core::ecs::HierarchyTraversalGuard::new(world.next_entity_id() as usize, 0);
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![part_root];
    let mut common: Option<Quat> = None;
    while let Some(entity) = stack.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if !guard.step() {
            return None;
        }
        if meshes.get(entity).is_some() {
            let rotation = bind_transform_relative_to(world, entity, part_root)?.rotation;
            match common {
                Some(first) if first.angle_between(rotation) > SHAPE_ROTATION_EPSILON => {
                    return None;
                }
                Some(_) => {}
                None => common = Some(rotation),
            }
        }
        if let Some(kids) = children.get(entity) {
            stack.extend(kids.0.iter().copied());
        }
    }
    common
}

/// Angular tolerance (radians, ≈0.06°) for treating two authored rotations
/// as equal when deciding whether a part's shape rotation cancels its root's.
const SHAPE_ROTATION_EPSILON: f32 = 1.0e-3;

/// Set `part_root`'s rotation to the inverse of `bind`'s and divide out its
/// scale, so a part authored in the placement root's axes and parented under
/// a bone whose bind transform is `bind` keeps those axes at bind pose. The
/// authored root rotation is replaced, not composed: on FO3 / FNV head parts
/// it is either identity (male hair, brows) or already this same inverse
/// (eyes, mouth, teeth), so composing would double-cancel the latter. The
/// authored translation is kept as an offset in the placement root's axes.
///
/// One more authoring shape needs the shapes' own rotation: 36 of the 67
/// FNV hair NIFs (female, child, ghoul) author a 180° root and the same 180°
/// on every shape, so root × shape is identity and the vertices are in the
/// actor's axes. Replacing the root alone would leave the shapes' 180° in
/// place and swap up with forward (#5157). When the shapes share one rotation
/// that cancels the authored root's, it is divided out too. A shape rotation
/// that does NOT cancel the root (an intentional tilt) is left untouched.
pub(super) fn align_part_root_to_actor_axes(world: &mut World, part_root: EntityId, bind: Transform) {
    let inverse_rotation = bind.rotation.inverse();
    let inverse_scale = if bind.scale.abs() > f32::EPSILON {
        1.0 / bind.scale
    } else {
        1.0
    };
    let cancelled_shape = world
        .get::<Transform>(part_root)
        .map(|root| root.rotation)
        .zip(common_shape_rotation(world, part_root))
        .and_then(|(root, shape)| {
            let is_identity = |q: Quat| q.angle_between(Quat::IDENTITY) <= SHAPE_ROTATION_EPSILON;
            (!is_identity(shape) && is_identity(root * shape)).then_some(shape)
        });
    let root_rotation = cancelled_shape.map_or(inverse_rotation, |shape| inverse_rotation * shape.inverse());
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        if let Some(local) = transforms.get_mut(part_root) {
            *local = Transform::new(
                inverse_rotation * (local.translation * inverse_scale),
                root_rotation,
                inverse_scale * local.scale,
            );
        }
    }
}

/// The FO3 / FNV `HairTint` lighting-shader tint factor for one vertex.
///
/// Decoded from the shipped pixel shaders (`shaderpackage003.sdp`
/// `SM3002.pso` and its `HairTint` siblings, identical in both games):
///
/// ```text
/// add r1.xyz, r0.x(-0.5), c2(HairTint)   ; HairTint - 0.5
/// mad r1.xyz, v0.y, r1, 0.5              ; lerp(0.5, HairTint, vertexColor.g)
/// add r1.xyz, r1, r1                     ; x2
/// mul r1.xyz, r1, layered_diffuse
/// ```
///
/// So the vertex colour's green channel masks the tint, 0.5 is neutral and
/// the result is an x2 overlay — the other vertex-colour channels are not a
/// colour at all (the vanilla hair meshes author R 0.3-0.55, G = B = 1).
pub(super) fn fallout_hair_tint_factor(hair_tint: [f32; 3], mask: f32) -> [f32; 3] {
    hair_tint.map(|channel| 2.0 * (0.5 + mask * (channel - 0.5)))
}

/// Replace each Fallout hair mesh's vertex colours (a tint mask the lit
/// path would otherwise multiply into the albedo, turning hair teal) with
/// the per-vertex [`fallout_hair_tint_factor`], which the lit path's
/// `albedo *= vertexColor` then applies exactly. A mesh without vertex
/// colours reads as an all-white mask, as a missing colour stream does in
/// Gamebryo. Without an authored HCLR the tint is the neutral 0.5 — an
/// engine choice, not a measured FO3 default — which leaves the texture as
/// authored.
pub(super) fn bake_fallout_hair_tint(
    meshes: &mut [byroredux_nif::import::ImportedMesh],
    hair_tint: Option<[f32; 3]>,
) {
    let tint = hair_tint.unwrap_or([0.5; 3]);
    for mesh in meshes {
        if mesh.colors.len() != mesh.positions.len() {
            mesh.colors = vec![[1.0; 4]; mesh.positions.len()];
        }
        for color in &mut mesh.colors {
            let [r, g, b] = fallout_hair_tint_factor(tint, color[1]);
            *color = [r, g, b, color[3]];
        }
    }
}

/// Convert parsed EGM morphs from Gamebyro's Z-up frame into the importer's
/// Y-up vertex frame, with the same mapping the NIF importer applies to the
/// base vertices they deform.
pub(super) fn yup_egm_morphs(morphs: &mut [byroredux_facegen::EgmMorph]) {
    for morph in morphs {
        for delta in &mut morph.deltas {
            *delta = byroredux_core::math::coord::zup_to_yup_pos(*delta);
        }
    }
}

pub(super) fn head_parts_use_head_bone_mount(game: GameKind) -> bool {
    matches!(game, GameKind::Fallout3NV)
}
