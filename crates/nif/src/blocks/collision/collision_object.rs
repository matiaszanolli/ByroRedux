//! Collision-object variants.
//!
//! Base + Bhk + BhkN P + BhkP + SystemBinary — wrappers that link a rigid
//! body / phantom into the scene-graph collision-target field.

use crate::blocks::NiObject;
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;


/// NiCollisionObject — base class for collision attachments.
///
/// Per nif.xml `NiCollisionObject` has exactly one field, a weak `Target`
/// pointer back at the NiAVObject this collision object is attached to.
/// Bethesda Havok subclasses add `Flags` and `Body` on top (see
/// `BhkCollisionObject` below), but the plain base occasionally appears
/// as a direct block in Oblivion scenes (#125). Because Oblivion NIFs
/// have no `block_sizes`, an unknown-type fallback cascades the whole
/// parse; parsing even the base 4 bytes is enough to keep the loop
/// alive.
#[derive(Debug)]
pub struct NiCollisionObjectBase {
    pub target_ref: BlockRef,
}


impl NiCollisionObjectBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        Ok(Self { target_ref })
    }
}

/// bhkCollisionObject — attaches a rigid body to a NiAVObject.
/// Concrete subclass of bhkNiCollisionObject (NiCollisionObject base).
#[derive(Debug)]
pub struct BhkCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    pub body_ref: BlockRef,
}


impl BhkCollisionObject {
    pub fn parse(stream: &mut NifStream, is_blend: bool) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let body_ref = stream.read_block_ref()?;
        // bhkBlendCollisionObject adds heirGain(f32) + velGain(f32) = 8 bytes.
        if is_blend {
            let _heir_gain = stream.read_f32_le()?;
            let _vel_gain = stream.read_f32_le()?;
            // Pre-Oblivion-mainline content (bsver < 9) ships an
            // additional pair of `Unknown Float 1` + `Unknown Float 2`
            // — see nif.xml line 3428-3429:
            //   <field name="Unknown Float 1" type="float" vercond="#BSVER# #LT# 9" />
            //   <field name="Unknown Float 2" type="float" vercond="#BSVER# #LT# 9" />
            // Pre-#549 the parser ignored this gate; on the boxtest
            // skeleton.nif (v10.2.0.0 / bsver=6) the missing 8 bytes
            // drifted the stream and pushed every downstream block
            // (4 bhkRigidBody, the host NiNode, etc.) into NiUnknown
            // — surfaced by the M33 audit as NIF-04 ("6 Oblivion
            // bhkRigidBody fail"), but the bug is here.
            if stream.bsver() < 9 {
                let _unknown_float_1 = stream.read_f32_le()?;
                let _unknown_float_2 = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            target_ref,
            flags,
            body_ref,
        })
    }
}

/// bhkNPCollisionObject — Fallout 4 / Fallout 76 collision object.
///
/// FO4 replaced the classic bhkRigidBody chain with a new physics
/// subsystem ("NP" physics); NP files reference a `bhkSystem` subclass
/// (physics system or ragdoll system) instead of an individual rigid
/// body, and the actual collision binary lives inside that system's
/// `ByteArray`. Per nif.xml (`#FO4# #F76#`):
///
/// ```text
/// NiCollisionObject : { target_ref: Ptr<NiAVObject> }
/// bhkNPCollisionObject extends NiCollisionObject {
///     flags:   u16  (bhkCOFlags — same encoding as classic bhk)
///     data:    Ref<bhkSystem>
///     body_id: u32
/// }
/// ```
///
/// Until the FO4 NP data is fully consumed by the physics bridge we
/// record the raw references so downstream systems can resolve the
/// physics system on demand. Replaces the earlier skip-only dispatch
/// that dropped all FO4 collision. See #124 / audit NIF-513.
#[derive(Debug)]
pub struct BhkNPCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    pub data_ref: BlockRef,
    pub body_id: u32,
}


impl BhkNPCollisionObject {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let body_id = stream.read_u32_le()?;
        Ok(Self {
            target_ref,
            flags,
            data_ref,
            body_id,
        })
    }
}

/// bhkPhysicsSystem / bhkRagdollSystem — FO4 / FO76 NP-physics data.
///
/// Both concrete subclasses of the abstract `bhkSystem` hold a single
/// `ByteArray` (`uint data_size; byte data[data_size]`). The byte blob
/// contains the Havok-serialised physics tree (HKX-like). We store the
/// raw bytes so the physics bridge can hand them off to a Havok parser
/// later without re-parsing the outer NIF; reading them instead of
/// skipping keeps the block-sizes path intact across `block_size = 0`
/// or missing-size corner cases.
#[derive(Debug)]
pub struct BhkSystemBinary {
    pub type_name: &'static str,
    pub data: Vec<u8>,
}

impl NiObject for BhkSystemBinary {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkSystemBinary {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let data_size = stream.read_u32_le()? as usize;
        let data = stream.read_bytes(data_size)?;
        Ok(Self { type_name, data })
    }
}

/// `bhkPCollisionObject` — Havok collision object wrapping a phantom
/// (typically `bhkAabbPhantom`) rather than a rigid body. Concrete
/// subclass of `bhkNiCollisionObject`. nif.xml line 3432.
///
/// Wire layout is byte-identical to the standard `bhkCollisionObject`
/// (target_ref + flags u16 + body Ref, 10 B total); the only runtime
/// difference is that `body` references a `bhkPhantom` subclass
/// instead of a `bhkEntity`. We expose it as its own struct so
/// consumers can pattern-match on "this is a phantom, not a body."
#[derive(Debug)]
pub struct BhkPCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    /// Reference to the `bhkPhantom` subclass (e.g. `bhkAabbPhantom`,
    /// `bhkSimpleShapePhantom`) that supplies the collision volume.
    pub body_ref: BlockRef,
}


impl BhkPCollisionObject {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let body_ref = stream.read_block_ref()?;
        Ok(Self {
            target_ref,
            flags,
            body_ref,
        })
    }
}

impl_ni_object!(
    NiCollisionObjectBase => "NiCollisionObject",
    BhkCollisionObject => "bhkCollisionObject",
    BhkNPCollisionObject => "bhkNPCollisionObject",
    BhkPCollisionObject => "bhkPCollisionObject",
);
