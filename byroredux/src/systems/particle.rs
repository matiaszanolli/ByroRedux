//! CPU particle integration + spawn.

use byroredux_core::ecs::{GlobalTransform, ParticleEmitter, ParticleForceField, TotalTime, World};

/// Convert a list of imported NIF force fields (Z-up local space) to
/// engine Y-up local space. Mirrors the per-axis swap used elsewhere
/// (translation: `[x, z, -y]`; direction: same). Applied at scene-
/// build time so the per-particle inner loop in
/// [`integrate_force_fields`] doesn't pay the conversion every frame.
/// See #984 / NIF-D5-ORPHAN-A2.
pub fn convert_force_fields_zup_to_yup(
    src: &[byroredux_nif::import::ImportedParticleForceField],
) -> Vec<ParticleForceField> {
    use byroredux_nif::import::ImportedParticleForceField as I;
    fn zup_to_yup(v: [f32; 3]) -> [f32; 3] {
        [v[0], v[2], -v[1]]
    }
    src.iter()
        .map(|f| match *f {
            I::Gravity {
                direction,
                strength,
                decay,
            } => ParticleForceField::Gravity {
                direction: zup_to_yup(direction),
                strength,
                decay,
            },
            I::Vortex {
                axis,
                strength,
                decay,
            } => ParticleForceField::Vortex {
                axis: zup_to_yup(axis),
                strength,
                decay,
            },
            I::Drag {
                strength,
                direction,
                use_direction,
            } => ParticleForceField::Drag {
                strength,
                // When use_direction is false the axis is unused but
                // we still keep its (converted) value for stability;
                // an all-zero axis would degenerate the simulator's
                // dot-product projection.
                direction: if use_direction {
                    zup_to_yup(direction)
                } else {
                    [0.0, 0.0, 0.0]
                },
            },
            I::Turbulence { frequency, scale } => ParticleForceField::Turbulence { frequency, scale },
            I::Air {
                direction,
                strength,
                falloff,
            } => ParticleForceField::Air {
                direction: zup_to_yup(direction),
                strength,
                falloff,
            },
            I::Radial { strength, falloff } => ParticleForceField::Radial { strength, falloff },
        })
        .collect()
}

/// Apply every `ParticleForceField` on the emitter to one particle's
/// velocity for a `dt` step. Origin-anchored — distances are measured
/// from the emitter's world-space spawn origin (`host_translation`).
/// Force-field math is first-pass; vanilla NIF reference data may
/// shift the falloff exponents at a future pass. See #984.
fn integrate_force_fields(
    fields: &[ParticleForceField],
    host_translation: [f32; 3],
    position: [f32; 3],
    velocity: &mut [f32; 3],
    age: f32,
    dt: f32,
) {
    if fields.is_empty() {
        return;
    }
    let rx = position[0] - host_translation[0];
    let ry = position[1] - host_translation[1];
    let rz = position[2] - host_translation[2];
    let dist = (rx * rx + ry * ry + rz * rz).sqrt();
    for f in fields {
        match *f {
            ParticleForceField::Gravity {
                direction,
                strength,
                decay,
            } => {
                let atten = 1.0 / (1.0 + decay * dist);
                let s = strength * atten * dt;
                velocity[0] += direction[0] * s;
                velocity[1] += direction[1] * s;
                velocity[2] += direction[2] * s;
            }
            ParticleForceField::Vortex {
                axis,
                strength,
                decay,
            } => {
                // Tangential force = axis × radial. Pre-normalise the
                // axis once per particle so authored magnitudes
                // stay scale-invariant against axis vector length.
                let an = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
                if an <= 1e-6 {
                    continue;
                }
                let ax = axis[0] / an;
                let ay = axis[1] / an;
                let az = axis[2] / an;
                let tx = ay * rz - az * ry;
                let ty = az * rx - ax * rz;
                let tz = ax * ry - ay * rx;
                let atten = 1.0 / (1.0 + decay * dist);
                let s = strength * atten * dt;
                velocity[0] += tx * s;
                velocity[1] += ty * s;
                velocity[2] += tz * s;
            }
            ParticleForceField::Drag {
                strength,
                direction,
            } => {
                // Isotropic when direction is zero; otherwise project
                // velocity onto direction and damp that component only.
                let dn =
                    (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
                let damping = (strength * dt).min(1.0);
                if dn <= 1e-6 {
                    velocity[0] -= velocity[0] * damping;
                    velocity[1] -= velocity[1] * damping;
                    velocity[2] -= velocity[2] * damping;
                } else {
                    let dx = direction[0] / dn;
                    let dy = direction[1] / dn;
                    let dz = direction[2] / dn;
                    let along = velocity[0] * dx + velocity[1] * dy + velocity[2] * dz;
                    velocity[0] -= along * dx * damping;
                    velocity[1] -= along * dy * damping;
                    velocity[2] -= along * dz * damping;
                }
            }
            ParticleForceField::Turbulence { frequency, scale } => {
                // Cheap hash-based pseudo-noise — three independent
                // axes seeded by particle position + age * frequency.
                // Outputs a signed unit-range scalar per axis. A
                // follow-up may swap in curl noise for visually
                // smoother gusts (#984 follow-up).
                fn hash01(x: u32) -> f32 {
                    let mut h = x.wrapping_mul(2654435761);
                    h ^= h >> 13;
                    h = h.wrapping_mul(0x85EB_CA77);
                    h ^= h >> 16;
                    (h as f32) / (u32::MAX as f32)
                }
                let t = age * frequency;
                let seed_x = ((position[0] * 17.0 + t) as i32 as u32).wrapping_add(0xA341_1B07);
                let seed_y = ((position[1] * 17.0 + t * 1.3) as i32 as u32).wrapping_add(0xB571_3C9F);
                let seed_z = ((position[2] * 17.0 + t * 0.7) as i32 as u32).wrapping_add(0xC621_5DD3);
                let nx = hash01(seed_x) * 2.0 - 1.0;
                let ny = hash01(seed_y) * 2.0 - 1.0;
                let nz = hash01(seed_z) * 2.0 - 1.0;
                let s = scale * dt;
                velocity[0] += nx * s;
                velocity[1] += ny * s;
                velocity[2] += nz * s;
            }
            ParticleForceField::Air {
                direction,
                strength,
                falloff,
            } => {
                let atten = 1.0 / (1.0 + falloff * dist);
                let s = strength * atten * dt;
                velocity[0] += direction[0] * s;
                velocity[1] += direction[1] * s;
                velocity[2] += direction[2] * s;
            }
            ParticleForceField::Radial { strength, falloff } => {
                if dist <= 1e-6 {
                    continue;
                }
                let inv = 1.0 / dist;
                let dx = rx * inv;
                let dy = ry * inv;
                let dz = rz * inv;
                let atten = 1.0 / (1.0 + falloff * dist);
                let s = strength * atten * dt;
                velocity[0] += dx * s;
                velocity[1] += dy * s;
                velocity[2] += dz * s;
            }
        }
    }
}

/// CPU particle system: spawn at the configured rate, integrate
/// velocity + gravity, expire by age. Runs in `Update` after the
/// scene-graph propagation has settled the host transforms — particles
/// spawn in **world space** by sampling the host's `GlobalTransform`
/// translation at spawn time, which matches the legacy Gamebryo
/// behavior where particles detach from their host once emitted (so
/// the host can rotate / move without dragging old smoke/fire along).
///
/// See #401 — pre-fix every parsed particle block was discarded and
/// every torch / fire / magic FX rendered as an invisible node.
///
/// The PRNG is a tiny xorshift seeded by `(entity, frame_count)` so
/// per-emitter behavior is deterministic for tests and replays without
/// requiring a `rand` dependency. Particles burn ~10 FLOPs each per
/// integration step — well below the budget for the typical worst-case
/// (a brazier emitter at 64 live particles).
pub(crate) fn particle_system(world: &World, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let total_time_secs = world.resource::<TotalTime>().0;
    let frame_seed = (total_time_secs * 1000.0) as u64;

    // Read each emitter entity's world-space spawn origin from
    // GlobalTransform. We only mutate ParticleEmitter (live SoA +
    // accumulator), so the GlobalTransform query stays read-only and
    // doesn't fight any other PostUpdate writer.
    let Some((gt_q, mut em_q)) = world.query_2_mut::<GlobalTransform, ParticleEmitter>() else {
        return;
    };

    for (entity, em) in em_q.iter_mut() {
        let host_translation = match gt_q.get(entity) {
            Some(g) => g.translation,
            None => continue,
        };

        // Tiny xorshift32, seeded per-emitter per-frame. Avoids a `rand`
        // dependency and gives reproducible behavior under fixed-step
        // tests. The lower bits of `entity.index()` jitter sufficiently
        // across emitters spawned in the same frame.
        let mut state: u32 = (frame_seed as u32).wrapping_add(entity.wrapping_mul(2654435761));
        if state == 0 {
            state = 0x9E37_79B9; // golden-ratio fallback
        }
        let mut rng = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32) / (u32::MAX as f32)
        };

        // 1. Integrate live particles (velocity + gravity + authored
        //    NIF force fields, #984) and age them. The force-field
        //    loop short-circuits when `force_fields` is empty so
        //    heuristic-preset emitters (no authored modifiers) stay
        //    on the pre-#984 fast path.
        let host = [host_translation.x, host_translation.y, host_translation.z];
        let len = em.particles.len();
        for i in 0..len {
            em.particles.velocities[i][0] += em.gravity[0] * dt;
            em.particles.velocities[i][1] += em.gravity[1] * dt;
            em.particles.velocities[i][2] += em.gravity[2] * dt;
            integrate_force_fields(
                &em.force_fields,
                host,
                em.particles.positions[i],
                &mut em.particles.velocities[i],
                em.particles.ages[i],
                dt,
            );
            em.particles.positions[i][0] += em.particles.velocities[i][0] * dt;
            em.particles.positions[i][1] += em.particles.velocities[i][1] * dt;
            em.particles.positions[i][2] += em.particles.velocities[i][2] * dt;
            em.particles.ages[i] += dt;
        }

        // 2. Expire particles whose age exceeds their lifespan. Iterate
        //    backwards so swap_remove doesn't skip survivors.
        let mut i = em.particles.len();
        while i > 0 {
            i -= 1;
            if em.particles.ages[i] >= em.particles.lifes[i] {
                em.particles.swap_remove(i);
            }
        }

        // 3. Spawn new particles at the configured rate. Fractional
        //    spawns accumulate across frames so a 30 Hz emitter under a
        //    60 fps frame still averages exactly 30 spawns/sec.
        em.spawn_accumulator += em.rate * dt;
        let spawn_count = em.spawn_accumulator.floor() as i32;
        em.spawn_accumulator -= spawn_count as f32;

        let cap = em.max_particles as usize;
        for _ in 0..spawn_count.max(0) {
            if em.particles.len() >= cap {
                break;
            }
            let local_offset = em.shape.sample(&mut rng);
            let world_pos = [
                host_translation.x + local_offset[0],
                host_translation.y + local_offset[1],
                host_translation.z + local_offset[2],
            ];

            // Build a velocity vector inside the declination cone around
            // local +Z, then jitter speed.
            let phi = rng() * std::f32::consts::TAU;
            let dec = em.declination + (rng() - 0.5) * em.declination_variation;
            let sin_dec = dec.sin();
            let cos_dec = dec.cos();
            let dir = [sin_dec * phi.cos(), sin_dec * phi.sin(), cos_dec];
            let speed = em.speed + (rng() - 0.5) * em.speed_variation;
            let vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];

            let life = em.life + (rng() - 0.5) * em.life_variation;
            // Guard against zero/negative life so the expire pass can
            // handle the particle correctly on the very next tick.
            let life = life.max(0.05);

            em.particles
                .push(world_pos, vel, life, em.start_color, em.start_size);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for `particle_system` — issue #401.
    use super::*;
    use byroredux_core::ecs::resources::TotalTime;
    use byroredux_core::ecs::{EmitterShape, ParticleEmitter, World};
    use byroredux_core::math::Vec3;

    fn world_with_emitter(em: ParticleEmitter, host_pos: Vec3) -> (World, u32) {
        let mut world = World::new();
        world.insert_resource(TotalTime(0.0));
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(host_pos, byroredux_core::math::Quat::IDENTITY, 1.0),
        );
        world.insert(e, em);
        (world, e)
    }

    #[test]
    fn spawn_rate_accumulates_to_integer_count_per_frame() {
        // 30 spawns/sec, 0.5s frame → 15 particles per tick.
        let mut em = ParticleEmitter::default();
        em.rate = 30.0;
        em.life = 100.0; // never expire
        em.max_particles = 1024;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.5);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 15);
    }

    #[test]
    fn fractional_rate_carries_across_frames() {
        // 25 spawns/sec at 1/60 s frames → 0.4167 spawn/frame.
        // After 6 frames we should see 2-3 spawns (fractional carry).
        let mut em = ParticleEmitter::default();
        em.rate = 25.0;
        em.life = 100.0;
        em.max_particles = 1024;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        for _ in 0..6 {
            particle_system(&world, 1.0 / 60.0);
        }
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Floor(6 / 60 * 25) = floor(2.5) = 2.
        assert_eq!(em.particles.len(), 2);
    }

    #[test]
    fn cap_at_max_particles_drops_extra_spawns() {
        let mut em = ParticleEmitter::default();
        em.rate = 1000.0;
        em.life = 100.0;
        em.max_particles = 8;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 1.0);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 8);
    }

    #[test]
    fn expire_pass_drops_particles_past_their_life() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0; // no new spawns
        em.life = 0.5;
        em.max_particles = 8;
        // Pre-seed two particles, one already past its life.
        em.particles
            .push([0.0, 0.0, 0.0], [0.0; 3], 0.5, [1.0; 4], 1.0);
        em.particles.ages[0] = 1.0; // already expired
        em.particles
            .push([0.0, 0.0, 0.0], [0.0; 3], 1.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 1);
        assert!((em.particles.lifes[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn integration_applies_velocity_and_gravity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.life = 100.0;
        em.gravity = [0.0, 0.0, -10.0];
        em.particles
            .push([0.0, 0.0, 0.0], [1.0, 0.0, 5.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.5);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // After 0.5s with v=(1,0,5) and a=(0,0,-10):
        // velocity_after = (1, 0, 5 + (-10)*0.5) = (1, 0, 0)
        // position_after = (0,0,0) + new_velocity*dt = (0.5, 0, 0)
        // Note: semi-implicit Euler — gravity updates v first, then x.
        assert!((em.particles.velocities[0][2] - 0.0).abs() < 1e-5);
        assert!((em.particles.positions[0][0] - 0.5).abs() < 1e-5);
        assert!((em.particles.positions[0][2] - 0.0).abs() < 1e-5);
    }

    // ── #984 / NIF-D5-ORPHAN-A2 — force-field per-variant fixtures ─

    /// Spawn one particle, run one tick, and assert the velocity
    /// matches the analytical expectation for a single Gravity field.
    /// Force: F = direction * strength / (1 + decay * dist) per
    /// `integrate_force_fields`.
    #[test]
    fn force_field_gravity_drives_velocity_toward_direction() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Gravity {
            direction: [0.0, -10.0, 0.0],
            strength: 1.0,
            decay: 0.0,
        }];
        em.particles
            .push([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Δv = -10 * 1.0 * 0.1 = -1.0 on Y.
        assert!((em.particles.velocities[0][1] - (-1.0)).abs() < 1e-5);
    }

    /// Vortex field on Y axis should impart tangential velocity in
    /// the X–Z plane. Particle at (1,0,0) with axis (0,1,0) and
    /// strength 1.0 should accelerate toward +Z (right-hand rule).
    #[test]
    fn force_field_vortex_drives_tangential_velocity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Vortex {
            axis: [0.0, 1.0, 0.0],
            strength: 1.0,
            decay: 0.0,
        }];
        em.particles
            .push([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Tangential vec = axis × radial = (0,1,0) × (1,0,0) = (0,0,-1).
        // Δv_z = -1 * 1.0 * 0.1 = -0.1.
        assert!((em.particles.velocities[0][2] - (-0.1)).abs() < 1e-5);
        // X / Y components stay near zero (no force on those axes).
        assert!(em.particles.velocities[0][0].abs() < 1e-5);
        assert!(em.particles.velocities[0][1].abs() < 1e-5);
    }

    /// Isotropic drag (no direction) damps every velocity component
    /// proportionally. Δv = -v * strength * dt.
    #[test]
    fn force_field_isotropic_drag_damps_velocity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Drag {
            strength: 0.5,
            direction: [0.0, 0.0, 0.0],
        }];
        em.particles
            .push([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // damping = 0.5 * 0.1 = 0.05; new vx = 10 * (1 - 0.05) = 9.5.
        assert!((em.particles.velocities[0][0] - 9.5).abs() < 1e-5);
    }

    /// Turbulence must perturb the velocity by something non-zero
    /// (we don't pin the exact value — the noise hash is implementation
    /// detail). Confirms the branch is wired rather than no-op.
    #[test]
    fn force_field_turbulence_perturbs_velocity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Turbulence {
            frequency: 5.0,
            scale: 10.0,
        }];
        em.particles
            .push([0.5, 0.5, 0.5], [0.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        let v = em.particles.velocities[0];
        let mag2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        assert!(mag2 > 0.0, "turbulence must perturb velocity, got {v:?}");
    }

    /// Air imparts directional acceleration proportional to strength,
    /// modulated by `1 / (1 + falloff * dist)`. With falloff=0 the
    /// distance is ignored and the math is identical to Gravity.
    #[test]
    fn force_field_air_drives_directional_velocity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Air {
            direction: [3.0, 0.0, 0.0],
            strength: 1.0,
            falloff: 0.0,
        }];
        em.particles
            .push([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Δv_x = 3 * 1.0 * 0.1 = 0.3.
        assert!((em.particles.velocities[0][0] - 0.3).abs() < 1e-5);
    }

    /// Radial field pushes outward from the emitter origin. With a
    /// particle at (2,0,0) and strength 1.0, falloff 0.0, the velocity
    /// gains +0.1 along X over 0.1 s.
    #[test]
    fn force_field_radial_drives_outward_velocity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, 0.0];
        em.force_fields = vec![ParticleForceField::Radial {
            strength: 1.0,
            falloff: 0.0,
        }];
        em.particles
            .push([2.0, 0.0, 0.0], [0.0, 0.0, 0.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // unit-dir along X = (1, 0, 0); Δv_x = 1 * 1.0 * 0.1 = 0.1.
        assert!((em.particles.velocities[0][0] - 0.1).abs() < 1e-5);
    }

    /// Empty force_fields list keeps the pre-#984 fast path —
    /// gravity-only integration. Pins that the new branch doesn't
    /// regress emitters that authored no field modifiers (the vast
    /// majority of vanilla content).
    #[test]
    fn empty_force_fields_match_pre_984_gravity_only_behaviour() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.gravity = [0.0, 0.0, -10.0];
        em.particles
            .push([0.0, 0.0, 0.0], [1.0, 0.0, 5.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.5);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Same expectations as `integration_applies_velocity_and_gravity`.
        assert!((em.particles.velocities[0][2] - 0.0).abs() < 1e-5);
        assert!((em.particles.positions[0][0] - 0.5).abs() < 1e-5);
        assert!((em.particles.positions[0][2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn spawn_uses_host_world_translation_as_origin() {
        let mut em = ParticleEmitter::default();
        em.rate = 100.0;
        em.life = 100.0;
        em.shape = EmitterShape::Point;
        em.speed = 0.0;
        em.declination = 0.0;
        let host = Vec3::new(50.0, 80.0, 12.5);
        let (world, e) = world_with_emitter(em, host);
        particle_system(&world, 0.05); // 5 spawns
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        for p in &em.particles.positions {
            assert!((p[0] - host.x).abs() < 1e-4);
            assert!((p[1] - host.y).abs() < 1e-4);
            assert!((p[2] - host.z).abs() < 1e-4);
        }
    }
}
