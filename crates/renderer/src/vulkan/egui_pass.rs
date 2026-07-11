//! egui overlay render pass — Phase 4 of the debug-UI plan.
//!
//! Owns a dedicated `VkRenderPass` writing the swapchain image with
//! `loadOp = LOAD` + `initialLayout = PRESENT_SRC_KHR`, plus one
//! framebuffer per swapchain image. Records draws on top of whatever
//! composite already wrote, immediately before the optional
//! screenshot copy and the final queue submit.
//!
//! egui texture lifecycle:
//!
//! * `free_textures` from the previous frame's `TexturesDelta.free`
//!   runs first. The fence wait at the top of `draw_frame` already
//!   ensures the prior frame's command buffer has fully GPU-
//!   completed, so the textures pointed to are no longer in use.
//! * `set_textures` uploads any new / updated textures. The egui-ash-
//!   renderer crate spins up its own one-shot command buffer + waits
//!   on the supplied queue, so the uploads finish synchronously before
//!   `cmd_draw` references them.
//! * `cmd_draw` records vertex / index + draw-indexed calls into the
//!   main frame's command buffer (caller-supplied).
//! * The freshly-arrived `TexturesDelta.free` is stashed for next
//!   frame's free path — same 1-frame defer the deferred-destroy
//!   queue in `MeshRegistry` / `TextureRegistry` uses.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use ash::vk;
use egui::{Context as EguiContext, FullOutput, TextureId};
use egui_ash_renderer::{Options, Renderer};
use gpu_allocator::vulkan::Allocator;

/// The Vulkan handles [`EguiPass::dispatch`] records and uploads against.
/// Groups `device`, the frame's command buffer, the graphics queue, and
/// the one-shot upload command pool that travel together into dispatch.
#[derive(Clone, Copy)]
pub struct EguiDispatchCtx<'a> {
    /// Logical device the overlay records against.
    pub device: &'a ash::Device,
    /// The current frame's command buffer (already recording).
    pub cmd: vk::CommandBuffer,
    /// Graphics queue used for the synchronous egui texture upload. Passed
    /// as the shared `Mutex` (not a bare handle) so `dispatch` can scope
    /// the lock to just the `set_textures` submit — the tessellate +
    /// cmd_draw steps only record into `cmd` and need no queue held.
    /// CONC-D1-01 (#1713).
    pub queue: &'a Mutex<vk::Queue>,
    /// Command pool the egui texture-upload one-shot buffer comes from.
    pub upload_command_pool: vk::CommandPool,
}

/// Owns the egui-ash-renderer instance plus the render-pass +
/// framebuffer chain that targets the swapchain.
pub struct EguiPass {
    /// egui-ash-renderer pipeline owner. Holds its own descriptor
    /// pool / set layout / vertex+index buffer pool. Drop is
    /// well-defined; we don't need to call anything explicit on it.
    renderer: Renderer,
    /// The owning VkRenderPass. Single subpass, single color
    /// attachment, `loadOp = LOAD` so composite's swapchain write is
    /// preserved.
    render_pass: vk::RenderPass,
    /// One framebuffer per swapchain image, each attaching that
    /// image's view to this render pass.
    framebuffers: Vec<vk::Framebuffer>,
    /// Cached current swapchain extent — used as the render area
    /// and to compute the egui viewport.
    extent: vk::Extent2D,
    /// Texture IDs queued for deletion next frame. egui asks us to
    /// free them on frame N; we defer the actual destroy to N+1
    /// because the GPU may still be reading them via frame N-1's
    /// already-recorded command buffer. The fence wait at the top
    /// of `draw_frame` between frame submissions gives the deferred
    /// free its safety guarantee.
    pending_free: Vec<TextureId>,
}

impl EguiPass {
    pub fn new(
        device: ash::Device,
        allocator: Arc<Mutex<Allocator>>,
        swapchain_format: vk::Format,
        swapchain_image_views: &[vk::ImageView],
        swapchain_extent: vk::Extent2D,
        in_flight_frames: usize,
    ) -> Result<Self> {
        let render_pass = create_render_pass(&device, swapchain_format)?;
        let framebuffers = create_framebuffers(
            &device,
            render_pass,
            swapchain_image_views,
            swapchain_extent,
        )?;

        let opts = Options {
            in_flight_frames,
            enable_depth_test: false,
            enable_depth_write: false,
            // egui-ash-renderer's fragment shader gamma-corrects when
            // `srgb_framebuffer == false`. The composite RP writes
            // the swapchain image after ACES tone-mapping in linear
            // space, then the swapchain's surface format (commonly
            // `B8G8R8A8_SRGB`) handles the gamma on store. So the
            // "framebuffer is sRGB" answer is whether the swapchain
            // format itself is an `_SRGB` variant.
            srgb_framebuffer: is_srgb_format(swapchain_format),
        };

        let renderer = Renderer::with_gpu_allocator(allocator, device, render_pass, opts)
            .map_err(|e| anyhow!("egui-ash-renderer init failed: {e:?}"))?;

        Ok(Self {
            renderer,
            render_pass,
            framebuffers,
            extent: swapchain_extent,
            pending_free: Vec::new(),
        })
    }

    /// Recreate framebuffers + extent for a new swapchain (resize /
    /// recreate). The render pass itself stays — the swapchain
    /// format is the same after resize.
    pub fn recreate_framebuffers(
        &mut self,
        device: &ash::Device,
        image_views: &[vk::ImageView],
        extent: vk::Extent2D,
    ) -> Result<()> {
        for fb in self.framebuffers.drain(..) {
            // SAFETY: swapchain recreation only runs after `device_wait_idle`,
            // so no command buffer can still reference `fb`.
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        self.framebuffers = create_framebuffers(device, self.render_pass, image_views, extent)?;
        self.extent = extent;
        Ok(())
    }

    /// Record one egui draw into the supplied frame command buffer.
    ///
    /// The caller must have already finished the composite render
    /// pass; this method begins + ends its own render pass on top
    /// of the swapchain image at `swapchain_image_index`.
    ///
    /// Returns silently on an empty primitive list — egui produces
    /// no shapes when the overlay is hidden or contains no widgets.
    pub fn dispatch(
        &mut self,
        ctx: EguiDispatchCtx,
        swapchain_image_index: u32,
        egui_ctx: &EguiContext,
        output: FullOutput,
    ) -> Result<()> {
        let EguiDispatchCtx {
            device,
            cmd,
            queue,
            upload_command_pool,
        } = ctx;
        // 1. Process previous frame's deferred frees first — by now
        // the fence at the top of `draw_frame` has waited on the
        // previous frame's command buffer, so the textures aren't
        // referenced any more.
        if !self.pending_free.is_empty() {
            let drained = std::mem::take(&mut self.pending_free);
            self.renderer
                .free_textures(&drained)
                .map_err(|e| anyhow!("egui free_textures: {e:?}"))?;
        }

        // 2. Upload new / updated textures. set_textures uses its
        // own one-shot command buffer on the supplied queue and
        // waits internally before returning, so the textures are
        // GPU-resident by the time cmd_draw reads them.
        //
        // CONC-D1-01 (#1713): scope the queue lock to just this upload.
        // The submit+wait inside set_textures is one egui-ash-renderer
        // call we can't split, so the lock necessarily spans its internal
        // wait — but no wider. The tessellate + cmd_draw steps below only
        // record into `cmd`, so they run with the queue released.
        if !output.textures_delta.set.is_empty() {
            let q = queue.lock().unwrap_or_else(|e| e.into_inner());
            self.renderer
                .set_textures(*q, upload_command_pool, &output.textures_delta.set)
                .map_err(|e| anyhow!("egui set_textures: {e:?}"))?;
        }

        // 3. Tessellate shapes into ClippedPrimitives. egui returns
        // borrowed shapes inside `FullOutput`; tessellate consumes
        // them, which is fine — we don't need them again.
        let primitives = egui_ctx.tessellate(output.shapes, output.pixels_per_point);

        // 4. Begin RP + draw + end RP. Skip the whole pass on an empty
        // primitive list — the RP's initialLayout == finalLayout ==
        // PRESENT_SRC_KHR, so not recording it is layout-neutral.
        if !primitives.is_empty() {
            let rp_begin = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[swapchain_image_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: self.extent,
                });
            // SAFETY: `cmd` is the caller's currently-recording command
            // buffer (per this method's doc comment); `rp_begin` references
            // `self.render_pass` / `self.framebuffers` which are both live.
            unsafe {
                device.cmd_begin_render_pass(cmd, &rp_begin, vk::SubpassContents::INLINE);
            }
            // INVARIANT (REG-05 / #1637, #1491): the render pass begin MUST be
            // balanced with `cmd_end_render_pass` even when `cmd_draw` fails.
            // Capture the draw result but DON'T `?`-bail here: the render
            // pass is open, and an early return would leave the caller's
            // command buffer with a dangling begin (the pending-screenshot
            // copy + end_command_buffer that follow would then record
            // inside an active RP — VUID-vkEndCommandBuffer-commandBuffer-00060,
            // and an invalid buffer gets submitted). Always balance the
            // begin with an end first, then propagate.
            let draw_result = self
                .renderer
                .cmd_draw(cmd, self.extent, output.pixels_per_point, &primitives)
                .map_err(|e| anyhow!("egui cmd_draw: {e:?}"));
            // SAFETY: matches the `cmd_begin_render_pass` above on the same
            // `cmd` — see the INVARIANT comment: always balanced even when
            // `cmd_draw` errors, so the buffer never ends inside an open RP.
            unsafe { device.cmd_end_render_pass(cmd) };
            draw_result?;
        }

        // 5. Stash this frame's frees for next frame.
        self.pending_free = output.textures_delta.free;

        Ok(())
    }

    /// Free render pass + framebuffers. Called from `VulkanContext::drop`
    /// in reverse-construction order. The `Renderer`'s own resources
    /// (pipeline / descriptor pool / per-frame buffers) drop with the
    /// owned field.
    pub fn destroy(&mut self, device: &ash::Device) {
        // #1427 — flush the last frame's deferred texture frees before the
        // renderer's descriptor pool is torn down (its Drop runs when
        // `EguiPass` drops). Without this the final `TexturesDelta.free` set
        // never returns to the renderer's free-list, leaving descriptor-pool
        // accounting mismatched at teardown. The device is still alive here,
        // so the frees are valid; errors are ignored on the teardown path.
        if !self.pending_free.is_empty() {
            let drained = std::mem::take(&mut self.pending_free);
            let _ = self.renderer.free_textures(&drained);
        }
        // SAFETY: caller contract (called from `VulkanContext::drop` in
        // reverse-construction order) guarantees the device is idle and no
        // command buffer references these framebuffers or the render pass.
        for fb in self.framebuffers.drain(..) {
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        unsafe { device.destroy_render_pass(self.render_pass, None) };
        // `self.renderer`'s Drop runs when `EguiPass` itself drops.
    }
}

// ── internal helpers ───────────────────────────────────────────────

/// Build the egui render pass — single color attachment writing the
/// swapchain image. `loadOp = LOAD` preserves whatever composite
/// already wrote; `initialLayout = PRESENT_SRC_KHR` matches the
/// final layout composite leaves the image in.
fn create_render_pass(
    device: &ash::Device,
    swapchain_format: vk::Format,
) -> Result<vk::RenderPass> {
    let attachment = vk::AttachmentDescription::default()
        .format(swapchain_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    let color_ref = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

    let color_attachments = [color_ref];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachments);

    // Incoming dep — chain after composite's swapchain write. The
    // composite RP's outgoing dependency sets `dstStage = NONE`, so
    // the egui RP must declare its own
    // COLOR_ATTACHMENT_OUTPUT/WRITE → COLOR_ATTACHMENT_OUTPUT/READ|WRITE
    // edge to stitch the two passes.
    let in_dep = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );

    // Outgoing dep — make the implicit EXTERNAL dependency explicit
    // (EGUI-04). The egui RP is the last pass before present; without an
    // out-dependency Vulkan synthesizes one
    // (dstStage = BOTTOM_OF_PIPE, dstAccess = 0), but relying on that
    // implicit edge is fragile. Declare the color write → present chain so
    // the egui draw completes before the swapchain image is presented.
    let out_dep = vk::SubpassDependency::default()
        .src_subpass(0)
        .dst_subpass(vk::SUBPASS_EXTERNAL)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::empty());

    let attachments = [attachment];
    let subpasses = [subpass];
    let dependencies = [in_dep, out_dep];
    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);

    // SAFETY: `device` is live; `info` (with its `attachments` /
    // `subpasses` / `dependencies` slices) is fully populated above and
    // outlives this call.
    let rp = unsafe {
        device
            .create_render_pass(&info, None)
            .map_err(|e| anyhow!("egui render pass: {e}"))?
    };
    Ok(rp)
}

fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    image_views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    let mut out = Vec::with_capacity(image_views.len());
    for view in image_views {
        let attachments = [*view];
        let info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        // SAFETY: `device` is live; `info` references `render_pass` (caller-
        // supplied, live) and `attachments` (this loop iteration's `view`,
        // live for the call's duration).
        let fb = unsafe {
            device
                .create_framebuffer(&info, None)
                .map_err(|e| anyhow!("egui framebuffer: {e}"))?
        };
        out.push(fb);
    }
    Ok(out)
}

/// True for any sRGB-encoded swapchain format. egui-ash-renderer's
/// `srgb_framebuffer` option flips the shader's gamma curve based
/// on this; getting it wrong yields a visibly over-saturated
/// (sRGB↔sRGB) or muddy (linear↔linear) overlay.
fn is_srgb_format(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::B8G8R8A8_SRGB
            | vk::Format::R8G8B8A8_SRGB
            | vk::Format::A8B8G8R8_SRGB_PACK32
            | vk::Format::B8G8R8_SRGB
            | vk::Format::R8G8B8_SRGB
    )
}
