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
    /// P3 player-body attach: the job assembles the *player's* visual body
    /// off `NPC_ 0x7`. The player entity already carries its identity
    /// (FormID sentinel, ActorValues, Inventory/EquipmentSlots, scene-alias
    /// candidate), so the placement root gets no identity stamps, no AI
    /// package, no loot-appearance state, and no bone colliders or ragdoll
    /// template — the player's physics presence stays the Character-mode
    /// capsule, and interaction/combat rays keep excluding exactly one body.
    /// See `crate::player_body`.
    player_body: bool,
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

// #5091 — the two spawn arms are separate state machines and now live
// in their own files; this module keeps the job driver, the shared
// placement / parenting helpers, and the tests.
mod prebaked;
mod runtime;

use prebaked::{PrebakedNpcState, PrebakedPhase, advance_prebaked_unit, prepare_prebaked_state};
use runtime::{RuntimeNpcState, RuntimePhase, advance_runtime_unit, prepare_runtime_state};


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
            player_body: false,
        }
    }

    /// Mark this job as assembling the player's body (`crate::player_body`).
    /// Same phase machine, stripped of everything that would duplicate the
    /// player entity's already-stamped state or add physics presence beside
    /// the Character-mode capsule.
    pub(crate) fn into_player_body(mut self) -> Self {
        self.player_body = true;
        self
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
            player_body: false,
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
                            self.player_body,
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
                            self.player_body,
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
                PrebakedPhase::HeadParts(_) => "PNAM head-part fallback",
                PrebakedPhase::Armor(_) => "armor",
                PrebakedPhase::Finalize => "finalization",
            },
            Self::Done => "completed actor",
        }
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
///
/// `player_body` roots get transform + name only: every identity stamp
/// below already has its player-entity counterpart (stamped by
/// `inventory::attach_to_player` from the same `NPC_ 0x7` record), and a
/// second actor carrying `ActorValues`/`FactionRanks` beside the player
/// would be a phantom target for AI and combat scans.
fn spawn_placement_root<'a>(
    world: &mut World,
    npc: &'a NpcRecord,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &'a EsmIndex,
    player_body: bool,
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
    if player_body {
        return (placement_root, resolved);
    }
    // #5391 — the authored placement, kept fixed for "near editor
    // location" packages however far later packages walk the actor.
    world.insert(
        placement_root,
        byroredux_core::ecs::components::EditorPlacement {
            translation: ref_pos,
        },
    );
    stamp_faction_ranks(world, placement_root, &resolved);
    // #4817 — `CombatDisposition` is NOT stamped here: this root is live for
    // every frame the job yields, before its body loads and before the
    // caller restores a parked `Dead`. It lands in `arm_combat_disposition`
    // at finalize instead.
    stamp_actor_values(world, placement_root, &resolved, index);
    stamp_spell_list(world, placement_root, &resolved, index);
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
    use super::prebaked::*;
    use super::runtime::*;
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
            let mut morphs = [morph(delta)];
            let allocation = morphs[0].deltas.as_ptr();
            yup_egm_morphs(&mut morphs);
            assert_eq!(allocation, morphs[0].deltas.as_ptr());
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
        let mut morphs = [morph([0.0, 0.0, 1.0])];
        yup_egm_morphs(&mut morphs);
        let up = byroredux_facegen::apply_morphs(&base, &morphs, &[1.0])[0];
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
            false,
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
                false,
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

    /// #4817 — a resumable job's placement root is live for every frame the
    /// job yields. It must not carry `CombatDisposition` (the hostility
    /// perceiver gate) until finalize, which runs in the same synchronous
    /// step as the caller's parked-`Dead` restore.
    #[test]
    fn combat_disposition_is_armed_at_finalize_not_at_prepare() {
        use byroredux_plugin::esm::records::{ActorAiData, Aggression, Confidence};
        let npc = NpcRecord {
            form_id: 0x383F7,
            ai_data: Some(ActorAiData {
                aggression: Aggression::VeryAggressive,
                confidence: Confidence::Average,
                attack_radius: None,
            }),
            ..Default::default()
        };
        let index = EsmIndex::default();
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
            false,
        );
        assert!(
            world
                .get::<crate::systems::CombatDisposition>(state.placement_root)
                .is_none(),
            "a mid-job root must not be a hostility perceiver"
        );
        state.skel_root = Some(world.spawn());
        assert!(matches!(
            finalize_prebaked(&mut state, &mut world, &npc, &index),
            UnitOutcome::Complete(Some(_))
        ));
        assert!(world
            .get::<crate::systems::CombatDisposition>(state.placement_root)
            .is_some());
    }

    /// #4822 — an actor with no own or racial `SPLO` still gets a
    /// `SpellList`, so a quest's `AddSpell` has somewhere to land.
    #[test]
    fn spell_less_actor_gets_an_empty_spell_list_that_add_spell_can_use() {
        let npc = NpcRecord {
            form_id: 0x383F7,
            ..Default::default()
        };
        let index = EsmIndex::default();
        let mut world = World::new();
        let root = prepare_prebaked_state(
            &mut world,
            &npc,
            GameKind::Skyrim,
            "Skyrim.esm",
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index,
            false,
        )
        .placement_root;
        assert_eq!(
            world.get::<byroredux_scripting::SpellList>(root).map(|l| l.0.clone()),
            Some(Vec::new())
        );
        assert!(byroredux_scripting::add_spell(&world, root, 0xABCD));
        assert_eq!(
            world.get::<byroredux_scripting::SpellList>(root).unwrap().0,
            vec![0xABCD]
        );
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
            player_body: false,
            head_fallback: Vec::new(),
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

    /// #5095 — vanilla Skyrim ships no facegeom for the player record, so
    /// the pre-baked miss is the player's *normal* path, and it used to
    /// jump straight to armor: a headless third-person body. The miss now
    /// routes through the NPC's authored PNAM head parts when it has any
    /// (the player record authors ManHead / eyes / hair / brows), and only
    /// an NPC with neither source stays headless.
    #[test]
    fn missing_prebaked_facegen_falls_back_to_authored_head_parts() {
        let mut world = World::new();
        let mut state = PrebakedNpcState {
            skeleton_path: None,
            appearance: NpcLootAppearance::default(),
            placement_root: world.spawn(),
            skel_root: None,
            skel_map: HashMap::new(),
            facegen_path: None,
            tint_path: None,
            armor: Vec::new(),
            facegen_hidden_mask: 0,
            equipped_armor_count: 0,
            combat_anim_draugr: false,
            player_body: true,
            head_fallback: vec![
                "actors\\character\\character assets\\malehead.nif".into(),
                "actors\\character\\character assets\\eyes.nif".into(),
            ],
            phase: PrebakedPhase::Facegen,
        };

        state.skip_missing_facegen();
        assert_eq!(
            state.phase,
            PrebakedPhase::HeadParts(0),
            "the miss enters the head-part fallback, not armor"
        );
    }

    /// #5095 — the fallback source itself: the NPC's `PNAM` head-part list
    /// resolved through `HDPT.MODL`, authored order kept, unresolvable
    /// FormIDs and path-less records skipped.
    #[test]
    fn prebaked_head_fallback_paths_follow_pnam_order_and_skip_unresolvable() {
        use byroredux_plugin::esm::records::HdptRecord;

        let hdpt = |form_id: u32, model: &str| HdptRecord {
            form_id,
            editor_id: String::new(),
            full_name: String::new(),
            model_path: model.to_owned(),
            flags: 0,
        };
        let mut index = EsmIndex::default();
        index.head_parts.insert(0xAA00_0001, hdpt(0xAA00_0001, "malehead.nif"));
        index.head_parts.insert(0xAA00_0002, hdpt(0xAA00_0002, ""));
        // 0xAA00_0003 deliberately absent — a dangling PNAM reference.

        let npc_with_parts = NpcRecord {
            form_id: 7,
            face_morphs: Some(byroredux_plugin::esm::records::NpcFaceMorphs {
                head_parts: vec![0xAA00_0001, 0xAA00_0003, 0xAA00_0002],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            prebaked_head_fallback_paths(&npc_with_parts, &index),
            vec!["malehead.nif".to_owned()],
            "authored order, dangling refs and path-less HDPTs skipped"
        );

        let npc_without = NpcRecord::default();
        assert!(prebaked_head_fallback_paths(&npc_without, &index).is_empty());
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

    /// P3 player body — the placement root assembled for the player's body
    /// carries transform + name only. Every identity stamp has its
    /// player-entity counterpart already (`inventory::attach_to_player`
    /// resolves the same `NPC_ 0x7`), and a second actor carrying
    /// `ActorValues`/`FactionRanks` beside the player would be a phantom
    /// target for AI and combat scans. The control arm pins that the
    /// ordinary NPC path still stamps the same record.
    #[test]
    fn player_body_placement_root_carries_no_identity_stamps() {
        let npc = NpcRecord {
            form_id: 0x7,
            editor_id: "Player".into(),
            ..NpcRecord::default()
        };
        let mut index = EsmIndex::default();
        index.npcs.insert(0x7, npc.clone());

        let mut world = World::new();
        world.insert_resource(StringPool::default());
        let player_root = prepare_prebaked_state(
            &mut world,
            &npc,
            GameKind::Skyrim,
            "skyrim.esm",
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index,
            true,
        )
        .placement_root;
        assert!(
            world.get::<byroredux_core::ecs::components::actor_values::ActorValues>(player_root)
                .is_none(),
            "the player body root must not become a second actor with ActorValues"
        );
        assert!(world.get::<FactionRanks>(player_root).is_none());
        assert!(
            world.get::<Inventory>(player_root).is_none(),
            "the player entity owns the real inventory; a second resolution's \
             rows on the body root would orphan its equip indices"
        );

        let mut world = World::new();
        world.insert_resource(StringPool::default());
        let npc_root = prepare_prebaked_state(
            &mut world,
            &npc,
            GameKind::Skyrim,
            "skyrim.esm",
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index,
            false,
        )
        .placement_root;
        assert!(
            world.get::<Inventory>(npc_root).is_some(),
            "control: the prebaked prepare inserts the NPC's own inventory \
             (ActorValues are conditional on authored stats, absent in this \
             empty index)"
        );
    }

    /// P3 player body — the finalize unit keeps the player entity's own
    /// state out of the body root (no inventory/equipment copies, no AI
    /// package, no loot-appearance state) while still landing the
    /// `AnimationTarget` the future third-person animation pass will
    /// consume. The control arm pins the ordinary NPC finalize contract on
    /// the same state.
    #[test]
    fn player_body_finalize_keeps_player_state_out_of_the_body_root() {
        let npc = NpcRecord {
            form_id: 0x7,
            ..NpcRecord::default()
        };
        let index = EsmIndex::default();

        let mut world = World::new();
        let placement_root = world.spawn();
        let mut state = PrebakedNpcState {
            skeleton_path: None,
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
            player_body: true,
            head_fallback: Vec::new(),
            phase: PrebakedPhase::Finalize,
        };
        // The armor phases push worn roots into `original_roots` during a
        // real spawn; give the appearance one so the player arm's skip is
        // attributable to the player flag rather than install()'s own
        // empty-appearance early-return (which the control arm exercises).
        state.appearance.original_roots.push(world.spawn());
        assert!(matches!(
            finalize_prebaked(&mut state, &mut world, &npc, &index),
            UnitOutcome::Complete(Some(root)) if root == placement_root
        ));
        assert!(
            world.get::<Inventory>(placement_root).is_none(),
            "the prebaked finalize never inserts inventory (that is prepare's \
             job) — the player-body gate is the loot/AI/walk skips below"
        );
        assert!(
            world
                .get::<super::loot_appearance::NpcLootAppearance>(placement_root)
                .is_none(),
            "the player is not a loot source; no appearance-restoration state"
        );
        assert!(
            world
                .get::<crate::components::AnimationTarget>(placement_root)
                .is_some(),
            "the skeleton target is the future third-person animation hook"
        );

        // Control: the same finalize still fully finalizes an ordinary NPC.
        let mut world = World::new();
        let placement_root = world.spawn();
        let mut state = PrebakedNpcState {
            player_body: false,
            placement_root,
            skel_root: Some(world.spawn()),
            ..state
        };
        assert!(matches!(
            finalize_prebaked(&mut state, &mut world, &npc, &index),
            UnitOutcome::Complete(Some(root)) if root == placement_root
        ));
        assert!(
            world
                .get::<super::loot_appearance::NpcLootAppearance>(placement_root)
                .is_some(),
            "control: the ordinary NPC finalize installs loot-appearance state"
        );
    }
}
