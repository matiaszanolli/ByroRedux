//! Player-derived pools, recurring update ticks, the animation batch, cinematics, spin / turntable / lights, head look and the player body facing tail.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;

mod combat_damage;
mod cinematics;
mod papyrus_demo_on_init;
mod recurring_update;
mod player_stats;
mod papyrus_demo_on_update;
mod animation;
mod image_space;
mod ambient_motion;
mod head_look;
mod obscript_anim;
mod player_body;

/// Player-derived pools, recurring update ticks, the animation batch, cinematics, spin / turntable / lights, head look and the player body facing tail.
pub(super) fn register_player_and_animation(scheduler: &mut Scheduler) {
    combat_damage::register_combat_damage_systems(scheduler);
    cinematics::register_cinematics_systems(scheduler);
    papyrus_demo_on_init::register_papyrus_demo_on_init_systems(scheduler);
    recurring_update::register_recurring_update_systems(scheduler);
    player_stats::register_player_stats_systems(scheduler);
    papyrus_demo_on_update::register_papyrus_demo_on_update_systems(scheduler);
    animation::register_animation_systems(scheduler);
    image_space::register_image_space_systems(scheduler);
    ambient_motion::register_ambient_motion_systems(scheduler);
    head_look::register_head_look_systems(scheduler);
    obscript_anim::register_obscript_anim_systems(scheduler);
    player_body::register_player_body_systems(scheduler);
}
