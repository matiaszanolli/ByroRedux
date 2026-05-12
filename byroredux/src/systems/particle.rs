//! CPU particle integration + spawn.

use byroredux_core::ecs::{GlobalTransform, ParticleEmitter, TotalTime, World};

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

        // 1. Integrate live particles (velocity + gravity) and age them.
        let len = em.particles.len();
        for i in 0..len {
            em.particles.velocities[i][0] += em.gravity[0] * dt;
            em.particles.velocities[i][1] += em.gravity[1] * dt;
            em.particles.velocities[i][2] += em.gravity[2] * dt;
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
