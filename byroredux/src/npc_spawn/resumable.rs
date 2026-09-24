//! Cooperative NPC assembly.
//!
//! A placed actor is a bundle of independently loadable NIFs.  Keeping that
//! bundle inside one REFR-sized operation made a single runtime-FaceGen actor
//! the largest remaining EXAL apply outlier.  This module makes one top-level
//! actor part (skeleton, body piece, head part, or armor piece) the cooperative
//! work unit while preserving the synchronous public spawn functions through
//! an unlimited-budget driver.

use super::loot_appearance::{NpcLootAppearance, RestorePart};
use super::*;
use crate::cell_loader::FrameTimeBudget;
use byroredux_plugin::esm::records::actor::head_part;
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

// Runtime/Prebaked carry their whole scratch state by value; boxing them
// would add an indirection to every spawn-phase advance for no memory win
// (one spawner instance per in-flight NPC).
#[allow(clippy::large_enum_variant)]
enum NpcSpawnState {
    BeginRuntime,
    BeginPrebaked { plugin_name: String },
    Runtime(Box<RuntimeNpcState>),
    Prebaked(PrebakedNpcState),
    Done,
}

struct RuntimeNpcState {
    appearance: NpcLootAppearance,
    placement_root: EntityId,
    skel_root: Option<EntityId>,
    skel_map: SkeletonMap,
    /// Skeleton NIF for this actor. Per-game canonical path for `NPC_`;
    /// the record's own MODL for `CREA`, whose skeleton is per-creature
    /// (#2567). A field rather than a `humanoid_skeleton_path(game)` call in
    /// the Skeleton phase precisely so the two can differ.
    skeleton_path: String,
    /// Actor-specific idle clip, when the actor doesn't animate off the
    /// shared per-cell humanoid idle pool. `Some` for creatures — a rat's
    /// skeleton shares no bone names with the humanoid rig, so the pooled
    /// clip drives nothing (#2567).
    idle_kf_path: Option<String>,
    /// M42.10 — the creature's own walk-forward clip, beside its skeleton
    /// (same per-directory convention as `idle_kf_path`). `None` for
    /// humanoids, which resolve the shared per-cell clip by path at
    /// finalize.
    walk_kf_path: Option<String>,
    /// M42.11 — body class for the gendered humanoid walk-clip lookup at
    /// finalize (`humanoid_walk_kf_path`). Creatures carry their resolved
    /// ACBS gender (unused by the per-directory creature path).
    gender: Gender,
    is_child: bool,
    body_paths: Vec<String>,
    head_path: Option<String>,
    /// The race / gender head `ICON` (e.g. FO3 `Characters\Female\HeadHuman.dds`),
    /// replacing the head NIF's own (male default) base texture.
    head_texture: Option<String>,
    hair_path: Option<String>,
    /// Per-NPC HCLR colour in the renderer's normalized RGB convention.
    /// Classic hair textures are palettes rather than a final actor colour.
    hair_tint: Option<[f32; 3]>,
    /// Fallout 3 / New Vegas loose hair meshes are authored at the actor
    /// origin and need the shared head-bone translation. Oblivion uses the
    /// same FaceGen record fields but its hair NIFs already carry their own
    /// actor-space placement; mounting those below `Bip01 Head` applies the
    /// head's rotated basis a second time.
    head_parts_use_head_bone_mount: bool,
    /// Fallout 3 / New Vegas hair is lit by the `HairTint` lighting-shader
    /// variants, whose vertex colour is a tint mask rather than a colour
    /// (see [`fallout_hair_tint_factor`]).
    hair_uses_fallout_tint_mask: bool,
    brow_path: Option<String>,
    eye_paths: Vec<String>,
    /// Mouth / teeth / tongue (and, on Oblivion, ears) — the head
    /// sub-meshes that are *not* eyes, so they take no eye texture
    /// override. #3420.
    head_sub_paths: Vec<String>,
    eye_texture_override: Option<String>,
    inventory: Option<Inventory>,
    equipment_slots: Option<EquipmentSlots>,
    equipped_weapon: Option<EquippedWeapon>,
    armor: Vec<RuntimeArmor>,
    equipped_armor_count: u32,
    /// Blend the head / hand cuts into the neighbouring skin at spawn
    /// (Oblivion / FO3 / FNV runtime-assembled actors — see `seam_blend`).
    blend_skin_seams: bool,
    /// Built once after the skeleton phase when `blend_skin_seams`.
    seam_context: Option<std::sync::Arc<super::seam_blend::SeamContext>>,
    phase: RuntimePhase,
    /// P2 combat tail — this actor's race resolves to the Draugr family,
    /// so finalize inserts `DraugrCombatAnim` and the combat-feedback
    /// system plays the attack/hit/death takes on it
    /// (`docs/engine/p2-combat-anim-sound-fixture.md`). Keyed on the
    /// RACE editor id (`DraugrRace*`), the same discriminator the body
    /// meshes follow; humans never set it.
    combat_anim_draugr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePhase {
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

struct RuntimeArmor {
    model_path: String,
    resolved_fid: u32,
    source_fid: u32,
    hidden_biped_mask: u32,
    ownership: NpcEquipmentPart,
}

struct PrebakedNpcState {
    appearance: NpcLootAppearance,
    placement_root: EntityId,
    skel_root: Option<EntityId>,
    skel_map: SkeletonMap,
    skeleton_path: Option<String>,
    facegen_path: Option<String>,
    tint_path: Option<String>,
    armor: Vec<PrebakedArmor>,
    /// #3409 — biped bits an equipped item took from the FaceGen head, in
    /// `hide_skin_partitions` format. See `NpcEquipState::facegen_hidden_mask`.
    facegen_hidden_mask: u32,
    equipped_armor_count: u32,
    /// #4700 — [`RuntimeNpcState::combat_anim_draugr`]'s pre-baked twin,
    /// from the same `Use Traits`-resolved race.
    combat_anim_draugr: bool,
    phase: PrebakedPhase,
}

impl PrebakedNpcState {
    /// Missing per-NPC FaceGen is a visual degradation, not the end of the
    /// spawn job. Armor, animation/ragdoll targeting, AI, and descendant
    /// tagging still have to finalize on the already-loaded skeleton.
    fn skip_missing_facegen(&mut self) -> UnitOutcome {
        self.phase = PrebakedPhase::Armor(0);
        UnitOutcome::Continue
    }
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
    hidden_biped_mask: u32,
    ownership: NpcEquipmentPart,
}

enum UnitOutcome {
    Continue,
    Complete(Option<EntityId>),
}

impl NpcSpawnJob {
    /// Begin a kf-era spawn (Oblivion / FO3 / FNV) — M41.0 Phase 1b.
    /// `advance` walks skeleton + body + head-part + armor NIFs one at a
    /// time under a caller-supplied [`crate::cell_loader::FrameTimeBudget`],
    /// yielding [`NpcSpawnProgress::Pending`] when the budget runs out and
    /// [`NpcSpawnProgress::Complete`] with the placement-root `EntityId`
    /// once the whole actor is assembled. An unlimited budget drives it to
    /// completion in one call — the shape a synchronous caller wants;
    /// exterior streaming instead retains the job across frames and resumes
    /// it with a fresh per-frame budget.
    ///
    /// `CellRoot` ownership stays outside this API: synchronous callers
    /// stamp the final entity range themselves, while EXAL stamps every
    /// yielded range before returning to the render loop.
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

    /// Begin a pre-baked-FaceGen spawn (Skyrim / FO4 / FO76 / Starfield) —
    /// M41.0 Phase 4. Same [`Self::advance`] budget/yield contract as
    /// [`Self::runtime`].
    ///
    /// Pre-baked path: `meshes\actors\character\facegendata\facegeom\
    /// <plugin>\<formid:08x>.nif` carries the per-NPC **head only**
    /// (matching Bethesda's FaceGen SDK head-only bake convention — a real
    /// vanilla FaceGeom NIF has no torso/limb geometry; see #2093 /
    /// SKY-D3-NEW-01) — no FaceGen morph evaluator (the SDK pre-applies the
    /// slider table before shipping). Body coverage comes from
    /// `RACE.WNAM`'s default skin ARMO (equipped as the lowest-priority
    /// layer in [`super::build_npc_equip_state`]) plus whatever OTFT/CNTO
    /// armor resolves on top of it. Skeleton load + skinning resolution
    /// stays identical to the kf-era path; the head NIF replaces the
    /// race-default head only.
    ///
    /// Skyrim+ vanilla ships zero `.kf` files. Pre-baked-track NPCs expose
    /// their skeleton root as a Havok animation target; supported IDLE
    /// events are resolved from `.hkx` by the archive-backed runtime.
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
                RuntimePhase::HeadSubPart(_) => "head sub-part",
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

/// Every RACE head-part mesh matching one of `roles`, in role order,
/// filtered to the section this actor's gender may wear.
///
/// The `section.is_none_or(...)` rule is the one the eye selector has
/// used since #3037: an untagged entry is shared (that is how Oblivion
/// authors its whole head section), a tagged one only applies to its
/// own gender. Roles a game does not author simply contribute nothing.
fn head_part_paths(
    race: &RaceRecord,
    game: GameKind,
    roles: &[head_part::Role],
    want_gender_tag: u8,
) -> Vec<String> {
    head_part_entries(&race.head_parts, game, roles, want_gender_tag)
}

/// The race / gender base texture (`ICON`) for head-part `role`.
fn head_part_texture(
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
fn head_part_entries(
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
fn head_part_path(
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
    let (placement_root, resolved) = spawn_placement_root(world, npc, ref_pos, ref_rot, ref_scale, index);
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
    // FO3/FNV RACE DATA bit 2 is the authored Child flag. Oblivion reuses
    // that bit for BeastRace, so the game gate is part of the translation.
    let is_child = matches!(game, GameKind::Fallout3NV)
        && race.is_some_and(|race| race.race_flags & 0x04 != 0);
    let body_paths = humanoid_body_paths(game, gender, is_child)
        .iter()
        .filter(|path| {
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
        .map(|path| (*path).to_owned())
        .collect();

    let want_gender_tag = match gender {
        Gender::Male => 0,
        Gender::Female => 1,
    };
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
                inventory_index: armor.inv_idx,
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
        phase: RuntimePhase::Skeleton,
    }
}

/// P2 combat tail — the race editor id is the family discriminator
/// (`DraugrRace…`); humans never match, so only draugr get combat takes.
/// Case-insensitive because editor ids are authoring text. Shared by both
/// spawn paths (#4700): Draugr are Skyrim-only, so the pre-baked path is
/// the one that actually meets them.
fn is_draugr_race(race: Option<&RaceRecord>) -> bool {
    race.is_some_and(|race| race.editor_id.to_ascii_lowercase().contains("draugr"))
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
fn prepare_creature_state(
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
                inventory_index: armor.inv_idx,
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
        phase: RuntimePhase::Skeleton,
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_runtime_unit(
    state: &mut RuntimeNpcState,
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    // #2567 — `game` used to select the skeleton path here; that moved onto
    // `RuntimeNpcState::skeleton_path` at prepare time so creatures can carry
    // their own, and nothing else in this function needed it.
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
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
                mat_provider,
                None,
                None,
                None,
            );
            let fallback_collider =
                keyframe_live_ragdoll_bones(world, state.placement_root, &skel_map);
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
                    // Hands carry the wrist cut; they are the only body part
                    // blended (per NPC, so this bypasses the import cache for
                    // them). The torso / legs stay shared and untouched.
                    let hand_seams = state
                        .seam_context
                        .clone()
                        .filter(|_| is_hand_part(body_path));
                    let mut tone_sampler = super::seam_blend::ToneSampler::new(tex_provider);
                    let mut blend_hand = |scene: &mut byroredux_nif::import::ImportedScene| {
                        let Some(context) = hand_seams.as_deref() else {
                            return;
                        };
                        let source = body_path.to_ascii_lowercase();
                        for (index, mesh) in scene.meshes.iter_mut().enumerate() {
                            let own_texture =
                                context.textures.get(&(source.clone(), index)).cloned();
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
                    > = if state.seam_context.is_some() && is_hand_part(body_path) {
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
            if let Some(skeleton) = state.skel_root {
                world.insert(
                    state.placement_root,
                    crate::components::AnimationTarget {
                        skeleton_root: skeleton,
                        consumed_idle_serial: 0,
                    },
                );
                // P2 combat tail — Draugr-race actors play the Draugr
                // combat clip family (attack/hit/death takes) through
                // `systems::combat_anim`; presence of this component is
                // the family marker. Inserted only with a skeleton —
                // without one there is nothing for a take to animate
                // (`docs/engine/p2-combat-anim-sound-fixture.md`).
                if state.combat_anim_draugr {
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
                let idle_handle = match state.idle_kf_path.as_deref() {
                    Some(path) => {
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
                    }
                    None => pick_idle_handle(idle_pool, npc.form_id),
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
            let walk_handle = if let Some(path) = state.walk_kf_path.as_deref() {
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
            apply_ai_package_behavior(
                world,
                state.placement_root,
                &byroredux_plugin::equip::ResolvedNpc::resolve(npc, index),
                index,
            );
            // Eviction state restores in stamp_quest_reference after the caller
            // assigns the placed ACHR identity. npc.form_id is only the shared
            // base record and cannot identify a particular actor's snapshot.
            tag_descendants_as_actor(world, state.placement_root);
            super::loot_appearance::install(
                world,
                state.placement_root,
                std::mem::take(&mut state.appearance),
                &state.skel_map,
            );
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
    // The EGM deltas are in Gamebyro's Z-up frame, but the importer converts
    // every NiTriShape vertex to Y-up (`zup_point_to_yup`) before this hook
    // sees it. Adding them raw applied forward/back deltas as up/down — most
    // visibly across the nose, the most heavily morphed region.
    let yup_morphs = egm_file.as_ref().map(|egm| {
        (
            yup_egm_morphs(&egm.fggs_morphs),
            yup_egm_morphs(&egm.fgga_morphs),
        )
    });
    let mut hook_state = match (recipe, egm_file.as_ref(), yup_morphs.as_ref()) {
        (Some(recipe), Some(egm), Some(morphs)) => {
            Some((egm, morphs, recipe.fggs, recipe.fgga, npc.form_id))
        }
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
    let mut tone_sampler = super::seam_blend::ToneSampler::new(tex_provider);
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
fn apply_head_morphs(
    scene: &mut byroredux_nif::import::ImportedScene,
    morphs: Option<(
        &byroredux_facegen::EgmFile,
        &(
            Vec<byroredux_facegen::EgmMorph>,
            Vec<byroredux_facegen::EgmMorph>,
        ),
        [f32; 50],
        [f32; 30],
        u32,
    )>,
) {
    let Some((egm, (fggs_morphs, fgga_morphs), fggs, fgga, form_id)) = morphs else {
        return;
    };
    let mut deformed_meshes = 0;
    for mesh in &mut scene.meshes {
        if mesh.positions.is_empty() {
            continue;
        }
        let after_sym = byroredux_facegen::apply_morphs(&mesh.positions, fggs_morphs, &fggs);
        mesh.positions = byroredux_facegen::apply_morphs(&after_sym, fgga_morphs, &fgga);
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
fn is_hand_part(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|file| file.to_ascii_lowercase().contains("hand"))
}

/// Resolve the shared humanoid head node used by the classic runtime-hair
/// assets. `Bip01 Head` is the vanilla FO3/FNV spelling; the alternate name
/// keeps the mount compatible with later Creation skeleton conventions.
fn shared_head_mount(
    skeleton: &std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) -> Option<EntityId> {
    ["Bip01 Head", "NPC Head [Head]"]
        .into_iter()
        .find_map(|name| crate::name_lookup::get_case_insensitive(skeleton, name).copied())
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
                // basis themselves; hair and brow roots are identity. Setting
                // the root to the inverse head bind rotation serves both, so
                // every part keeps the actor's axes at bind pose while still
                // inheriting head animation.
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
/// a parse that is not inserted (`peek_or_parse_scene`), so spawn order —
/// the outfit loads after the head — does not matter.
fn build_seam_context(
    state: &RuntimeNpcState,
    world: &mut World,
    tex_provider: &TextureProvider,
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
    let sources = state
        .body_paths
        .iter()
        .chain(state.armor.iter().map(|armor| &armor.model_path));
    for path in sources {
        let Some(scene) = crate::scene::peek_or_parse_scene(world, path, tex_provider) else {
            continue;
        };
        let source = path.to_ascii_lowercase();
        let pool = world.resource::<StringPool>();
        for (index, mesh) in scene.meshes.iter().enumerate() {
            let Some(texture) = mesh
                .material
                .textures
                .base_color
                .and_then(|symbol| pool.resolve(symbol))
                .map(str::to_owned)
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
fn subtree_has_skinned_mesh(world: &World, root: EntityId) -> bool {
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
fn bind_transform_relative_to(
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

/// Set `part_root`'s rotation to the inverse of `bind`'s and divide out its
/// scale, so a part authored in the placement root's axes and parented under
/// a bone whose bind transform is `bind` keeps those axes at bind pose. The
/// authored root rotation is replaced, not composed: on FO3 / FNV head parts
/// it is either identity (hair, brows) or already this same inverse (eyes,
/// mouth, teeth), so composing would double-cancel the latter. The authored
/// translation is kept as an offset in the placement root's axes.
fn align_part_root_to_actor_axes(world: &mut World, part_root: EntityId, bind: Transform) {
    let inverse_rotation = bind.rotation.inverse();
    let inverse_scale = if bind.scale.abs() > f32::EPSILON {
        1.0 / bind.scale
    } else {
        1.0
    };
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        if let Some(local) = transforms.get_mut(part_root) {
            *local = Transform::new(
                inverse_rotation * (local.translation * inverse_scale),
                inverse_rotation,
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
fn fallout_hair_tint_factor(hair_tint: [f32; 3], mask: f32) -> [f32; 3] {
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
fn bake_fallout_hair_tint(
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
fn yup_egm_morphs(morphs: &[byroredux_facegen::EgmMorph]) -> Vec<byroredux_facegen::EgmMorph> {
    morphs
        .iter()
        .map(|morph| byroredux_facegen::EgmMorph {
            scale: morph.scale,
            deltas: morph
                .deltas
                .iter()
                .map(|&delta| byroredux_core::math::coord::zup_to_yup_pos(delta))
                .collect(),
        })
        .collect()
}

fn head_parts_use_head_bone_mount(game: GameKind) -> bool {
    matches!(game, GameKind::Fallout3NV)
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
    let (placement_root, resolved) = spawn_placement_root(world, npc, ref_pos, ref_rot, ref_scale, index);
    // #4092 (D5-01) — the "Use Traits" terminal off the spawn boundary's
    // one resolution (#4457).
    let traits = resolved.r#traits;
    let gender = Gender::from_acbs_flags(traits.acbs_flags);
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
        phase: PrebakedPhase::Skeleton,
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_prebaked_unit(
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
            let fallback_collider =
                keyframe_live_ragdoll_bones(world, state.placement_root, &skel_map);
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
fn finalize_prebaked(
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
        // #4700 — the Draugr family marker, beside `AnimationTarget` as on
        // the runtime path. Without it every take and impact/death sound
        // `combat_feedback_system` gates on it stayed silent on Skyrim,
        // the only game with Draugr.
        if state.combat_anim_draugr {
            world.insert(
                state.placement_root,
                crate::components::DraugrCombatAnim::default(),
            );
        }
        // M42.10/M42.11 — prebaked-path actors (Skyrim+) resolve
        // the same decoded HKX walk clip the runtime path uses,
        // with the same authored-stride speed derivation.
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
    apply_ai_package_behavior(
        world,
        state.placement_root,
        &byroredux_plugin::equip::ResolvedNpc::resolve(npc, index),
        index,
    );
    // The caller restores eviction state after stamping the placed
    // ACHR identity, shared with the runtime-mesh spawn path above.
    tag_descendants_as_actor(world, state.placement_root);
    super::loot_appearance::install(
        world,
        state.placement_root,
        std::mem::take(&mut state.appearance),
        &state.skel_map,
    );
    UnitOutcome::Complete(Some(state.placement_root))
}

/// Apply an equip-time biped mask before the scene builder uploads meshes.
/// Unsupported geometry paths simply report no triangle/body-part association
/// and remain unchanged, preserving a safe visual fallback for formats that
/// carry no classic dismember or FO4-family sub-index segmentation metadata.
fn hide_skin_partitions(scene: &mut byroredux_nif::import::ImportedScene, hidden_biped_mask: u32) {
    let removed: usize = scene
        .meshes
        .iter_mut()
        .map(|mesh| mesh.hide_skin_partitions(hidden_biped_mask))
        .sum();
    if removed > 0 {
        log::debug!(
            "NPC skin equip mask {hidden_biped_mask:#010X}: hid {removed} displaced triangle(s)"
        );
    }
}

/// #4457 — the population boundary. Resolves every TPLT template category
/// ONCE (`ResolvedNpc::resolve`) and hands the result to each stamp, so a
/// stamp cannot read the shell's raw fields by accident and a new consumer
/// must go through the resolved type. Returns the placement root with the
/// resolved records so the caller's equip/AI work reuses the same view.
fn spawn_placement_root<'a>(
    world: &mut World,
    npc: &'a NpcRecord,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &'a EsmIndex,
) -> (EntityId, byroredux_plugin::equip::ResolvedNpc<'a>) {
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
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(npc, index);
    stamp_faction_ranks(world, placement_root, &resolved);
    stamp_actor_values(world, placement_root, &resolved, index);
    stamp_creature_attack(world, placement_root, &resolved);
    stamp_character_components(world, placement_root, &resolved);
    (placement_root, resolved)
}

fn parent_equipment_part(world: &mut World, part_root: EntityId, ownership: NpcEquipmentPart) {
    parent_part(world, ownership.actor, part_root);
    world.insert(part_root, ownership);
}

pub(super) fn parent_part(world: &mut World, placement_root: EntityId, part_root: EntityId) {
    world.insert(part_root, Parent(placement_root));
    add_child(world, placement_root, part_root);
    // A yielded actor can render for several frames before finalization.
    // Keep each newly attached mesh on the actor layer immediately rather
    // than exposing an Architecture-layer transient. Tag from `part_root`,
    // not `placement_root` (#2276 / PERF-D7-02): every previously attached
    // part was already tagged by its own `parent_part` call, so re-walking
    // the whole growing subtree from the actor root on each of the ~11-16
    // attaches per NPC just re-tags entities that are already correct.
    // `part_root`'s own subtree (just loaded this tick, never tagged) is
    // the only part that's actually new.
    tag_descendants_as_actor(world, part_root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_head_mount_resolves_classic_head_case_insensitively() {
        let mut world = World::new();
        let head = world.spawn();
        let skeleton =
            std::collections::HashMap::from([(std::sync::Arc::<str>::from("bIp01 hEaD"), head)]);
        assert_eq!(shared_head_mount(&skeleton), Some(head));
    }

    /// Regression: FO3/FNV hair was parented under `Bip01 Head` with the
    /// bone's full bind basis, which the vanilla skeleton rotates 90° about
    /// the vertical axis (measured on FO3 `skeleton.nif`: head at Y 112.8,
    /// 90° about +Y in engine space). Hair meshes are authored in the
    /// actor's axes around the head pivot, so the hair rendered rotated 90°
    /// and dropped onto the neck. The mount must cancel the bind rotation.
    #[test]
    fn head_mounted_hair_keeps_actor_axes_at_the_head_pivot() {
        use byroredux_core::ecs::{Children, GlobalTransform};
        use byroredux_core::math::{Quat, Vec3};
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<Children>();

        let placement = world.spawn();
        let placement_rotation = Quat::from_rotation_y(0.6);
        world.insert(
            placement,
            Transform::new(Vec3::new(500.0, 20.0, -300.0), placement_rotation, 1.0),
        );
        let spine = world.spawn();
        world.insert(
            spine,
            Transform::new(Vec3::new(0.0, 90.0, 0.0), Quat::from_rotation_z(0.4), 1.0),
        );
        world.insert(spine, Parent(placement));
        add_child(&mut world, placement, spine);
        let head = world.spawn();
        world.insert(
            head,
            Transform::new(
                Vec3::new(0.0, 22.8, 0.0),
                Quat::from_rotation_z(-0.4) * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                1.0,
            ),
        );
        world.insert(head, Parent(spine));
        add_child(&mut world, spine, head);
        let hair = world.spawn();
        world.insert(hair, Transform::IDENTITY);

        let bind =
            bind_transform_relative_to(&world, head, placement).expect("head reaches placement");
        align_part_root_to_actor_axes(&mut world, hair, bind);
        world.insert(hair, Parent(head));
        add_child(&mut world, head, hair);
        for entity in [placement, spine, head, hair] {
            world.insert(entity, GlobalTransform::IDENTITY);
        }
        byroredux_core::ecs::make_transform_propagation_system()(&world, 0.0);

        let placement_global = *world.get::<GlobalTransform>(placement).unwrap();
        let head_global = *world.get::<GlobalTransform>(head).unwrap();
        let hair_global = *world.get::<GlobalTransform>(hair).unwrap();
        assert!(
            hair_global
                .rotation
                .angle_between(placement_global.rotation)
                < 1.0e-4,
            "hair must keep the actor's axes, not the head bone's rotated basis"
        );
        assert!((hair_global.translation - head_global.translation).length() < 1.0e-3);
        // Without the counter-rotation the hair inherits the 90° bind basis.
        assert!(
            head_global
                .rotation
                .angle_between(placement_global.rotation)
                > 1.0
        );

        // Eyes / mouth / teeth author the inverse head basis on their own
        // root. Replacing (not composing) keeps them in the actor's axes too.
        let eye = world.spawn();
        world.insert(
            eye,
            Transform::new(Vec3::ZERO, bind.rotation.inverse(), 1.0),
        );
        world.insert(eye, GlobalTransform::IDENTITY);
        align_part_root_to_actor_axes(&mut world, eye, bind);
        world.insert(eye, Parent(head));
        add_child(&mut world, head, eye);
        byroredux_core::ecs::make_transform_propagation_system()(&world, 0.0);
        let eye_global = *world.get::<GlobalTransform>(eye).unwrap();
        assert!(
            eye_global.rotation.angle_between(placement_global.rotation) < 1.0e-4,
            "a root that already cancels the head basis must not be cancelled twice"
        );
    }

    /// Regression: FO3 / FNV female heads rendered with the head NIF's male
    /// default skin against a female body. The race `ICON` for the head role
    /// is selected per gender; Oblivion's untagged entry serves both.
    #[test]
    fn head_texture_follows_the_race_icon_for_the_actors_gender() {
        let fallout = RaceRecord {
            head_part_textures: vec![
                (0, "Characters\\Male\\HeadHuman.dds".into(), Some(0)),
                (1, "Characters\\Head\\EarsHuman.dds".into(), Some(0)),
                (0, "Characters\\Female\\HeadHuman.dds".into(), Some(1)),
            ],
            ..Default::default()
        };
        let head = |race: &RaceRecord, game, tag| {
            head_part_texture(race, game, head_part::Role::Head, tag)
        };
        assert_eq!(
            head(&fallout, GameKind::Fallout3NV, 1).as_deref(),
            Some("Characters\\Female\\HeadHuman.dds")
        );
        assert_eq!(
            head(&fallout, GameKind::Fallout3NV, 0).as_deref(),
            Some("Characters\\Male\\HeadHuman.dds")
        );
        let oblivion = RaceRecord {
            head_part_textures: vec![(0, "Characters\\Imperial\\HeadHuman.dds".into(), None)],
            ..Default::default()
        };
        for tag in [0, 1] {
            assert_eq!(
                head(&oblivion, GameKind::Oblivion, tag).as_deref(),
                Some("Characters\\Imperial\\HeadHuman.dds")
            );
        }
    }

    /// The FO3 / FNV `HairTint` shader formula (`2 * lerp(0.5, tint, mask)`).
    #[test]
    fn fallout_hair_tint_is_a_masked_x2_overlay_around_neutral_grey() {
        // Neutral HairTint leaves the texture as authored.
        assert_eq!(fallout_hair_tint_factor([0.5; 3], 1.0), [1.0; 3]);
        // A masked-out vertex ignores the tint entirely.
        assert_eq!(fallout_hair_tint_factor([0.9, 0.1, 0.3], 0.0), [1.0; 3]);
        // MegatonMoriartysCustomer01's HCLR (7, 6, 5): twice the plain
        // multiply the old path applied.
        let tint = [7.0 / 255.0, 6.0 / 255.0, 5.0 / 255.0];
        let factor = fallout_hair_tint_factor(tint, 1.0);
        for (f, t) in factor.iter().zip(tint) {
            assert!((f - 2.0 * t).abs() < 1.0e-6);
        }
    }

    /// Regression: FO3 hair rendered teal. Its vertex colours are a tint mask
    /// (measured on `hairmessy02.nif`: R 0.30-0.55, G = B = 1), which the lit
    /// path multiplied into the albedo as a colour. The bake replaces them
    /// with the shader's per-vertex tint factor, which is grey for a grey
    /// HCLR regardless of the mask's other channels.
    #[test]
    fn fallout_hair_bake_replaces_the_mask_colours_with_the_tint_factor() {
        use byroredux_nif::import::ImportedMesh;
        let mesh = |positions: usize, colors: Vec<[f32; 4]>| {
            ImportedMesh::from_geometry(
                vec![[0.0; 3]; positions],
                colors,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        };
        let mut meshes = vec![
            mesh(2, vec![[0.3, 1.0, 1.0, 1.0], [0.55, 1.0, 1.0, 0.5]]),
            mesh(3, Vec::new()),
        ];
        bake_fallout_hair_tint(&mut meshes, Some([0.25; 3]));
        assert_eq!(
            meshes[0].colors,
            vec![[0.5, 0.5, 0.5, 1.0], [0.5, 0.5, 0.5, 0.5]]
        );
        assert_eq!(meshes[1].colors, vec![[0.5, 0.5, 0.5, 1.0]; 3]);

        let mut neutral = vec![mesh(1, vec![[0.3, 1.0, 1.0, 1.0]])];
        bake_fallout_hair_tint(&mut neutral, None);
        assert_eq!(neutral[0].colors, vec![[1.0, 1.0, 1.0, 1.0]]);
    }

    /// Regression: FaceGen noses rendered broken. EGM deltas are Z-up (as
    /// authored), the importer's head vertices are Y-up, and the deltas were
    /// added raw — so a Gamebyro up/down delta moved vertices forward/back and
    /// vice versa. They must go through the importer's own mapping first.
    #[test]
    fn egm_deltas_are_applied_in_the_imported_vertex_frame() {
        use byroredux_core::math::coord::zup_to_yup_pos;
        let morph = |delta: [f32; 3]| byroredux_facegen::EgmMorph {
            scale: 1.0,
            deltas: vec![delta],
        };
        // A vertex imported from Gamebyro (1, 2, 3).
        let base = [zup_to_yup_pos([1.0, 2.0, 3.0])];
        for delta in [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.5, -0.25, 2.0]] {
            let morphs = yup_egm_morphs(&[morph(delta)]);
            let got = byroredux_facegen::apply_morphs(&base, &morphs, &[1.0])[0];
            // Same result as morphing in Gamebyro space, then importing.
            let want = zup_to_yup_pos([1.0 + delta[0], 2.0 + delta[1], 3.0 + delta[2]]);
            for (g, w) in got.iter().zip(want) {
                assert!(
                    (g - w).abs() < 1.0e-6,
                    "delta {delta:?}: {got:?} vs {want:?}"
                );
            }
        }
        // Gamebyro up (+Z) is engine up (+Y).
        let up = byroredux_facegen::apply_morphs(
            &base,
            &yup_egm_morphs(&[morph([0.0, 0.0, 1.0])]),
            &[1.0],
        )[0];
        assert!(up[1] > base[0][1]);
    }

    #[test]
    fn only_fallout_runtime_hair_uses_the_shared_head_bone_basis() {
        assert!(head_parts_use_head_bone_mount(GameKind::Fallout3NV));
        assert!(!head_parts_use_head_bone_mount(GameKind::Oblivion));
        assert!(!head_parts_use_head_bone_mount(GameKind::Skyrim));
    }

    #[test]
    fn prebaked_skeleton_uses_inherited_race_and_gender() {
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.races.insert(
            2,
            RaceRecord {
                skeleton_models: ["male.nif".into(), "female.nif".into()],
                ..Default::default()
            },
        );
        index.npcs.insert(
            3,
            NpcRecord {
                form_id: 3,
                race_form_id: 2,
                acbs_flags: 1,
                ..Default::default()
            },
        );
        let shell = NpcRecord {
            form_id: 4,
            race_form_id: 99,
            acbs_flags: 0,
            template_form_id: 3,
            template_flags: byroredux_plugin::equip::TEMPLATE_FLAG_USE_TRAITS,
            ..Default::default()
        };
        let mut world = World::new();
        let state = prepare_prebaked_state(
            &mut world,
            &shell,
            GameKind::Skyrim,
            "Skyrim.esm",
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index,
        );
        assert_eq!(state.skeleton_path.as_deref(), Some(r"meshes\female.nif"));
    }

    #[test]
    fn equipment_parts_keep_shared_item_ownership_without_tagging_body() {
        let mut world = World::new();
        let actor = world.spawn();
        let body = world.spawn();
        parent_part(&mut world, actor, body);
        let gear = NpcEquipmentPart {
            actor,
            inventory_index: Some(InventoryIndex(3)),
            form_id: 0x1234,
            intrinsic_skin: false,
            hidden_biped_mask: 0,
        };
        let torso = world.spawn();
        let hands = world.spawn();
        parent_equipment_part(&mut world, torso, gear);
        parent_equipment_part(&mut world, hands, gear);
        let skin = world.spawn();
        let skin_owner = NpcEquipmentPart {
            inventory_index: None,
            form_id: 0x5678,
            intrinsic_skin: true,
            hidden_biped_mask: 4,
            ..gear
        };
        parent_equipment_part(&mut world, skin, skin_owner);
        let ownership = world.query::<NpcEquipmentPart>().unwrap();
        assert_eq!(ownership.get(torso), Some(&gear));
        assert_eq!(ownership.get(hands), Some(&gear));
        assert_eq!(ownership.get(skin), Some(&skin_owner));
        assert!(ownership.get(body).is_none());
        assert!(ownership.get(actor).is_none());
        let parents = world.query::<Parent>().unwrap();
        for part in [body, torso, hands, skin] {
            assert_eq!(parents.get(part).unwrap().0, actor);
        }
    }

    /// The `CaucasianOldAged` (`000987DF`) head section, as parsed from
    /// `FalloutNV.esm`: every role authored twice, once per gender.
    fn fnv_race() -> RaceRecord {
        RaceRecord {
            form_id: 0x000987DF,
            editor_id: "CaucasianOldAged".to_string(),
            head_parts: vec![
                (0, r"Characters\Head\HeadOld.NIF".into(), Some(0)),
                (2, r"Characters\Head\MouthHuman.NIF".into(), Some(0)),
                (3, r"Characters\Head\TeethLowerHuman.NIF".into(), Some(0)),
                (4, r"Characters\Head\TeethUpperHuman.NIF".into(), Some(0)),
                (5, r"Characters\Head\TongueHuman.NIF".into(), Some(0)),
                (6, r"Characters\Head\EyeLeftHuman.NIF".into(), Some(0)),
                (7, r"Characters\Head\EyeRightHuman.NIF".into(), Some(0)),
                (0, r"Characters\Head\HeadOldFemale.NIF".into(), Some(1)),
                (2, r"Characters\Head\MouthHuman.NIF".into(), Some(1)),
                (6, r"Characters\Head\EyeLeftHumanFemale.NIF".into(), Some(1)),
                (
                    7,
                    r"Characters\Head\EyeRightHumanFemale.NIF".into(),
                    Some(1),
                ),
            ],
            // The flat list the head used to be read from: male head first.
            body_models: vec![
                r"Characters\Head\HeadOld.NIF".into(),
                r"Characters\Head\HeadOldFemale.NIF".into(),
            ],
            ..RaceRecord::default()
        }
    }

    /// #3418 (FNV-2026-08-27-D4-02) — the head used to be
    /// `body_models.first()`, which is the male head on all 22 vanilla
    /// FNV races. Every female NPC then had its female-authored FGGS /
    /// FGGA morph deltas applied to the male base mesh.
    #[test]
    fn fnv_head_is_selected_per_gender() {
        let race = fnv_race();
        assert_eq!(
            head_part_path(&race, GameKind::Fallout3NV, head_part::Role::Head, 0).as_deref(),
            Some(r"Characters\Head\HeadOld.NIF"),
        );
        assert_eq!(
            head_part_path(&race, GameKind::Fallout3NV, head_part::Role::Head, 1).as_deref(),
            Some(r"Characters\Head\HeadOldFemale.NIF"),
            "the female head is authored and tagged — pre-#3418 it was never read",
        );
    }

    /// A race with no head-part table at all (Skyrim+, or a malformed
    /// head section) must keep falling back to `body_models.first()`.
    #[test]
    fn head_falls_back_to_body_models_without_a_head_part_table() {
        let race = RaceRecord {
            body_models: vec!["fallback.nif".into()],
            ..RaceRecord::default()
        };
        assert_eq!(
            head_part_path(&race, GameKind::Skyrim, head_part::Role::Head, 1),
            None,
            "Skyrim authors no RACE head-part table",
        );
        assert_eq!(
            race.body_models.first().map(String::as_str),
            Some("fallback.nif")
        );
    }

    /// #3420 — the head sub-parts the spawner mounts beside the eyes.
    /// Pre-fix mouth / teeth / tongue were parsed, indexed and dropped,
    /// leaving every FNV NPC with a hole behind the lips.
    #[test]
    fn fnv_head_sub_parts_cover_the_oral_cavity() {
        let race = fnv_race();
        let male = head_part_paths(&race, GameKind::Fallout3NV, &head_part::ORAL_ROLES, 0);
        assert_eq!(
            male,
            vec![
                r"Characters\Head\MouthHuman.NIF".to_string(),
                r"Characters\Head\TeethLowerHuman.NIF".to_string(),
                r"Characters\Head\TeethUpperHuman.NIF".to_string(),
                r"Characters\Head\TongueHuman.NIF".to_string(),
            ],
        );
        // The female section of this fixture authors only the mouth —
        // an actor never picks up the other gender's meshes.
        assert_eq!(
            head_part_paths(&race, GameKind::Fallout3NV, &head_part::ORAL_ROLES, 1),
            vec![r"Characters\Head\MouthHuman.NIF".to_string()],
        );
    }

    /// #3420's Oblivion arm. Its nine-slot table puts the eyes at 7 / 8,
    /// so the hard-coded Fallout pair (6 / 7) selected the *tongue* and
    /// the left eye — and the spawner then painted the actor's eye
    /// texture over both. Oblivion's head section is ungendered, so
    /// every entry is `None`-tagged and applies to either gender.
    #[test]
    fn oblivion_eye_roles_skip_the_tongue() {
        let race = RaceRecord {
            head_parts: vec![
                (0, r"Characters\Imperial\HeadHuman.nif".into(), None),
                (1, r"Characters\Imperial\EarsHuman.nif".into(), None),
                (2, r"Characters\Imperial\EarsHuman.nif".into(), None),
                (3, r"Characters\Imperial\MouthHuman.nif".into(), None),
                (4, r"Characters\Imperial\TeethLowerHuman.nif".into(), None),
                (5, r"Characters\Imperial\TeethUpperHuman.nif".into(), None),
                (6, r"Characters\Imperial\TongueHuman.nif".into(), None),
                (7, r"Characters\Imperial\EyeLeftHuman.nif".into(), None),
                (8, r"Characters\Imperial\EyeRightHuman.nif".into(), None),
            ],
            ..RaceRecord::default()
        };
        let eyes = head_part_paths(
            &race,
            GameKind::Oblivion,
            &[head_part::Role::LeftEye, head_part::Role::RightEye],
            1,
        );
        assert_eq!(
            eyes,
            vec![
                r"Characters\Imperial\EyeLeftHuman.nif".to_string(),
                r"Characters\Imperial\EyeRightHuman.nif".to_string(),
            ],
        );
        assert!(
            !eyes.iter().any(|path| path.contains("Tongue")),
            "the tongue is index 6 on Oblivion, not an eye",
        );
        assert_eq!(
            head_part_path(&race, GameKind::Oblivion, head_part::Role::EarFemale, 1).as_deref(),
            Some(r"Characters\Imperial\EarsHuman.nif"),
        );
    }

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

    /// #4700 — a Skyrim Draugr spawned through the pre-baked path (the
    /// only path Skyrim actors take) must carry `DraugrCombatAnim`, derived
    /// from its `Use Traits`-resolved race exactly as the runtime path
    /// derives it; a human spawned the same way must not.
    #[test]
    fn prebaked_draugr_spawn_carries_the_combat_anim_marker() {
        let mut index = EsmIndex::default();
        for (form_id, editor_id) in [(0x2, "DraugrRace"), (0x5, "NordRace")] {
            index.races.insert(
                form_id,
                RaceRecord {
                    editor_id: editor_id.into(),
                    skeleton_models: ["male.nif".into(), "female.nif".into()],
                    ..Default::default()
                },
            );
        }
        for (race_form_id, expect_marker) in [(0x2, true), (0x5, false)] {
            let npc = NpcRecord {
                form_id: 0x383F7,
                race_form_id,
                ..Default::default()
            };
            let mut world = World::new();
            let mut state = prepare_prebaked_state(
                &mut world,
                &npc,
                GameKind::Skyrim,
                "Skyrim.esm",
                Vec3::ZERO,
                Quat::IDENTITY,
                1.0,
                &index,
            );
            // The skeleton phase's product; the GPU units in between add
            // meshes, not the marker.
            state.skel_root = Some(world.spawn());
            assert!(matches!(
                finalize_prebaked(&mut state, &mut world, &npc, &index),
                UnitOutcome::Complete(Some(root)) if root == state.placement_root
            ));
            assert_eq!(
                world
                    .get::<crate::components::DraugrCombatAnim>(state.placement_root)
                    .is_some(),
                expect_marker,
                "race {race_form_id:#x}"
            );
            assert!(world.has::<crate::components::AnimationTarget>(state.placement_root));
        }
    }

    #[test]
    fn missing_prebaked_facegen_continues_to_armor_and_finalization() {
        let mut world = World::new();
        let placement_root = world.spawn();
        let mut state = PrebakedNpcState {
            skeleton_path: humanoid_skeleton_path(GameKind::Skyrim).map(str::to_owned),
            appearance: NpcLootAppearance::default(),
            placement_root,
            skel_root: Some(world.spawn()),
            skel_map: HashMap::new(),
            facegen_path: None,
            tint_path: None,
            armor: Vec::new(),
            facegen_hidden_mask: 0,
            equipped_armor_count: 0,
            combat_anim_draugr: false,
            phase: PrebakedPhase::Facegen,
        };

        assert!(matches!(
            state.skip_missing_facegen(),
            UnitOutcome::Continue
        ));
        assert_eq!(state.phase, PrebakedPhase::Armor(0));
        assert_eq!(state.placement_root, placement_root);
        assert!(state.skel_root.is_some());
    }

    /// Regression for #2276 (PERF-D7-02): `parent_part` used to re-walk
    /// the whole `placement_root` subtree on every attach, so a bug
    /// anywhere else that left an already-attached sibling untagged
    /// would get silently "fixed" by the next unrelated attach — masking
    /// the real bug and doing wasted, ever-growing work per call. It must
    /// now scope the walk to `part_root`, touching only the subtree that
    /// was just attached.
    #[test]
    fn parent_part_only_tags_the_newly_attached_subtree() {
        use byroredux_core::ecs::components::RenderLayer;
        use byroredux_core::ecs::MeshHandle;

        let mut world = World::new();
        let placement_root = world.spawn();

        // A sibling already hanging off `placement_root` that was never
        // tagged. Pre-fix, `parent_part`'s full-tree walk from
        // `placement_root` would tag this as a side effect of attaching
        // the unrelated part below.
        let stale_sibling = world.spawn();
        world.insert(stale_sibling, MeshHandle(1));
        add_child(&mut world, placement_root, stale_sibling);

        // The part being attached this call, with its own descendant.
        let part_root = world.spawn();
        let part_child = world.spawn();
        world.insert(part_root, MeshHandle(2));
        world.insert(part_child, MeshHandle(3));
        add_child(&mut world, part_root, part_child);

        parent_part(&mut world, placement_root, part_root);

        let layer_q = world.query::<RenderLayer>().unwrap();
        assert_eq!(
            layer_q.get(part_root).copied(),
            Some(RenderLayer::Actor),
            "the newly attached part root must be tagged"
        );
        assert_eq!(
            layer_q.get(part_child).copied(),
            Some(RenderLayer::Actor),
            "the newly attached part's own descendants must be tagged"
        );
        assert!(
            layer_q.get(stale_sibling).is_none(),
            "parent_part must scope tagging to the newly attached subtree, \
             not re-walk placement_root's whole tree (#2276)"
        );
    }
}
