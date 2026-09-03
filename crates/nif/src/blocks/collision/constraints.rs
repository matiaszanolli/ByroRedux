//! Havok constraint variants.
//!
//! BhkConstraint stubs + BhkBreakableConstraint with its inner-wrapped
//! constraint payload (#117 / #557).

use crate::blocks::NiObject;
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

/// A Havok constraint block.
///
/// Always holds the shared `bhkConstraintCInfo` base (two entity refs
/// plus priority). On FO3+ the joint geometry a humanoid ragdoll uses is
/// decoded into [`BhkConstraintData`] (M41.x): bare `bhkRagdollConstraint`
/// / `bhkLimitedHingeConstraint`, and — the dominant FNV form —
/// `bhkMalleableConstraint` wrapping one of those (surfaced as the inner
/// joint, with the malleable block's outer entity refs as the bodies).
/// Every other type stays a `type_name`-only stub. See #117 / M41.x.
#[derive(Debug)]
pub struct BhkConstraint {
    /// RTTI class name — one of the seven constraint types.
    pub type_name: &'static str,
    pub entity_a: BlockRef,
    pub entity_b: BlockRef,
    pub priority: u32,
    /// Decoded per-variant serialization data, when we parse it
    /// (FO3+ Ragdoll / LimitedHinge). [`BhkConstraintData::Other`]
    /// otherwise.
    pub data: BhkConstraintData,
}

impl NiObject for BhkConstraint {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Decoded per-variant `bhkConstraintCInfo` payload. Only the variants
/// a humanoid ragdoll articulation uses are decoded today (M41.x); the
/// rest stay [`Other`](BhkConstraintData::Other) (the bytes are still
/// consumed/skipped, just not surfaced).
#[derive(Debug, Clone)]
pub enum BhkConstraintData {
    /// `bhkRagdollConstraintCInfo` — a 3-DOF cone/twist ball joint.
    Ragdoll(RagdollCInfo),
    /// `bhkLimitedHingeConstraintCInfo` — a 1-DOF angle-limited hinge.
    LimitedHinge(LimitedHingeCInfo),
    /// `bhkPrismaticConstraintCInfo` — a 1-DOF sliding rail (#3792).
    Prismatic(PrismaticCInfo),
    /// Any constraint type we don't decode the body of yet.
    Other,
}

/// `bhkRagdollConstraintCInfo`, FO3/FNV (`!#NI_BS_LTE_16#`) layout from
/// nif.xml. All vectors are **raw Havok Z-up** — coordinate conversion
/// happens at the import boundary (Phase 2), not here. The trailing
/// `bhkConstraintMotorCInfo` is left for `block_size` recovery (it's the
/// last field and carries nothing slice 1 needs).
#[derive(Debug, Clone)]
pub struct RagdollCInfo {
    pub twist_a: [f32; 4],
    pub plane_a: [f32; 4],
    pub motor_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub twist_b: [f32; 4],
    pub plane_b: [f32; 4],
    pub motor_b: [f32; 4],
    pub pivot_b: [f32; 4],
    /// Max angle around the axis orthogonal to Plane A and Twist A.
    /// Cone min is `-cone_max_angle` (not stored).
    pub cone_max_angle: f32,
    pub plane_min_angle: f32,
    pub plane_max_angle: f32,
    pub twist_min_angle: f32,
    pub twist_max_angle: f32,
    pub max_friction: f32,
}

impl RagdollCInfo {
    /// 8 × Vec4 + 6 × f32 = 152 bytes (matches the FNV Ragdoll prefix in
    /// [`BhkBreakableConstraint::fnv_motor_prefix_size`]).
    fn parse_fo3(stream: &mut NifStream) -> io::Result<Self> {
        let twist_a = super::read_vec4(stream)?;
        let plane_a = super::read_vec4(stream)?;
        let motor_a = super::read_vec4(stream)?;
        let pivot_a = super::read_vec4(stream)?;
        let twist_b = super::read_vec4(stream)?;
        let plane_b = super::read_vec4(stream)?;
        let motor_b = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        Ok(Self {
            twist_a,
            plane_a,
            motor_a,
            pivot_a,
            twist_b,
            plane_b,
            motor_b,
            pivot_b,
            cone_max_angle: stream.read_f32_le()?,
            plane_min_angle: stream.read_f32_le()?,
            plane_max_angle: stream.read_f32_le()?,
            twist_min_angle: stream.read_f32_le()?,
            twist_max_angle: stream.read_f32_le()?,
            max_friction: stream.read_f32_le()?,
        })
    }

    /// Oblivion / Morrowind (`#NI_BS_LTE_16#`) layout from nif.xml:
    /// 6 × Vec4 + 6 × f32 = 120 bytes. Differs from [`Self::parse_fo3`] in
    /// BOTH count (no Motor A / Motor B — those are FO3+ additions) and
    /// order (pivot/plane/twist, A then B — vs FO3's twist/plane/motor/
    /// pivot). The absent motors are zeroed; the PHYSAL translate boundary
    /// (`import::collision::ragdoll_joint`) reads only the common subset
    /// (twist/plane/pivot + the angle limits), so a zeroed motor is
    /// invisible downstream — the same `RagdollCInfo` feeds every game.
    fn parse_oblivion(stream: &mut NifStream) -> io::Result<Self> {
        let pivot_a = super::read_vec4(stream)?;
        let plane_a = super::read_vec4(stream)?;
        let twist_a = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        let plane_b = super::read_vec4(stream)?;
        let twist_b = super::read_vec4(stream)?;
        Ok(Self {
            twist_a,
            plane_a,
            motor_a: [0.0; 4],
            pivot_a,
            twist_b,
            plane_b,
            motor_b: [0.0; 4],
            pivot_b,
            cone_max_angle: stream.read_f32_le()?,
            plane_min_angle: stream.read_f32_le()?,
            plane_max_angle: stream.read_f32_le()?,
            twist_min_angle: stream.read_f32_le()?,
            twist_max_angle: stream.read_f32_le()?,
            max_friction: stream.read_f32_le()?,
        })
    }
}

/// `bhkLimitedHingeConstraintCInfo`, FO3/FNV (`!#NI_BS_LTE_16#`) layout
/// from nif.xml. Raw Havok Z-up (see [`RagdollCInfo`]).
#[derive(Debug, Clone)]
pub struct LimitedHingeCInfo {
    pub axis_a: [f32; 4],
    pub perp_axis_in_a1: [f32; 4],
    pub perp_axis_in_a2: [f32; 4],
    pub pivot_a: [f32; 4],
    pub axis_b: [f32; 4],
    pub perp_axis_in_b1: [f32; 4],
    pub perp_axis_in_b2: [f32; 4],
    pub pivot_b: [f32; 4],
    pub min_angle: f32,
    pub max_angle: f32,
    pub max_friction: f32,
}

impl LimitedHingeCInfo {
    /// 8 × Vec4 + 3 × f32 = 140 bytes (matches the FNV LimitedHinge
    /// prefix in [`BhkBreakableConstraint::fnv_motor_prefix_size`]).
    fn parse_fo3(stream: &mut NifStream) -> io::Result<Self> {
        let axis_a = super::read_vec4(stream)?;
        let perp_axis_in_a1 = super::read_vec4(stream)?;
        let perp_axis_in_a2 = super::read_vec4(stream)?;
        let pivot_a = super::read_vec4(stream)?;
        let axis_b = super::read_vec4(stream)?;
        let perp_axis_in_b1 = super::read_vec4(stream)?;
        let perp_axis_in_b2 = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        Ok(Self {
            axis_a,
            perp_axis_in_a1,
            perp_axis_in_a2,
            pivot_a,
            axis_b,
            perp_axis_in_b1,
            perp_axis_in_b2,
            pivot_b,
            min_angle: stream.read_f32_le()?,
            max_angle: stream.read_f32_le()?,
            max_friction: stream.read_f32_le()?,
        })
    }

    /// `bhkHingeConstraintCInfo`, FO3/FNV (`!#NI_BS_LTE_16#`) layout from
    /// nif.xml line 2447 — 8 × Vec4 = 128 bytes, the size
    /// [`BhkBreakableConstraint::wrapped_payload_size`] already lists for
    /// `(1, false)`.
    ///
    /// #3330 — a plain hinge is *"a basic hinge with no angular limits or
    /// motor"* (nif.xml's own description), i.e. a `bhkLimitedHingeConstraint`
    /// minus `Min Angle` / `Max Angle` / `Max Friction`. Its first eight
    /// Vector4 fields are byte-identical in name and order to the limited
    /// hinge's, so it decodes into the same [`LimitedHingeCInfo`] and
    /// surfaces through the existing `LimitedHinge` canonical joint — no new
    /// joint kind, no solver-side change.
    ///
    /// The limits are synthesized as `±PI` rather than authored: "no angular
    /// limit" has no representation in `LimitedHingeCInfo`, and a full turn
    /// either way is the closest faithful encoding. `max_friction` is 0 (the
    /// struct has no such field to read).
    ///
    /// Before this, `bhkHingeConstraint` reached `extract_ragdoll` as
    /// `BhkConstraintData::Other` and was dropped, severing the FNV Sentry
    /// Turret and Mini Sentry Turret skeletons into two disconnected
    /// components each — the base separating from the yaw/pitch/brain
    /// assembly.
    fn parse_hinge_fo3(stream: &mut NifStream) -> io::Result<Self> {
        let axis_a = super::read_vec4(stream)?;
        let perp_axis_in_a1 = super::read_vec4(stream)?;
        let perp_axis_in_a2 = super::read_vec4(stream)?;
        let pivot_a = super::read_vec4(stream)?;
        let axis_b = super::read_vec4(stream)?;
        let perp_axis_in_b1 = super::read_vec4(stream)?;
        let perp_axis_in_b2 = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        Ok(Self {
            axis_a,
            perp_axis_in_a1,
            perp_axis_in_a2,
            pivot_a,
            axis_b,
            perp_axis_in_b1,
            perp_axis_in_b2,
            pivot_b,
            min_angle: -std::f32::consts::PI,
            max_angle: std::f32::consts::PI,
            max_friction: 0.0,
        })
    }

    /// `bhkHingeConstraintCInfo`, Oblivion / Morrowind (`#NI_BS_LTE_16#`)
    /// layout — 5 × Vec4 = 80 bytes, in the order `Pivot A`,
    /// `Perp Axis In A1`, `Perp Axis In A2`, `Pivot B`, `Axis B`.
    ///
    /// `Axis A` is **not serialized** in this era. It is not guessed: nif.xml
    /// states of `Perp Axis In A2` that it *"is always the vector product of
    /// Axis A and Perp Axis In A1"*, so with two orthonormal perpendiculars
    /// in hand the rotation axis is recovered exactly as
    /// `Perp Axis In A1 × Perp Axis In A2`. A degenerate (parallel or zero)
    /// pair yields a zero axis, which the PHYSAL translate boundary already
    /// rejects as non-finite/degenerate input the same way it does for any
    /// other joint. `Perp Axis In B1` has no Oblivion counterpart and is
    /// zeroed, exactly as [`Self::parse_oblivion`] does for the limited hinge.
    fn parse_hinge_oblivion(stream: &mut NifStream) -> io::Result<Self> {
        let pivot_a = super::read_vec4(stream)?;
        let perp_axis_in_a1 = super::read_vec4(stream)?;
        let perp_axis_in_a2 = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        let axis_b = super::read_vec4(stream)?;
        // Axis A = Perp A1 × Perp A2 (nif.xml's own stated identity).
        let axis_a = [
            perp_axis_in_a1[1] * perp_axis_in_a2[2] - perp_axis_in_a1[2] * perp_axis_in_a2[1],
            perp_axis_in_a1[2] * perp_axis_in_a2[0] - perp_axis_in_a1[0] * perp_axis_in_a2[2],
            perp_axis_in_a1[0] * perp_axis_in_a2[1] - perp_axis_in_a1[1] * perp_axis_in_a2[0],
            0.0,
        ];
        Ok(Self {
            axis_a,
            perp_axis_in_a1,
            perp_axis_in_a2,
            pivot_a,
            axis_b,
            perp_axis_in_b1: [0.0; 4],
            perp_axis_in_b2: [0.0; 4],
            pivot_b,
            min_angle: -std::f32::consts::PI,
            max_angle: std::f32::consts::PI,
            max_friction: 0.0,
        })
    }

    /// Oblivion / Morrowind (`#NI_BS_LTE_16#`) layout from nif.xml:
    /// 7 × Vec4 + 3 × f32 = 124 bytes. No Perp Axis In B1 (an FO3+
    /// addition) and a pivots-first order. The absent perp axis is zeroed;
    /// the PHYSAL translate boundary DOES read it (#2448 / PHYS-02), but
    /// the solver's `frame_rot` falls back to a synthesized perpendicular
    /// for a degenerate (zero) input, so the zeroed value is still safe.
    fn parse_oblivion(stream: &mut NifStream) -> io::Result<Self> {
        let pivot_a = super::read_vec4(stream)?;
        let axis_a = super::read_vec4(stream)?;
        let perp_axis_in_a1 = super::read_vec4(stream)?;
        let perp_axis_in_a2 = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        let axis_b = super::read_vec4(stream)?;
        let perp_axis_in_b2 = super::read_vec4(stream)?;
        Ok(Self {
            axis_a,
            perp_axis_in_a1,
            perp_axis_in_a2,
            pivot_a,
            axis_b,
            perp_axis_in_b1: [0.0; 4],
            perp_axis_in_b2,
            pivot_b,
            min_angle: stream.read_f32_le()?,
            max_angle: stream.read_f32_le()?,
            max_friction: stream.read_f32_le()?,
        })
    }
}

/// `bhkPrismaticConstraintCInfo` (#3792) — nif.xml: "Creates a rail
/// between two bodies that allows translation along a single axis with
/// linear limits and a motor. All three rotation axes and the remaining
/// two translation axes are fixed." Raw Havok Z-up (see [`RagdollCInfo`]).
#[derive(Debug, Clone)]
pub struct PrismaticCInfo {
    pub sliding_a: [f32; 4],
    pub rotation_a: [f32; 4],
    pub plane_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub sliding_b: [f32; 4],
    pub rotation_b: [f32; 4],
    pub plane_b: [f32; 4],
    pub pivot_b: [f32; 4],
    pub min_distance: f32,
    pub max_distance: f32,
    pub friction: f32,
}

impl PrismaticCInfo {
    /// FO3/FNV (`!#NI_BS_LTE_16#`) layout, nif.xml `since="20.2.0.7"`:
    /// 8 × Vec4 + 3 × f32 = 140 bytes (matches the FNV Prismatic prefix
    /// in [`BhkBreakableConstraint::fnv_motor_prefix_size`]). Field
    /// order: Sliding A, Rotation A, Plane A, Pivot A, Sliding B,
    /// Rotation B, Plane B, Pivot B — note this is NOT the same order as
    /// the Oblivion layout below (nif.xml's own comment: "In reality
    /// Havok loads these as Transform A and Transform B using
    /// hkTransform").
    fn parse_fo3(stream: &mut NifStream) -> io::Result<Self> {
        let sliding_a = super::read_vec4(stream)?;
        let rotation_a = super::read_vec4(stream)?;
        let plane_a = super::read_vec4(stream)?;
        let pivot_a = super::read_vec4(stream)?;
        let sliding_b = super::read_vec4(stream)?;
        let rotation_b = super::read_vec4(stream)?;
        let plane_b = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        Ok(Self {
            sliding_a,
            rotation_a,
            plane_a,
            pivot_a,
            sliding_b,
            rotation_b,
            plane_b,
            pivot_b,
            min_distance: stream.read_f32_le()?,
            max_distance: stream.read_f32_le()?,
            friction: stream.read_f32_le()?,
        })
    }

    /// Oblivion / Morrowind (`#NI_BS_LTE_16#`) layout, nif.xml
    /// `until="20.0.0.5"`: 8 × Vec4 + 3 × f32 = 140 bytes, no Motor
    /// field. Field order: Pivot A, Rotation A, Plane A, Sliding A,
    /// Sliding B, Pivot B, Rotation B, Plane B.
    fn parse_oblivion(stream: &mut NifStream) -> io::Result<Self> {
        let pivot_a = super::read_vec4(stream)?;
        let rotation_a = super::read_vec4(stream)?;
        let plane_a = super::read_vec4(stream)?;
        let sliding_a = super::read_vec4(stream)?;
        let sliding_b = super::read_vec4(stream)?;
        let pivot_b = super::read_vec4(stream)?;
        let rotation_b = super::read_vec4(stream)?;
        let plane_b = super::read_vec4(stream)?;
        Ok(Self {
            sliding_a,
            rotation_a,
            plane_a,
            pivot_a,
            sliding_b,
            rotation_b,
            plane_b,
            pivot_b,
            min_distance: stream.read_f32_le()?,
            max_distance: stream.read_f32_le()?,
            friction: stream.read_f32_le()?,
        })
    }
}

impl BhkConstraint {
    /// Read the shared `bhkConstraintCInfo` prefix — 16 bytes:
    /// `num_entities u32 + entity_a i32 + entity_b i32 + priority u32`.
    /// Returns `(entity_a, entity_b, priority)`.
    fn parse_base(stream: &mut NifStream) -> io::Result<(BlockRef, BlockRef, u32)> {
        let _num_entities = stream.read_u32_le()?;
        let entity_a = stream.read_block_ref()?;
        let entity_b = stream.read_block_ref()?;
        let priority = stream.read_u32_le()?;
        Ok((entity_a, entity_b, priority))
    }

    /// `bhkBallSocketConstraintChain` — nif.xml declares it
    /// `inherit="bhkSerializable"`, **not** `bhkConstraint`, so it has NO
    /// leading `bhkConstraintCInfo`; its real entity refs + priority live in
    /// the TRAILING `Constraint Chain Info`. Pre-#1604 the dispatch routed it
    /// through [`Self::parse`], whose unconditional [`Self::parse_base`]
    /// reinterpreted the first 16 B of pivot data as entity/priority refs
    /// (garbage), and the chain was effectively dropped (`block_size`
    /// recovered the misalignment on FO3+/Skyrim).
    ///
    /// Layout (nif.xml `bhkBallSocketConstraintChain`; verified — a 7-entity
    /// chain is exactly `block_size` 260):
    /// - `num_pivots: u32`
    /// - `pivots: [bhkBallAndSocketConstraintCInfo; num_pivots / 2]` — each is
    ///   two `Vector4` (Pivot A, Pivot B) = 32 B
    /// - `tau`, `damping`, `constraint_force_mixing`, `max_error_distance`:
    ///   4 × `f32`
    /// - `bhkConstraintChainCInfo`: `num_chained: u32`, then `num_chained`
    ///   chained-entity `Ptr`s, then a `bhkConstraintCInfo` (16 B) — the real
    ///   `entity_a` / `entity_b` / `priority`.
    ///
    /// Byte-exact for every Bethesda format (block_sizes or not), so it can
    /// never cascade and stores the chain's real refs instead of garbage.
    /// The pivot/float/chained data is consumed but not yet retained — no
    /// consumer reads it; `data` stays [`BhkConstraintData::Other`].
    pub fn parse_ball_socket_chain(
        stream: &mut NifStream,
        type_name: &'static str,
    ) -> io::Result<Self> {
        let num_pivots = stream.read_u32_le()?;
        // Each bhkBallAndSocketConstraintCInfo = Pivot A + Pivot B (2 × Vec4).
        // A corrupt count walks the stream to EOF (read errors out) rather
        // than looping unboundedly.
        for _ in 0..(num_pivots / 2) {
            let _pivot_a = super::read_vec4(stream)?;
            let _pivot_b = super::read_vec4(stream)?;
        }
        let _tau = stream.read_f32_le()?;
        let _damping = stream.read_f32_le()?;
        let _constraint_force_mixing = stream.read_f32_le()?;
        let _max_error_distance = stream.read_f32_le()?;
        // bhkConstraintChainCInfo: chained-entity Ptr array, then the real
        // trailing bhkConstraintCInfo.
        let num_chained = stream.read_u32_le()?;
        for _ in 0..num_chained {
            let _chained = stream.read_block_ref()?;
        }
        let (entity_a, entity_b, priority) = Self::parse_base(stream)?;
        Ok(Self {
            type_name,
            entity_a,
            entity_b,
            priority,
            data: BhkConstraintData::Other,
        })
    }

    /// Decode the inner constraint of an FO3+ `bhkMalleableConstraint`.
    /// Layout (nif.xml `bhkMalleableConstraintCInfo`, `since 20.2.0.7`):
    /// a `Type u32`, then an inner `bhkConstraintCInfo` (16 B, whose
    /// entities are −1/−1 — the real bodies are the OUTER base the caller
    /// already read), then the typed inner CInfo. The trailing `Strength`
    /// f32 (and the inner CInfo's motor) are left for `block_size`
    /// recovery. FNV humanoid ragdolls wrap 14 of their 17 joints this way,
    /// so a malleable-wrapped Ragdoll surfaces identically to a bare one.
    fn parse_fo3_malleable_inner(stream: &mut NifStream) -> io::Result<BhkConstraintData> {
        let wrapped_type = stream.read_u32_le()?;
        // Inner bhkConstraintCInfo — discard (entities are −1/−1).
        let _inner = Self::parse_base(stream)?;
        Ok(match wrapped_type {
            7 => BhkConstraintData::Ragdoll(RagdollCInfo::parse_fo3(stream)?),
            2 => BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_fo3(stream)?),
            // 1 Hinge — #3330, decoded into the same canonical joint.
            1 => BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_hinge_fo3(stream)?),
            // 6 Prismatic — #3792.
            6 => BhkConstraintData::Prismatic(PrismaticCInfo::parse_fo3(stream)?),
            other => {
                // #1609 — mirror the Oblivion size-skip on FO3+: consume the
                // undecoded inner CInfo's FIXED body so stream consumption is
                // self-consistent even on a (hypothetical) sizeless file,
                // matching the decoded Ragdoll/LimitedHinge arms above instead
                // of relying entirely on the outer `block_size` seek. Reuses
                // the authoritative FNV size tables on `BhkBreakableConstraint`
                // (same module → private-fn access): non-motor types
                // (BallAndSocket 32 / Hinge 128 / StiffSpring 36) use their
                // full fixed body; motor-bearing Prismatic uses its fixed
                // prefix (140). Like the decoded arms, any variable Motor + the
                // malleable `Strength` trailer stay with `block_size` recovery
                // (no FO3+ file is sizeless). Unknown types fall through with
                // no skip — identical to the prior behaviour.
                let skip = BhkBreakableConstraint::wrapped_payload_size(other, false)
                    .or_else(|| BhkBreakableConstraint::fnv_motor_prefix_size(other));
                if let Some(size) = skip {
                    stream.skip(size)?;
                }
                BhkConstraintData::Other
            }
        })
    }

    /// Parse a constraint block by type name. For the two joints a
    /// humanoid ragdoll uses (`bhkRagdollConstraint` /
    /// `bhkLimitedHingeConstraint`, bare or malleable-wrapped) the typed
    /// CInfo is decoded into [`BhkConstraintData`] in the era-correct field
    /// order — Oblivion (`#NI_BS_LTE_16#`) or FO3+ (`!#NI_BS_LTE_16#`).
    /// Every other type reads its base (and, on Oblivion, skips its fixed
    /// payload) and stays [`BhkConstraintData::Other`]; on FO3+ the outer
    /// walker seeks past any unread remainder via `block_size`.
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let (entity_a, entity_b, priority) = Self::parse_base(stream)?;

        // Oblivion byte-exact payload sizes (post-base bytes). Derived
        // from nif.xml with `#NI_BS_LTE_16#` active. A zero means
        // "drop through to the FO3+ short-stub path". nif.xml
        // `#NI_BS_LTE_16#` is `(#BSVER# #LTE# 16)` — a *bsver* test, not a
        // NIF-version one; matches the sibling rigid_body.rs gate. (#1608)
        let is_oblivion = stream.bsver() <= crate::version::bsver::NI_BS_LTE_16;
        if is_oblivion {
            // PHYSAL per-game seam: decode the two joints a humanoid
            // ragdoll uses in the Oblivion (`#NI_BS_LTE_16#`) field order.
            // Everything downstream of the resulting `BhkConstraintData` is
            // game-agnostic — only the byte layout differs here, so this is
            // the *only* Oblivion-specific code in the ragdoll path.
            match type_name {
                "bhkRagdollConstraint" => {
                    return Ok(Self {
                        type_name,
                        entity_a,
                        entity_b,
                        priority,
                        data: BhkConstraintData::Ragdoll(RagdollCInfo::parse_oblivion(stream)?),
                    });
                }
                "bhkLimitedHingeConstraint" => {
                    return Ok(Self {
                        type_name,
                        entity_a,
                        entity_b,
                        priority,
                        data: BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_oblivion(
                            stream,
                        )?),
                    });
                }
                // #3330 — sibling of the FO3+ arm; `Axis A` is recovered from
                // the two perpendiculars (see `parse_hinge_oblivion`).
                "bhkHingeConstraint" => {
                    return Ok(Self {
                        type_name,
                        entity_a,
                        entity_b,
                        priority,
                        data: BhkConstraintData::LimitedHinge(
                            LimitedHingeCInfo::parse_hinge_oblivion(stream)?,
                        ),
                    });
                }
                // #3792 — sibling of the three arms above.
                "bhkPrismaticConstraint" => {
                    return Ok(Self {
                        type_name,
                        entity_a,
                        entity_b,
                        priority,
                        data: BhkConstraintData::Prismatic(PrismaticCInfo::parse_oblivion(
                            stream,
                        )?),
                    });
                }
                _ => {}
            }

            // The remaining types aren't decoded yet; consume their fixed
            // Oblivion payload by byte size and stay `Other`.
            let payload_size: Option<u64> = match type_name {
                // 2 × Vec4
                "bhkBallAndSocketConstraint" => Some(32),
                // `bhkHingeConstraint` (5 × Vec4 = 80) and
                // `bhkPrismaticConstraint` (8 × Vec4 + 3 × f32 = 140) are
                // decoded above (since #3330 / #3792) and no longer skipped
                // here.
                // 2 × Vec4 + f32
                "bhkStiffSpringConstraint" => Some(36),
                // Malleable wrapper has a runtime-dispatched inner
                // CInfo — handle separately below.
                "bhkMalleableConstraint" => None,
                _ => None,
            };

            if let Some(size) = payload_size {
                stream.skip(size)?;
                return Ok(Self {
                    type_name,
                    entity_a,
                    entity_b,
                    priority,
                    data: BhkConstraintData::Other,
                });
            }

            if type_name == "bhkMalleableConstraint" {
                // Oblivion layout: type u32 + nested bhkConstraintCInfo
                // (16, entities −1/−1) + wrapped CInfo + tau f32 + damping
                // f32. Decode an inner Ragdoll / LimitedHinge so a
                // malleable-wrapped joint surfaces identically to a bare
                // one; other inner types skip by size and stay `Other`.
                let wrapped_type = stream.read_u32_le()?;
                let _nested = Self::parse_base(stream)?;
                let data = match wrapped_type {
                    7 => BhkConstraintData::Ragdoll(RagdollCInfo::parse_oblivion(stream)?),
                    2 => {
                        BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_oblivion(stream)?)
                    }
                    // 1 Hinge — #3330.
                    1 => BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_hinge_oblivion(
                        stream,
                    )?),
                    // 6 Prismatic — #3792.
                    6 => BhkConstraintData::Prismatic(PrismaticCInfo::parse_oblivion(stream)?),
                    other => {
                        let inner_size: u64 = match other {
                            0 => 32, // Ball and Socket
                            8 => 36, // Stiff Spring
                            unknown => {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!(
                                        "bhkMalleableConstraint: unknown inner type {unknown} — \
                                         stream position unreliable"
                                    ),
                                ));
                            }
                        };
                        stream.skip(inner_size)?;
                        BhkConstraintData::Other
                    }
                };
                // Tau + Damping (Oblivion trailer).
                stream.skip(8)?;
                return Ok(Self {
                    type_name,
                    entity_a,
                    entity_b,
                    priority,
                    data,
                });
            }
        }

        // FO3+ (FNV/FO3, `!#NI_BS_LTE_16#`). For the two variants a
        // humanoid ragdoll uses, decode the typed CInfo prefix
        // (field order + sizes from nif.xml, cross-checked against the
        // breakable-constraint byte tables below). The trailing
        // `bhkConstraintMotorCInfo` is deliberately NOT consumed here:
        // it's the last field of the struct, carries nothing slice 1
        // needs, and the outer parse_nif loop absolute-seeks to the next
        // block via the header's block_sizes table (always present on
        // v >= 20.2.0.7) — so skipping the motor-type dispatch keeps a
        // recoverable block from ever becoming a hard parse error on
        // unexpected motor data. Every other type stays a name-only stub.
        let data = match type_name {
            "bhkRagdollConstraint" => BhkConstraintData::Ragdoll(RagdollCInfo::parse_fo3(stream)?),
            "bhkLimitedHingeConstraint" => {
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_fo3(stream)?)
            }
            // #3330 — a limitless hinge is a limited hinge minus the three
            // trailing scalars; same `LimitedHinge` canonical joint.
            "bhkHingeConstraint" => {
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_hinge_fo3(stream)?)
            }
            // #3792 — 1-DOF sliding rail. Same "fixed prefix only, motor
            // left for block_size recovery" posture as Ragdoll/LimitedHinge
            // above.
            "bhkPrismaticConstraint" => {
                BhkConstraintData::Prismatic(PrismaticCInfo::parse_fo3(stream)?)
            }
            "bhkMalleableConstraint" => Self::parse_fo3_malleable_inner(stream)?,
            _ => BhkConstraintData::Other,
        };
        Ok(Self {
            type_name,
            entity_a,
            entity_b,
            priority,
            data,
        })
    }
}

/// `bhkBreakableConstraint` — wrapper around another constraint that
/// can "break" (stop applying force) once a force threshold is
/// exceeded. nif.xml line 7027.
///
/// Byte-accurate parse is critical on Oblivion (no block_sizes
/// recovery). The wrapped payload size depends on the inner
/// `hkConstraintType` enum, which maps identically to the sizes
/// [`BhkConstraint::parse`] hard-codes; we reuse that same table here.
/// On FO3+ the outer walker seeks via `block_size` if the inner type
/// is one we haven't sized (e.g. `Malleable`, which carries nested
/// CInfo dispatch).
#[derive(Debug)]
pub struct BhkBreakableConstraint {
    /// Outer `bhkConstraintCInfo` — the two entities this wrapper
    /// constrains.
    pub entity_a: BlockRef,
    pub entity_b: BlockRef,
    pub priority: u32,
    /// `hkConstraintType` enum value identifying the inner data
    /// layout (0 = Ball and Socket, 1 = Hinge, …, 13 = Malleable).
    pub wrapped_type: u32,
    /// Force magnitude above which the constraint releases.
    pub threshold: f32,
    /// When `true`, the constraint is destroyed once the threshold
    /// is hit; when `false`, it stops applying force but stays
    /// present so the game can re-enable it.
    pub remove_when_broken: bool,
    /// The wrapped joint's decoded CInfo, when `wrapped_type` is one of
    /// the four kinds a canonical joint exists for (Hinge/LimitedHinge/
    /// Prismatic/Ragdoll — 1/2/6/7). `Other` for BallAndSocket(0) /
    /// StiffSpring(8) / a nested Malleable(13), which either have no
    /// canonical joint kind yet or dispatch to their own inner type.
    ///
    /// #3792 — before this, the wrapped payload was `stream.skip`ped
    /// (only `wrapped_type` + the trailer fields survived), so a
    /// breakable-wrapped Ragdoll/LimitedHinge/Prismatic joint reached
    /// `extract_ragdoll` with no geometry to rebuild it from and was
    /// dropped outright (#1850). Decoding it here — reusing the exact
    /// same per-variant `parse_fo3`/`parse_oblivion` field order as a
    /// bare [`BhkConstraint`] — closes that: `extract_ragdoll`'s
    /// `BhkBreakableConstraint` arm now has the same `BhkConstraintData`
    /// a bare constraint would carry.
    pub data: BhkConstraintData,
}

impl BhkBreakableConstraint {
    /// Payload size (in bytes, past the 16-byte outer bhkConstraintCInfo
    /// and 4-byte wrapped type discriminator) for the wrapped CInfo,
    /// keyed on both the wrapped-type discriminator AND the parser's
    /// version branch. `None` means "wrapped-payload size depends on a
    /// runtime-dispatched motor type byte; rely on `block_size`
    /// recovery for the trailer."
    ///
    /// nif.xml sizes per `#NI_BS_LTE_16#` (Oblivion until 20.0.0.5) vs
    /// `!#NI_BS_LTE_16#` (FNV/FO3+ since 20.2.0.7):
    ///
    /// | type | Oblivion | FNV | Notes |
    /// |-----:|---------:|----:|-------|
    /// | 0 BallAndSocket | 32 | 32 | size attr on struct, no version diff |
    /// | 1 Hinge | 80 | **128** | FNV adds Axis A + Perp Axis In B1 + Pivot A (3 × Vec4 = +48) |
    /// | 2 LimitedHinge | 124 | None | FNV adds Perp Axis In B1 (Vec4) + variable Motor |
    /// | 6 Prismatic | 140 | None | FNV adds variable Motor |
    /// | 7 Ragdoll | 120 | None | FNV adds Motor A + Motor B (2 × Vec4) + variable Motor |
    /// | 8 StiffSpring | 36 | 36 | size attr on struct, no version diff |
    /// | 13 Malleable | None | None | nested-CInfo dispatch — outside this table |
    ///
    /// Pre-#633 the table was Oblivion-only and only consulted on the
    /// Oblivion branch — so FNV constraint blocks fell into the FO3+
    /// short-stub and silently zeroed `threshold` / `remove_when_broken`.
    /// Post-fix, the FNV-derivable rows let the parser fully consume
    /// the wrapped payload and read the trailer fields. Motor-bearing
    /// FNV constraints still rely on `block_size` recovery — no
    /// regression vs the old behaviour, just a wider correct path.
    fn wrapped_payload_size(wrapped_type: u32, is_oblivion: bool) -> Option<u64> {
        match (wrapped_type, is_oblivion) {
            // BallAndSocket — 2 × Vec4 = 32 B regardless of version.
            (0, _) => Some(32),
            // Hinge — Oblivion 5 × Vec4, FNV 8 × Vec4.
            (1, true) => Some(80),
            (1, false) => Some(128),
            // LimitedHinge — Oblivion 7 × Vec4 + 3 × f32 = 124. FNV adds
            // 1 × Vec4 + variable Motor; size depends on motor type.
            (2, true) => Some(124),
            (2, false) => None,
            // Prismatic — Oblivion 8 × Vec4 + 3 × f32 = 140. FNV adds
            // variable Motor; size depends on motor type.
            (6, true) => Some(140),
            (6, false) => None,
            // Ragdoll — Oblivion 6 × Vec4 + 6 × f32 = 120. FNV adds
            // 2 × Vec4 (Motor A/B) + 6 × f32 + variable Motor; size
            // depends on motor type.
            (7, true) => Some(120),
            (7, false) => None,
            // StiffSpring — 2 × Vec4 + f32 = 36 B regardless of version,
            // no Motor field.
            (8, _) => Some(36),
            // 13 Malleable wraps another CInfo with its own type dispatch.
            _ => None,
        }
    }

    /// Fixed-prefix byte count (positions + scalars, no motor) for
    /// FNV motor-bearing constraints. Returns `None` for any wrapped
    /// type that doesn't carry a runtime motor on FNV.
    ///
    /// Layouts per nif.xml (`!#NI_BS_LTE_16#` branch):
    ///   - LimitedHinge: 8 × Vec4 + 3 × f32 = 140 B (then Motor)
    ///   - Prismatic:    8 × Vec4 + 3 × f32 = 140 B (then Motor)
    ///   - Ragdoll:      8 × Vec4 + 6 × f32 = 152 B (then Motor)
    fn fnv_motor_prefix_size(wrapped_type: u32) -> Option<u64> {
        match wrapped_type {
            2 => Some(140), // LimitedHinge
            6 => Some(140), // Prismatic
            7 => Some(152), // Ragdoll
            _ => None,
        }
    }

    /// Consume a `bhkConstraintMotorCInfo` from the stream — 1 byte
    /// `hkMotorType` discriminator + conditional payload. Sizes per
    /// nif.xml:
    ///   - 0 NONE: 0 B
    ///   - 1 POSITION: 25 B
    ///   - 2 VELOCITY: 18 B
    ///   - 3 SPRING:   17 B
    ///
    /// Errors on an unknown motor type — the stream position would be
    /// unreliable past the byte we just read.
    fn consume_motor(stream: &mut NifStream) -> io::Result<()> {
        let motor_type = stream.read_u8()?;
        let payload: u64 = match motor_type {
            0 => 0,
            1 => 25,
            2 => 18,
            3 => 17,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "bhkConstraintMotorCInfo: unknown motor type {other} — \
                         stream position unreliable"
                    ),
                ));
            }
        };
        stream.skip(payload)?;
        Ok(())
    }

    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Outer bhkConstraintCInfo (16 bytes).
        let (entity_a, entity_b, priority) = BhkConstraint::parse_base(stream)?;
        // Wrapped constraint: type(u32) + inner bhkConstraintCInfo(16)
        // + variable inner data.
        let wrapped_type = stream.read_u32_le()?;
        // Inner bhkConstraintCInfo — always 16 bytes.
        stream.skip(16)?;
        // nif.xml `#NI_BS_LTE_16#` = `(#BSVER# #LTE# 16)` — a bsver test,
        // not a NIF-version one; matches rigid_body.rs. (#1608)
        let is_oblivion = stream.bsver() <= crate::version::bsver::NI_BS_LTE_16;

        // #3792 — decode the wrapped payload for the four types a
        // canonical joint exists for (Hinge/LimitedHinge/Prismatic/
        // Ragdoll = 1/2/6/7), reusing bare `BhkConstraint`'s own per-era
        // field-order parsers instead of a byte-count skip. Byte
        // consumption is identical to the pre-#3792 skip sizes (each
        // `parse_fo3`/`parse_oblivion` reads exactly what
        // `wrapped_payload_size`/`fnv_motor_prefix_size` tabulate below),
        // so the trailer stays reachable exactly as before — what
        // changes is that the geometry now survives instead of being
        // discarded. This closes #1850: a breakable-wrapped joint now
        // reaches `extract_ragdoll` with real `BhkConstraintData`, not
        // `Other`.
        //
        // BallAndSocket(0) / StiffSpring(8) have no canonical joint kind
        // yet (same posture as a bare `BhkConstraint` of either type) and
        // stay a byte-count skip into `Other`. Malleable(13) has no
        // fixed size on either version (nested dispatch) and falls
        // through to `block_size` recovery below, unchanged.
        let (data, decoded) = match (wrapped_type, is_oblivion) {
            (1, true) => (
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_hinge_oblivion(stream)?),
                true,
            ),
            (1, false) => (
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_hinge_fo3(stream)?),
                true,
            ),
            (2, true) => (
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_oblivion(stream)?),
                true,
            ),
            (2, false) => (
                BhkConstraintData::LimitedHinge(LimitedHingeCInfo::parse_fo3(stream)?),
                true,
            ),
            (6, true) => (
                BhkConstraintData::Prismatic(PrismaticCInfo::parse_oblivion(stream)?),
                true,
            ),
            (6, false) => (
                BhkConstraintData::Prismatic(PrismaticCInfo::parse_fo3(stream)?),
                true,
            ),
            (7, true) => (
                BhkConstraintData::Ragdoll(RagdollCInfo::parse_oblivion(stream)?),
                true,
            ),
            (7, false) => (
                BhkConstraintData::Ragdoll(RagdollCInfo::parse_fo3(stream)?),
                true,
            ),
            (0, _) => {
                // BallAndSocket — 2 × Vec4, no version difference.
                stream.skip(32)?;
                (BhkConstraintData::Other, true)
            }
            (8, _) => {
                // StiffSpring — 2 × Vec4 + f32, no version difference.
                stream.skip(36)?;
                (BhkConstraintData::Other, true)
            }
            _ => (BhkConstraintData::Other, false), // Malleable(13) or unknown
        };

        if decoded {
            // The FNV motor-bearing types (LimitedHinge/Prismatic/Ragdoll)
            // only had their fixed prefix consumed above — the trailing
            // `bhkConstraintMotorCInfo` is variable-sized and must be
            // consumed separately so the trailer fields are reachable.
            if !is_oblivion && matches!(wrapped_type, 2 | 6 | 7) {
                Self::consume_motor(stream)?;
            }
            let threshold = stream.read_f32_le()?;
            let remove_when_broken = stream.read_u8()? != 0;
            return Ok(Self {
                entity_a,
                entity_b,
                priority,
                wrapped_type,
                threshold,
                remove_when_broken,
                data,
            });
        }

        // Malleable (wrapped_type == 13) wraps another CInfo with its
        // own type dispatch — outside this table on either version.
        // `block_size` recovery in the outer walker handles the byte
        // skip; trailer fields default to zero.
        Ok(Self {
            entity_a,
            entity_b,
            priority,
            wrapped_type,
            threshold: 0.0,
            remove_when_broken: false,
            data: BhkConstraintData::Other,
        })
    }
}

impl_ni_object!(BhkBreakableConstraint => "bhkBreakableConstraint");
