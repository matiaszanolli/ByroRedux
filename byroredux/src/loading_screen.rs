//! Transition-owned loading presentation. Two backends, both drawing the
//! installed game's own LSCR art rather than a bundled imitation:
//! the legacy flat ICON artwork + DESC tip (Oblivion / FO3 / FNV), and the
//! Creation-era 3D model stage (Skyrim / FO4) — the NNAM model posed by
//! SNAM/RNAM/XNAM or a TNAM→TRNS transform, framed by its own camera with
//! the classic slow turntable. Selection is shared with the plugin crate
//! (`first_backend_load_screen`: only well-formed, unconditional records
//! whose art this engine can resolve). Full legacy XML/NIF menu animation,
//! TRNS "Around Origin" semantics, ONAM/ZNAM interactive zoom bounds and
//! the MOD2 camera-path NIFs remain TODO.

use crate::cell_loader::{LoadedCellIndex, PendingCellTransition};
use byroredux_core::ecs::components::Children;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{MeshHandle, Transform, World};
use byroredux_core::math::{coord, Mat4, Quat, Vec3};
use byroredux_plugin::esm::records::{
    first_backend_load_screen, LoadScreenRecord, LoadScreenTransform, LoadScreenVerdict,
    ResolvedLoadScreenModel,
};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{GpuLight, SkyParams, VulkanContext};

use crate::render::RenderFrameView;
use crate::fog::FogMedium;

/// LSCR header flag 0x8000 — "No Rotation" (FO4+; Skyrim's flag set has no
/// such bit, so its stages always turn).
const FLAG_NO_ROTATION: u32 = 0x8000;

/// Flat ambient the model stage renders under while the destination cell
/// streams in behind it. The NIF's own embedded lights (most load-screen
/// art carries one or two NiPointLights) ride along and shape it.
const STAGE_AMBIENT: [f32; 3] = [0.7; 3];

/// Pinned linear exposure for the cover frames (the meter's fixed mode
/// writes this multiplier directly). The auto meter cannot meter a
/// mostly-black frame without riding its ceiling; see the override site
/// in `render_one_frame`. Calibrated on Skyrim's
/// `LoadScreenArt\LoadScreenReagent01.nif` cover 2026-10-02.
pub(crate) const STAGE_EXPOSURE_LINEAR: f32 = 1.5;

/// Elevation of the stage camera above the model's bounds centre, as an
/// angle from the horizontal. A shallow down-tilt — the classic Bethesda
/// load-screen look — without claiming any per-game authored value (the
/// authored camera path NIFs, MOD2, are a separate TODO).
const STAGE_CAMERA_ELEVATION_RAD: f32 = 0.3;

/// Margin between the posed model's bounding sphere and the frame edge
/// when fitting the camera distance to the vertical FOV.
const STAGE_FRAME_MARGIN: f32 = 1.35;

#[derive(Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    AwaitingPresentation,
    Presented,
    Loading,
    DestinationReady,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Owner {
    #[default]
    Door,
    Save,
}

/// The fixed framing camera for one model stage: an eye point on a
/// shallow arc around the posed bounds, looking at its centre.
#[derive(Debug)]
pub(crate) struct StageCamera {
    eye: Vec3,
    target: Vec3,
}

/// Live state of one spawned Creation-era loading model. Owned here, not
/// on the ECS beyond the `LoadingModelStage` marker the turntable system
/// reads — the draw filter and retirement both key off this struct.
#[derive(Debug)]
pub(crate) struct ModelStage {
    #[allow(dead_code)]
    key: String,
    root: EntityId,
    /// Every mesh handle under the stage root — the per-frame draw filter
    /// retains exactly these while the cover is up.
    mesh_handles: std::collections::HashSet<u32>,
    camera: StageCamera,
    /// Posed bounds centre + a light-retention radius: the NIF's own
    /// lights are inside the posed bounds by construction, streaming
    /// destination lights are not.
    center: Vec3,
    light_radius: f32,
}

enum Artwork {
    /// One retained original image, reused on repeated transitions. This
    /// avoids allocating a new non-reusable bindless slot at every door.
    Image { key: String, texture: u32 },
    /// A freshly spawned model stage; retired (despawned + GPU-released)
    /// when the cover is dismissed.
    Stage(Box<ModelStage>),
}

#[derive(Default)]
pub(crate) struct LoadingScreen {
    phase: Phase,
    owner: Owner,
    pending: Option<PendingCellTransition>,
    artwork: Option<Artwork>,
    tip: Option<String>,
    /// A stage awaiting the App-side despawn/GPU-release poll. Set by the
    /// same transitions that end the cover (`cancel`), drained exactly
    /// once by [`LoadingScreen::take_retired_stage`].
    retired_stage: Option<Box<ModelStage>>,
    restore_capture: bool,
}

impl LoadingScreen {
    pub(crate) fn active(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub(crate) fn waiting_for_presentation(&self) -> bool {
        self.phase == Phase::AwaitingPresentation
    }

    /// The live model stage while the cover owns the frame — every phase
    /// from first presentation through the held destination-ready frames,
    /// matching how the legacy image cover stays up until a coherent
    /// destination frame exists.
    pub(crate) fn active_stage(&self) -> Option<&ModelStage> {
        if !self.active() {
            return None;
        }
        match self.artwork.as_ref()? {
            Artwork::Stage(stage) => Some(stage.as_ref()),
            Artwork::Image { .. } => None,
        }
    }

    pub(crate) fn texture(&self) -> Option<u32> {
        if !self.active() {
            return None;
        }
        match self.artwork.as_ref()? {
            Artwork::Image { texture, .. } => Some(*texture),
            Artwork::Stage(_) => None,
        }
    }

    pub(crate) fn tip(&self) -> Option<&str> {
        if self.active() {
            self.tip.as_deref()
        } else {
            None
        }
    }

    pub(crate) fn begin(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        pending: PendingCellTransition,
    ) -> Result<(), PendingCellTransition> {
        if !self.begin_artwork(world, ctx, Owner::Door) {
            return Err(pending);
        }
        self.pending = Some(pending);
        Ok(())
    }

    pub(crate) fn begin_save(&mut self, world: &mut World, ctx: &mut VulkanContext) -> bool {
        self.begin_artwork(world, ctx, Owner::Save)
    }

    pub(crate) fn owns_save(&self) -> bool {
        self.active() && self.owner == Owner::Save
    }

    pub(crate) fn take_presented_save(&mut self) -> bool {
        if self.owner != Owner::Save || self.phase != Phase::Presented {
            return false;
        }
        self.phase = Phase::Loading;
        true
    }

    fn begin_artwork(&mut self, world: &mut World, ctx: &mut VulkanContext, owner: Owner) -> bool {
        // Own an Arc, not an ECS read guard, across archive I/O and GPU work.
        // In particular do not nest InputState access under the index lock.
        let Some(index) = world
            .try_resource::<LoadedCellIndex>()
            .map(|loaded| std::sync::Arc::clone(&loaded.0))
        else {
            return false;
        };
        let Some((screen, verdict)) = first_backend_load_screen(&index) else {
            return false;
        };
        self.tip = Some(screen.description.clone());
        let args = crate::cli_args::effective_args();
        match verdict {
            LoadScreenVerdict::Model(Some(resolved)) => {
                // CLI archive/load-order identity is included because identical
                // Bethesda paths may name different models in different installs.
                let key = format!("{:?}:{:?}:{}", index.game, args, resolved.model_path);
                if !self.spawn_model_stage(world, ctx, screen, &resolved, &key) {
                    self.tip = None;
                    return false;
                }
            }
            LoadScreenVerdict::Model(None) => {
                // CLI archive/load-order identity is included because identical
                // Bethesda paths may name different images in different installs.
                let key = format!("{:?}:{:?}:{}", index.game, args, screen.icon);
                if !self.present_image_artwork(world, ctx, screen, &key) {
                    self.tip = None;
                    return false;
                }
            }
            LoadScreenVerdict::Rejected(_) => {
                self.tip = None;
                return false;
            }
        }
        log::info!(
            "loading.screen: begin LSCR={:08X} owner={owner:?}",
            screen.form_id
        );
        self.restore_capture = world
            .try_resource::<crate::components::InputState>()
            .is_some_and(|input| input.mouse_captured);
        self.pending = None;
        self.owner = owner;
        self.phase = Phase::AwaitingPresentation;
        true
    }

    fn present_image_artwork(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        screen: &LoadScreenRecord,
        key: &str,
    ) -> bool {
        if self.artwork.as_ref().is_none_or(|art| match art {
            Artwork::Image { key: k, .. } => k != key,
            Artwork::Stage(_) => true,
        }) {
            let args = crate::cli_args::effective_args();
            let provider = crate::asset_provider::build_texture_provider(&args);
            let Some(bytes) = provider.extract(&screen.icon) else {
                log::warn!("loading.screen: missing original artwork {}", screen.icon);
                return false;
            };
            let Some(allocator) = ctx.allocator.as_ref() else {
                return false;
            };
            let upload = GpuUploadCtx {
                device: &ctx.device,
                allocator,
                queue: &ctx.graphics_queue,
                command_pool: ctx.transfer_pool,
            };
            let texture = match ctx.texture_registry.load_dds_with_clamp(
                upload,
                &format!("loading-screen@{key}"),
                &bytes,
                0,
            ) {
                Ok(texture) => texture,
                Err(error) => {
                    log::warn!("loading.screen: artwork upload failed: {error:#}");
                    return false;
                }
            };
            if let Some(Artwork::Image { texture: old, .. }) = self.artwork.take() {
                ctx.texture_registry.drop_texture(&ctx.device, old);
            } else if let Some(stage) = self.retired_stage.take() {
                retire_stage(world, Some(ctx), *stage);
            }
            self.artwork = Some(Artwork::Image {
                key: key.to_owned(),
                texture,
            });
        }
        true
    }

    /// Spawn the Creation-era model stage: extract the NNAM model through
    /// the archive provider, import it through the same cached loose-NIF
    /// path the CLI mesh viewer uses, then pose the root with the record's
    /// authored transform and frame it by its own bounds.
    fn spawn_model_stage(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        screen: &LoadScreenRecord,
        resolved: &ResolvedLoadScreenModel,
        key: &str,
    ) -> bool {
        let args = crate::cli_args::effective_args();
        let tex_provider = crate::asset_provider::build_texture_provider(&args);
        let mut mat_provider = crate::asset_provider::build_material_provider(&args);
        // STAT MODL paths are usually folder-relative; the archive lookup
        // wants the `meshes\`-prefixed canonical form (same key the cell
        // loader's import registry uses).
        let archive_path = crate::cell_loader::canonical_model_path_key(&resolved.model_path);
        let Some(bytes) = tex_provider.extract_mesh(&archive_path) else {
            log::warn!("loading.screen: missing model artwork {archive_path}");
            return false;
        };
        // Peek the cached import first: bounds + the zero-mesh rejection
        // are decidable without spawning anything.
        let Some(scene) =
            crate::scene::peek_or_parse_scene(world, &archive_path, &tex_provider, Some(&mut mat_provider))
        else {
            log::warn!("loading.screen: model import failed {archive_path}");
            return false;
        };
        if scene.meshes.is_empty() {
            log::warn!("loading.screen: model has no meshes {archive_path}");
            return false;
        }
        let Some(local_bounds) = scene.geometry_bounds() else {
            log::warn!("loading.screen: model has no finite geometry {archive_path}");
            return false;
        };
        let (count, root) = crate::scene::load_nif_bytes(
            world,
            ctx,
            &bytes,
            &archive_path,
            &tex_provider,
            Some(&mut mat_provider),
        );
        let Some(root) = root else {
            log::warn!("loading.screen: model spawn failed {archive_path}");
            return false;
        };
        if count == 0 {
            // Nodes spawned but no mesh took — clean the empty hierarchy up
            // rather than leaking it into the destination cell.
            despawn_subtree(world, root);
            log::warn!("loading.screen: model spawned zero meshes {archive_path}");
            return false;
        }
        let pose = stage_pose(screen, resolved.transform.as_ref());
        pose_root(world, root, &pose);
        world.insert(
            root,
            crate::components::LoadingModelStage {
                no_rotation: screen.flags & FLAG_NO_ROTATION != 0,
            },
        );
        let mesh_handles = collect_stage_mesh_handles(world, root);
        let posed = posed_bounds(&pose, local_bounds);
        let fov_y = active_fov_y(world).unwrap_or(std::f32::consts::FRAC_PI_3);
        let Some(camera) = stage_camera(&posed, fov_y) else {
            despawn_subtree(world, root);
            log::warn!("loading.screen: model cannot be framed {archive_path}");
            return false;
        };
        let (center, light_radius) = posed_center_radius(&posed);
        if let Some(stage) = self.retired_stage.take() {
            // Defensive: the App poll drains retirement every frame, so this
            // only fires if a begin raced ahead of one poll.
            retire_stage(world, Some(ctx), *stage);
        }
        let transform_source = match &resolved.transform {
            Some(trns) => format!("trns={}", trns.editor_id),
            None => "inline".to_string(),
        };
        log::info!(
            "loading.screen: model stage {} meshes={} {transform_source} \
             posed-radius={light_radius:.0} no_rotation={}",
            archive_path,
            mesh_handles.len(),
            screen.flags & FLAG_NO_ROTATION != 0
        );
        self.artwork = Some(Artwork::Stage(Box::new(ModelStage {
            key: key.to_owned(),
            root,
            mesh_handles,
            camera,
            center,
            light_radius,
        })));
        true
    }

    pub(crate) fn take_presented_transition(&mut self) -> Option<PendingCellTransition> {
        if self.owner != Owner::Door || self.phase != Phase::Presented {
            return None;
        }
        self.phase = Phase::Loading;
        self.pending.take()
    }

    pub(crate) fn destination_ready(&mut self) {
        if self.active() {
            self.phase = Phase::DestinationReady;
        }
    }

    pub(crate) fn loading_started(&mut self) {
        if self.active() {
            self.phase = Phase::Loading;
        }
    }

    pub(crate) fn focus_lost(&mut self) {
        self.restore_capture = false;
    }

    pub(crate) fn take_restore_capture(&mut self) -> bool {
        !self.active() && std::mem::take(&mut self.restore_capture)
    }

    pub(crate) fn cancel(&mut self) {
        self.pending = None;
        self.tip = None;
        if let Some(Artwork::Stage(stage)) = self.artwork.take() {
            self.retired_stage = Some(stage);
        }
        self.phase = Phase::Idle;
    }

    /// A stage that ended its cover and awaits entity despawn + GPU
    /// release. Drained by the App's per-frame poll.
    pub(crate) fn take_retired_stage(&mut self) -> Option<Box<ModelStage>> {
        self.retired_stage.take()
    }

    /// Call only after a real submitted frame (not an out-of-date/zero-size
    /// early return). Keep the cover through one coherent destination frame.
    pub(crate) fn frame_presented(&mut self, geometry_ready: bool) {
        match self.phase {
            Phase::AwaitingPresentation => {
                self.phase = Phase::Presented;
                log::info!("loading.screen: presented before scene teardown");
            }
            Phase::DestinationReady if geometry_ready => {
                self.cancel();
                log::info!("loading.screen: dismissed after destination frame");
            }
            _ => {}
        }
    }
}

// ── stage helpers ──────────────────────────────────────────────────────

/// The authored Creation-era stage pose in the engine's Y-up space.
/// Skyrim authors it inline (SNAM scale / RNAM i16 degrees / XNAM
/// translation); FO4 points TNAM at a TRNS record carrying the same
/// triple as floats. Both are Z-up menu-space and go through the same
/// REFR coordinate conversion the cell loader uses.
pub(crate) struct StagePose {
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) scale: f32,
}

fn stage_pose(screen: &LoadScreenRecord, transform: Option<&LoadScreenTransform>) -> StagePose {
    let (translation_zup, rotation_deg, scale) = match transform {
        Some(trns) => (trns.translation, trns.rotation_deg, trns.scale),
        None => (
            screen.initial_translation.unwrap_or([0.0; 3]),
            screen
                .initial_rotation
                .map(|r| [r[0] as f32, r[1] as f32, r[2] as f32])
                .unwrap_or([0.0; 3]),
            screen.initial_scale.unwrap_or(1.0),
        ),
    };
    StagePose {
        translation: Vec3::from_array(coord::zup_to_yup_pos(translation_zup)),
        rotation: crate::cell_loader::euler_zup_to_quat_yup_refr(
            rotation_deg[0].to_radians(),
            rotation_deg[1].to_radians(),
            rotation_deg[2].to_radians(),
        ),
        scale,
    }
}

/// Compose the authored stage pose onto the NIF root's own local
/// transform (usually identity, but never assumed), so no authored
/// root-node transform is discarded.
fn pose_root(world: &mut World, root: EntityId, pose: &StagePose) {
    let Some(mut tq) = world.query_mut::<Transform>() else {
        return;
    };
    let Some(transform) = tq.get_mut(root) else {
        return;
    };
    let (pos, rot, scale) = byroredux_core::ecs::GlobalTransform::compose_trs(
        pose.translation,
        pose.rotation,
        pose.scale,
        transform.translation,
        transform.rotation,
        transform.scale,
    );
    transform.translation = pos;
    transform.rotation = rot;
    transform.scale = scale;
}

/// Every mesh handle in the stage subtree, via the same Children walk the
/// cell teardown uses.
fn collect_stage_mesh_handles(world: &World, root: EntityId) -> std::collections::HashSet<u32> {
    let children = world.query::<Children>();
    let meshes = world.query::<MeshHandle>();
    let mut handles = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(meshes) = &meshes {
            if let Some(handle) = meshes.get(entity) {
                handles.insert(handle.0);
            }
        }
        if let Some(children) = &children {
            if let Some(row) = children.get(entity) {
                stack.extend(row.0.iter().copied());
            }
        }
    }
    handles
}

/// Entity list of a whole subtree, root included. Does not despawn — the
/// retirement path must read component state (GPU handles) before the
/// entities go away.
fn collect_subtree(world: &World, root: EntityId) -> Vec<EntityId> {
    let children = world.query::<Children>();
    let mut stack = vec![root];
    let mut victims = Vec::new();
    while let Some(entity) = stack.pop() {
        victims.push(entity);
        if let Some(children) = &children {
            if let Some(row) = children.get(entity) {
                stack.extend(row.0.iter().copied());
            }
        }
    }
    victims
}

/// Despawn a whole subtree without GPU-release bookkeeping — the spawn
/// failure paths use this before any mesh was registered.
fn despawn_subtree(world: &mut World, root: EntityId) {
    let victims = collect_subtree(world, root);
    world.despawn_batch(victims);
}

/// AABB of the local-space bounds after the stage pose (all eight corners
/// transformed, re-enclosed).
fn posed_bounds(pose: &StagePose, (min, max): ([f32; 3], [f32; 3])) -> ([f32; 3], [f32; 3]) {
    let mut out_min = [f32::MAX; 3];
    let mut out_max = [f32::MIN; 3];
    for ix in [min[0], max[0]] {
        for iy in [min[1], max[1]] {
            for iz in [min[2], max[2]] {
                let p = pose.rotation * (Vec3::new(ix, iy, iz) * pose.scale) + pose.translation;
                for axis in 0..3 {
                    out_min[axis] = out_min[axis].min(p[axis]);
                    out_max[axis] = out_max[axis].max(p[axis]);
                }
            }
        }
    }
    (out_min, out_max)
}

fn posed_center_radius(posed: &([f32; 3], [f32; 3])) -> (Vec3, f32) {
    let (min, max) = posed;
    let center = Vec3::new(
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    );
    let radius = (Vec3::from_array(*max) - Vec3::from_array(*min)).length() * 0.5;
    (center, radius)
}

/// Fixed framing camera: eye on a shallow-elevation arc in front of the
/// posed centre, at a distance fitting the bounds sphere into the
/// vertical FOV with [`STAGE_FRAME_MARGIN`]. `None` for a zero-size or
/// non-finite bounds pair.
fn stage_camera(posed: &([f32; 3], [f32; 3]), fov_y: f32) -> Option<StageCamera> {
    let (center, radius) = posed_center_radius(posed);
    if !radius.is_finite() || radius <= f32::EPSILON || fov_y <= f32::EPSILON {
        return None;
    }
    let distance = (radius / (fov_y * 0.5).tan()) * STAGE_FRAME_MARGIN;
    let eye = center
        + Vec3::new(
            0.0,
            distance * STAGE_CAMERA_ELEVATION_RAD.sin(),
            distance * STAGE_CAMERA_ELEVATION_RAD.cos(),
        );
    Some(StageCamera { eye, target: center })
}

fn active_fov_y(world: &World) -> Option<f32> {
    let active = world.try_resource::<byroredux_core::ecs::ActiveCamera>()?;
    let cam_entity = active.0;
    drop(active);
    let cq = world.query::<byroredux_core::ecs::Camera>()?;
    cq.get(cam_entity).map(|camera| camera.fov_y)
}

impl ModelStage {
    /// Retain only this stage's draws while the cover owns the frame: the
    /// destination cell streams in behind the cover and must not leak a
    /// half-built frame. Water rides the same rule (no stage water).
    pub(crate) fn filter_frame_draws(
        &self,
        draw_commands: &mut Vec<DrawCommand>,
        water_commands: &mut Vec<byroredux_renderer::vulkan::water::WaterDrawCommand>,
    ) {
        draw_commands.retain(|command| self.mesh_handles.contains(&command.mesh_handle));
        water_commands.clear();
    }

    /// Keep only the model's own embedded lights (inside the posed
    /// bounds) — streaming destination lights must not flare the stage.
    pub(crate) fn keeps_light(&self, light: &GpuLight) -> bool {
        let p = Vec3::new(
            light.position_radius[0],
            light.position_radius[1],
            light.position_radius[2],
        );
        (p - self.center).length() <= self.light_radius * 1.5
    }

    /// Take over the frame camera: reuse the live projection (aspect and
    /// FOV stay correct through window resizes) with the stage's own
    /// view. The stage sits near the world origin, so the render origin
    /// is exactly zero — no precision rebasing needed.
    pub(crate) fn apply_to_frame(&self, frame: &mut RenderFrameView) {
        let forward = (self.camera.target - self.camera.eye).normalize_or_zero();
        let mut up = Vec3::Y;
        if forward.cross(up).length_squared() < 1e-8 {
            up = Vec3::X;
        }
        let view = Mat4::look_at_rh(self.camera.eye, self.camera.target, up);
        let proj = Mat4::from_cols_array(&frame.proj_mat);
        frame.view_proj = (proj * view).to_cols_array();
        frame.camera_pos = self.camera.eye.to_array();
        frame.render_origin = [0.0; 3];
        frame.cam_forward = forward.to_array();
        let right = forward.cross(up).normalize_or_zero();
        let cam_up = right.cross(forward).normalize_or_zero();
        frame.cam_right = right.to_array();
        frame.cam_up = cam_up.to_array();
        frame.ambient = STAGE_AMBIENT;
        frame.fog_medium = FogMedium::default();
        frame.fog_color = [0.0; 3];
        frame.fog_near = frame.camera_far;
        frame.fog_far = frame.camera_far;
        frame.fog_clip = 0.0;
        frame.fog_power = 0.0;
        frame.sky = SkyParams::default();
        frame.aperture = 0.0;
        frame.focus_dist = (self.camera.target - self.camera.eye).length();
    }
}

/// Despawn a retired stage's subtree and release its GPU state — the same
/// refcount/BLAS/texture discipline as cell teardown, minus the cell
/// machinery. Runs on the App poll (and defensively at re-begin). A `None`
/// renderer (teardown races) still despawns the entities.
pub(crate) fn retire_stage(
    world: &mut World,
    ctx: Option<&mut VulkanContext>,
    stage: ModelStage,
) {
    let victims = collect_subtree(world, stage.root);
    let Some(ctx) = ctx else {
        world.despawn_batch(victims);
        return;
    };
    let fallback_tex = ctx.texture_registry.fallback();
    let (mesh_drops, texture_drops, _terrain_slots) =
        crate::cell_loader::collect_victim_gpu_handles(world, &victims, fallback_tex);
    world.despawn_batch(victims);
    // Mirror cell teardown: BLAS drops exactly when the last holder goes.
    let mut handle_drop_count: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();
    for &mh in &mesh_drops {
        *handle_drop_count.entry(mh).or_insert(0) += 1;
    }
    let freed: Vec<u32> = handle_drop_count
        .iter()
        .filter_map(|(&h, &c)| match ctx.mesh_registry.refcount(h) {
            Some(rc) if rc == c => Some(h),
            _ => None,
        })
        .collect();
    if let Some(ref mut accel) = ctx.accel_manager {
        for &mh in &freed {
            accel.drop_blas(mh);
        }
    }
    ctx.mesh_registry.drop_meshes(&mesh_drops);
    ctx.texture_registry
        .drop_textures(&ctx.device, &texture_drops);
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::EsmIndex;

    #[test]
    fn cover_requires_presentation_and_a_ready_destination_frame() {
        let mut screen = LoadingScreen {
            phase: Phase::AwaitingPresentation,
            ..Default::default()
        };
        assert!(screen.take_presented_transition().is_none());
        assert!(screen.waiting_for_presentation());
        screen.frame_presented(false);
        assert_eq!(screen.phase, Phase::Presented);
        screen.take_presented_transition();
        assert_eq!(screen.phase, Phase::Loading);
        screen.frame_presented(true);
        assert!(screen.active());
        screen.destination_ready();
        screen.frame_presented(false);
        assert!(screen.active());
        screen.frame_presented(true);
        assert!(!screen.active());
    }

    #[test]
    fn superseding_a_ready_destination_keeps_the_cover_until_new_completion() {
        let mut screen = LoadingScreen {
            phase: Phase::DestinationReady,
            ..Default::default()
        };
        screen.loading_started();
        screen.frame_presented(true);
        assert!(screen.active());
        screen.destination_ready();
        screen.frame_presented(true);
        assert!(!screen.active());
    }

    #[test]
    fn save_cover_has_a_separate_once_only_presentation_gate() {
        let mut screen = LoadingScreen {
            phase: Phase::AwaitingPresentation,
            owner: Owner::Save,
            ..Default::default()
        };
        assert!(screen.owns_save());
        assert!(!screen.take_presented_save());
        screen.frame_presented(false);
        assert!(screen.take_presented_transition().is_none());
        assert_eq!(screen.phase, Phase::Presented);
        assert!(screen.take_presented_save());
        assert!(!screen.take_presented_save());
        screen.frame_presented(true);
        assert!(screen.owns_save());
        // Either successful synchronous drain or early failure returns to
        // a world frame; neither may strand input behind the loading cover.
        screen.destination_ready();
        screen.frame_presented(false);
        assert!(screen.owns_save());
        screen.frame_presented(true);
        assert!(!screen.owns_save());
    }

    #[test]
    fn save_drain_cannot_consume_a_door_cover() {
        let mut screen = LoadingScreen {
            phase: Phase::Presented,
            ..Default::default()
        };
        assert!(!screen.owns_save());
        assert!(!screen.take_presented_save());
        assert_eq!(screen.phase, Phase::Presented);
    }

    #[test]
    fn cancellation_restores_capture_once_but_focus_loss_never_recaptures() {
        let mut screen = LoadingScreen {
            phase: Phase::Loading,
            restore_capture: true,
            ..Default::default()
        };
        assert!(!screen.take_restore_capture());
        screen.cancel();
        assert!(screen.take_restore_capture());
        assert!(!screen.take_restore_capture());
        screen.phase = Phase::Loading;
        screen.restore_capture = true;
        screen.focus_lost();
        screen.cancel();
        assert!(!screen.take_restore_capture());
    }

    #[test]
    fn cancel_retires_a_model_stage_exactly_once() {
        let mut screen = LoadingScreen {
            phase: Phase::Loading,
            artwork: Some(Artwork::Stage(Box::new(ModelStage {
                key: "k".into(),
                root: 7,
                mesh_handles: [11u32].into(),
                camera: StageCamera {
                    eye: Vec3::ZERO,
                    target: Vec3::ZERO,
                },
                center: Vec3::ZERO,
                light_radius: 1.0,
            }))),
            ..Default::default()
        };
        assert!(screen.active_stage().is_some());
        screen.cancel();
        assert!(screen.active_stage().is_none());
        assert!(screen.take_retired_stage().is_some());
        assert!(screen.take_retired_stage().is_none());
    }

    #[test]
    fn stage_pose_converts_the_authored_triple_to_yup() {
        let screen_record = LoadScreenRecord {
            initial_scale: Some(2.0),
            initial_translation: Some([10.0, 20.0, 30.0]),
            ..Default::default()
        };
        let pose = stage_pose(&screen_record, None);
        // [x, z, -y] Z-up→Y-up, same as every REFR placement.
        assert_eq!(pose.translation, Vec3::new(10.0, 30.0, -20.0));
        assert_eq!(pose.scale, 2.0);
    }

    #[test]
    fn trns_transform_wins_over_inline_skyrim_defaults() {
        let screen_record = LoadScreenRecord {
            initial_scale: Some(9.0),
            ..Default::default()
        };
        let trns = LoadScreenTransform {
            form_id: 1,
            editor_id: "t".into(),
            around_origin: false,
            translation: [1.0, 2.0, 3.0],
            rotation_deg: [90.0, 0.0, 0.0],
            scale: 0.5,
            zoom_bounds: None,
            malformed_fields: Vec::new(),
        };
        let pose = stage_pose(&screen_record, Some(&trns));
        assert_eq!(pose.translation, Vec3::new(1.0, 3.0, -2.0));
        assert_eq!(pose.scale, 0.5);
    }

    #[test]
    fn posed_bounds_enclose_the_rotated_corners() {
        let pose = StagePose {
            translation: Vec3::new(100.0, 0.0, 0.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            scale: 1.0,
        };
        let (min, max) = posed_bounds(&pose, ([-10.0, -1.0, -2.0], [10.0, 1.0, 2.0]));
        // A +90° Y rotation maps the local X extent (±10) onto Z and the
        // local Z extent (±2) onto X, then the translation shifts +X.
        assert!((min[0] - 98.0).abs() < 1e-4);
        assert!((max[0] - 102.0).abs() < 1e-4);
        assert!((min[2] + 10.0).abs() < 1e-4);
        assert!((max[2] - 10.0).abs() < 1e-4);
        assert!((min[1] + 1.0).abs() < 1e-5);
        assert!((max[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn stage_camera_fits_the_bounds_sphere_into_the_fov() {
        let posed = ([-10.0, -10.0, -10.0], [10.0, 10.0, 10.0]);
        let camera = stage_camera(&posed, std::f32::consts::FRAC_PI_2).unwrap();
        let distance = (camera.eye - Vec3::ZERO).length();
        // radius = 10·√3 ≈ 17.32; margin 1.35 × radius / tan(45°).
        assert!((distance - 17.32 * STAGE_FRAME_MARGIN).abs() < 0.1);
        assert!(camera.eye.y > 0.0, "camera sits on a downward arc");
        assert!(camera.eye.z > 0.0);
    }

    #[test]
    fn stage_camera_rejects_degenerate_bounds() {
        assert!(stage_camera(&([0.0; 3], [0.0; 3]), 1.0).is_none());
        assert!(stage_camera(&([-1.0; 3], [1.0; 3]), 0.0).is_none());
    }

    #[test]
    fn selector_never_widens_restricted_or_malformed_screens() {
        let mut index = EsmIndex {
            game: byroredux_plugin::esm::reader::GameKind::Oblivion,
            ..Default::default()
        };
        index.load_screens.insert(
            1,
            LoadScreenRecord {
                form_id: 1,
                icon: "test.dds".into(),
                ..Default::default()
            },
        );
        assert!(first_backend_load_screen(&index).is_some());
        index
            .load_screens
            .get_mut(&1)
            .unwrap()
            .malformed_fields
            .push(*b"LNAM");
        assert!(first_backend_load_screen(&index).is_none());
        let record = index.load_screens.get_mut(&1).unwrap();
        record.malformed_fields.clear();
        record.conditions.push(Default::default());
        assert!(first_backend_load_screen(&index).is_none());
        index.load_screens.get_mut(&1).unwrap().conditions.clear();
        index.load_screens.get_mut(&1).unwrap().locations.push(
            byroredux_plugin::esm::records::LoadScreenLocation {
                direct: 123,
                world: 0,
                grid_x: 0,
                grid_y: 0,
            },
        );
        assert!(first_backend_load_screen(&index).is_none());
        index.load_screens.get_mut(&1).unwrap().locations.clear();
        index.game = byroredux_plugin::esm::reader::GameKind::Skyrim;
        assert!(
            first_backend_load_screen(&index).is_none(),
            "an ICON-only record is not Skyrim artwork"
        );
    }

    #[test]
    fn skyrim_model_resolves_through_statics_and_a_broken_trns_rejects() {
        let mut index = EsmIndex {
            game: byroredux_plugin::esm::reader::GameKind::Fallout4,
            ..Default::default()
        };
        index.load_screens.insert(
            5,
            LoadScreenRecord {
                form_id: 5,
                description: "tip".into(),
                model: 9,
                transform: 0,
                ..Default::default()
            },
        );
        // NNAM with no STAT behind it: rejected.
        assert!(first_backend_load_screen(&index).is_none());
        index.cells.statics.insert(
            9,
            byroredux_plugin::esm::cell::StaticObject {
                form_id: 9,
                editor_id: "s".into(),
                model_path: "LoadScreenArt\\Model.nif".into(),
                record_type: byroredux_plugin::record::RecordType(*b"STAT"),
                light_data: None,
                addon_data: None,
                has_script: false,
                script_instance: None,
                script_form_id: 0,
                visible_when_distant: false,
            },
        );
        let (_, verdict) = first_backend_load_screen(&index).unwrap();
        let LoadScreenVerdict::Model(Some(resolved)) = verdict else {
            panic!("expected a resolved model, got {verdict:?}");
        };
        assert_eq!(resolved.model_path, "LoadScreenArt\\Model.nif");
        // A TNAM whose TRNS is missing: the whole screen is ineligible
        // rather than posed at a guess.
        index.load_screens.get_mut(&5).unwrap().transform = 77;
        assert!(first_backend_load_screen(&index).is_none());
        index.load_screen_transforms.insert(
            77,
            LoadScreenTransform {
                form_id: 77,
                editor_id: String::new(),
                around_origin: false,
                translation: [1.0, 2.0, 3.0],
                rotation_deg: [0.0; 3],
                scale: 1.0,
                zoom_bounds: None,
                malformed_fields: Vec::new(),
            },
        );
        let (_, verdict) = first_backend_load_screen(&index).unwrap();
        assert!(matches!(verdict, LoadScreenVerdict::Model(Some(_))));
    }
}
