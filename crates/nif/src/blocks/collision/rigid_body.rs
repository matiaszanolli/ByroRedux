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
    /// `true` only for the `bhkRigidBodyT` block type. The T variant activates
    /// the CInfo translation/rotation; plain `bhkRigidBody` stores those bytes
    /// but the engine must treat them as identity (#2316).
    pub is_t: bool,
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
        // #1329 — v10.0.1.x ("old Oblivion", nifly/openmw `VER_OB_OLD`)
        // Havok rigid bodies predate the `since="10.1.0.0"` CInfo
        // additions: the 16-byte duplicated-filter/entity CInfo prefix
        // and the max-velocity / penetration-depth triple are both
        // absent, and `bhkWorldObject` carries an extra 4-byte Unknown
        // after the shape ref. These rare early-Oblivion meshes
        // (handscythe01 / oar01 / ungrdltraphingedoor) cascade-truncate
        // without this path. Handled separately so the main path below
        // stays byte-for-byte as validated for Oblivion v20 / FO3 / FNV /
        // Skyrim / FO4. Gated `version < 10.1.0.0`, so it cannot affect
        // any other shipping title. Layout cross-checked against openmw
        // `bhkRigidBodyCInfo::read`.
        if stream.version().uses_old_rigid_body_layout() {
            return Self::parse_oblivion_old(stream);
        }

        let bsver = stream.bsver();

        if bsver >= crate::version::bsver::FALLOUT4 {
            return Self::parse_fo4_cinfo2014(stream);
        }

        // bhkWorldObject: shape ref + havok filter + world object CInfo
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        // bhkWorldObjectCInfo: 4 unused + broadphase(1) + 3 unused + 3 property u32s = 20 bytes
        stream.skip(20)?;

        // bhkEntityCInfo: response(1) + unused(1) + callback_delay(2) = 4 bytes
        stream.skip(4)?;

        if bsver <= crate::version::bsver::FO3_FNV {
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
        } else if bsver < crate::version::bsver::FALLOUT4 {
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
        // bsver >= crate::version::bsver::FALLOUT4 (FO4+) never reaches here — the CInfo2014
        // layout is different enough (see `parse_fo4_cinfo2014`) that it's
        // branched out above, before any of this shared Oblivion/FO3/FNV/
        // Skyrim body is read.

        let translation = read_vec4(stream)?;
        let rotation = read_vec4(stream)?;
        let linear_velocity = read_vec4(stream)?;
        let angular_velocity = read_vec4(stream)?;
        let inertia_tensor = read_matrix3(stream)?;
        let center_of_mass = read_vec4(stream)?;
        let mass = stream.read_f32_le()?;
        let linear_damping = stream.read_f32_le()?;
        let angular_damping = stream.read_f32_le()?;

        if bsver >= crate::version::bsver::SKYRIM_LE {
            // Skyrim+: timeFactor, gravityFactor before friction
            let _time_factor = stream.read_f32_le()?;
            let _gravity_factor = stream.read_f32_le()?;
        }

        let friction = stream.read_f32_le()?;

        if bsver >= crate::version::bsver::SKYRIM_LE {
            let _rolling_friction = stream.read_f32_le()?;
        }

        let restitution = stream.read_f32_le()?;

        let (max_linear_velocity, max_angular_velocity, penetration_depth) =
            if bsver <= crate::version::bsver::FO3_FNV {
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

        if bsver <= crate::version::bsver::FO3_FNV {
            // Oblivion/FO3/FNV (CInfo550_660): Unused 05[12] padding.
            stream.skip(12)?;
        } else {
            // Skyrim LE/SE (CInfo2010) — bsver < FALLOUT4 is guaranteed
            // here since FO4+ branches out to `parse_fo4_cinfo2014` before
            // reaching this shared body. AutoRemoveLevel(1) +
            // ResponseModifierFlags(1) + NumShapeKeysInContactPoint(1) +
            // ForceCollidedOntoPPU(bool,1) + Unused 04[12] = 16 B.
            // Pre-#546 this skipped only 4 — the 12-byte Unused 04 trailer
            // was consumed by the next block's reads, drifting the stream.
            stream.skip(16)?;
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
        // the previous `bsver < crate::version::bsver::SKYRIM_LE` value contradicted that without
        // shipping cause. Boundary tests cover bsver=75 (u32 path)
        // and bsver=76 (u16 path) at the bottom of this file. See
        // NIF-D2-NEW-05 (audit 2026-05-12), original landing #127.
        let body_flags = if bsver < crate::version::bsver::RIGID_BODY_FLAGS16 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };

        Ok(Self {
            shape_ref,
            havok_filter,
            is_t: false,
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

impl BhkRigidBody {
    /// v10.0.1.x ("old Oblivion") rigid-body layout — see the version
    /// gate in [`BhkRigidBody::parse`]. Mirrors openmw
    /// `bhkRigidBodyCInfo::read`'s `version < 10.1.0.0` / `bethVersion < 83`
    /// branch.
    fn parse_oblivion_old(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject: shape + (VER_OB_OLD Unknown) + filter + WorldObjCInfo
        let shape_ref = stream.read_block_ref()?;
        stream.skip(4)?; // Unknown — only present `<= VER_OB_OLD` (10.0.1.x)
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?; // bhkWorldObjCInfo (4 unused + broadphase(1) + 3 unused + property(12))

        // bhkRigidBodyCInfo: the 16-byte `since=10.1.0.0` duplicated
        // filter/entity prefix is absent here; only the `bethVer < 83`
        // 4-byte Unused remains.
        stream.skip(4)?;

        let translation = read_vec4(stream)?;
        let rotation = read_vec4(stream)?;
        let linear_velocity = read_vec4(stream)?;
        let angular_velocity = read_vec4(stream)?;
        let inertia_tensor = read_matrix3(stream)?; // 3×(Vector3 + 4-byte pad) = 48 B
        let center_of_mass = read_vec4(stream)?;
        let mass = stream.read_f32_le()?;
        let linear_damping = stream.read_f32_le()?;
        let angular_damping = stream.read_f32_le()?;
        let friction = stream.read_f32_le()?;
        let restitution = stream.read_f32_le()?;
        // `since=10.1.0.0`: max linear/angular velocity + penetration depth — absent.

        let motion_type = stream.read_u8()?;
        let deactivator_type = stream.read_u8()?;
        let solver_deactivation = stream.read_u8()?;
        let quality_type = stream.read_u8()?;
        stream.skip(12)?; // Unused (bethVer < 83, non-FO4)

        let num_constraints = stream.read_u32_le()?;
        let mut constraint_refs: Vec<BlockRef> = stream.allocate_vec(num_constraints)?;
        for _ in 0..num_constraints {
            constraint_refs.push(stream.read_block_ref()?);
        }
        let body_flags = stream.read_u32_le()?; // u32 pre-Skyrim (bethVer < 76)

        Ok(Self {
            shape_ref,
            havok_filter,
            is_t: false,
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
            max_linear_velocity: 0.0,
            max_angular_velocity: 0.0,
            penetration_depth: 0.0,
            motion_type,
            deactivator_type,
            solver_deactivation,
            quality_type,
            constraint_refs,
            body_flags,
        })
    }
}

impl BhkRigidBody {
    /// FO4+ (`bsver >= FALLOUT4`, i.e. Fallout 4 / Fallout 76 / Starfield)
    /// rigid-body layout — `bhkRigidBodyCInfo2014` per nif.xml (lines
    /// 2887-2930). This is a genuinely different field order from the
    /// Skyrim `CInfo2010` layout the shared body above decodes, not a
    /// smaller variant of it:
    ///   * `Gravity Factor` has no paired `Time Factor` where Skyrim's
    ///     pair sits (right after Angular Damping) — `Time Factor`
    ///     reappears much later, right after `Penetration Depth`.
    ///   * `Collision Response` and a `Process Contact Callback Delay`
    ///     are re-read a *second* time inside the CInfo body itself,
    ///     after `Penetration Depth`/`Time Factor`/4 bytes of padding.
    ///   * `Quality Type` moves after that second callback delay instead
    ///     of sitting immediately after `Solver Deactivation`.
    ///
    /// Fixes #4156: pre-fix this era fell through to the Skyrim CInfo2010
    /// field order with only a 4-byte skip standing in for the whole
    /// CInfo2014 prefix — every FO4/FO76/Starfield NIF using the classic
    /// (non-`BhkSystemBinary`, see `havok_packfile`/#3809) `bhkRigidBody`
    /// chain yielded garbage mass/friction/motion_type feeding straight
    /// into the PHYSAL solver's Static/Dynamic/Keyframed classification.
    ///
    /// Layout derived purely from nif.xml per this project's no-guessing
    /// policy — not corpus-verified byte-for-byte the way the Havok
    /// packfile decoder in this same directory is, since CInfo2014's
    /// wire shape isn't independently cross-checkable the way a
    /// self-describing container's internal offsets are. A fixture built
    /// from this exact field layout is pinned below.
    fn parse_fo4_cinfo2014(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject: shape ref + havok filter + world object CInfo (20 B)
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?;

        // bhkEntityCInfo: response(1) + unused(1) + callback_delay(2)
        stream.skip(4)?;

        // bhkRigidBodyCInfo2014 prefix: Unused 01[4] + duplicated Havok
        // Filter(4) + Unused 02[12] = 20 B.
        stream.skip(4)?;
        let _cinfo_filter = stream.read_u32_le()?;
        stream.skip(12)?;

        let translation = read_vec4(stream)?;
        let rotation = read_vec4(stream)?;
        let linear_velocity = read_vec4(stream)?;
        let angular_velocity = read_vec4(stream)?;
        let inertia_tensor = read_matrix3(stream)?;
        let center_of_mass = read_vec4(stream)?;
        let mass = stream.read_f32_le()?;
        let linear_damping = stream.read_f32_le()?;
        let angular_damping = stream.read_f32_le()?;
        // Gravity Factor — unlike Skyrim, no paired Time Factor sits here.
        let _gravity_factor = stream.read_f32_le()?;
        let friction = stream.read_f32_le()?;
        let _rolling_friction_multiplier = stream.read_f32_le()?;
        let restitution = stream.read_f32_le()?;
        let max_linear_velocity = stream.read_f32_le()?;
        let max_angular_velocity = stream.read_f32_le()?;
        let motion_type = stream.read_u8()?;
        let deactivator_type = stream.read_u8()?;
        let solver_deactivation = stream.read_u8()?;
        stream.skip(1)?; // Unused 03
        let penetration_depth = stream.read_f32_le()?;
        let _time_factor = stream.read_f32_le()?; // Time Factor lands here, not by Gravity Factor.
        stream.skip(4)?; // Unused 04
        let _collision_response = stream.read_u8()?; // second Collision Response read
        stream.skip(1)?; // Unused 05
        let _process_contact_callback_delay = stream.read_u16_le()?; // second callback delay read
        let quality_type = stream.read_u8()?;
        stream.skip(4)?; // AutoRemoveLevel(1) + ResponseModifierFlags(1) + NumShapeKeysInContactPoint(1) + ForceCollidedOntoPPU(1)
        stream.skip(3)?; // Unused 06

        // Constraint refs
        let num_constraints = stream.read_u32_le()?;
        let mut constraint_refs: Vec<BlockRef> = stream.allocate_vec(num_constraints)?;
        for _ in 0..num_constraints {
            constraint_refs.push(stream.read_block_ref()?);
        }

        // Body flags: u16 — bsver >= FALLOUT4 (130) is always above
        // RIGID_BODY_FLAGS16 (76).
        let body_flags = stream.read_u16_le()? as u32;

        Ok(Self {
            shape_ref,
            havok_filter,
            is_t: false,
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
