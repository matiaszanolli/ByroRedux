//! Magic shaders (M47.3): ObScript `PlayMagicShaderVisuals` / `StopMagicShaderVisuals` /
//! `PlayMagicEffectVisuals`.
//!
//! GECK *PlayMagicShaderVisuals*: "plays the EffectShader visuals on the calling reference. Visuals will
//! play at least one cycle, regardless of duration, and if no duration is specified they play
//! indefinitely or until the art stops animating"; *StopMagicShaderVisuals*: "stops playing the
//! specified effect shader" ("there is no need to stop an effect shader that you called with a
//! duration"); *PlayMagicEffectVisuals* plays a magic effect's own visuals, which for an effect is the
//! effect shader its record names (`MGEF` DATA, "Effect Shader").
//!
//! What is drawn. An `EFSH` has a Fill/Texture ("membrane") layer with a colour and an alpha timeline —
//! fade in, full alpha, fade out to a persistent ratio, plus a pulse — an edge layer, and a particle
//! layer. The engine has no effect-shader renderer, so the shader is shown as what its fill layer does to
//! the surface it is on: the reference's materials glow in the fill colour, with the fill's alpha
//! envelope as the glow's strength ([`fill_alpha`]; the binary's tint system applies it). The fill
//! *texture*, the edge layer and the particles are not drawn. A shader with no membrane plays, for the
//! commands' and scripts' sake, and draws nothing.
//!
//! Playing shaders are presentation, not gameplay state: not saved.

use std::collections::HashMap;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{EfshFill, EsmIndex};

/// A shader playing on a reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayingShader {
    pub shader: u32,
    pub elapsed: f32,
    /// How long it was asked to play, `None` for "until stopped / until the art stops".
    pub duration: Option<f32>,
}

/// The shaders playing on a reference. `NOT_SAVED_BY_DESIGN`: a shader's look is presentation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayingShaders(pub Vec<PlayingShader>);

impl Component for PlayingShaders {
    type Storage = SparseSetStorage<Self>;
}

/// The fill layer of every `EFSH`, and each `MGEF`'s effect shader. `NOT_SAVED_BY_DESIGN`: rebuilt from
/// the records at every content load.
#[derive(Debug, Default, Clone)]
pub struct ShaderCatalog {
    fills: HashMap<u32, Option<EfshFill>>,
    effect_shaders: HashMap<u32, u32>,
}

impl Resource for ShaderCatalog {}

impl ShaderCatalog {
    pub fn from_index(index: &EsmIndex) -> Self {
        Self {
            fills: index.effect_shaders.iter().map(|(&form, shader)| (form, shader.fill.filter(|fill| !fill.no_membrane))).collect(),
            effect_shaders: index
                .magic_effects
                .values()
                .filter(|mgef| mgef.effect_shader_id != 0)
                .map(|mgef| (mgef.form_id, mgef.effect_shader_id))
                .collect(),
        }
    }

    pub fn knows(&self, shader: u32) -> bool {
        self.fills.contains_key(&shader)
    }

    pub fn fill(&self, shader: u32) -> Option<EfshFill> {
        self.fills.get(&shader).copied().flatten()
    }

    /// The effect shader a magic effect names.
    pub fn of_effect(&self, mgef: u32) -> Option<u32> {
        self.effect_shaders.get(&mgef).copied()
    }
}

/// The fill layer's alpha `elapsed` seconds into a play, `duration` the length it was asked for.
///
/// The timeline is the one the record authors: alpha rises over *fade in*, holds for *full alpha time*,
/// falls over *fade out* to the *persistent ratio* the shader then rests at, with a sinusoidal pulse of
/// the authored amplitude and frequency on top. A play that was given a duration fades out over *fade
/// out* when the duration is up ("visuals will play at least one cycle": the timeline's own length
/// stretches a short duration).
pub fn fill_alpha(fill: &EfshFill, elapsed: f32, duration: Option<f32>) -> f32 {
    let rise_end = fill.fade_in.max(0.0);
    let full_end = rise_end + fill.full_alpha_time.max(0.0);
    let fall_end = full_end + fill.fade_out.max(0.0);
    let base = if elapsed < rise_end {
        if rise_end > 0.0 { elapsed / rise_end } else { 1.0 }
    } else if elapsed < full_end {
        1.0
    } else if elapsed < fall_end {
        let t = (elapsed - full_end) / (fall_end - full_end).max(f32::EPSILON);
        1.0 + (fill.persistent_alpha_ratio - 1.0) * t
    } else {
        fill.persistent_alpha_ratio
    };
    let pulse = fill.pulse_amplitude * (std::f32::consts::TAU * fill.pulse_frequency * elapsed).sin();
    let mut alpha = (base + pulse).clamp(0.0, 1.0);
    if let Some(duration) = duration {
        let end = duration.max(fall_end);
        if elapsed > end {
            let out = fill.fade_out.max(f32::EPSILON);
            alpha *= (1.0 - (elapsed - end) / out).clamp(0.0, 1.0);
        }
    }
    alpha
}

/// How long a play lasts before it is removed: its duration (at least one timeline) plus the fade out.
fn lifetime(fill: Option<EfshFill>, duration: Option<f32>) -> Option<f32> {
    let duration = duration?;
    let fill = fill?;
    let timeline = fill.fade_in.max(0.0) + fill.full_alpha_time.max(0.0) + fill.fade_out.max(0.0);
    Some(duration.max(timeline) + fill.fade_out.max(0.0))
}

/// Play `shader` on `entity` for `duration` seconds (`None`: until stopped). `false` when the load order
/// has no such shader. Playing a shader that is already playing restarts it.
pub fn play_shader(world: &World, entity: EntityId, shader: u32, duration: Option<f32>) -> bool {
    if !world.try_resource::<ShaderCatalog>().is_some_and(|catalog| catalog.knows(shader)) {
        return false;
    }
    let Some(mut playing) = world.query_mut::<PlayingShaders>() else { return false };
    let fresh = PlayingShader { shader, elapsed: 0.0, duration };
    match playing.get_mut(entity) {
        Some(list) => match list.0.iter_mut().find(|p| p.shader == shader) {
            Some(existing) => *existing = fresh,
            None => list.0.push(fresh),
        },
        None => {
            playing.insert(entity, PlayingShaders(vec![fresh]));
        }
    }
    true
}

/// Stop `shader` on `entity`; `false` when it was not playing.
pub fn stop_shader(world: &World, entity: EntityId, shader: u32) -> bool {
    let Some(mut playing) = world.query_mut::<PlayingShaders>() else { return false };
    let Some(list) = playing.get_mut(entity) else { return false };
    let before = list.0.len();
    list.0.retain(|p| p.shader != shader);
    let stopped = list.0.len() != before;
    if list.0.is_empty() {
        playing.remove(entity);
    }
    stopped
}

/// Advance every playing shader and remove the ones whose time is up.
pub fn magic_shader_clock_system(world: &World, dt: f32) {
    let Some(catalog) = world.try_resource::<ShaderCatalog>().map(|catalog| catalog.clone()) else { return };
    let Some(mut playing) = world.query_mut::<PlayingShaders>() else { return };
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let entities: Vec<EntityId> = playing.iter().map(|(entity, _)| entity).collect();
    for entity in entities {
        if let Some(list) = playing.get_mut(entity) {
            for shader in &mut list.0 {
                shader.elapsed += dt;
            }
            list.0.retain(|p| lifetime(catalog.fill(p.shader), p.duration).is_none_or(|life| p.elapsed < life));
        }
        if playing.get(entity).is_some_and(|list| list.0.is_empty()) {
            playing.remove(entity);
        }
    }
}

/// The tint the shaders on an entity put on its surface this frame: the sum of each fill colour (0..1)
/// weighted by its alpha.
pub fn current_tint(world: &World, entity: EntityId) -> Option<[f32; 3]> {
    let catalog = world.try_resource::<ShaderCatalog>()?;
    let playing = world.get::<PlayingShaders>(entity)?;
    let mut tint = [0.0f32; 3];
    for shader in &playing.0 {
        let Some(fill) = catalog.fill(shader.shader) else { continue };
        let alpha = fill_alpha(&fill, shader.elapsed, shader.duration);
        for (channel, value) in tint.iter_mut().zip(fill.color) {
            *channel += f32::from(value) / 255.0 * alpha;
        }
    }
    Some(tint)
}

/// This subsystem's resources and components.
pub fn install(world: &mut World) {
    world.register::<PlayingShaders>();
    world.init_resource::<ShaderCatalog>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill() -> EfshFill {
        EfshFill { color: [255, 0, 0], fade_in: 1.0, full_alpha_time: 2.0, fade_out: 1.0, persistent_alpha_ratio: 0.5, pulse_amplitude: 0.0, pulse_frequency: 0.0, no_membrane: false }
    }

    #[test]
    fn the_alpha_follows_fade_in_full_fade_out_and_the_persistent_ratio() {
        let fill = fill();
        assert_eq!(fill_alpha(&fill, 0.0, None), 0.0);
        assert!((fill_alpha(&fill, 0.5, None) - 0.5).abs() < 1e-6, "halfway up");
        assert_eq!(fill_alpha(&fill, 2.0, None), 1.0, "full");
        assert!((fill_alpha(&fill, 3.5, None) - 0.75).abs() < 1e-6, "halfway down to the persistent ratio");
        assert_eq!(fill_alpha(&fill, 10.0, None), 0.5, "persistent");
    }

    #[test]
    fn a_play_with_a_duration_fades_out_at_its_end() {
        let fill = fill();
        // The timeline is 4 s; a 10 s play holds the persistent ratio until 10 s, then fades out over 1 s.
        assert_eq!(fill_alpha(&fill, 9.0, Some(10.0)), 0.5);
        assert!((fill_alpha(&fill, 10.5, Some(10.0)) - 0.25).abs() < 1e-6);
        assert_eq!(fill_alpha(&fill, 11.0, Some(10.0)), 0.0);
    }

    fn world_with_shader() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<PlayingShaders>();
        let mut catalog = ShaderCatalog::default();
        catalog.fills.insert(0x77, Some(fill()));
        catalog.fills.insert(0x78, None);
        world.insert_resource(catalog);
        let entity = world.spawn();
        (world, entity)
    }

    #[test]
    fn a_shader_plays_ticks_is_stopped_and_expires_after_its_duration() {
        let (world, entity) = world_with_shader();
        assert!(!play_shader(&world, entity, 0x99, None), "an unknown shader does not play");
        assert!(play_shader(&world, entity, 0x77, Some(1.0)));
        assert!(play_shader(&world, entity, 0x78, None), "a shader with no membrane plays and draws nothing");
        assert_eq!(current_tint(&world, entity), Some([0.0, 0.0, 0.0]));
        magic_shader_clock_system(&world, 2.0);
        let tint = current_tint(&world, entity).unwrap();
        assert!(tint[0] > 0.9 && tint[1] == 0.0, "mid-timeline the red fill is nearly full: {tint:?}");
        // Duration 1 s is stretched to the 4 s timeline, then fades out over 1 s: gone by 5 s.
        magic_shader_clock_system(&world, 3.5);
        assert_eq!(world.get::<PlayingShaders>(entity).unwrap().0.iter().map(|p| p.shader).collect::<Vec<_>>(), vec![0x78]);
        assert!(stop_shader(&world, entity, 0x78));
        assert!(world.get::<PlayingShaders>(entity).is_none());
        assert!(!stop_shader(&world, entity, 0x78));
    }
}
