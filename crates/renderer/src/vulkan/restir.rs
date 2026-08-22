//! ReSTIR-DI direct-shadow reservoir buffers (Bitterli et al. 2020).
//!
//! Owns the screen-sized, per-frame-in-flight reservoir SSBOs the fragment
//! shader reads (previous frame) and writes (current frame) to reuse
//! direct soft-shadow samples temporally — fixing the un-denoised
//! per-frame WRS shadow crawl. One [`Reservoir`] per screen pixel, indexed
//! `pixelY * screenWidth + pixelX` (matching `triangle.frag`).
//!
//! This holder follows the **screen-sized-resource-in-set-1 precedent**
//! already used for the SSAO texture (binding 7) and depth-history
//! (binding 15): the buffer memory lives here (with its own
//! recreate-on-resize), and is *written into* the scene descriptor set
//! (set 1, bindings 16 = curr, 17 = prev) via
//! [`super::scene_buffer::SceneBuffers::write_reservoir_buffers`]. The
//! scene descriptor set itself stays content-sized and is not recreated on
//! resize — only the reservoir descriptor writes are refreshed.
//!
//! ## Ping-pong
//!
//! With `MAX_FRAMES_IN_FLIGHT == 2`, frame N writes slot N and reads slot
//! `(N+1) % 2` as its temporal history — identical to the SVGF history
//! scheme. The per-frame fence guarantees slot `(N+1)%2` was fully written
//! two frames ago before the new read.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use super::texture::with_one_time_commands;
use anyhow::Result;
use ash::vk;

/// Bytes per [`Reservoir`] — must match the `struct Reservoir` std430 layout
/// in `shaders/include/bindings.glsl` (8 × 4-byte scalars). Pinned by
/// [`tests::reservoir_stride_matches_shader`].
pub const RESERVOIR_STRIDE: vk::DeviceSize = 32;

/// Screen-sized, per-frame-in-flight reservoir storage buffers.
pub struct ReservoirBuffers {
    /// One GPU-only STORAGE buffer per frame-in-flight, sized
    /// `width * height * RESERVOIR_STRIDE`.
    buffers: Vec<GpuBuffer>,
    width: u32,
    height: u32,
}

impl ReservoirBuffers {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                Self::byte_size(width, height),
                // TRANSFER_DST for the zero-fill below (#2152 / CHAIN-D2-05).
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            )?);
        }
        // #2152 / CHAIN-D2-05 — zero both slots on creation. Without this,
        // the temporal ping-pong's first-use read (and every read after a
        // resize, see `recreate_on_resize`) samples undefined device memory,
        // relying entirely on the shader-side surface/W/M/nan gate to reject
        // it. SVGF and TAA (the analogous history consumers) both clear
        // their history on init; this brings ReSTIR in line, near-free
        // (one-time, once per swapchain generation).
        Self::zero_fill(device, queue, pool, &buffers)?;
        // PERF-D5-NEW-04 / #1814 — this is the largest single VRAM
        // addition of the Session 49 denoiser overhaul (~127 MB at
        // 1080p, ~236 MB at 1440p, ~531 MB at 4K across both FIF
        // slots) and had no attributing telemetry anywhere; see
        // docs/engine/memory-budget.md's "ReSTIR Reservoirs" section.
        log::info!(
            "ReSTIR reservoir buffers created: {}x{}, {:.1} MB/slot × {} slots = {:.1} MB total",
            width,
            height,
            Self::byte_size(width, height) as f64 / (1024.0 * 1024.0),
            MAX_FRAMES_IN_FLIGHT,
            (Self::byte_size(width, height) * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize) as f64
                / (1024.0 * 1024.0),
        );
        Ok(Self {
            buffers,
            width,
            height,
        })
    }

    fn byte_size(width: u32, height: u32) -> vk::DeviceSize {
        (width as vk::DeviceSize) * (height as vk::DeviceSize) * RESERVOIR_STRIDE
    }

    /// The reservoir buffer this frame writes (set-1 binding 16).
    pub fn curr_buffer(&self, frame: usize) -> vk::Buffer {
        self.buffers[frame].buffer
    }

    /// The reservoir buffer this frame reads as temporal history (binding 17):
    /// the other frame-in-flight slot, mirroring the SVGF ping-pong.
    pub fn prev_buffer(&self, frame: usize) -> vk::Buffer {
        self.buffers[(frame + 1) % MAX_FRAMES_IN_FLIGHT].buffer
    }

    /// Byte size of one reservoir buffer (for the descriptor range).
    pub fn buffer_size(&self) -> vk::DeviceSize {
        Self::byte_size(self.width, self.height)
    }

    /// Recreate at a new extent after a swapchain resize. History is
    /// meaningless across a resize, and the temporal pass's shader-side
    /// packed surface-ID + normal check already rejects a stale slot's
    /// contents — but the freshly-allocated slot at the new extent is
    /// undefined device memory until zeroed below (#2152 / CHAIN-D2-05:
    /// same reasoning as `new()`, this is the other point where the
    /// resource is (re)created without ever being written first).
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
        width: u32,
        height: u32,
    ) -> Result<()> {
        // SAFETY: called from the fenced swapchain-resize path
        // (`recreate_swapchain` waits both frames-in-flight first), so no
        // in-flight command references these buffers.
        for mut buf in self.buffers.drain(..) {
            buf.destroy(device, allocator);
        }
        self.width = width;
        self.height = height;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                Self::byte_size(width, height),
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            )?);
        }
        Self::zero_fill(device, queue, pool, &self.buffers)?;
        // PERF-D5-NEW-04 / #1814 — same telemetry as `new()`; a resize
        // is exactly when this footprint changes, so it's the other
        // point where under-tracked VRAM growth would otherwise go
        // unnoticed.
        log::info!(
            "ReSTIR reservoir buffers recreated: {}x{}, {:.1} MB/slot × {} slots = {:.1} MB total",
            width,
            height,
            Self::byte_size(width, height) as f64 / (1024.0 * 1024.0),
            MAX_FRAMES_IN_FLIGHT,
            (Self::byte_size(width, height) * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize) as f64
                / (1024.0 * 1024.0),
        );
        Ok(())
    }

    /// Zero every reservoir slot via `vkCmdFillBuffer`, submitted as a
    /// one-time command and fence-waited before returning. #2152 /
    /// CHAIN-D2-05 — called once from `new()` and once from
    /// `recreate_on_resize()`, both of which only ever run outside the
    /// per-frame hot path (startup / swapchain resize), so the extra
    /// submit + wait here is not a per-frame cost.
    fn zero_fill(
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
        buffers: &[GpuBuffer],
    ) -> Result<()> {
        with_one_time_commands(device, queue, pool, |cmd| {
            for buf in buffers {
                // SAFETY: `cmd` is a valid, recording primary command buffer
                // (guaranteed by `with_one_time_commands`); `buf.buffer` was
                // just created on this `device` with TRANSFER_DST usage and
                // is not yet referenced by any in-flight submission.
                unsafe {
                    device.cmd_fill_buffer(cmd, buf.buffer, 0, vk::WHOLE_SIZE, 0);
                }
            }
            Ok(())
        })
    }

    /// Destroy all reservoir buffers.
    ///
    /// # Safety
    /// Caller must ensure no in-flight command buffer references these
    /// buffers (device idle or both frames-in-flight fenced).
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut buf in self.buffers.drain(..) {
            buf.destroy(device, allocator);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rust-side stride must match the std430 `struct Reservoir` in
    /// `bindings.glsl` (8 scalars × 4 bytes). If the shader struct grows,
    /// this and the GLSL must move together.
    #[test]
    fn reservoir_stride_matches_shader() {
        // lightAndSurface + W + M + packed(histLen, depth) + accumR/G/B
        // + pad0 (packed geometric normal). 8 scalars × 4 bytes.
        assert_eq!(RESERVOIR_STRIDE, 8 * 4);
    }

    /// #2152 / CHAIN-D2-05 — both `new()` and `recreate_on_resize()` must
    /// allocate with TRANSFER_DST and route through `zero_fill` so neither
    /// slot's first-use read (nor its first read after a resize) samples
    /// undefined device memory. A real Vulkan device is out of scope for
    /// `cargo test`, so this pins the source shape instead — mirrors the
    /// project's established pattern for GPU-only behavior (see the
    /// `include_str!`-based tests above).
    #[test]
    fn reservoir_buffers_zero_fill_on_create_and_resize() {
        let src = include_str!("restir.rs");
        assert!(
            src.contains("Self::zero_fill(device, queue, pool, &buffers)?;"),
            "new() must zero-fill its freshly allocated slots"
        );
        assert!(
            src.contains("Self::zero_fill(device, queue, pool, &self.buffers)?;"),
            "recreate_on_resize() must zero-fill its freshly allocated slots"
        );
        // Built from two literals (rather than one) so this assertion's own
        // text — read back via the `include_str!` above — doesn't count
        // itself as a third match.
        let usage_flags = format!(
            "{}{}",
            "vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::", "TRANSFER_DST"
        );
        assert_eq!(
            src.matches(&usage_flags).count(),
            2,
            "both allocation sites must add TRANSFER_DST for the zero-fill"
        );
    }

    #[test]
    fn temporal_reuse_validates_surface_depth_and_does_not_seed_neighbor_radiance() {
        let src = include_str!("../../shaders/triangle.frag");
        assert!(
            src.contains("rpSurfaceId == surfaceId")
                && src.contains("temporalDepthCompatible")
                // #2777 / REN-D2-01 — `worldDist` is now clamped to match
                // the packHalf2x16-clamped `rpHistoryDepth.y` before
                // differencing (see `restir_depth_compatibility_gates_
                // clamp_world_dist_to_match_packed_history`).
                && src.contains("abs(rpHistoryDepth.y - min(worldDist, 65504.0))")
                && src.contains("dot(geomN, rpGeomN) >= TEMPORAL_NORMAL_COS"),
            "ReSTIR temporal history must validate surface identity, depth, and normal"
        );
        assert!(
            !src.contains("SPATIAL_SEED_HIST") && !src.contains("spatColSum"),
            "spatial reuse may borrow light candidates, never old surface radiance"
        );
        assert!(
            src.contains("bool cameraStatic = dofParams.w > 0.5")
                && src.contains("cameraStatic ? 64.0 : 16.0")
                && src.contains("cameraStatic ? 0.025 : 0.1"),
            "direct-light history must converge when parked and remain responsive in motion"
        );
    }

    #[test]
    fn spatial_reuse_requires_same_surface_depth_and_normal() {
        let src = include_str!("../../shaders/triangle.frag");
        let bindings = include_str!("../../shaders/include/bindings.glsl");
        assert!(bindings.contains("float histLenAndDepth;"));
        assert!(
            src.contains("rnSurfaceId == surfaceId")
                && src.contains("spatialDepthCompatible")
                && src.contains("dot(geomN, nGeomN) >= SPATIAL_NORMAL_COS"),
            "spatial reservoir reuse must not cross stable receiver, depth, or normal edges"
        );
        assert!(
            src.contains("packHalf2x16(vec2(")
                && src.contains("floatBitsToUint(rn.histLenAndDepth)"),
            "depth compatibility must stay inside the existing 32-byte reservoir stride"
        );
    }

    #[test]
    fn authored_non_shadow_lights_keep_structural_restir_visibility() {
        let src = include_str!("../../shaders/triangle.frag");
        let lighting = include_str!("../../shaders/include/lighting.glsl");
        let common = include_str!("../../shaders/include/shadow_common.glsl");
        assert!(
            src.contains("bool needsVisibility = visibilityMaskNeedsTrace(")
                && src.contains("if (!useRestir || !needsVisibility)")
                // #2554 — the `&& shadowFade > 0.01` clause this used to pin
                // was removed from the streaming gate. It was incidental to
                // this test's subject (structural visibility for
                // no-prop-shadow LIGH sources); gating the stream on the
                // distance fade is what zeroed those lights outright past
                // SHADOW_FADE_END. See `restir_far_field_converges_to_
                // unshadowed_radiance` for the pin that now owns the fade.
                && src.contains("if (directShadowRayEnabled && needsVisibility)"),
            "no-prop-shadow LIGH sources must retain structural ReSTIR visibility"
        );
        assert!(
            src.contains("visibilityMaskNeedsTrace(lights[rpLightIndex].params.z)")
                && src.contains("visibilityMaskNeedsTrace(lights[rnLightIndex].params.z)")
                && src.contains("visibilityMaskNeedsTrace(lights[restirY].params.z)")
                && src.contains("traceLightTransmittance("),
            "temporal, spatial, and final ReSTIR stages must preserve structural selections"
        );
        assert!(
            common.contains("uint visibilityOpaqueMask(")
                && common.contains("VISIBILITY_MASK_ALL_OPAQUE")
                && lighting.contains("vec3 traceLightTransmittance("),
            "direct-light visibility must preserve the explicit layer mask"
        );
    }

    /// #2554 / FNV-D3-01 — `shadowFade` must fade the SHADOW term, never the
    /// light itself.
    ///
    /// Lights that need visibility contribute nothing to `Lo` under ReSTIR
    /// (`if (!useRestir || !needsVisibility)`), so the reservoir estimate is
    /// their entire direct term. Pre-fix, both the streaming gate and the
    /// finalize scale keyed on `shadowFade`, so past `SHADOW_FADE_END` the
    /// estimate went to zero and every FNV light — including the sun, since
    /// `render/lights.rs` never emits `SHADOW_POLICY_NONE` — blacked out
    /// beyond ~171 m.
    ///
    /// The invariant: `shadowFade == 0` ⇒ direct light == the unshadowed BRDF
    /// value. That holds iff (a) the reservoir keeps streaming at any
    /// distance, and (b) the fade is applied to the visibility factor via
    /// `mix(vec3(1.0), transmissionFrame, shadowFade)` rather than as a bare
    /// multiply on the contribution. This is the same semantics the retired
    /// legacy-WRS arm implements by scaling only its shadow subtraction.
    #[test]
    fn restir_far_field_converges_to_unshadowed_radiance() {
        let src = include_str!("../../shaders/triangle.frag");

        // (a) Neither the stream nor its temporal/spatial reuse may be gated
        //     on the distance fade — a dead reservoir cannot carry the term.
        assert!(
            !src.contains("needsVisibility && shadowFade > 0.01")
                && !src.contains("useTemporal && shadowFade > 0.01")
                && !src.contains("useSpatial && shadowFade > 0.01"),
            "reservoir streaming/reuse must not be gated on shadowFade — that \
             is what zeroed distant lights instead of fading their shadows"
        );

        // (b) The fade lands on the visibility factor, and the finalize is a
        //     plain rad·W·V product with no residual `* shadowFade`.
        assert!(
            src.contains("visibility = mix(vec3(1.0), transmissionFrame, shadowFade);"),
            "shadowFade must interpolate visibility toward fully-lit, so the \
             far-field estimate converges to the unshadowed BRDF value"
        );
        assert!(
            src.contains("frameContribution = rad * restirW * visibility;"),
            "the ReSTIR finalize must not re-apply shadowFade to the whole \
             contribution — that fades the light, not the shadow"
        );
        assert!(
            !src.contains("rad * restirW * transmissionFrame * shadowFade"),
            "the pre-#2554 finalize form must not return"
        );

        // The shadow RAYS stay fade-gated: skipping them at range is the
        // intended perf saving, and is what makes the default visibility of
        // 1.0 load-bearing rather than merely defensive.
        assert!(
            src.contains("vec3 visibility = vec3(1.0);")
                && src.contains("if (shadowFade > 0.01) {"),
            "shadow rays must still be skipped past the fade, defaulting \
             visibility to fully lit"
        );

        // The legacy-WRS arm is the reference semantics being matched here,
        // not an independent re-derivation (issue's SIBLING check).
        assert!(
            src.contains("Lo - shadowable * W * (vec3(1.0) - transmission) * shadowFade"),
            "legacy-WRS reference semantics (fade applied only to the shadow \
             subtraction) must remain present as the comparison point"
        );
    }

    /// Ping-pong: curr/prev are always different slots at
    /// MAX_FRAMES_IN_FLIGHT == 2 (else temporal history aliases the write).
    #[test]
    fn ping_pong_slots_differ() {
        const {
            assert!(MAX_FRAMES_IN_FLIGHT >= 2);
        }
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            assert_ne!(f, (f + 1) % MAX_FRAMES_IN_FLIGHT);
        }
    }

    /// PERF-D5-NEW-04 / #1814 — pins the exact per-resolution byte totals
    /// documented in docs/engine/memory-budget.md's "ReSTIR Reservoirs"
    /// section against the live `RESERVOIR_STRIDE` / `MAX_FRAMES_IN_FLIGHT`
    /// constants. If either constant changes, this fails as the nudge to
    /// update the doc's table alongside the code (the whole point of the
    /// fix — the doc had silently drifted out of existence before #1814).
    #[test]
    fn byte_size_matches_documented_memory_budget_figures() {
        // (width, height, expected per-slot MB, expected 2-FIF total MB)
        // MB here is decimal (bytes / 1_000_000), matching every other
        // entry in memory-budget.md.
        let cases = [
            (1920u32, 1080u32, 66.4, 132.7),
            (2560, 1440, 118.0, 235.9),
            (3840, 2160, 265.4, 530.8),
        ];
        for (w, h, expected_per_slot_mb, expected_total_mb) in cases {
            let per_slot = ReservoirBuffers::byte_size(w, h) as f64 / 1_000_000.0;
            let total = per_slot * MAX_FRAMES_IN_FLIGHT as f64;
            assert!(
                (per_slot - expected_per_slot_mb).abs() < 0.1,
                "{w}x{h}: per-slot {per_slot:.1} MB != documented {expected_per_slot_mb} MB"
            );
            assert!(
                (total - expected_total_mb).abs() < 0.1,
                "{w}x{h}: total {total:.1} MB != documented {expected_total_mb} MB"
            );
        }
    }
}
