//! Legacy (pre-NiPSys) particle system — pre-10.1 NetImmerse path.
//!
//! The pre-10.1 particle stack is a `NiParticleSystemController` hanging
//! off a `NiBSParticleNode`, driving one of `NiAutoNormalParticles` /
//! `NiRotatingParticles`, fed through a linked chain of
//! `NiParticleModifier` subclasses (grow/fade, color, rotation, gravity,
//! bomb, planar/spherical collider).
//!
//! **Scope is nif.xml-completeness / defensive, not a vanilla-corpus
//! dependency.** The checked-in per-block-type baselines for all 7
//! supported games contain zero occurrences of any legacy-stack block
//! type — Oblivion's own 8032-NIF sweep shows only `NiParticleSystem 547`
//! (the *modern* stack, routed to the renderer), and no
//! `NiAutoNormalParticles` / `NiRotatingParticles` /
//! `NiParticleSystemController` rows. So none of the target games' vanilla
//! `Meshes` archives require these parsers; they exist for nif.xml
//! coverage and for older NetImmerse-era / mod / non-`Meshes.bsa` content
//! that may still author them. Do not read this module as evidence that
//! vanilla Oblivion ships legacy particle FX — the corpus refutes that
//! (the legacy-emitter surfacing arm was confirmed dead code and removed
//! in #1327; see also `feedback_audit_findings.md` on stale premises).
//! The parsers target the "fields-present" superset that v3.3.0.13+ sees,
//! without the "until 3.1" / "until 4.2.2.0" legacy quirks removed before
//! the v20-era titles shipped.
//!
//! Oblivion has no `block_sizes` table, so a single field-width mistake
//! cascades into total parse failure for an entire mesh. Every parser
//! here either consumes a fixed known byte count or is covered by a
//! stream-position assertion in the block-dispatch regression tests.
//! See issue #143.

use super::base::NiAVObjectData;
use super::traits::{HasAVObject, HasObjectNET};
use super::tri_shape::parse_geometry_data_base;
use super::NiObject;
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiPoint3, NiTransform};
use std::any::Any;
use std::io;

use super::controller::NiTimeControllerBase;

// ── NiParticleModifier base ───────────────────────────────────────────
//
// All modifiers share a (next_modifier: ref, controller: ptr) prefix.
// `Controller` (Ptr) is present since 3.3.0.13, which is every game we
// target. Ptr on the wire is a u32 hash, same shape as a BlockRef.

/// Parse the `NiParticleModifier` base fields.
fn parse_particle_modifier_base(stream: &mut NifStream) -> io::Result<(BlockRef, BlockRef)> {
    let next_modifier = stream.read_block_ref()?;
    let controller = stream.read_block_ref()?;
    Ok((next_modifier, controller))
}

// ── NiParticleColorModifier ───────────────────────────────────────────

#[derive(Debug)]
pub struct NiParticleColorModifier {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub color_data_ref: BlockRef,
}

impl NiParticleColorModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
        let color_data_ref = stream.read_block_ref()?;
        Ok(Self {
            next_modifier,
            controller,
            color_data_ref,
        })
    }
}

// ── NiParticleGrowFade ────────────────────────────────────────────────

#[derive(Debug)]
pub struct NiParticleGrowFade {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub grow: f32,
    pub fade: f32,
}

impl NiParticleGrowFade {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
        let grow = stream.read_f32_le()?;
        let fade = stream.read_f32_le()?;
        Ok(Self {
            next_modifier,
            controller,
            grow,
            fade,
        })
    }
}

// ── NiParticleRotation ────────────────────────────────────────────────

#[derive(Debug)]
pub struct NiParticleRotation {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub random_initial_axis: bool,
    pub initial_axis: [f32; 3],
    pub rotation_speed: f32,
}

impl NiParticleRotation {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
        let random_initial_axis = stream.read_u8()? != 0;
        let axis = stream.read_ni_point3()?;
        let rotation_speed = stream.read_f32_le()?;
        Ok(Self {
            next_modifier,
            controller,
            random_initial_axis,
            initial_axis: [axis.x, axis.y, axis.z],
            rotation_speed,
        })
    }
}

// ── NiParticleBomb ────────────────────────────────────────────────────
//
// Adds Decay, Duration, DeltaV, Start, DecayType, SymmetryType (since
// 4.1.0.12, gated — see `NiParticleBomb::parse`; the type's own
// `until="V10_0_1_0"` ceiling means Oblivion, v20.0.0.5, cannot author
// one at all, #2526 / NIF-D1-NEW-01), Position, Direction.

#[derive(Debug)]
pub struct NiParticleBomb {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub decay: f32,
    pub duration: f32,
    pub delta_v: f32,
    pub start: f32,
    pub decay_type: u32,
    pub symmetry_type: u32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl NiParticleBomb {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
        let decay = stream.read_f32_le()?;
        let duration = stream.read_f32_le()?;
        let delta_v = stream.read_f32_le()?;
        let start = stream.read_f32_le()?;
        let decay_type = stream.read_u32_le()?;
        // Symmetry Type (`NiParticleBomb`, since="4.1.0.12"). #2526 /
        // NIF-D1-NEW-01: `NiParticleBomb` itself is `until="V10_0_1_0"`
        // per nif.xml (contrary to the previous "always present for
        // Oblivion" comment — Oblivion's v20.0.0.5 is past this type's
        // own ceiling, so it can never author one), giving a real,
        // non-empty [4.1.0.12, 10.0.1.0] window — unlike Has Radii /
        // Has Rotation Angles / Has Rotation Axes above, this field is
        // gated, not dropped.
        let symmetry_type = if stream.version() >= crate::version::NifVersion::V4_1_0_12 {
            stream.read_u32_le()?
        } else {
            0
        };
        let p = stream.read_ni_point3()?;
        let d = stream.read_ni_point3()?;
        Ok(Self {
            next_modifier,
            controller,
            decay,
            duration,
            delta_v,
            start,
            decay_type,
            symmetry_type,
            position: [p.x, p.y, p.z],
            direction: [d.x, d.y, d.z],
        })
    }
}

// ── NiGravity ─────────────────────────────────────────────────────────
//
// Since 3.3.0.13: adds `decay` before force. Oblivion has this.
// `unknown 01/02` (until 2.3) are absent.

#[derive(Debug)]
pub struct NiGravity {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub decay: f32,
    pub force: f32,
    /// FieldType enum (0 = point, 1 = planar).
    pub field_type: u32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl NiGravity {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
        let decay = stream.read_f32_le()?;
        let force = stream.read_f32_le()?;
        let field_type = stream.read_u32_le()?;
        let p = stream.read_ni_point3()?;
        let d = stream.read_ni_point3()?;
        Ok(Self {
            next_modifier,
            controller,
            decay,
            force,
            field_type,
            position: [p.x, p.y, p.z],
            direction: [d.x, d.y, d.z],
        })
    }
}

// ── NiParticleCollider base ───────────────────────────────────────────
//
// NiParticleCollider adds: bounce, spawn_on_collide (since 4.2.0.2),
// die_on_collide (since 4.2.0.2). Both bools are present for Oblivion.
// Note: since nif.xml type `bool` is 8-bit from 4.1.0.1 onward, these
// are single-byte reads.

fn parse_particle_collider_base(
    stream: &mut NifStream,
) -> io::Result<(BlockRef, BlockRef, f32, bool, bool)> {
    let (next_modifier, controller) = parse_particle_modifier_base(stream)?;
    let bounce = stream.read_f32_le()?;
    let spawn_on_collide = stream.read_byte_bool()?;
    let die_on_collide = stream.read_byte_bool()?;
    Ok((
        next_modifier,
        controller,
        bounce,
        spawn_on_collide,
        die_on_collide,
    ))
}

// ── NiPlanarCollider ──────────────────────────────────────────────────

#[derive(Debug)]
pub struct NiPlanarCollider {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub bounce: f32,
    pub spawn_on_collide: bool,
    pub die_on_collide: bool,
    pub height: f32,
    pub width: f32,
    pub position: [f32; 3],
    pub x_vector: [f32; 3],
    pub y_vector: [f32; 3],
    /// NiPlane = (normal: Vec3, constant: f32).
    pub plane: [f32; 4],
}

impl NiPlanarCollider {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller, bounce, spawn_on_collide, die_on_collide) =
            parse_particle_collider_base(stream)?;
        let height = stream.read_f32_le()?;
        let width = stream.read_f32_le()?;
        let p = stream.read_ni_point3()?;
        let x = stream.read_ni_point3()?;
        let y = stream.read_ni_point3()?;
        let n = stream.read_ni_point3()?;
        let c = stream.read_f32_le()?;
        Ok(Self {
            next_modifier,
            controller,
            bounce,
            spawn_on_collide,
            die_on_collide,
            height,
            width,
            position: [p.x, p.y, p.z],
            x_vector: [x.x, x.y, x.z],
            y_vector: [y.x, y.y, y.z],
            plane: [n.x, n.y, n.z, c],
        })
    }
}

// ── NiSphericalCollider ───────────────────────────────────────────────

#[derive(Debug)]
pub struct NiSphericalCollider {
    pub next_modifier: BlockRef,
    pub controller: BlockRef,
    pub bounce: f32,
    pub spawn_on_collide: bool,
    pub die_on_collide: bool,
    pub radius: f32,
    pub position: [f32; 3],
}

impl NiSphericalCollider {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (next_modifier, controller, bounce, spawn_on_collide, die_on_collide) =
            parse_particle_collider_base(stream)?;
        let radius = stream.read_f32_le()?;
        let p = stream.read_ni_point3()?;
        Ok(Self {
            next_modifier,
            controller,
            bounce,
            spawn_on_collide,
            die_on_collide,
            radius,
            position: [p.x, p.y, p.z],
        })
    }
}

// ── NiParticleSystemController ────────────────────────────────────────
//
// Inherits NiTimeController. The field set below is the v3.3.0.13+
// superset (every "since 3.3.0.13" field is present for Oblivion,
// every "until 3.1" field is absent). The particle rows themselves
// are a variable-length array of 32-byte records.

#[derive(Debug)]
pub struct NiParticleSystemController {
    pub base: NiTimeControllerBase,

    pub speed: f32,
    pub speed_variation: f32,
    pub declination: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub initial_normal: [f32; 3],
    pub initial_color: [f32; 4],
    pub initial_size: f32,
    pub emit_start_time: f32,
    pub emit_stop_time: f32,
    pub reset_particle_system: u8,
    pub birth_rate: f32,
    pub lifetime: f32,
    pub lifetime_variation: f32,
    pub use_birth_rate: u8,
    pub spawn_on_death: u8,
    pub emitter_dimensions: [f32; 3],
    /// Ptr<NiAVObject> — stored as raw u32 hash.
    pub emitter: u32,
    pub num_spawn_generations: u16,
    pub percentage_spawned: f32,
    pub spawn_multiplier: u16,
    pub spawn_speed_chaos: f32,
    pub spawn_dir_chaos: f32,

    pub num_particles: u16,
    pub num_valid: u16,
    /// Raw particle records. Each is 32 bytes (velocity:vec3 +
    /// unknown_vector:vec3 + lifetime:f32 + lifespan:f32 + timestamp:f32,
    /// then unknown_short:u16 + vertex_id:u16). We keep them opaque — the
    /// renderer doesn't consume authored particles yet.
    pub particles: Vec<[u8; 32]>,

    pub unknown_ref: BlockRef,
    pub num_emitter_points: u32,
    pub emitter_points: Vec<u32>,
    pub trailer_emitter_type: u32,
    pub unknown_trailer_float: f32,
    pub trailer_emitter_modifier: BlockRef,
}

impl NiParticleSystemController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;

        let speed = stream.read_f32_le()?;
        let speed_variation = stream.read_f32_le()?;
        let declination = stream.read_f32_le()?;
        let declination_variation = stream.read_f32_le()?;
        let planar_angle = stream.read_f32_le()?;
        let planar_angle_variation = stream.read_f32_le()?;
        let n = stream.read_ni_point3()?;
        let initial_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let initial_size = stream.read_f32_le()?;
        let emit_start_time = stream.read_f32_le()?;
        let emit_stop_time = stream.read_f32_le()?;
        let reset_particle_system = stream.read_u8()?;
        let birth_rate = stream.read_f32_le()?;
        let lifetime = stream.read_f32_le()?;
        let lifetime_variation = stream.read_f32_le()?;
        let use_birth_rate = stream.read_u8()?;
        let spawn_on_death = stream.read_u8()?;
        let d = stream.read_ni_point3()?;
        let emitter = stream.read_u32_le()?;
        let num_spawn_generations = stream.read_u16_le()?;
        let percentage_spawned = stream.read_f32_le()?;
        let spawn_multiplier = stream.read_u16_le()?;
        let spawn_speed_chaos = stream.read_f32_le()?;
        let spawn_dir_chaos = stream.read_f32_le()?;

        let num_particles = stream.read_u16_le()?;
        let num_valid = stream.read_u16_le()?;

        // Particle records: 32 bytes each. Skip by reading raw bytes
        // because the layout is compact and the engine doesn't read them.
        // #388: allocate_vec bounds the count against stream budget.
        // F13 (#1589) — bulk read fixed 32-byte particle records in one
        // pass; the per-iteration read_bytes(32) churned a throwaway Vec<u8>.
        let particles: Vec<[u8; 32]> = stream.read_pod_vec(num_particles as usize)?;

        let unknown_ref = stream.read_block_ref()?;
        // #981 — bulk-read emitter-point index array via `read_u32_array`.
        let num_emitter_points = stream.read_u32_le()?;
        let emitter_points = stream.read_u32_array(num_emitter_points as usize)?;
        let trailer_emitter_type = stream.read_u32_le()?;
        let unknown_trailer_float = stream.read_f32_le()?;
        let trailer_emitter_modifier = stream.read_block_ref()?;

        Ok(Self {
            base,
            speed,
            speed_variation,
            declination,
            declination_variation,
            planar_angle,
            planar_angle_variation,
            initial_normal: [n.x, n.y, n.z],
            initial_color,
            initial_size,
            emit_start_time,
            emit_stop_time,
            reset_particle_system,
            birth_rate,
            lifetime,
            lifetime_variation,
            use_birth_rate,
            spawn_on_death,
            emitter_dimensions: [d.x, d.y, d.z],
            emitter,
            num_spawn_generations,
            percentage_spawned,
            spawn_multiplier,
            spawn_speed_chaos,
            spawn_dir_chaos,
            num_particles,
            num_valid,
            particles,
            unknown_ref,
            num_emitter_points,
            emitter_points,
            trailer_emitter_type,
            unknown_trailer_float,
            trailer_emitter_modifier,
        })
    }
}

// ── NiAutoNormalParticles / NiRotatingParticles ───────────────────────
//
// Both inherit NiParticles → NiGeometry → NiAVObject. In Oblivion the
// NiGeometry body reduces to:
//
//   NiAVObject base
//   data_ref (i32)
//   skin_instance_ref (i32)
//   has_shader (byte bool) + optional shader_name (sized string) +
//     implementation (i32), since 10.0.1.0 and until 20.1.0.3
//
// v20.0.0.5 sits inside that window, so both parsers share the same
// body. NiTriShape's parser covers the same range — we mirror its
// Oblivion path rather than try to factor it out in this fix.

#[derive(Debug)]
pub struct NiLegacyParticles {
    /// Type tag preserved so downstream consumers can distinguish
    /// `NiAutoNormalParticles` from `NiRotatingParticles`.
    pub type_name: &'static str,
    pub av: NiAVObjectData,
    pub data_ref: BlockRef,
    pub skin_instance_ref: BlockRef,
    pub has_shader: bool,
    pub shader_name: Option<String>,
    pub shader_implementation: i32,
}

impl NiObject for NiLegacyParticles {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(self)
    }
}

impl HasObjectNET for NiLegacyParticles {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
        self.av.net.name.as_ref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl HasAVObject for NiLegacyParticles {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl NiLegacyParticles {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;
        let data_ref = stream.read_block_ref()?;
        let skin_instance_ref = stream.read_block_ref()?;

        // NiGeometry has_shader chain (since 10.0.1.0, until 20.1.0.3) —
        // gated exactly as the sibling NiTriShape::parse does
        // (`tri_shape/ni_tri_shape.rs`). #2526 / NIF-D1-NEW-01: this field
        // was previously read unconditionally, which is only byte-correct
        // for the narrow window this parser's own object types
        // (NiAutoNormalParticles / NiRotatingParticles, both
        // `until="V10_0_1_0"` per nif.xml) can actually intersect with —
        // i.e. version == V10_0_1_0 exactly. `bool` is 8-bit from
        // 4.1.0.1 onward — every game Redux targets.
        let has_shader = if stream.version() >= crate::version::NifVersion::V10_0_1_0
            && stream.version() <= crate::version::NifVersion::V20_1_0_3
        {
            stream.read_byte_bool()?
        } else {
            false
        };
        let (shader_name, shader_implementation) = if has_shader {
            let name = stream.read_sized_string()?;
            let impl_ = stream.read_u32_le()? as i32;
            (Some(name), impl_)
        } else {
            (String::new().into(), 0)
        };

        Ok(Self {
            type_name,
            av,
            data_ref,
            skin_instance_ref,
            has_shader,
            shader_name: shader_name.filter(|s: &String| !s.is_empty()),
            shader_implementation,
        })
    }
}

// ── NiAutoNormalParticlesData / NiRotatingParticlesData ───────────────
//
// Both inherit NiParticlesData → NiGeometryData, and both are themselves
// `until="V10_0_1_0"` per nif.xml — NOT reachable by Oblivion (v20.0.0.5)
// or any later-supported game; see the module doc for the
// nif.xml-completeness rationale. #2526 / NIF-D1-NEW-01 corrected the
// stale "at v20.0.0.5" framing this comment previously carried, which
// was itself the wrong premise behind the missing version gates below:
//
//   NiGeometryData base (via parse_geometry_data_base)
//   // NiParticlesData fields (no num_particles since 4.0.0.2, no
//   //                          particle_radius since 10.0.1.0)
//   has_radii:bool + radii:float[num_vertices]? — (since 10.1.0.0, no
//     until) is STRICTLY ABOVE this object's own 10.0.1.0 ceiling —
//     structurally unreachable, never read
//   num_active:u16
//   has_sizes:bool + sizes:float[num_vertices]? — no version gate on
//     the field itself, always present
//   has_rotations:bool + rotations:Quat[num_vertices]? — (since
//     10.0.1.0, no until) valid only at exactly version == 10.0.1.0
//   has_rotation_angles:bool + rotation_angles:f32[num_vertices]? —
//     (since 20.0.0.4, no until) structurally unreachable, never read
//   has_rotation_axes:bool + rotation_axes:Vec3[num_vertices]? — same
//     as rotation_angles, never read
//
// NiRotatingParticlesData additionally carries `has_rotations_2` +
// `rotations_2` — but only up to 4.2.2.0, so at this object's own
// ceiling (10.0.1.0) there is NO extra tail. The Rotating and
// AutoNormal variants are byte-identical.

#[derive(Debug)]
pub struct NiLegacyParticlesData {
    pub type_name: &'static str,
    pub num_vertices: u16,
    pub vertices: Vec<NiPoint3>,
    pub center: NiPoint3,
    pub radius: f32,
    pub num_active: u16,
    pub radii: Vec<f32>,
    pub sizes: Vec<f32>,
    /// Quaternion per particle (x, y, z, w), when has_rotations is set.
    pub rotations: Vec<[f32; 4]>,
    pub rotation_angles: Vec<f32>,
    pub rotation_axes: Vec<[f32; 3]>,
}

impl NiObject for NiLegacyParticlesData {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLegacyParticlesData {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        // NiGeometryData base (vertices, normals, colors, UVs, bound).
        let (vertices, _data_flags, _normals, center, radius, _colors, _uvs) =
            parse_geometry_data_base(stream)?;
        let num_vertices = vertices.len() as u16;

        // Has Radii (`NiParticlesData`, since="10.1.0.0", nif.xml has no
        // `until` — open-ended) is structurally unreachable here: this
        // parser's own object types (`NiAutoNormalParticlesData` /
        // `NiRotatingParticlesData`, parents `until="V10_0_1_0"` per
        // nif.xml) close out strictly *before* the field's own `since`
        // ceiling opens (10.1.0.0 > 10.0.1.0) — the intersection is
        // empty, so it can never legitimately be present. #2526 /
        // NIF-D1-NEW-01: previously read unconditionally regardless,
        // corrupting stream position for any genuine pre-Gamebryo
        // instance. Never read; always absent.
        let radii: Vec<f32> = Vec::new();

        let num_active = stream.read_u16_le()?;

        let has_sizes = stream.read_byte_bool()?;
        let sizes = if has_sizes {
            stream.read_f32_array(num_vertices as usize)?
        } else {
            Vec::new()
        };

        // Has Rotations (`NiParticlesData`, since="10.0.1.0", no `until`
        // on the field itself) — same window shape as Has Shader above:
        // this parser's own object types close out at exactly
        // `until="V10_0_1_0"`, giving a single-point valid window of
        // version == V10_0_1_0. #2526 / NIF-D1-NEW-01: same bug class as
        // the issue's four named fields, caught by the SIBLING
        // completeness check when auditing the rest of this parser —
        // was read unconditionally regardless of version.
        let has_rotations = if stream.version() >= crate::version::NifVersion::V10_0_1_0 {
            stream.read_byte_bool()?
        } else {
            false
        };
        // #2525 / PERF-D8-NEW-02 — bulk-read the raw f32 stream in one
        // call then swizzle w,x,y,z (Gamebryo's on-disk quaternion
        // order) to [x,y,z,w] in the `.map()`, mirroring the idiom
        // #1263 established at `bs_geometry.rs:434-444` instead of 4
        // individual `read_f32_le()` calls per particle.
        let rotations: Vec<[f32; 4]> = if has_rotations {
            stream
                .read_f32_array(num_vertices as usize * 4)?
                .chunks_exact(4)
                .map(|c| [c[1], c[2], c[3], c[0]])
                .collect()
        } else {
            Vec::new()
        };

        // Has Rotation Angles / Has Rotation Axes (`NiRotatingParticlesData`,
        // both since="20.0.0.4", nif.xml has no `until` — open-ended) are
        // structurally unreachable for the same reason as Has Radii
        // above: this parser's own object types close out at
        // `until="V10_0_1_0"`, strictly below the fields' 20.0.0.4
        // opening — the intersection is empty. #2526 / NIF-D1-NEW-01:
        // previously read unconditionally regardless (the stale "present
        // in Oblivion v20.0.0.5" comment reflected the same wrong
        // premise this issue corrects — Oblivion cannot author this
        // object type at all, its own version is past the type's
        // ceiling). Never read; always absent.
        let rotation_angles: Vec<f32> = Vec::new();
        let rotation_axes: Vec<[f32; 3]> = Vec::new();

        // NiRotatingParticlesData `has_rotations_2` field is `until 4.2.2.0`
        // — absent in Oblivion, nothing further to consume.

        Ok(Self {
            type_name,
            num_vertices,
            vertices,
            center,
            radius,
            num_active,
            radii,
            sizes,
            rotations,
            rotation_angles,
            rotation_axes,
        })
    }
}

impl_ni_object!(
    NiParticleColorModifier,
    NiParticleGrowFade,
    NiParticleRotation,
    NiParticleBomb,
    NiGravity,
    NiPlanarCollider,
    NiSphericalCollider,
    NiParticleSystemController,
);
