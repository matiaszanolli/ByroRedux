//! Runtime tuning and debug-control resources. (#5467 split from `components.rs`)

use byroredux_core::ecs::Resource;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.

/// REND-#1451 — live tuning knobs for the point/spot light attenuation
/// model, pushed into the renderer (`VulkanContext::light_atten_knee` /
/// `light_atten_legacy`) each frame so the REND-#1451 controlled bench
/// can sweep the attenuation without a rebuild. Mutated by the
/// `light.atten` console command; read in the draw path just before
/// `draw_frame`.
///
/// `knee_frac` is the authored radius as a fraction of the cull radius
/// (`= 1 / LIGHT_RANGE_EXTENSION` geometry → default `0.5` for the
/// current `2.0` extension). Lower ⇒ light is dimmer at the authored
/// radius. `legacy` reverts the shader to the pre-fix window-only
/// formula (75% at the authored radius — the bright near-zone ring) for
/// A/B comparison.
pub(crate) struct LightTuning {
    pub(crate) knee_frac: f32,
    pub(crate) legacy: bool,
}

impl Resource for LightTuning {}

impl Default for LightTuning {
    fn default() -> Self {
        Self {
            knee_frac: 0.5,
            legacy: false,
        }
    }
}

/// Stage 1 (RENDERING-PLAN.md) — color-pipeline tuning, pushed into the
/// renderer's exposure/tonemap state each frame (mirroring `LightTuning`'s
/// REND-#1451 pattern) and mutated by the `exposure` / `tonemap` console
/// commands. Boot state comes from `--auto-exposure` / `--tonemap` via
/// `RendererConfig`; the resource defaults are the renderer's own
/// constants (`DEFAULT_EXPOSURE` / `DEFAULT_ADAPTATION_SECONDS`), which
/// the default impl reads directly (#5198 — hand-typed copies went stale
/// once already).
pub(crate) struct ExposureTuning {
    /// EV100 metering of the post-bloom scene (Frostbite §5.6) vs the fixed
    /// constant.
    pub(crate) auto: bool,
    /// Fixed-mode exposure (the historical 0.85).
    pub(crate) fixed_exposure: f32,
    /// Metering bias in photographic stops; positive = darker.
    pub(crate) compensation_stops: f32,
    /// Adaptation time constant in seconds (0 = snap).
    pub(crate) adaptation_seconds: f32,
    /// Display transform selection: AgX when true, ACES otherwise.
    pub(crate) agx: bool,
}

impl Resource for ExposureTuning {}

impl Default for ExposureTuning {
    fn default() -> Self {
        // #5198 — read the renderer's constants instead of hand-typed
        // copies: `render_one_frame` pushes this resource over the context
        // field every frame (including frame 0), so a drifted default
        // silently overrides the renderer's own tuning.
        use byroredux_renderer::vulkan::exposure::{
            DEFAULT_ADAPTATION_SECONDS, DEFAULT_EXPOSURE,
        };
        Self {
            auto: false,
            fixed_exposure: DEFAULT_EXPOSURE,
            compensation_stops: 0.0,
            adaptation_seconds: DEFAULT_ADAPTATION_SECONDS,
            agx: false,
        }
    }
}

/// Deferred operator control for named renderer views and the one-record
/// selected visibility-ray probe. Console commands mutate this resource;
/// `render_one_frame` applies requests at the next frame boundary, where it
/// has exclusive access to `VulkanContext`.
#[derive(Default)]
pub(crate) struct RenderDebugControl {
    pub(crate) active_mode: byroredux_renderer::RenderDebugMode,
    pub(crate) pending_mode: Option<byroredux_renderer::RenderDebugMode>,
    pub(crate) pending_probe_pixel: Option<[u32; 2]>,
    pub(crate) pending_probe_generation: Option<u32>,
    pub(crate) last_probe: Option<byroredux_renderer::SelectedRayProbeResult>,
    pub(crate) last_error: Option<String>,
}

impl Resource for RenderDebugControl {}
