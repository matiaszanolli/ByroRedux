//! Regression tests for [`insert_procedural_fallback_resources`] —
//! issue #542 / M33-10.
//!
//! Pre-fix the no-WTHR fallback branch installed `SkyParamsRes` +
//! `CellLightingRes` but skipped `GameTimeRes` + `WeatherDataRes`.
//! `weather_system` (`systems.rs:1136`) early-returns on missing
//! resources, so the fallback sun stayed pinned at the initial
//! `sun_direction = [-0.4, 0.8, -0.45]` and `sun_intensity = 4.0`
//! forever — every cell whose worldspace failed to resolve a WTHR
//! got a frozen sky.
//!
//! The fix inserts both resources alongside the existing ones,
//! with a synthetic NAM0 table whose 6 TOD slots all carry the
//! same procedural Mojave defaults so the lerp is a no-op for
//! colour while `sun_direction` / `sun_intensity` still animate.

use super::*;
use crate::systems::weather_system;
use byroredux_core::ecs::World;

fn run_fallback_world() -> World {
    let mut world = World::new();
    let sun_dir = [-0.4_f32, 0.8, -0.45];
    insert_procedural_fallback_resources(&mut world, sun_dir);
    world
}

/// All four resources land in the world.
#[test]
fn fallback_inserts_all_four_resources() {
    let world = run_fallback_world();
    assert!(world.try_resource::<CellLightingRes>().is_some());
    assert!(world.try_resource::<SkyParamsRes>().is_some());
    assert!(world.try_resource::<WeatherDataRes>().is_some());
    assert!(world.try_resource::<GameTimeRes>().is_some());
}

/// `weather_system` runs (no early-return on missing resource)
/// and updates `SkyParamsRes.sun_direction` in response to a
/// changed `GameTimeRes.hour`. Pin against the initial direction
/// the procedural fallback set at install time.
#[test]
fn weather_system_animates_sun_arc_across_hours() {
    let world = run_fallback_world();
    let initial_dir = world.try_resource::<SkyParamsRes>().unwrap().sun_direction;

    // Default game time is hour=10. Advance to mid-afternoon and
    // confirm the sun moves. Direct field write because
    // `weather_system`'s own `dt × time_scale / 3600` advance is
    // tiny per frame and would need millions of dt=1.0 ticks to
    // span a few hours — manipulating the resource directly
    // bypasses the multiplier and tests the system's response
    // shape, not its real-time pacing.
    {
        let mut gt = world.try_resource_mut::<GameTimeRes>().unwrap();
        gt.hour = 16.0;
    }
    weather_system(&world, 0.0);
    let afternoon_dir = world.try_resource::<SkyParamsRes>().unwrap().sun_direction;
    assert!(
        (afternoon_dir[0] - initial_dir[0]).abs()
            + (afternoon_dir[1] - initial_dir[1]).abs()
            + (afternoon_dir[2] - initial_dir[2]).abs()
            > 0.01,
        "sun direction should differ between hour=10 init and hour=16 afternoon, \
         got initial={:?} afternoon={:?}",
        initial_dir,
        afternoon_dir,
    );

    // Push to hour=22 (night) and confirm sun_intensity drops to
    // zero — the per-hour intensity ramp is the second half of
    // the system's response that the fallback freeze was hiding.
    {
        let mut gt = world.try_resource_mut::<GameTimeRes>().unwrap();
        gt.hour = 22.0;
    }
    weather_system(&world, 0.0);
    let night_intensity = world.try_resource::<SkyParamsRes>().unwrap().sun_intensity;
    assert_eq!(
        night_intensity, 0.0,
        "sun_intensity must be 0 at hour=22 (night)",
    );
}

/// The procedural colours survive the synthetic-NAM0 round-trip:
/// `weather_system` re-writes `SkyParamsRes.sun_color` from the
/// `SKY_SUN` slot and `CellLightingRes.ambient` from `SKY_AMBIENT`.
/// Both should match the install-time procedural values exactly
/// because every TOD slot carries the same colour, so the lerp
/// of two equal endpoints is identity.
#[test]
fn weather_system_preserves_procedural_colors() {
    let world = run_fallback_world();
    weather_system(&world, 0.0);
    let sky = world.try_resource::<SkyParamsRes>().unwrap();
    assert_eq!(sky.sun_color, [1.0, 0.95, 0.8]);
    assert_eq!(sky.zenith_color, [0.15, 0.3, 0.65]);
    assert_eq!(sky.horizon_color, [0.55, 0.5, 0.42]);
    // #541 — SKY_LOWER routes through `weather_system` and lands
    // on `sky.lower_color`. The synthetic NAM0 entry seeds the
    // procedural `LOWER` constant (`HORIZON * 0.3`) at every TOD
    // slot, so the lerp identity preserves it.
    let expected_lower = [0.55_f32 * 0.3, 0.5_f32 * 0.3, 0.42_f32 * 0.3];
    for axis in 0..3 {
        assert!(
            (sky.lower_color[axis] - expected_lower[axis]).abs() < 1e-6,
            "lower_color[{axis}] = {} != {}",
            sky.lower_color[axis],
            expected_lower[axis]
        );
    }
    let cell = world.try_resource::<CellLightingRes>().unwrap();
    assert_eq!(cell.ambient, [0.15, 0.14, 0.12]);
    assert_eq!(cell.directional_color, [1.0, 0.95, 0.8]);
    assert_eq!(cell.fog_color, [0.65, 0.7, 0.8]);
}
