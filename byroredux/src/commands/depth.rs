//! Depth-buffer precision diagnostics (#3308).
//!
//! `depth.stats` — capture the live depth attachment and report what it
//! actually resolves, against what `Camera::depth_resolution_at` predicts.
//!
//! This is the GPU half of #3308's step-2 comparison gate. The analytic
//! functions in `byroredux_core::ecs::components::camera` say what the depth
//! buffer's resolution *should* be at a given range; this hands the CPU the
//! samples a real frame produced so the two can be checked against each
//! other, and so a future reversed-Z conversion has a live before/after to
//! be judged on rather than a screenshot and an opinion.
//!
//! Two-step by construction: the renderer records the copy inside the frame
//! and the readback lands one frame later, once the fence proves the GPU is
//! done. So the first invocation arms a capture and the next reports it —
//! the same shape the screenshot path has, for the same reason.

use super::shared::*;
use byroredux_core::ecs::components::Camera;
use byroredux_core::ecs::{ActiveCamera, DepthCaptureBridge};

/// `depth.stats` — arm a depth capture, then report the captured field.
pub(crate) struct DepthStatsCommand;

impl ConsoleCommand for DepthStatsCommand {
    fn name(&self) -> &str {
        "depth.stats"
    }

    fn description(&self) -> &str {
        "Capture the depth buffer and report measured vs analytic depth resolution (#3308)"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(bridge) = world.try_resource::<DepthCaptureBridge>() else {
            return CommandOutput::line(
                "DepthCaptureBridge not present — the renderer has not finished init",
            );
        };

        let Some(capture) = bridge.take_result() else {
            // Nothing landed yet: arm one and tell the caller to come back.
            bridge.request();
            return CommandOutput::line(
                "depth capture armed — run `depth.stats` again in a frame or two to read it",
            );
        };
        // Arm the next one too, so repeated invocations stream fresh frames
        // instead of alternating between "armed" and "report".
        bridge.request();

        let camera = world
            .try_resource::<ActiveCamera>()
            .map(|active| active.0)
            .and_then(|entity| world.get::<Camera>(entity).map(|cam| *cam));
        let Some(camera) = camera else {
            return CommandOutput::line("no active Camera — cannot decode depth samples");
        };

        let stats = camera.analyze_depth_field(&capture.samples);
        let mut out = vec![
            format!(
                "depth {}x{} ({} samples)  near={:.3} far={:.0}",
                capture.width, capture.height, stats.total, camera.near, camera.far
            ),
            format!(
                "  background(cleared)={}  geometry={}  invalid={}",
                stats.cleared,
                stats.total - stats.cleared - stats.invalid,
                stats.invalid
            ),
        ];
        if stats.invalid > 0 {
            out.push(
                "  WARNING: invalid samples present — the readback is suspect, not the analysis"
                    .to_string(),
            );
        }
        if stats.farthest > 0.0 {
            out.push(format!(
                "  nearest={:.1}  farthest={:.1}",
                stats.nearest, stats.farthest
            ));
        }
        out.push(
            "  band                samples  codes   BU/step  (reversed-Z would be)".to_string(),
        );
        for band in stats.bands.iter().filter(|b| b.samples > 0) {
            // `codes / samples` is the headline: when it collapses toward
            // zero the band's surfaces are sharing depth values, which is
            // z-fighting waiting to happen.
            out.push(format!(
                "  {:>9.0}..{:<9.0} {:>7}  {:>5}  {:>9.2}  ({:.4})",
                band.near_edge,
                band.far_edge,
                band.samples,
                band.distinct_codes,
                band.analytic_resolution,
                band.analytic_resolution_reversed,
            ));
        }
        if stats.bands.iter().all(|b| b.samples == 0) {
            out.push("  (no geometry in frame — every sample is background)".to_string());
        }
        CommandOutput::lines(out)
    }
}
