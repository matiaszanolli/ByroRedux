//! M65 — detection: the tick that produces every detector's level toward the
//! player, the actor-to-actor sight test (Q7), the line test, and the light
//! level (`docs/engine/sneak-detection.md` §4–§7), and the indicator's
//! presentation (§10). The formula itself is
//! `byroredux_core::stealth`.

mod indicator;
mod light;
mod sight;
mod tick;

pub(crate) use indicator::indicator_view;
pub(crate) use light::light_level_at;
pub(crate) use sight::{
    detection_range, has_line_of_sight, install_detection_config, line_clear, sees_body, sight_points,
    DetectionConfig,
};
/// The actor-to-actor sight test, used directly only by tests; production asks [`detects`].
#[cfg(test)]
pub(crate) use sight::sees;
pub(crate) use tick::{
    actor_value, alert_on_discovery, detection_tick_system, detects, install_stealth_rules, player_seen_by,
    unaware_of_player, PlayerActionNoise, PlayerDetection,
};
