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
        // #3630 — `analyze_depth_field` returns early with `bands` empty (and
        // `cleared`/`invalid` both 0) when `near <= 0.0 || far <= near`: its
        // documented contract for a capture that disagrees with the camera is
        // "report nothing rather than guess". `stats.bands.is_empty()` can
        // only be true on that early-return path — every other path always
        // populates at least one decade band, even with zero samples in it —
        // so it's an unambiguous signal of a rejected (degenerate) camera.
        // Without this check, `geometry = total - cleared - invalid` reads as
        // the full sample count (since cleared and invalid both stayed 0) and
        // the per-band loop *also* prints "no geometry in frame" (every band
        // is empty because there are no bands) — two contradictory lines,
        // neither saying the camera was rejected.
        if stats.bands.is_empty() && stats.total > 0 {
            return CommandOutput::line(format!(
                "depth {}x{} ({} samples)  degenerate camera (near={:.3}, far={:.0}) — analysis rejected",
                capture.width, capture.height, stats.total, camera.near, camera.far
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::DepthCapture;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    fn world_with_capture(camera: Camera, samples: Vec<f32>) -> World {
        let mut world = World::new();
        let camera_entity = world.spawn();
        world.insert(camera_entity, camera);
        world.insert_resource(ActiveCamera(camera_entity));
        world.insert_resource(DepthCaptureBridge {
            requested: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(Some(DepthCapture {
                width: 2,
                height: 2,
                samples,
            }))),
        });
        world
    }

    // #3630 — a degenerate camera (`near <= 0.0 || far <= near`) must be
    // reported as rejected, not read out as a full frame of geometry with an
    // empty band table.
    #[test]
    fn degenerate_camera_is_reported_as_rejected_not_as_geometry() {
        let world = world_with_capture(
            Camera::new(1.0, 1.0, 0.0, 100.0), // near == 0.0 — degenerate
            vec![0.5; 4],
        );

        let output = DepthStatsCommand.execute(&world, "").lines.join("\n");
        assert!(
            output.contains("degenerate camera") && output.contains("analysis rejected"),
            "expected an explicit rejection line, got: {output}"
        );
        assert!(
            !output.contains("geometry="),
            "a rejected camera must not report a geometry count: {output}"
        );
        assert!(
            !output.contains("no geometry in frame"),
            "a rejected camera must not be confused with a frame that legitimately \
             has no geometry: {output}"
        );
    }

    // Sibling of the above with the other half of the degenerate condition
    // (`far <= near`), to pin both branches of `analyze_depth_field`'s guard.
    #[test]
    fn far_at_or_behind_near_is_also_reported_as_rejected() {
        let world = world_with_capture(
            Camera::new(1.0, 1.0, 10.0, 10.0), // far == near — degenerate
            vec![0.5; 4],
        );

        let output = DepthStatsCommand.execute(&world, "").lines.join("\n");
        assert!(
            output.contains("degenerate camera") && output.contains("analysis rejected"),
            "expected an explicit rejection line, got: {output}"
        );
    }

    // A well-formed camera with a genuinely empty frame (all samples cleared
    // to the far plane) must still take the normal "no geometry in frame"
    // path — the fix must not swallow that legitimate case.
    #[test]
    fn well_formed_camera_with_no_geometry_still_reports_the_normal_message() {
        let world = world_with_capture(
            Camera::new(1.0, 1.0, 1.0, 100.0),
            vec![1.0; 4], // z >= 1.0 — every sample is background/cleared
        );

        let output = DepthStatsCommand.execute(&world, "").lines.join("\n");
        assert!(
            !output.contains("degenerate camera"),
            "a well-formed camera must not be reported as degenerate: {output}"
        );
        assert!(
            output.contains("no geometry in frame"),
            "expected the normal empty-frame message, got: {output}"
        );
    }
}
