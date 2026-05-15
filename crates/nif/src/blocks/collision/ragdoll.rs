//! Ragdoll + bone-pose templates (FO3+).
//!
//! BoneTransform, BonePose, BhkPoseArray, BhkRagdollTemplate,
//! BhkRagdollTemplateData — the persistent ragdoll articulation.

use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::io;
use std::sync::Arc;


/// One bone's pose-frame inside a [`BonePose`]. Translation + quaternion
/// rotation + scale = 40 bytes, matching nif.xml's `size="40"` pin.
#[derive(Debug, Clone, Copy)]
pub struct BoneTransform {
    pub translation: [f32; 3],
    /// `hkQuaternion` storage order is `(x, y, z, w)` per nif.xml's
    /// `hkQuaternion` struct — same on-disk layout as `Quaternion`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl BoneTransform {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let tx = stream.read_f32_le()?;
        let ty = stream.read_f32_le()?;
        let tz = stream.read_f32_le()?;
        let rx = stream.read_f32_le()?;
        let ry = stream.read_f32_le()?;
        let rz = stream.read_f32_le()?;
        let rw = stream.read_f32_le()?;
        let sx = stream.read_f32_le()?;
        let sy = stream.read_f32_le()?;
        let sz = stream.read_f32_le()?;
        Ok(Self {
            translation: [tx, ty, tz],
            rotation: [rx, ry, rz, rw],
            scale: [sx, sy, sz],
        })
    }
}

/// One full skeletal pose snapshot — `Num Transforms` × [`BoneTransform`].
/// In `.psa` files there's exactly one entry per bone listed in the
/// containing [`BhkPoseArray::bones`], but the on-disk count is parsed
/// independently so a malformed file can't reach into the next pose's
/// memory.
#[derive(Debug, Clone)]
pub struct BonePose {
    pub transforms: Vec<BoneTransform>,
}

impl BonePose {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let n = stream.read_u32_le()? as usize;
        // Each BoneTransform is exactly 40 bytes — bound the alloc
        // against the remaining stream so a junk count can't OOM.
        stream.check_alloc(n.saturating_mul(40))?;
        let mut transforms = Vec::with_capacity(n);
        for _ in 0..n {
            transforms.push(BoneTransform::parse(stream)?);
        }
        Ok(Self { transforms })
    }
}

/// `bhkPoseArray` — FO3/FNV death-pose library. Found at the root of
/// `meshes/idleanims/deathposes.psa` etc. The game samples a random
/// `poses[i]` entry and applies it to the ragdoll the moment the NPC
/// dies. See #980 / NIF-D5-NEW-04 and nif.xml.
#[derive(Debug)]
pub struct BhkPoseArray {
    /// Bone names this pose array targets — one entry per bone in the
    /// skeleton, matched by name at runtime so the file can be reused
    /// across creatures with different bone counts.
    pub bones: Vec<Option<Arc<str>>>,
    /// Pose snapshots — the engine picks one at random per death event.
    pub poses: Vec<BonePose>,
}


impl BhkPoseArray {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_bones = stream.read_u32_le()? as usize;
        // String-table indices are 4 bytes on disk.
        stream.check_alloc(num_bones.saturating_mul(4))?;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bones.push(stream.read_string()?);
        }
        let num_poses = stream.read_u32_le()? as usize;
        // BonePose carries at least 4 bytes for the inner count. Real
        // bound enforcement happens inside `BonePose::parse`.
        stream.check_alloc(num_poses.saturating_mul(4))?;
        let mut poses = Vec::with_capacity(num_poses);
        for _ in 0..num_poses {
            poses.push(BonePose::parse(stream)?);
        }
        Ok(Self { bones, poses })
    }
}

/// `bhkRagdollTemplate` — FO3/FNV per-creature ragdoll constraint
/// template. Inherits `NiExtraData` (parsed inline since the
/// `NiExtraData` machinery returns its own struct and this block
/// type re-uses only the trailing fields). The `bones` list holds
/// references to companion [`BhkRagdollTemplateData`] blocks describing
/// each constraint hierarchy. See #980 / NIF-D5-NEW-04 and nif.xml.
#[derive(Debug)]
pub struct BhkRagdollTemplate {
    /// Inherited `NiExtraData.Name`. `None` on pre-10.0.1.0 content
    /// (gated by `read_extra_data_name`) — `.rdt` files are FO3+ only
    /// so this is always `Some` in practice but the gate is kept for
    /// uniformity with the rest of the NiExtraData family.
    pub name: Option<Arc<str>>,
    /// Refs to constraint-data blocks. nif.xml types these as
    /// `Ref<NiObject>` — in vanilla content they always resolve to
    /// [`BhkRagdollTemplateData`], but we preserve the polymorphic
    /// `Ref<NiObject>` shape so future modders' substitutions still
    /// parse cleanly.
    pub bones: Vec<BlockRef>,
}


impl BhkRagdollTemplate {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — `Name` only, gated on stream version.
        let name = stream.read_extra_data_name()?;
        let num_bones = stream.read_u32_le()? as usize;
        // BlockRefs are 4 bytes each.
        stream.check_alloc(num_bones.saturating_mul(4))?;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bones.push(stream.read_block_ref()?);
        }
        Ok(Self { name, bones })
    }
}

/// `bhkRagdollTemplateData` — companion data block for
/// [`BhkRagdollTemplate`]. nif.xml schema carries a mass / restitution /
/// friction / radius / material block plus an array of polymorphic
/// `bhkWrappedConstraintData` entries — the inner constraint variant
/// switches on a `Type` discriminator and re-uses six different
/// constraint-info layouts (BallAndSocket / Hinge / LimitedHinge /
/// Prismatic / Ragdoll / StiffSpring / Malleable). The wrapper is
/// shared with `bhkBlendCollisionObject`'s constraint stack and would
/// need the full constraint-data parser family to expand.
///
/// First-pass stub per #980 / NIF-D5-NEW-04: parse the leading
/// scalars (Name / Mass / Restitution / Friction / Radius / Material)
/// + the constraint count, then skip the constraint array via
/// `block_size` so the stream stays aligned. The fixed-layout head
/// covers what we'd actually consume today (ragdoll mass tuning);
/// the polymorphic tail is left for a follow-up.
#[derive(Debug)]
pub struct BhkRagdollTemplateData {
    pub name: Option<Arc<str>>,
    pub mass: f32,
    pub restitution: f32,
    pub friction: f32,
    pub radius: f32,
    pub material: u32,
    /// Count of `bhkWrappedConstraintData` entries that follow on
    /// disk. The entries themselves are skipped through `block_size`
    /// (polymorphic constraint-CInfo expansion is a follow-up).
    pub num_constraints: u32,
}


impl BhkRagdollTemplateData {
    pub fn parse(stream: &mut NifStream, block_size: Option<u32>) -> io::Result<Self> {
        // Pin the start so we can compute how many bytes of constraint
        // payload to skip after the fixed-layout head.
        let start = stream.position();
        let name = stream.read_string()?;
        let mass = stream.read_f32_le()?;
        let restitution = stream.read_f32_le()?;
        let friction = stream.read_f32_le()?;
        let radius = stream.read_f32_le()?;
        let material = stream.read_u32_le()?;
        let num_constraints = stream.read_u32_le()?;
        // Skip the constraint array using block_size when available
        // (FO3+ has it; pre-Bethesda content never reaches this code
        // because the type is `#FO3_AND_LATER#`). Falls back to the
        // unknown-block-type dispatch handler if block_size is missing,
        // which would have happened anyway pre-#980.
        if let Some(sz) = block_size {
            let consumed = stream.position().saturating_sub(start);
            let remaining = (sz as u64).saturating_sub(consumed);
            if remaining > 0 {
                stream.skip(remaining)?;
            }
        }
        Ok(Self {
            name,
            mass,
            restitution,
            friction,
            radius,
            material,
            num_constraints,
        })
    }
}

impl_ni_object!(
    BhkPoseArray => "bhkPoseArray",
    BhkRagdollTemplate => "bhkRagdollTemplate",
    BhkRagdollTemplateData => "bhkRagdollTemplateData",
);
