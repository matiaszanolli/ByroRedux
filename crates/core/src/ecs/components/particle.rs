//! Particle emitter component — drives CPU-simulated billboard particle systems.
//!
//! Mirrors the `NiPSysEmitter` family (Box/Sphere/Cylinder/Mesh/Array) plus the
//! common modifier stack (gravity, age, color over life, grow/fade, rotation).
//! See `crates/nif/src/blocks/particle.rs` for the source-side parsers.
//!
//! The emitter owns its live particle SoA inline so the spawn / integrate /
//! expire systems can update it in place without crossing into another
//! component for the per-frame dynamic state. A separate render-data pass
//! reads the SoA + the entity's [`GlobalTransform`](super::GlobalTransform)
//! to build instance buffers for the billboard pipeline.
//!
//! Pre-#401 every parsed particle block was discarded — torches, fires,
//! magic FX rendered as invisible nodes. This component closes the loop.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Spatial spawn shape for a particle emitter. Mirrors the
/// `NiPSysBoxEmitter` / `NiPSysSphereEmitter` / `NiPSysCylinderEmitter`
/// / `NiPSysMeshEmitter` family from the parsed NIF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EmitterShape {
    /// Spawn from a single point (fallback when no shape was parsed).
    Point,
    /// Axis-aligned box centered on the emitter origin. `half_extents`
    /// in the same units as the host transform's scale.
    Box { half_extents: [f32; 3] },
    /// Sphere centered on the emitter origin.
    Sphere { radius: f32 },
    /// Cylinder along local +Z; `radius` is the disc radius, `height`
    /// is the length along Z.
    Cylinder { radius: f32, height: f32 },
}

impl EmitterShape {
    /// Sample a uniformly-distributed offset inside the shape using a
    /// supplied random scalar generator (`[0.0, 1.0)`).
    pub fn sample(self, mut rng: impl FnMut() -> f32) -> [f32; 3] {
        match self {
            Self::Point => [0.0, 0.0, 0.0],
            Self::Box { half_extents } => [
                (rng() * 2.0 - 1.0) * half_extents[0],
                (rng() * 2.0 - 1.0) * half_extents[1],
                (rng() * 2.0 - 1.0) * half_extents[2],
            ],
            Self::Sphere { radius } => {
                // Marsaglia-style rejection-free uniform-in-ball via cube-root.
                let phi = rng() * std::f32::consts::TAU;
                let cos_theta = rng() * 2.0 - 1.0;
                let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
                let r = radius * rng().cbrt();
                [
                    r * sin_theta * phi.cos(),
                    r * sin_theta * phi.sin(),
                    r * cos_theta,
                ]
            }
            Self::Cylinder { radius, height } => {
                let phi = rng() * std::f32::consts::TAU;
                let r = radius * rng().sqrt();
                [r * phi.cos(), r * phi.sin(), (rng() - 0.5) * height]
            }
        }
    }
}

/// Live particle SoA. One entry per active particle; the spawn system
/// pushes onto these vectors, the integrate system updates them, and
/// the expire system swap-removes when `age >= life`.
///
/// All vectors are kept the same length — every index `i` describes one
/// particle. Capacity grows up to [`ParticleEmitter::max_particles`].
#[derive(Debug, Default, Clone)]
pub struct ParticleSoA {
    /// World-space particle position. Spawned in world-space so the
    /// renderer doesn't need the host transform when building instance
    /// data; the host transform is sampled only at spawn time.
    pub positions: Vec<[f32; 3]>,
    /// World-space velocity. Integrated each frame.
    pub velocities: Vec<[f32; 3]>,
    /// Age in seconds since spawn.
    pub ages: Vec<f32>,
    /// Lifespan in seconds. Particle expires when `age >= life`.
    pub lifes: Vec<f32>,
    /// Per-particle base RGBA color sampled from the emitter's color
    /// curve at spawn (interpolated to fade-over-life by the renderer).
    pub colors: Vec<[f32; 4]>,
    /// Per-particle base size in world units (scaled by `size_over_life`
    /// in the renderer).
    pub sizes: Vec<f32>,
}

impl ParticleSoA {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Append a new particle. All SoA arrays grow by one.
    pub fn push(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        life: f32,
        color: [f32; 4],
        size: f32,
    ) {
        self.positions.push(position);
        self.velocities.push(velocity);
        self.ages.push(0.0);
        self.lifes.push(life);
        self.colors.push(color);
        self.sizes.push(size);
    }

    /// O(1) swap-remove at index. All SoA arrays shrink by one.
    pub fn swap_remove(&mut self, idx: usize) {
        self.positions.swap_remove(idx);
        self.velocities.swap_remove(idx);
        self.ages.swap_remove(idx);
        self.lifes.swap_remove(idx);
        self.colors.swap_remove(idx);
        self.sizes.swap_remove(idx);
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.velocities.clear();
        self.ages.clear();
        self.lifes.clear();
        self.colors.clear();
        self.sizes.clear();
    }
}

/// One authored force field captured from a `NiPSys*FieldModifier` on
/// the source NIF's modifier chain. The particle simulator folds each
/// active field into the per-particle velocity step every frame.
///
/// All centers / axes are stored in the **emitter's local frame at
/// spawn time** — the field's `field_object_ref` originally targets a
/// NiNode whose world position the simulator would re-anchor to. The
/// importer collapses that to the emitter origin (origin-anchored
/// fields are by far the dominant authoring pattern in vanilla
/// Bethesda content); a follow-up may add a `world_anchor: [f32; 3]`
/// slot if multi-anchor fields surface in the wild.
///
/// `decay` / `falloff` first-pass model: distance is divided into the
/// effective force as `force / (1 + decay * dist^attenuation)` for the
/// Gravity/Vortex/Radial variants, matching the Gamebryo
/// `NiPSysFieldModifier.attenuation` semantics qualitatively. These
/// curves are tuneable approximations rather than exact reproductions
/// — vanilla NIF reference data may shift the exponents at a future
/// pass. See #984 / NIF-D5-ORPHAN-A2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParticleForceField {
    /// Point-source gravity / directional gravity. `direction` is the
    /// gravity vector in the emitter's local frame; `strength` scales
    /// magnitude per unit time. `decay` divides force by
    /// `(1 + decay * dist)` for point falloff at increasing
    /// distance from the emitter origin; pass `0.0` for uniform.
    Gravity {
        direction: [f32; 3],
        strength: f32,
        decay: f32,
    },
    /// Rotational force around `axis` (local frame). `strength` is the
    /// tangential acceleration magnitude at unit distance; `decay`
    /// is the inverse falloff exponent.
    Vortex {
        axis: [f32; 3],
        strength: f32,
        decay: f32,
    },
    /// Velocity-proportional damping. `strength` is the damping rate
    /// (`v -= v * strength * dt`). When `direction` is non-zero the
    /// damping projects velocity onto the direction axis first
    /// (authored anisotropic drag, rare).
    Drag {
        strength: f32,
        direction: [f32; 3],
    },
    /// Pseudo-random per-particle force. `scale` is the per-frame
    /// acceleration magnitude, `frequency` drives the noise sampling
    /// rate (jitter cadence). The simulator's first-pass uses a fast
    /// hash-based noise; a follow-up may swap in curl noise.
    Turbulence { frequency: f32, scale: f32 },
    /// Directional wind. `direction` is the wind vector (local frame),
    /// `strength` scales force magnitude. `falloff` divides by
    /// `(1 + falloff * dist)` so distant particles feel weaker wind
    /// (matches `NiPSysFieldModifier.attenuation` semantics).
    Air {
        direction: [f32; 3],
        strength: f32,
        falloff: f32,
    },
    /// Radial push (positive `strength`) or pull (negative). Centered
    /// at the emitter origin; `falloff` divides by `(1 + falloff *
    /// dist)`. `NiPSysRadialFieldModifier.radial_type`'s linear /
    /// quadratic / constant enum collapses into the same falloff
    /// scalar at import time.
    Radial { strength: f32, falloff: f32 },
}

/// CPU-driven particle emitter. Attach to any entity that also has a
/// [`Transform`](super::Transform) + [`GlobalTransform`](super::GlobalTransform).
/// The emitter spawns particles in **world space** (transformed at spawn
/// time by the entity's world position) so subsequent host-entity motion
/// doesn't drag old particles along — matching the legacy Gamebryo
/// behavior where particles detach into world space the moment they spawn.
#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    /// Spawn-shape parameters.
    pub shape: EmitterShape,
    /// Particles spawned per second. Fractional rates accumulate across
    /// frames in [`ParticleEmitter::spawn_accumulator`].
    pub rate: f32,
    /// Hard cap on simultaneous live particles. Spawn requests above
    /// the cap are dropped.
    pub max_particles: u32,
    /// Average lifespan in seconds. Per-particle randomized by
    /// [`ParticleEmitter::life_variation`].
    pub life: f32,
    /// Lifespan jitter in seconds (uniform `[life - var/2, life + var/2)`).
    pub life_variation: f32,
    /// Initial speed magnitude in world units / second.
    pub speed: f32,
    /// Per-particle speed jitter (uniform `[speed - var/2, speed + var/2)`).
    pub speed_variation: f32,
    /// Local +Z opening angle in radians. 0 = straight up; π/2 = full hemisphere.
    pub declination: f32,
    /// Declination jitter in radians.
    pub declination_variation: f32,
    /// Per-frame world acceleration applied to every live particle (e.g.
    /// `[0, 0, -9.8]` for true gravity, `[0, 0, +1.5]` for a buoyant
    /// flame that floats upward).
    pub gravity: [f32; 3],
    /// Spawn color (sampled at spawn — particles linearly fade alpha to
    /// 0 over their life by default).
    pub start_color: [f32; 4],
    /// End color. Renderer LERPs between `start_color` and `end_color`
    /// against `age / life`.
    pub end_color: [f32; 4],
    /// Spawn size in world units.
    pub start_size: f32,
    /// End size in world units. Renderer LERPs between `start_size` and
    /// `end_size` against `age / life`.
    pub end_size: f32,
    /// Texture path resolved from the parent NiTexturingProperty / shader
    /// (e.g. `"textures\\fx\\flame01.dds"`). The renderer looks this up
    /// in the [`crate::texture::FixedString`]-keyed texture registry.
    pub texture_path: Option<String>,
    /// Source-blend factor (Vulkan enum value). Default: SRC_ALPHA (6).
    pub src_blend: u8,
    /// Destination-blend factor. Default: ONE (1) for additive blending,
    /// which is what magic FX and flames use.
    pub dst_blend: u8,
    /// Fractional spawn carry between frames. Updated by the spawn
    /// system each tick: integer-floor goes out as the spawn count, the
    /// fractional remainder rolls forward.
    pub spawn_accumulator: f32,
    /// Authored force fields from the source NIF's
    /// `NiPSys*FieldModifier` chain. Empty for emitters spawned via a
    /// heuristic preset (`torch_flame()`, `smoke()`, etc.) or for NIFs
    /// that don't author field modifiers — the simulator falls back
    /// to `gravity` alone in that case. See [`ParticleForceField`]
    /// and #984 / NIF-D5-ORPHAN-A2.
    pub force_fields: Vec<ParticleForceField>,
    /// Live particle SoA. The emitter owns the dynamic state inline.
    pub particles: ParticleSoA,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            shape: EmitterShape::Point,
            rate: 30.0,
            max_particles: 256,
            life: 1.5,
            life_variation: 0.5,
            speed: 0.0,
            speed_variation: 0.0,
            declination: 0.0,
            declination_variation: 0.0,
            gravity: [0.0, 0.0, 0.0],
            start_color: [1.0, 1.0, 1.0, 1.0],
            end_color: [1.0, 1.0, 1.0, 0.0],
            start_size: 4.0,
            end_size: 4.0,
            texture_path: None,
            // Additive blending — most flame/glow effects use this and it
            // composites correctly without back-to-front sorting.
            src_blend: 6, // SRC_ALPHA
            dst_blend: 1, // ONE
            spawn_accumulator: 0.0,
            force_fields: Vec::new(),
            particles: ParticleSoA::default(),
        }
    }
}

impl ParticleEmitter {
    /// Heuristic preset for a small flickering torch flame. Used by the
    /// NIF importer when a `NiParticleSystem` is attached to a node
    /// whose name contains `torch`/`fire`/`flame`. The preset still
    /// drives shape / rate / colour for emitter blocks whose parsers
    /// remain opaque (`NiPSysBlock`), but authored force fields are
    /// now wired through `force_fields` post-#984 / NIF-D5-ORPHAN-A2;
    /// the simulator integrates `gravity` + `force_fields` together.
    pub fn torch_flame() -> Self {
        Self {
            shape: EmitterShape::Sphere { radius: 1.2 },
            rate: 35.0,
            max_particles: 128,
            life: 0.8,
            life_variation: 0.3,
            speed: 4.5,
            speed_variation: 1.0,
            declination: 0.25,
            declination_variation: 0.15,
            gravity: [0.0, 0.0, 12.0], // upward buoyancy
            start_color: [1.0, 0.65, 0.18, 1.0],
            end_color: [0.9, 0.15, 0.0, 0.0],
            start_size: 5.0,
            end_size: 9.0,
            texture_path: None,
            src_blend: 6,
            dst_blend: 1,
            spawn_accumulator: 0.0,
            force_fields: Vec::new(),
            particles: ParticleSoA::default(),
        }
    }

    /// Heuristic preset for grey smoke. Used when the host node name
    /// contains `smoke` / `steam` / `ash`.
    ///
    /// Pre-#707 the start color was a flat dark grey (0.5/0.5/0.5)
    /// which on a dark interior backdrop (e.g. Whiterun Dragonsreach)
    /// rendered as nearly-black columns. Warmed the base toward the
    /// embers' hot-orange tint and lifted the value so smoke is
    /// readable against dim cell-light environments. End color
    /// remains a cool darkening as the particle rises and cools
    /// off — matches the visual intuition of fire smoke.
    pub fn smoke() -> Self {
        Self {
            shape: EmitterShape::Sphere { radius: 1.0 },
            rate: 15.0,
            max_particles: 96,
            life: 2.5,
            life_variation: 0.7,
            speed: 6.0,
            speed_variation: 1.2,
            declination: 0.1,
            declination_variation: 0.1,
            gravity: [0.0, 0.0, 6.0],
            start_color: [0.65, 0.55, 0.45, 0.7],
            end_color: [0.25, 0.25, 0.27, 0.0],
            start_size: 8.0,
            end_size: 22.0,
            texture_path: None,
            src_blend: 6, // SRC_ALPHA
            dst_blend: 7, // ONE_MINUS_SRC_ALPHA — non-additive smoke
            spawn_accumulator: 0.0,
            force_fields: Vec::new(),
            particles: ParticleSoA::default(),
        }
    }

    /// Heuristic preset for hot embers / sparks rising off a fire.
    /// Used when the host node name contains `spark` / `ember` /
    /// `cinder`. Visually distinguishes from `torch_flame()` (which
    /// is the larger softer flame body): embers are smaller, faster,
    /// brighter at start, fade to dim red, additive blend so they
    /// glow against the smoke column.
    ///
    /// Pre-#707 these names fell into the `torch_flame()` fallback,
    /// producing oversized dim glows where Bethesda authored small
    /// bright glints. This is a heuristic band-aid until the proper
    /// `NiPSysColorModifier` → `NiColorData` data path lands (see
    /// #707 follow-up).
    pub fn embers() -> Self {
        Self {
            shape: EmitterShape::Sphere { radius: 0.6 },
            rate: 22.0,
            max_particles: 96,
            life: 1.4,
            life_variation: 0.5,
            speed: 7.0,
            speed_variation: 1.5,
            declination: 0.15,
            declination_variation: 0.1,
            gravity: [0.0, 0.0, 8.0], // strong upward buoyancy
            start_color: [1.0, 0.55, 0.18, 1.0],
            end_color: [0.6, 0.05, 0.0, 0.0],
            start_size: 1.5,
            end_size: 0.5,
            texture_path: None,
            src_blend: 6,
            dst_blend: 1, // additive — glints against smoke
            spawn_accumulator: 0.0,
            force_fields: Vec::new(),
            particles: ParticleSoA::default(),
        }
    }

    /// Heuristic preset for blue magical sparkles. Used when the host
    /// node name contains `magic` / `enchant` / `sparkle`.
    pub fn magic_sparkles() -> Self {
        Self {
            shape: EmitterShape::Sphere { radius: 2.0 },
            rate: 40.0,
            max_particles: 192,
            life: 1.0,
            life_variation: 0.4,
            speed: 8.0,
            speed_variation: 2.0,
            declination: std::f32::consts::FRAC_PI_2,
            declination_variation: std::f32::consts::FRAC_PI_4,
            gravity: [0.0, 0.0, 0.0],
            start_color: [0.4, 0.7, 1.0, 1.0],
            end_color: [0.1, 0.3, 0.9, 0.0],
            start_size: 3.0,
            end_size: 1.0,
            texture_path: None,
            src_blend: 6,
            dst_blend: 1, // additive
            spawn_accumulator: 0.0,
            force_fields: Vec::new(),
            particles: ParticleSoA::default(),
        }
    }
}

impl Component for ParticleEmitter {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deterministic_rng(values: Vec<f32>) -> impl FnMut() -> f32 {
        let mut iter = values.into_iter().cycle();
        move || iter.next().unwrap()
    }

    #[test]
    fn box_sample_spans_extents() {
        let shape = EmitterShape::Box {
            half_extents: [2.0, 3.0, 4.0],
        };
        // rng yielding 1.0 → upper corner; 0.0 → lower corner.
        let upper = shape.sample(deterministic_rng(vec![1.0]));
        assert!((upper[0] - 2.0).abs() < 1e-5);
        assert!((upper[1] - 3.0).abs() < 1e-5);
        assert!((upper[2] - 4.0).abs() < 1e-5);
        let lower = shape.sample(deterministic_rng(vec![0.0]));
        assert!((lower[0] + 2.0).abs() < 1e-5);
        assert!((lower[1] + 3.0).abs() < 1e-5);
        assert!((lower[2] + 4.0).abs() < 1e-5);
    }

    #[test]
    fn sphere_sample_within_radius() {
        let shape = EmitterShape::Sphere { radius: 5.0 };
        for trial in 0..32 {
            let seed = (trial as f32 + 1.0) * 0.123;
            let mut counter = 0;
            let p = shape.sample(|| {
                counter += 1;
                ((seed * counter as f32).sin() * 0.5 + 0.5).clamp(0.0, 0.999)
            });
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!(r <= 5.0 + 1e-4, "sphere sample out of bounds: r={r}");
        }
    }

    #[test]
    fn soa_swap_remove_keeps_arrays_in_sync() {
        let mut soa = ParticleSoA::default();
        soa.push([1.0, 0.0, 0.0], [0.0; 3], 1.0, [1.0; 4], 1.0);
        soa.push([2.0, 0.0, 0.0], [0.0; 3], 2.0, [1.0; 4], 2.0);
        soa.push([3.0, 0.0, 0.0], [0.0; 3], 3.0, [1.0; 4], 3.0);
        assert_eq!(soa.len(), 3);
        soa.swap_remove(0);
        assert_eq!(soa.len(), 2);
        assert_eq!(soa.positions[0][0], 3.0);
        assert_eq!(soa.lifes[0], 3.0);
        assert_eq!(soa.sizes[0], 3.0);
    }

    #[test]
    fn presets_have_finite_nondefault_config() {
        for preset in [
            ParticleEmitter::torch_flame(),
            ParticleEmitter::smoke(),
            ParticleEmitter::magic_sparkles(),
            ParticleEmitter::embers(),
        ] {
            assert!(preset.rate > 0.0);
            assert!(preset.life > 0.0);
            assert!(preset.max_particles > 0);
            assert!(preset.start_size > 0.0);
            assert!(preset.start_color[3] > 0.0);
        }
    }

    /// #707 (band-aid) — embers must be visually distinguishable from
    /// the torch flame: smaller, additive, brighter at start. Pin the
    /// invariants so a future tweak can't accidentally collapse the
    /// two presets.
    #[test]
    fn embers_preset_is_distinct_from_torch_flame() {
        let embers = ParticleEmitter::embers();
        let flame = ParticleEmitter::torch_flame();
        assert!(
            embers.start_size < flame.start_size,
            "embers must be smaller than the flame body"
        );
        assert_eq!(embers.dst_blend, 1, "embers use additive blend");
        // Both embers and flame are warm — pin that the embers' start
        // is at least as warm/bright as the flame's start so the
        // visual hierarchy (bright glints over a softer flame) holds.
        assert!(embers.start_color[0] >= flame.start_color[0]);
        // End color fades to alpha 0 — without that the embers would
        // accumulate into a static glow rather than rising-and-fading.
        assert_eq!(embers.end_color[3], 0.0);
    }

    /// #707 (band-aid) — the smoke preset's start color was darkened
    /// to the point of rendering near-black against dim interiors
    /// (Whiterun Dragonsreach hearth screenshot). Pin the brightness
    /// floor so a future "darken smoke" change doesn't silently
    /// regress that scene.
    #[test]
    fn smoke_preset_start_brightness_above_black_floor() {
        let smoke = ParticleEmitter::smoke();
        let luma = 0.2126 * smoke.start_color[0]
            + 0.7152 * smoke.start_color[1]
            + 0.0722 * smoke.start_color[2];
        assert!(
            luma > 0.45,
            "smoke start luma {luma} too dark — would vanish into a dim cell"
        );
        // Warm tint at the base (R >= B) so smoke off a fire reads
        // as fire-smoke rather than industrial chimney smoke.
        assert!(
            smoke.start_color[0] >= smoke.start_color[2],
            "smoke base should be warm-tinted (R >= B)"
        );
    }
}
