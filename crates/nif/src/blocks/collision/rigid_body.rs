//! BhkRigidBody parser.
//!
//! The most-touched Havok type — holds mass, inertia tensor, velocity, and
//! the per-shape collision filter. Constraint refs live here too.

use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::io;

use super::{read_matrix3, read_vec4};

/// bhkRigidBody / bhkRigidBodyT — Havok rigid body with physics properties.
/// bhkRigidBodyT has active translation/rotation (same binary layout).
#[derive(Debug)]
pub struct BhkRigidBody {
    // bhkWorldObject
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    // Physics CInfo
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    pub inertia_tensor: [f32; 12],
    pub center_of_mass: [f32; 4],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
    pub penetration_depth: f32,
    pub motion_type: u8,
    pub deactivator_type: u8,
    pub solver_deactivation: u8,
    pub quality_type: u8,
    pub constraint_refs: Vec<BlockRef>,
    pub body_flags: u32,
}


impl BhkRigidBody {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bsver = stream.bsver();

        // bhkWorldObject: shape ref + havok filter + world object CInfo
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        // bhkWorldObjectCInfo: 4 unused + broadphase(1) + 3 unused + 3 property u32s = 20 bytes
        stream.skip(20)?;

        // bhkEntityCInfo: response(1) + unused(1) + callback_delay(2) = 4 bytes
        stream.skip(4)?;

        if bsver <= 34 {
            // bhkRigidBodyCInfo550_660 (Oblivion / FO3 / FNV)
            // Duplicated filter + entity CInfo (since 10.1.0.0).
            // Prefix layout (nif.xml line 2808):
            //   Unused 01[4] + HavokFilter(u32) + Unused 02[4] +
            //   Collision Response(u8) + Unused 03(u8) +
            //   Process Contact Callback Delay(u16) + Unused 04[4] = 20 B.
            stream.skip(4)?; // unused
            let _cinfo_filter = stream.read_u32_le()?;
            stream.skip(4)?; // unused
            stream.skip(4)?; // response + unused + callback_delay
            stream.skip(4)?; // unused
        } else if bsver < 130 {
            // bhkRigidBodyCInfo2010 (Skyrim LE / SE — bsver 83-127).
            // Pre-#546 this prefix was missing entirely and the parser
            // walked straight into `Translation` from 20 bytes early,
            // trashing every subsequent field. All 12,866 vanilla
            // Skyrim SE bhkRigidBody blocks fell into NiUnknown.
            //
            // Prefix layout (nif.xml line 2844):
            //   Unused 01[4] + HavokFilter(u32) + Unused 02[4] +
            //   Unknown Int 1(u32) + Collision Response(u8) +
            //   Unused 03(u8) + Process Contact Callback Delay(u16)
            //   = 20 B. Semantically distinct from 550_660 (Unknown Int 1
            //   replaces 550_660's trailing Unused 04) but same wire size.
            stream.skip(4)?; // Unused 01
            let _cinfo_filter = stream.read_u32_le()?; // duplicated havok filter
            stream.skip(4)?; // Unused 02
            let _unknown_int_1 = stream.read_u32_le()?;
            stream.skip(4)?; // response + unused + callback_delay
        }
        // bsver >= 130 (FO4+): bhkRigidBodyCInfo2014 has a very different
        // layout — motion system / deactivator / quality / penetration
        // depth / time factor are interleaved with callback delay. That
        // path is knowingly incomplete and is tracked separately; we
        // preserve the pre-#546 behaviour of reading straight into
        // Translation here so FO4 doesn't newly regress.

        let translation = read_vec4(stream)?;
        let rotation = read_vec4(stream)?;
        let linear_velocity = read_vec4(stream)?;
        let angular_velocity = read_vec4(stream)?;
        let inertia_tensor = read_matrix3(stream)?;
        let center_of_mass = read_vec4(stream)?;
        let mass = stream.read_f32_le()?;
        let linear_damping = stream.read_f32_le()?;
        let angular_damping = stream.read_f32_le()?;

        if bsver >= 83 {
            // Skyrim+: timeFactor, gravityFactor before friction
            let _time_factor = stream.read_f32_le()?;
            let _gravity_factor = stream.read_f32_le()?;
        }

        let friction = stream.read_f32_le()?;

        if bsver >= 83 {
            let _rolling_friction = stream.read_f32_le()?;
        }

        let restitution = stream.read_f32_le()?;

        let (max_linear_velocity, max_angular_velocity, penetration_depth) = if bsver <= 34 {
            // Oblivion/FO3: max velocities + penetration depth (since 10.1.0.0)
            (
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            )
        } else {
            // Skyrim+: max velocities + penetration depth in different order
            let mlv = stream.read_f32_le()?;
            let mav = stream.read_f32_le()?;
            let pd = stream.read_f32_le()?;
            (mlv, mav, pd)
        };

        let motion_type = stream.read_u8()?;
        // Deactivator Type is present on *every* CInfo per nif.xml — the
        // prior "Skyrim+: removed, hardcoded 0" branch was one of the
        // three root causes of #546. Only the FO4+ CInfo2014 reorders it
        // (and still carries it), so we read it unconditionally here.
        let deactivator_type = stream.read_u8()?;
        let solver_deactivation = stream.read_u8()?;
        let quality_type = stream.read_u8()?;

        if bsver <= 34 {
            // Oblivion/FO3/FNV (CInfo550_660): Unused 05[12] padding.
            stream.skip(12)?;
        } else if bsver < 130 {
            // Skyrim LE/SE (CInfo2010): AutoRemoveLevel(1) +
            // ResponseModifierFlags(1) + NumShapeKeysInContactPoint(1) +
            // ForceCollidedOntoPPU(bool,1) + Unused 04[12] = 16 B.
            // Pre-#546 this skipped only 4 — the 12-byte Unused 04 trailer
            // was consumed by the next block's reads, drifting the stream.
            stream.skip(16)?;
        } else {
            // FO4+ (CInfo2014): different layout — see comment in prefix
            // block above. Preserve pre-#546 4-byte skip to avoid
            // introducing a new regression.
            stream.skip(4)?;
        }

        // Constraint refs
        let num_constraints = stream.read_u32_le()?;
        let mut constraint_refs: Vec<BlockRef> = stream.allocate_vec(num_constraints)?;
        for _ in 0..num_constraints {
            constraint_refs.push(stream.read_block_ref()?);
        }

        // Body flags: u32 in pre-Skyrim, u16 in Skyrim+ per nif.xml
        // (`#SKY_AND_LATER#` resolves to BSVER >= 76 in the niftools
        // schema). No Bethesda title ships in the BSVER 76..=82 gap,
        // so the cutoff is structurally invisible to vanilla content
        // — but the parser doctrine pins to nif.xml's threshold and
        // the previous `bsver < 83` value contradicted that without
        // shipping cause. Boundary tests cover bsver=75 (u32 path)
        // and bsver=76 (u16 path) at the bottom of this file. See
        // NIF-D2-NEW-05 (audit 2026-05-12), original landing #127.
        let body_flags = if bsver < 76 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };

        Ok(Self {
            shape_ref,
            havok_filter,
            translation,
            rotation,
            linear_velocity,
            angular_velocity,
            inertia_tensor,
            center_of_mass,
            mass,
            linear_damping,
            angular_damping,
            friction,
            restitution,
            max_linear_velocity,
            max_angular_velocity,
            penetration_depth,
            motion_type,
            deactivator_type,
            solver_deactivation,
            quality_type,
            constraint_refs,
            body_flags,
        })
    }
}

impl_ni_object!(BhkRigidBody => "bhkRigidBody");
