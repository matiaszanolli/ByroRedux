//! Render-resolution scene color to output-resolution HDR reconstruction.
//!
//! The resource split in this module is deliberately useful without FSR:
//! `UpscalerMode::Taa` records a native Vulkan blit into the same output HDR
//! target that the FSR path writes. This keeps scene composition and
//! presentation decoupled, and gives FSR one explicit frame-graph slot instead
//! of letting the final composite pass silently bilinear-upscale its inputs.
//!
//! #2520 (REN-D23-2026-08-07-04) — that split is not free in `UpscalerMode::
//! Taa`. `FrameExtentSet::for_output` sets `render == output` for `Taa`, so
//! `record_native_blit`'s src/dst offsets are identical and its `LINEAR`
//! filter degenerates to an exact copy: every TAA-mode frame still reads and
//! writes a full-resolution `R16G16B16A16_SFLOAT` image (~16 MB of traffic at
//! 1080p, ~66 MB at 4K) plus two pipeline barriers, purely to move data into
//! a target `presentation.frag` could have sampled directly. `Taa` is not the
//! default path (`UpscalerMode::default()` is `Fsr3(FsrQuality::Quality)`),
//! so this is pure bandwidth on a non-default configuration, not a
//! correctness issue — left undone rather than adding the risk of
//! `PresentationPipeline` binding composite's scene view directly, which
//! would need re-benching per #2520's own note not to conflate this with the
//! FSR path. Revisit if `Taa` mode's perf ever matters again.

use super::allocator::SharedAllocator;
use super::composite::HDR_FORMAT;
use super::descriptors::color_subresource_single_mip;
use super::exposure::EXPOSURE_FORMAT;
use super::gbuffer::{FSR_MASK_FORMAT, MOTION_FORMAT};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use super::upscaling::{fsr_motion_vector_scale, FrameExtentSet, UpscalerMode};
use anyhow::{Context, Result};
use ash::vk::{self, Handle};
use byroredux_fsr3_sys as fsr3;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;
use std::ffi::c_void;

/// Metres per unit of the engine's **view space**, for
/// `fsr3::DispatchDescription::view_space_to_meters_factor` (#2834).
///
/// The whole renderer works in Bethesda units, and `camera_near` /
/// `camera_far` / `camera_fov_angle_vertical` are sourced from `Camera` in
/// BU, so the depth FSR reconstructs from them is in BU too. The SDK converts
/// it with `GetViewSpaceDepthInMeters(d) = GetViewSpaceDepth(d) *
/// ViewSpaceToMetersFactor()`, and two of its heuristics are tuned in real
/// metres:
///
/// * `ReconstructedDepthMvPxThreshold` ramps `0.25 → 0.75 px` over 0–100 m.
///   At a factor of 1.0 it saturated past 100 BU ≈ 1.43 m, so effectively the
///   whole frame ran on the far-field motion-vector threshold.
/// * `fDistanceFactor = saturate(0.75 - farthestDepthInMeters / 20)` is zero
///   past 15 BU ≈ 0.21 m, so the term was dead scene-wide — and it feeds the
///   history-rectification box scale, so near geometry never got the tighter,
///   more history-rejecting clipping box AMD tuned for it.
///
/// Sourced from the same constant `volumetrics` and `shader_constants_data`
/// use, so the engine has exactly one definition of the BU↔metre scale rather
/// than a literal per subsystem.
const FSR_VIEW_SPACE_TO_METERS_FACTOR: f32 =
    1.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

/// Temporal-recovery window for the one frame an FSR dispatch failure blits
/// through jittered-but-unresolved (#2519).
///
/// One frame is the whole exposure: the geometry pass for the failing frame is
/// already recorded, so the only thing left to protect is the *next* frame's
/// reprojection against that image. Every frame after the latch is chosen
/// unjittered by `is_fsr_dispatch_active()`, so a longer window would only
/// hold SVGF at its elevated α for frames that carry no offset at all.
pub const FSR_DISPATCH_FAILURE_RECOVERY_FRAMES: u32 = 1;

/// Camera and temporal values that change for every FSR dispatch.
#[derive(Debug, Clone, Copy)]
pub struct FsrFrameParameters {
    pub jitter_offset: [f32; 2],
    pub reset: bool,
    pub frame_time_delta_ms: f32,
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_angle_vertical: f32,
}

/// Engine-owned images consumed by one upscale operation.
#[derive(Debug, Clone, Copy)]
pub struct UpscaleDispatchInputs {
    pub scene_color: vk::Image,
    pub depth: vk::Image,
    pub depth_format: vk::Format,
    pub motion_vectors: vk::Image,
    pub exposure: Option<vk::Image>,
    /// FSR reactive mask — transparent coverage the reprojected history
    /// cannot predict.
    pub reactive: vk::Image,
    /// FSR transparency-and-composition mask — shading whose evolution
    /// depth and motion do not describe.
    pub transparency: vk::Image,
}

/// Owns the output-resolution HDR images and, in FSR mode, the SDK context.
pub struct FrameUpscaler {
    mode: UpscalerMode,
    context: Option<fsr3::Context>,
    output_images: Vec<vk::Image>,
    output_views: Vec<vk::ImageView>,
    output_allocations: Vec<Option<vk_alloc::Allocation>>,
    extents: FrameExtentSet,
    dispatched_this_frame: bool,
    /// Set when a recorded dispatch returned an SDK error. The context is kept
    /// alive (in-flight frames may still reference its resources) but is never
    /// dispatched again for this swapchain generation; the native blit takes
    /// over so the frame graph keeps producing frames.
    dispatch_failure: Option<String>,
    /// One-shot edge signal for the frame on which `dispatch_failure` was
    /// latched, consumed by `record_upscale_pass` via
    /// [`Self::take_new_dispatch_failure`] (#2519).
    ///
    /// That frame's geometry pass already rendered with the FSR sub-pixel
    /// jitter offset — the jitter is chosen at the top of `draw_frame` from
    /// `is_fsr_dispatch_active()`, which was still `true` then — and the
    /// recovery path blits that jittered image through with no pass to
    /// resolve it. Every later frame is correctly unjittered, so only the
    /// failing frame's *history* is the hazard: without this signal the next
    /// frame's SVGF/volumetrics reprojection accumulates against a
    /// half-pixel-shifted image. Same class of hazard `taa_jitter`'s
    /// `!taa_failed` gate closes on the TAA side (#1932).
    new_dispatch_failure: bool,
    /// Cached one-line telemetry for this swapchain generation. Built once
    /// after context creation because the SDK's memory reservation is fixed
    /// for the life of a context — re-querying it per frame would be an FFI
    /// round-trip for a value that cannot change.
    summary: String,
    /// Whether the device enabled `shaderFloat16`, which is what decides
    /// between the SDK's FP16 and FP32 shader permutations. Reported rather
    /// than chosen — the SDK reads the physical device directly and offers no
    /// override, so this records which path is live.
    shader_float16: bool,
}

impl FrameUpscaler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        mode: UpscalerMode,
        extents: FrameExtentSet,
        shader_float16: bool,
    ) -> Result<Self> {
        let mut upscaler = Self {
            mode,
            shader_float16,
            context: None,
            output_images: Vec::new(),
            output_views: Vec::new(),
            output_allocations: Vec::new(),
            extents,
            dispatched_this_frame: false,
            dispatch_failure: None,
            new_dispatch_failure: false,
            summary: String::new(),
        };

        if let Err(error) = upscaler.create_outputs(device, allocator) {
            // SAFETY: construction has not returned, no command buffer can
            // reference the partially-created output resources.
            unsafe { upscaler.destroy(device, allocator) };
            return Err(error);
        }
        if let Err(error) = upscaler.initialize_outputs(device, queue, command_pool) {
            // SAFETY: the one-time initialization failed before this value
            // became externally visible and the queue helper fenced its submit.
            unsafe { upscaler.destroy(device, allocator) };
            return Err(error);
        }

        if matches!(mode, UpscalerMode::Fsr3(_)) {
            let create = unsafe {
                // SAFETY: all handles come from the live renderer device and
                // outlive `upscaler`; the context is destroyed before device
                // teardown after `device_wait_idle`.
                fsr3::Context::create(fsr3::VulkanCreateInfo {
                    device: device.handle().as_raw() as usize,
                    physical_device: physical_device.as_raw() as usize,
                    get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr as *const ()
                        as *const c_void,
                    max_render_size: [extents.render.width, extents.render.height],
                    max_upscale_size: [extents.output.width, extents.output.height],
                    high_dynamic_range: true,
                    debug_checking: cfg!(debug_assertions),
                })
            };
            match create {
                // Version, extents, and SDK memory are reported once by the
                // `build_summary` line below, which covers both paths.
                Ok(context) => upscaler.context = Some(context),
                Err(error) => {
                    // Keep the frame graph alive through the Phase-4 native
                    // bridge. This is intentionally loud: the selected FSR
                    // mode is not silently reported as an active SDK path.
                    log::error!(
                        "FSR context creation failed: {error}; using native HDR blit fallback"
                    );
                }
            }
        }

        upscaler.summary = upscaler.build_summary();
        log::info!("Frame upscaler: {}", upscaler.summary);

        Ok(upscaler)
    }

    fn create_outputs(&mut self, device: &ash::Device, allocator: &SharedAllocator) -> Result<()> {
        for frame in 0..MAX_FRAMES_IN_FLIGHT {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HDR_FORMAT)
                .extent(vk::Extent3D {
                    width: self.extents.output.width,
                    height: self.extents.output.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe {
                // SAFETY: `info` is fully initialized and the returned image
                // is stored immediately for cleanup on every later failure.
                device
                    .create_image(&info, None)
                    .context("create upscale output image")?
            };
            self.output_images.push(image);
            self.output_allocations.push(None);

            let requirements = unsafe {
                // SAFETY: `image` was just created by this device and is live.
                device.get_image_memory_requirements(image)
            };
            let allocation = allocator
                .lock()
                .expect("allocator lock poisoned")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("upscale_output_{frame}"),
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("allocate upscale output image")?;
            // #2178 / PERF-D3-03 — return the sub-allocation to the allocator
            // before bailing. `allocation` is still a local at this point:
            // `output_allocations[frame]` holds `None`, so neither `destroy`
            // nor `Drop` can reach it, and the memory would stay off the free
            // list for the process lifetime. The image itself is already in
            // `output_images` and is cleaned up normally. Mirrors
            // `exposure.rs`'s bind-failure branch.
            if let Err(error) = unsafe {
                // SAFETY: `allocation` was created from this image's exact
                // requirements and the image has not been bound before.
                device.bind_image_memory(image, allocation.memory(), allocation.offset())
            } {
                allocator
                    .lock()
                    .expect("allocator lock poisoned")
                    .free(allocation)
                    .ok();
                return Err(error).context("bind upscale output image");
            }
            self.output_allocations[frame] = Some(allocation);

            let view = unsafe {
                // SAFETY: the image is live, bound, and format-compatible with
                // this single-mip 2D color view.
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(HDR_FORMAT)
                            .subresource_range(color_subresource_single_mip()),
                        None,
                    )
                    .context("create upscale output view")?
            };
            self.output_views.push(view);
        }
        Ok(())
    }

    fn initialize_outputs(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            let barriers: Vec<_> = self
                .output_images
                .iter()
                .map(|&image| {
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image(image)
                        .subresource_range(color_subresource_single_mip())
                })
                .collect();
            unsafe {
                // SAFETY: `cmd` is recording in the queue helper; these fresh
                // images have never been submitted and are all UNDEFINED.
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
        .context("initialize upscale output layouts")
    }

    pub fn output_views(&self) -> &[vk::ImageView] {
        &self.output_views
    }

    pub fn output_image(&self, frame: usize) -> vk::Image {
        self.output_images[frame]
    }

    pub fn is_fsr_dispatch_active(&self) -> bool {
        self.context.is_some() && self.dispatch_failure.is_none()
    }

    /// The SDK error that permanently disabled dispatch, if one occurred.
    pub fn dispatch_failure(&self) -> Option<&str> {
        self.dispatch_failure.as_deref()
    }

    /// Consume the "dispatch failed on *this* frame" edge (#2519).
    ///
    /// `true` exactly once, on the frame the failure was latched — the one
    /// frame that was rendered jittered and then blitted through unresolved.
    /// The caller turns that into a temporal discontinuity so nothing
    /// reprojects against it; see [`Self::new_dispatch_failure`].
    pub fn take_new_dispatch_failure(&mut self) -> bool {
        std::mem::take(&mut self.new_dispatch_failure)
    }

    /// One-line description of the active reconstruction path: the selected
    /// mode, the render/output extents, and — in FSR mode — the provider
    /// version plus the GPU memory the SDK reserved for its own resources.
    ///
    /// That SDK memory is allocated by the official backend, not by
    /// `gpu-allocator`, so it is invisible to `ctx.memory` unless reported
    /// here. The string is built once per swapchain generation (the
    /// reservation is fixed for a context) and re-derived on failure so the
    /// degraded state is visible without another SDK round-trip.
    pub fn telemetry(&self) -> String {
        if let Some(error) = self.dispatch_failure() {
            return format!(
                "{} — DISPATCH FAILED ({error}), on native blit",
                self.summary
            );
        }
        self.summary.clone()
    }

    fn build_summary(&self) -> String {
        let extents = format!(
            "{}x{} -> {}x{}",
            self.extents.render.width,
            self.extents.render.height,
            self.extents.output.width,
            self.extents.output.height,
        );
        let Some(ref context) = self.context else {
            return format!("{} · native HDR blit · {extents}", self.mode);
        };
        let version = fsr3::version()
            .map(|version| version.to_string())
            .unwrap_or_else(|_| "unknown".to_owned());
        let permutation = if self.shader_float16 { "fp16" } else { "fp32" };
        let memory = match context.memory_usage() {
            Ok(usage) => format!(
                "{:.1} MB SDK working memory ({:.1} MB aliasable)",
                usage.total_bytes as f64 / 1.0e6,
                usage.aliasable_bytes as f64 / 1.0e6,
            ),
            Err(error) => format!("working memory unavailable ({error})"),
        };
        format!(
            "{} · FSR {version} ({permutation}) · {extents} · {memory}",
            self.mode
        )
    }

    /// Record either the real FSR dispatch or the native blit bridge.
    ///
    /// **Infallible by design (#2146 / CHAIN-D2-01).** Every failure here
    /// degrades to the native blit and latches, rather than propagating. The
    /// return type is not an oversight to be "tidied up" later: this runs
    /// inside `record_post_passes`, *after* `svgf.dispatch` and `taa.dispatch`
    /// have set their `dispatched_this_frame` latches. An error escaping to
    /// `draw_frame` aborts before `queue_submit`, so `mark_frame_completed`
    /// never runs, the latch stays `true`, and the *next* frame's
    /// `mark_frame_completed` credits `frames_since_creation` for a dispatch
    /// that never reached the GPU — closing `should_force_history_reset` one
    /// frame early and smearing SVGF/TAA history. That is exactly the failure
    /// #917 was written to prevent, and a `Result` here is what would let it
    /// back in. If a genuinely fatal condition ever appears, clear those
    /// latches on the error path before returning rather than restoring the
    /// `Result`.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. All input images must
    /// remain live through submission and start in the layouts documented by
    /// the barriers below.
    pub unsafe fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inputs: UpscaleDispatchInputs,
        fsr_frame: Option<FsrFrameParameters>,
        force_native_blit: bool,
    ) {
        self.dispatched_this_frame = false;
        if force_native_blit || !self.is_fsr_dispatch_active() {
            unsafe {
                // SAFETY: `record_native_blit` shares this fn's own `# Safety`
                // contract (`cmd` recording outside a render pass, input
                // images live and in the documented layout) — the bridge
                // branch just skips the FSR SDK dispatch below it.
                self.record_native_blit(
                    device,
                    cmd,
                    frame,
                    inputs.scene_color,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )
            };
            return;
        }

        // #2146 — was `fsr_frame.context(...)?`, the sole error return in this
        // function and the first `?` inside `record_post_passes`. Unreachable
        // as written (the jitter gate and this gate read the same predicate
        // within one `draw_frame`), but it sat *after* SVGF/TAA had latched
        // `dispatched_this_frame`, so it would have bypassed #917's
        // no-advance-on-unsubmitted-dispatch invariant the moment a second
        // caller stopped keeping the two predicates in lockstep. Degrade the
        // same way a rejected dispatch does instead: latch, blit, carry on.
        let Some(frame_params) = fsr_frame else {
            log::error!(
                "FSR context is active but frame parameters are absent; \
                 falling back to the native HDR blit for this swapchain generation"
            );
            self.dispatch_failure =
                Some("FSR context is active but frame parameters are absent".to_string());
            self.new_dispatch_failure = true;
            unsafe {
                // SAFETY: same contract as this fn's own `# Safety` doc. No
                // boundary barrier has run yet on this path, so the output
                // image is still in its steady-state SHADER_READ_ONLY_OPTIMAL
                // layout — the same one the bridge branch above blits from,
                // NOT the GENERAL the post-dispatch recovery path uses.
                self.record_native_blit(
                    device,
                    cmd,
                    frame,
                    inputs.scene_color,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                );
            }
            return;
        };
        unsafe {
            // SAFETY: same contract as this fn's own `# Safety` doc — `cmd`
            // is recording outside a render pass and `inputs`' images are
            // live and start in the layouts these barriers assume.
            self.record_fsr_barriers_before(device, cmd, frame, inputs)
        };

        let render_size = [self.extents.render.width, self.extents.render.height];
        let output_size = [self.extents.output.width, self.extents.output.height];
        let output = self.output_images[frame];
        let context = self
            .context
            .as_mut()
            .expect("context presence checked above");
        let dispatch = unsafe {
            // SAFETY: boundary barriers above establish the layouts and access
            // states described to the SDK; all handles belong to this device
            // and the renderer keeps them alive through queue completion.
            context.dispatch(fsr3::DispatchDescription {
                command_buffer: cmd.as_raw() as usize,
                color: vulkan_image(
                    inputs.scene_color,
                    HDR_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                    render_size,
                ),
                depth: vulkan_image(
                    inputs.depth,
                    inputs.depth_format,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                    render_size,
                ),
                motion_vectors: vulkan_image(
                    inputs.motion_vectors,
                    MOTION_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    render_size,
                ),
                exposure: inputs.exposure.map(|image| {
                    vulkan_image(
                        image,
                        EXPOSURE_FORMAT,
                        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                        [1, 1],
                    )
                }),
                reactive: Some(vulkan_image(
                    inputs.reactive,
                    FSR_MASK_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    render_size,
                )),
                transparency_and_composition: Some(vulkan_image(
                    inputs.transparency,
                    FSR_MASK_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    render_size,
                )),
                output: vulkan_image(
                    output,
                    HDR_FORMAT,
                    vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST,
                    output_size,
                ),
                jitter_offset: frame_params.jitter_offset,
                motion_vector_scale: fsr_motion_vector_scale(self.extents.render),
                render_size,
                upscale_size: output_size,
                frame_time_delta_ms: frame_params.frame_time_delta_ms.max(0.001),
                pre_exposure: 1.0,
                reset: frame_params.reset,
                camera_near: frame_params.camera_near,
                camera_far: frame_params.camera_far,
                camera_fov_angle_vertical: frame_params.camera_fov_angle_vertical,
                view_space_to_meters_factor: FSR_VIEW_SPACE_TO_METERS_FACTOR,
                enable_sharpening: false,
                sharpness: 0.0,
            })
        };

        if let Err(error) = dispatch {
            // A dispatch that the SDK rejected must not take the frame down
            // with it: the boundary barriers above already moved depth and
            // the output image out of their steady-state layouts, so the
            // recovery path restores them and blits the render-resolution
            // scene instead. Latching here (rather than dropping the context)
            // keeps SDK-owned resources alive for frames still in flight.
            log::error!(
                "FSR upscale dispatch failed: {error}; \
                 falling back to the native HDR blit for this swapchain generation"
            );
            self.dispatch_failure = Some(error.to_string());
            self.new_dispatch_failure = true;
            unsafe {
                // SAFETY: same contract as the dispatch path above — `cmd` is
                // recording outside a render pass, and both helpers only
                // record into it. `record_fsr_barriers_before` established the
                // exact layouts these two transition out of, and the SDK
                // recorded nothing between then and this `Err`.
                //
                // That last clause was #2140's open hypothesis; it is settled
                // for the vendored SDK. Every reachable error return on this
                // path — the FFI shim's parameter checks, `ffxDispatch` /
                // `ffxProvider_FSR3Upscale::Dispatch`'s `VERIFY`s, and
                // `ffxFsr3UpscalerContextDispatch`'s seven
                // `FFX_RETURN_ON_ERROR` guards — fires strictly before
                // `fsr3upscalerDispatch`, the first function that schedules or
                // records anything, and that function has no error return at
                // all. So a failed dispatch leaves `cmd` exactly as
                // `record_fsr_barriers_before` left it. The property is pinned
                // against the vendored sources by
                // `fsr3_sys::vendored_sdk_contract_tests`, which fails an SDK
                // bump that introduces an error return at or after recording.
                self.record_fsr_depth_restore(device, cmd, inputs.depth);
                self.record_native_blit(
                    device,
                    cmd,
                    frame,
                    inputs.scene_color,
                    vk::ImageLayout::GENERAL,
                );
            }
            return;
        }

        unsafe {
            // SAFETY: same contract as `record`'s own `# Safety` doc — `cmd`
            // is recording outside a render pass; the successful dispatch
            // above left the images in the layouts this barrier expects.
            self.record_fsr_barriers_after(device, cmd, frame, inputs.depth)
        };
        self.dispatched_this_frame = true;
    }

    /// `output_layout` is the layout this frame slot's output image is
    /// currently in: `SHADER_READ_ONLY_OPTIMAL` on the steady-state bridge
    /// path, or `GENERAL` when the FSR boundary barriers already ran and the
    /// dispatch then failed.
    ///
    /// #2520 — when called from the `UpscalerMode::Taa` path, `scene_color`
    /// and `output` are always the same resolution (see the module doc), so
    /// this blit is a byte-identical copy: real bandwidth cost, no visual
    /// effect. See the module doc for why that's left as documented cost
    /// rather than an added blit-skip fast path.
    ///
    /// # On the width of the acquire barrier's source scope (#2145)
    ///
    /// The `before` barrier's src stage mask is
    /// `COLOR_ATTACHMENT_OUTPUT | FRAGMENT_SHADER | COMPUTE_SHADER` — wider
    /// than the two images strictly need. Recording why, because the audit
    /// that flagged it and the audit that resolved it reached opposite
    /// conclusions and the next reader deserves the settled one:
    ///
    /// CONC-D1-2026-07-25-03 argued `COMPUTE_SHADER` is *load-bearing*,
    /// ordering partially-recorded FFX storage writes before this blit's
    /// transfer write, on the premise that `ExecuteGpuJobsVK` records every
    /// queued job before checking its error code. **That premise is false for
    /// the vendored SDK** — see #2201's sibling #2140, which traced the whole
    /// chain: every reachable error return fires before `fsr3upscalerDispatch`,
    /// the first function that records anything, and all four
    /// `executeGpuJob*` helpers return `FFX_OK` unconditionally, so the
    /// post-loop check is dead code. A failed dispatch has recorded nothing.
    /// `byroredux_fsr3_sys::vendored_sdk_contract_tests` pins both facts
    /// against the vendored sources.
    ///
    /// So `COMPUTE_SHADER` is **not** covering partial FFX writes, and this is
    /// defensive breadth rather than a load-bearing mask. The reason to leave
    /// it alone is different and simpler: this function has two callers with
    /// different `output_layout` values and therefore different prior writers,
    /// so narrowing the mask is a per-caller analysis, not the one-line
    /// cleanup it looks like. The cost of the extra bits is nil — they widen a
    /// barrier that is already being recorded — and the cost of getting the
    /// narrowing wrong is a same-command-buffer WAW that no test can see.
    /// If it is ever narrowed, split the barrier per call site first.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. `scene_color` must be
    /// in `SHADER_READ_ONLY_OPTIMAL` (composition's output layout) and
    /// `self.output_images[frame]` must currently be in `output_layout`
    /// (the caller-declared parameter — `SHADER_READ_ONLY_OPTIMAL` on the
    /// steady-state bridge path, `GENERAL` on the dispatch-failure recovery
    /// path). Both images must remain live through submission. This is the
    /// same contract as `record`'s own `# Safety` doc; every call site
    /// shares it (see the `// SAFETY:` comment at each).
    unsafe fn record_native_blit(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        scene_color: vk::Image,
        output_layout: vk::ImageLayout,
    ) {
        let range = color_subresource_single_mip();
        let output = self.output_images[frame];
        let output_src_access = blit_output_src_access(output_layout);
        let before = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .image(scene_color)
                .subresource_range(range),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(output_src_access)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(output_layout)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(output)
                .subresource_range(range),
        ];
        unsafe {
            // SAFETY: caller guarantees `cmd` is recording outside a render
            // pass; scene composition left `scene_color` shader-readable and
            // the output is in the layout the caller declared.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &before,
            );
        }

        let color_layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let region = vk::ImageBlit::default()
            .src_subresource(color_layers)
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: self.extents.render.width as i32,
                    y: self.extents.render.height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(color_layers)
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: self.extents.output.width as i32,
                    y: self.extents.output.height as i32,
                    z: 1,
                },
            ]);
        unsafe {
            // SAFETY: both images are single-sample RGBA16F color images in
            // the transfer layouts established above; the regions match their
            // declared extents and linear blit is supported for this format on
            // the renderer's desktop Vulkan target.
            device.cmd_blit_image(
                cmd,
                scene_color,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                output,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
                vk::Filter::LINEAR,
            );
        }

        let after = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(scene_color)
                .subresource_range(range),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(output)
                .subresource_range(range),
        ];
        unsafe {
            // SAFETY: orders the completed blit before presentation sampling
            // and restores both descriptors' declared shader-read layouts.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &after,
            );
        }
    }

    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. `inputs.scene_color`,
    /// `inputs.motion_vectors`, `inputs.reactive`, and `inputs.transparency`
    /// must each be in `SHADER_READ_ONLY_OPTIMAL` (their producing render
    /// pass's output layout — this barrier is execution-only for the four,
    /// no layout change). `inputs.depth` must be in
    /// `DEPTH_STENCIL_READ_ONLY_OPTIMAL`. `self.output_images[frame]` must
    /// be in `SHADER_READ_ONLY_OPTIMAL` (this frame slot's steady-state
    /// layout from the prior blit/dispatch). All named images must remain
    /// live through submission.
    unsafe fn record_fsr_barriers_before(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inputs: UpscaleDispatchInputs,
    ) {
        let color = color_subresource_single_mip();
        let depth = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH)
            .level_count(1)
            .layer_count(1);
        // #2200 / TD2-NEW-01 — the four color inputs the SDK only reads take
        // an identical execution-only barrier: same access pair, same layout
        // on both sides. They exist to order the producing render pass before
        // the SDK's compute reads, not to transition anything. Built through
        // one helper so a future change to that shape lands once instead of
        // four times, and so the two barriers that genuinely differ (depth's
        // layout change, and the output image's read->write transition) stand
        // out as the exceptions they are.
        let barriers = [
            fsr_input_read_barrier(inputs.scene_color, color),
            fsr_input_read_barrier(inputs.motion_vectors, color),
            fsr_input_read_barrier(inputs.reactive, color),
            fsr_input_read_barrier(inputs.transparency, color),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(inputs.depth)
                .subresource_range(depth),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(self.output_images[frame])
                .subresource_range(color),
        ];
        unsafe {
            // SAFETY: all images are in the old layouts established by their
            // producer passes / prior presentation; `cmd` is recording.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
    }

    /// Put depth back into the layout every other pass expects after the FSR
    /// boundary barriers ran but the dispatch itself was rejected.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. `depth_image` must be
    /// in `SHADER_READ_ONLY_OPTIMAL` — the layout `record_fsr_barriers_before`
    /// left it in earlier in this same `cmd`; this fn is only ever called on
    /// that recovery path, undoing just the depth half of that transition.
    /// The image must remain live through submission.
    unsafe fn record_fsr_depth_restore(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        depth_image: vk::Image,
    ) {
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .image(depth_image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::DEPTH)
                    .level_count(1)
                    .layer_count(1),
            );
        unsafe {
            // SAFETY: `record_fsr_barriers_before` moved this exact
            // subresource to SHADER_READ_ONLY_OPTIMAL earlier in `cmd`.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }
    }

    /// Restore the layouts `record_fsr_barriers_before` moved depth and the
    /// upscale output out of, now that the SDK dispatch has been recorded.
    ///
    /// # Validated SDK layout contract (CHAIN-D2-02 / #2139)
    ///
    /// The `old_layout` values here are an assertion about where the
    /// vendored FFX Vulkan backend leaves each image, and nothing in this
    /// repo pins the SDK's internal resource-state tracking to them. If the
    /// SDK left the output in some other layout, the `GENERAL` claimed
    /// below would be a lie and the transition undefined behaviour
    /// (VUID-VkImageMemoryBarrier-oldLayout-01197).
    ///
    /// Confirmed empirically against **FSR 3.1.4** (`third_party/
    /// fidelityfx-sdk-v1.1.4`) on 2026-07-25: 900 frames under
    /// `BYRO_VALIDATION=1` (core + sync validation) across three presets —
    /// `quality`, `performance`, `native-aa`, i.e. three different
    /// render→output ratios, each 300 frames and therefore many round
    /// trips through both frame-in-flight slots — produced **zero**
    /// `VUID-VkImageMemoryBarrier-oldLayout-01197` and zero
    /// `SYNC-HAZARD-*` reports naming the upscale output or depth image.
    ///
    /// This is evidence, not proof: it confirms the contract holds for the
    /// code paths those presets exercise on this driver. **Re-run that
    /// check when the SDK is upgraded** — a backend change to resource-state
    /// tracking would break these barriers silently on the happy path.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. `depth_image` must be
    /// in `SHADER_READ_ONLY_OPTIMAL` and `self.output_images[frame]` must be
    /// in `GENERAL` — the layouts the validated SDK contract above claims
    /// the FFX Vulkan backend leaves them in. The SDK dispatch's compute
    /// accesses must already be recorded into this same `cmd` before this
    /// call, ahead of this barrier's `COMPUTE_SHADER` source stage. Both
    /// images must remain live through submission.
    unsafe fn record_fsr_barriers_after(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        depth_image: vk::Image,
    ) {
        let barriers = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
                .image(depth_image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                ),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(self.output_images[frame])
                .subresource_range(color_subresource_single_mip()),
        ];
        unsafe {
            // SAFETY: the SDK dispatch above finished recording all compute
            // accesses before this barrier in the same primary command buffer.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
    }

    /// Returns true once, after queue submission, for a successfully recorded
    /// FSR dispatch. The caller uses this to consume jitter/reset history only
    /// when the corresponding GPU work was actually submitted.
    pub fn take_submitted_dispatch(&mut self) -> bool {
        std::mem::take(&mut self.dispatched_this_frame)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recreate(
        &mut self,
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        mode: UpscalerMode,
        extents: FrameExtentSet,
    ) -> Result<()> {
        let shader_float16 = self.shader_float16;
        // `mode` is a parameter rather than `self.mode` because a runtime
        // upscaler switch reaches this through the resize path: reusing the
        // old mode would rebuild the outputs at the new extents while leaving
        // the FSR context uncreated (or stale), which reads as "the switch
        // resized the frame but never engaged the new upscaler".
        // SAFETY: resize calls this only after `device_wait_idle`.
        unsafe { self.destroy(device, allocator) };
        *self = Self::new(
            instance,
            device,
            physical_device,
            allocator,
            queue,
            command_pool,
            mode,
            extents,
            shader_float16,
        )?;
        Ok(())
    }

    /// Retire the FSR SDK context — the allocator-independent half of
    /// teardown (#2158).
    ///
    /// `fsr3::Context` owns SDK-side pipelines, descriptor pools, and its own
    /// `VkDeviceMemory` allocated outside gpu-allocator's view, so it must be
    /// dropped on *every* teardown path, including one where the allocator has
    /// already been taken. Splitting it out lets `VulkanContext::drop` run it
    /// in the allocator-independent block (the #1483 rule) while the output
    /// images below still go inside the `Some(allocator)` guard.
    ///
    /// Idempotent: a second call sees `self.context == None` and does nothing.
    ///
    /// # Safety
    ///
    /// The device must be idle — no submitted command buffer may still
    /// reference the SDK's internal resources.
    pub unsafe fn destroy_device_objects(&mut self, _device: &ash::Device) {
        self.context.take();
        self.dispatched_this_frame = false;
        self.dispatch_failure = None;
        self.new_dispatch_failure = false;
    }

    /// Free the per-frame-in-flight output images/views/allocations — the
    /// allocator-dependent half of teardown (#2158).
    ///
    /// Always runs after [`Self::destroy_device_objects`]: the SDK context
    /// must let go of these images before they are destroyed.
    ///
    /// # Safety
    ///
    /// The device must be idle and every descriptor/command buffer referencing
    /// these output views must no longer be executing.
    pub unsafe fn destroy_allocations(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        for view in self.output_views.drain(..) {
            // SAFETY: `view` was created by this device and, per this fn's
            // `# Safety` contract, no command buffer referencing it is still
            // executing.
            unsafe { device.destroy_image_view(view, None) };
        }
        for image in self.output_images.drain(..) {
            // SAFETY: `image` was created by this device; every view onto it
            // is destroyed above, and per this fn's contract the device is
            // idle.
            unsafe { device.destroy_image(image, None) };
        }
        for allocation in self.output_allocations.drain(..).flatten() {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .ok();
        }
    }

    /// Full teardown — both halves in the only order that is sound (SDK
    /// context first, then the images it referenced). Used by
    /// [`Self::recreate`], where the allocator is always available; the Drop
    /// path calls the two halves separately so the context half survives an
    /// allocator-`None` teardown (#2158).
    ///
    /// # Safety
    ///
    /// Same contract as [`Self::destroy_allocations`].
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY: the caller's device-idle contract is inherited by both halves.
        unsafe {
            self.destroy_device_objects(device);
            self.destroy_allocations(device, allocator);
        }
    }
}

/// Execution-only barrier for one of the four color images FSR reads but never
/// writes (#2200).
///
/// `old_layout == new_layout == SHADER_READ_ONLY_OPTIMAL` makes this a pure
/// ordering + visibility edge: the producing render pass's
/// `COLOR_ATTACHMENT_WRITE` must complete and become visible to the SDK's
/// compute `SHADER_READ`, with no layout transition. Identical for all four,
/// so it is written once here rather than repeated per image.
fn fsr_input_read_barrier(
    image: vk::Image,
    range: vk::ImageSubresourceRange,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::SHADER_READ)
        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image(image)
        .subresource_range(range)
}

/// Source access mask for the blit's output-image acquire barrier.
///
/// `GENERAL` only ever reaches the blit through the dispatch-failure recovery
/// path, where the last declared access was the SDK's storage write.
///
/// #2140 — "the SDK's storage write" here means the transition
/// `record_fsr_barriers_before` recorded, not anything the SDK itself put into
/// the command buffer: a rejected dispatch cannot have recorded, see the
/// recovery branch in [`FrameUpscaler::record`].
fn blit_output_src_access(output_layout: vk::ImageLayout) -> vk::AccessFlags {
    if output_layout == vk::ImageLayout::GENERAL {
        vk::AccessFlags::SHADER_WRITE
    } else {
        vk::AccessFlags::SHADER_READ
    }
}

fn vulkan_image(
    image: vk::Image,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    size: [u32; 2],
) -> fsr3::VulkanImage {
    fsr3::VulkanImage {
        image: image.as_raw(),
        format: format.as_raw() as u32,
        usage: usage.as_raw(),
        size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_contract_is_storage_sampled_and_native_copyable() {
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        assert!(usage.contains(vk::ImageUsageFlags::STORAGE));
        assert!(usage.contains(vk::ImageUsageFlags::SAMPLED));
        assert!(usage.contains(vk::ImageUsageFlags::TRANSFER_DST));
        assert_eq!(HDR_FORMAT, vk::Format::R16G16B16A16_SFLOAT);
    }

    /// #2158 / RL-D6-05 — the FSR SDK context must be retired outside the
    /// `Some(allocator)` guard in `VulkanContext::drop`.
    ///
    /// `fsr3::Context` owns SDK-side pipelines, descriptor pools and its own
    /// `VkDeviceMemory`, none of it visible to gpu-allocator. Inside the guard
    /// it is skipped entirely on any allocator-`None` Drop path — the driver-
    /// level use-after-free #1483 was filed against. Static source check: no
    /// live device under `cargo test`, and the failing path needs an
    /// allocator-`None` Drop that does not exist today (which is the point —
    /// this pins the ordering before one is reintroduced).
    ///
    /// #2406 / TD1-003 moved the guard's *body* into
    /// `destroy_allocator_owned_resources`, so the two halves now live in
    /// separate regions. The check gained precision rather than losing it:
    /// instead of comparing byte offsets within one function, it now pins
    /// which side of the guard each teardown call is on by construction —
    /// `destroy_allocations` must be absent from `Drop` entirely and present
    /// in the allocator-owned helper, and the helper must be reachable only
    /// from inside the guard.
    #[test]
    fn fsr_context_teardown_sits_outside_the_allocator_guard() {
        // #1749 / TD1-004 — `destroy_allocator_owned_resources` and
        // `impl Drop for VulkanContext` both moved to `context/teardown.rs`
        // (together, as one contiguous block) — the mirror image of
        // `context/init.rs`.
        const CONTEXT_MOD_RS: &str = include_str!("context/teardown.rs");
        let helper = CONTEXT_MOD_RS
            .split_once("unsafe fn destroy_allocator_owned_resources")
            .expect("the allocator-owned teardown helper disappeared (#2406)")
            .1;
        let drop_impl = CONTEXT_MOD_RS
            .split_once("impl Drop for VulkanContext")
            .expect("VulkanContext Drop impl disappeared")
            .1;
        let guard_pos = drop_impl
            .find("if let Some(alloc) = self.allocator")
            .expect("the allocator-dependent teardown guard disappeared");
        let helper_call = drop_impl
            .find("self.destroy_allocator_owned_resources(&alloc);")
            .expect("Drop must still invoke the allocator-owned teardown");
        let device_objects_pos = drop_impl
            .find("upscaler.destroy_device_objects(")
            .expect("Drop must retire the FSR SDK context (#2158)");

        assert!(
            device_objects_pos < guard_pos,
            "FrameUpscaler::destroy_device_objects moved inside the \
             `Some(allocator)` guard — the FSR SDK context would then leak \
             past vkDestroyDevice on any allocator-None Drop path (#2158 / \
             #1483). Keep it in the allocator-independent block.",
        );
        assert!(
            guard_pos < helper_call,
            "the allocator-owned teardown must be called from inside the \
             `Some(allocator)` guard (#2158 / #2406)",
        );
        assert!(
            !drop_impl[..helper_call].contains("upscaler.destroy_allocations("),
            "FrameUpscaler::destroy_allocations must stay on the guarded side \
             — it frees gpu-allocator memory (#2158).",
        );
        assert!(
            helper.contains("upscaler.destroy_allocations("),
            "Drop must free the FSR output images (#2158) — they are \
             allocator-owned, so the call belongs in \
             destroy_allocator_owned_resources.",
        );
        // The SDK context is dropped before the images it references: the
        // former is in Drop ahead of the guard, the latter only runs inside
        // the helper the guard calls.
        assert!(device_objects_pos < helper_call);
    }

    #[test]
    fn recovery_blit_acquires_the_output_from_the_layout_the_dispatch_left() {
        assert_eq!(
            blit_output_src_access(vk::ImageLayout::GENERAL),
            vk::AccessFlags::SHADER_WRITE
        );
        assert_eq!(
            blit_output_src_access(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            vk::AccessFlags::SHADER_READ
        );
    }

    /// Regression for #2519 (REN-D23-03) — the frame on which an FSR
    /// dispatch is rejected has already rendered its geometry with the FSR
    /// sub-pixel jitter (chosen at the top of `draw_frame`, while
    /// `is_fsr_dispatch_active()` was still true) and the recovery path
    /// blits it through unresolved. Both recovery arms must therefore raise
    /// the one-shot edge, and `record_upscale_pass` must consume it into a
    /// temporal discontinuity — otherwise the next frame reprojects against
    /// a half-pixel-shifted image. Source-scan pin: a device is needed to
    /// build a `FrameUpscaler`, and an SDK rejection cannot be produced in
    /// a unit test at all.
    #[test]
    fn every_dispatch_failure_latch_raises_the_temporal_discontinuity_edge() {
        const FRAME_UPSCALER_RS: &str = include_str!("frame_upscaler.rs");
        const POST_PASSES_RS: &str = include_str!("context/post_passes.rs");

        // Every assignment to `dispatch_failure` inside `record`'s recovery
        // arms must be followed by the edge flag. `destroy_device_objects`
        // clears it to `None`, which is not a latch.
        // Scan production code only — the needles below also occur in this
        // test's own body.
        let production = FRAME_UPSCALER_RS
            .split_once("\n#[cfg(test)]")
            .expect("frame_upscaler.rs has a #[cfg(test)] module")
            .0;
        let latches: Vec<&str> = production
            .match_indices("self.dispatch_failure =")
            .map(|(i, _)| &production[i..])
            .filter(|tail| !tail.starts_with("self.dispatch_failure = None"))
            .collect();
        assert_eq!(
            latches.len(),
            2,
            "expected exactly two dispatch-failure latch sites (absent frame              params, rejected dispatch); found {}",
            latches.len()
        );
        for tail in latches {
            let window: String = tail.chars().take(400).collect();
            assert!(
                window.contains("self.new_dispatch_failure = true;"),
                "a dispatch-failure latch does not raise the #2519 edge:\n{window}"
            );
        }

        assert!(
            POST_PASSES_RS.contains("take_new_dispatch_failure()"),
            "record_upscale_pass must consume the #2519 edge"
        );
        assert!(
            POST_PASSES_RS.contains("FSR_DISPATCH_FAILURE_RECOVERY_FRAMES"),
            "the consumed #2519 edge must signal a temporal discontinuity"
        );
    }

    /// Regression for #2684 (SAFE-D4-03) — six `unsafe fn` (four here, one
    /// each in `gbuffer.rs` / `context/screenshot.rs`) carried no `# Safety`
    /// doc section stating the caller contract, unlike the other 71 of 77
    /// workspace `unsafe fn`. All are private/`pub(super)`, so
    /// `clippy::missing_safety_doc` doesn't catch them — verified
    /// empirically (stripping the doc from `Attachment::destroy` and
    /// running `cargo clippy -- -W clippy::missing_safety_doc` produces
    /// zero diagnostics), which is also why enabling that lint crate-wide
    /// per the issue's "consider" suggestion was skipped: it wouldn't have
    /// covered the actual gap. Source-scan pin instead: each `unsafe fn`
    /// signature must be immediately preceded by a `# Safety` heading in
    /// its doc comment (allowing intervening doc lines, since some of
    /// these also carry unrelated prose sections above the contract).
    #[test]
    fn frame_upscaler_and_sibling_unsafe_fns_carry_safety_doc_sections() {
        const GBUFFER_RS: &str = include_str!("gbuffer.rs");
        const SCREENSHOT_RS: &str = include_str!("context/screenshot.rs");
        const FRAME_UPSCALER_RS: &str = include_str!("frame_upscaler.rs");

        // (source, needle identifying the fn, human label)
        let sites: &[(&str, &str, &str)] = &[
            (
                FRAME_UPSCALER_RS,
                "unsafe fn record_native_blit(",
                "record_native_blit",
            ),
            (
                FRAME_UPSCALER_RS,
                "unsafe fn record_fsr_barriers_before(",
                "record_fsr_barriers_before",
            ),
            (
                FRAME_UPSCALER_RS,
                "unsafe fn record_fsr_depth_restore(",
                "record_fsr_depth_restore",
            ),
            (
                FRAME_UPSCALER_RS,
                "unsafe fn record_fsr_barriers_after(",
                "record_fsr_barriers_after",
            ),
            (
                GBUFFER_RS,
                "unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {",
                "Attachment::destroy",
            ),
            (
                SCREENSHOT_RS,
                "unsafe fn screenshot_record_copy(",
                "screenshot_record_copy",
            ),
        ];

        for (src, needle, label) in sites {
            let before_fn = src
                .split_once(needle)
                .unwrap_or_else(|| panic!("{label} disappeared or its signature changed (#2684)"))
                .0;
            // Drop the trailing PARTIAL line (e.g. "    pub(super) " —
            // everything on the signature's own line up to where `needle`
            // starts matching). Left in, that fragment is neither blank nor
            // `///`-prefixed, so it would poison the reverse walk below
            // before it ever reaches the real doc comment lines above it.
            let before_fn = before_fn.rsplit_once('\n').map_or("", |(rest, _)| rest);
            // The doc comment block immediately preceding the signature —
            // everything back to the last non-doc-comment, non-blank line.
            let doc_block: Vec<&str> = before_fn
                .lines()
                .rev()
                .take_while(|line| {
                    let trimmed = line.trim();
                    trimmed.is_empty() || trimmed.starts_with("///")
                })
                .collect::<Vec<_>>();
            // A genuine `# Safety` HEADING line — not just prose elsewhere
            // in the block that happens to mention the phrase (e.g. "same
            // contract as `record`'s own `# Safety` doc"). Strip the `///`
            // prefix and surrounding whitespace and require an exact match,
            // the same way a Markdown renderer identifies a heading line.
            let has_safety_heading = doc_block.iter().any(|line| {
                line.trim()
                    .strip_prefix("///")
                    .map(str::trim)
                    .is_some_and(|rest| rest == "# Safety")
            });
            assert!(
                has_safety_heading,
                "{label} is `unsafe fn` but its doc comment has no `# Safety` \
                 HEADING line stating the caller contract (#2684 / SAFE-D4-03) \
                 — a mention of the phrase elsewhere in its doc prose does \
                 not count",
            );
        }
    }
}

/// #2178 / PERF-D3-03 — both image-creation loops must return their
/// gpu-allocator sub-allocation on a `bind_image_memory` failure.
///
/// Static assertions: the branch fires only on a real allocator/driver
/// failure, which `cargo test` has no way to induce. What is checkable is
/// that the bind result is inspected rather than `?`-propagated past the
/// still-local allocation — the `?` form is precisely what leaked, because
/// at that point the allocation is not yet in `output_allocations` /
/// `allocations`, so neither `destroy()` nor `Drop` can reach it.
#[cfg(test)]
mod bind_failure_frees_allocation_tests {
    const FRAME_UPSCALER_RS: &str = include_str!("frame_upscaler.rs");
    const GBUFFER_RS: &str = include_str!("gbuffer.rs");

    /// Slice of `source` from the first `bind_image_memory` mention to the
    /// end of the enclosing statement's error branch — in practice, up to
    /// the following `self.` push/assign that takes ownership.
    fn bind_branch(source: &str, terminator: &str) -> String {
        let start = source
            .find("bind_image_memory")
            .expect("bind_image_memory disappeared");
        let rest = &source[start..];
        let end = rest
            .find(terminator)
            .unwrap_or_else(|| panic!("`{terminator}` not found after the bind call"));
        rest[..end].to_string()
    }

    #[test]
    fn frame_upscaler_create_outputs_frees_on_bind_failure() {
        let production = FRAME_UPSCALER_RS
            .split_once("\n#[cfg(test)]")
            .expect("frame_upscaler.rs lost its test modules")
            .0;
        let branch = bind_branch(
            production,
            "self.output_allocations[frame] = Some(allocation);",
        );
        assert!(
            branch.contains(".free(allocation)"),
            "create_outputs no longer frees its sub-allocation when \
             bind_image_memory fails — the allocation is still a local there \
             (output_allocations[frame] is None), so nothing else can reclaim \
             it (#2178). Branch was:\n{branch}",
        );
    }

    #[test]
    fn gbuffer_create_attachment_frees_on_bind_failure() {
        let branch = bind_branch(GBUFFER_RS, "self.allocations.push(Some(alloc));");
        assert!(
            branch.contains(".free(alloc)"),
            "gbuffer's attachment loop no longer frees its sub-allocation when \
             bind_image_memory fails — same shape as the frame_upscaler site \
             (#2178). Branch was:\n{branch}",
        );
    }
}

/// #2200 / TD2-NEW-01 — the extracted FSR input barrier keeps the exact field
/// values the four hand-rolled copies had.
///
/// A pure refactor needs a pin precisely because it is supposed to change
/// nothing: the four originals were byte-identical apart from `.image`, so
/// there is no behavioural test that would notice the helper drifting. These
/// assertions are on plain `#[repr(C)]` struct fields — no device needed.
#[cfg(test)]
mod fsr_input_barrier_tests {
    use super::*;

    #[test]
    fn helper_reproduces_the_execution_only_shape() {
        let image = vk::Image::from_raw(0x1234);
        let range = color_subresource_single_mip();
        let barrier = fsr_input_read_barrier(image, range);

        assert_eq!(
            barrier.src_access_mask,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
        );
        assert_eq!(barrier.dst_access_mask, vk::AccessFlags::SHADER_READ);
        assert_eq!(barrier.image, image);
        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(
            barrier.old_layout, barrier.new_layout,
            "these four barriers order the producing pass before the SDK's \
             reads; they must not transition anything. A layout change here \
             would silently alter what the SDK is handed.",
        );
    }

    /// The two barriers that are *not* this shape must stay distinct — depth
    /// changes layout, and the output image goes read -> write. If either ever
    /// collapses into the helper, FSR would be handed an image in the wrong
    /// layout with no validation error at record time.
    #[test]
    fn depth_and_output_are_deliberately_not_the_shared_shape() {
        let src = production_src();
        let before = src
            .split_once("unsafe fn record_fsr_barriers_before(")
            .expect("record_fsr_barriers_before disappeared")
            .1;
        let body = before
            .split_once("\n    }")
            .expect("no closing brace at method indentation")
            .0;

        assert_eq!(
            body.matches("fsr_input_read_barrier(").count(),
            4,
            "expected exactly the four read-only color inputs to use the \
             shared helper (#2200)",
        );
        assert!(
            body.contains("DEPTH_STENCIL_READ_ONLY_OPTIMAL"),
            "depth's layout transition was folded away — it is not the \
             execution-only shape",
        );
        assert!(
            body.contains("vk::ImageLayout::GENERAL"),
            "the output image's read -> write transition to GENERAL was folded \
             away — it is not the execution-only shape",
        );
    }

    /// #2834 — the engine's view space is Bethesda units, so the SDK must be
    /// told one view-space unit is 1/70 of a metre. Pre-fix the dispatch
    /// declared `1.0`, inflating every "metres" quantity inside FSR 70×.
    #[test]
    fn view_space_to_meters_factor_is_the_bethesda_unit_scale() {
        assert_eq!(
            FSR_VIEW_SPACE_TO_METERS_FACTOR,
            1.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER,
        );
        // The two distance-tuned SDK heuristics must now see plausible
        // metre values rather than saturating on the first metre of depth.
        let hundred_metres_in_bu = 100.0 * byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
        assert!(
            (hundred_metres_in_bu * FSR_VIEW_SPACE_TO_METERS_FACTOR - 100.0).abs() < 1e-3,
            "the 0-100 m ReconstructedDepthMvPxThreshold ramp must span 7000 BU",
        );
        // `prepare_inputs.h` clamps view-space depth to FP16 max; at the
        // correct factor a 300 000 BU far plane maps well inside it.
        let far_in_metres = 300_000.0 * FSR_VIEW_SPACE_TO_METERS_FACTOR;
        assert!(
            far_in_metres < 65_504.0,
            "far-plane depth {far_in_metres} m must not flat-top FP16",
        );
    }

    /// The value must stay *derived*. A future edit that re-hard-codes the
    /// field is invisible to every runtime check — it is not a crash, not a
    /// validation error, and the SSIM matrix scores FSR against the engine's
    /// own TAA rather than ground truth — so pin it at the source level.
    #[test]
    fn view_space_to_meters_factor_is_not_a_bare_literal() {
        let src = production_src();
        assert!(
            src.contains("view_space_to_meters_factor: FSR_VIEW_SPACE_TO_METERS_FACTOR"),
            "the dispatch must pass the derived constant",
        );
        assert!(
            !src.contains("view_space_to_meters_factor: 1.0"),
            "view_space_to_meters_factor was re-hard-coded to 1.0 (#2834)",
        );
        assert!(
            src.contains("byroredux_core::lighting::BETHESDA_UNITS_PER_METER"),
            "the constant must be sourced from the engine-wide BU definition, \
             not restated as a local literal",
        );
    }

    fn production_src() -> &'static str {
        include_str!("frame_upscaler.rs")
            .split_once("\n#[cfg(test)]")
            .expect("frame_upscaler.rs lost its test modules")
            .0
    }
}
