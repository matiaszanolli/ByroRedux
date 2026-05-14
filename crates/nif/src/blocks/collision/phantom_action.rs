//! Phantoms + Havok actions.
//!
//! Simple + AABB phantoms, LiquidAction, OrientHingedBodyAction. Tail-type
//! round-trip parsers per #557 / NIF-12.

use super::super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

use super::read_vec4;

/// bhkSimpleShapePhantom — non-physical trigger volume with shape + transform.
#[derive(Debug)]
pub struct BhkSimpleShapePhantom {
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    pub transform: [[f32; 4]; 4],
}

impl NiObject for BhkSimpleShapePhantom {
    fn block_type_name(&self) -> &'static str {
        "bhkSimpleShapePhantom"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkSimpleShapePhantom {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject: shape ref + filter + world CInfo (20 bytes)
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?; // bhkWorldObjectCInfo

        // bhkSimpleShapePhantom adds 8 unused bytes (nif.xml `Unused 01`,
        // `byte length="8" binary="true"`) before the transform. Pre-#474
        // this 8-byte slot was skipped along with the trailing transform
        // via block_sizes recovery, costing 8 bytes per phantom block.
        stream.skip(8)?;

        // bhkSimpleShapePhantom: 4x4 transform (Matrix44).
        let mut transform = [[0.0f32; 4]; 4];
        for row in &mut transform {
            for val in row.iter_mut() {
                *val = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            shape_ref,
            havok_filter,
            transform,
        })
    }
}

/// `bhkAabbPhantom` — non-physical broad-phase-only phantom (trigger
/// volumes, region queries). nif.xml line 2778.
///
/// Inheritance chain: bhkAabbPhantom → bhkPhantom → bhkWorldObject.
/// Layout on disk (Bethesda, 20.0.0.5 and later, no `Unknown Int`
/// since the `until 10.0.1.2` gate excludes every Bethesda game):
/// ```text
///   bhkWorldObject     : shape_ref(4) + havok_filter(4) + CInfo(20) = 28 B
///   (bhkPhantom adds nothing)
///   bhkAabbPhantom     : Unused 01[8] + hkAabb (2 × Vec4 = 32) = 40 B
///   --------------------------------------------------------
///   Total                                                  = 68 B
/// ```
#[derive(Debug)]
pub struct BhkAabbPhantom {
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    /// World-space AABB min corner (x, y, z, w) — w unused per hkAabb.
    pub aabb_min: [f32; 4],
    /// World-space AABB max corner (x, y, z, w) — w unused per hkAabb.
    pub aabb_max: [f32; 4],
}

impl NiObject for BhkAabbPhantom {
    fn block_type_name(&self) -> &'static str {
        "bhkAabbPhantom"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkAabbPhantom {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject prefix (28 B): shape + filter + bhkWorldObjectCInfo.
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?; // bhkWorldObjectCInfo
                          // bhkAabbPhantom: 8 unused + 2 × Vec4 hkAabb.
        stream.skip(8)?;
        let aabb_min = read_vec4(stream)?;
        let aabb_max = read_vec4(stream)?;
        Ok(Self {
            shape_ref,
            havok_filter,
            aabb_min,
            aabb_max,
        })
    }
}

/// `bhkLiquidAction` — FO3+ custom `bhkUnaryAction`-flavoured Havok
/// action that applies surface-tension forces to a body of liquid.
/// nif.xml line 6893.
///
/// The base class is `bhkAction` (abstract, no fields), so the on-disk
/// body starts immediately. Layout:
/// ```text
///   Unused 01[12] + Initial Stick Force(f32) + Stick Strength(f32)
///   + Neighbor Distance(f32) + Neighbor Strength(f32) = 28 B
/// ```
/// The `Unused 01` slot explicitly differs from a `bhkUnaryAction` —
/// per nif.xml, `bhkLiquidAction` does NOT carry the Entity Ptr even
/// though the class heritage looks like it should.
#[derive(Debug)]
pub struct BhkLiquidAction {
    pub initial_stick_force: f32,
    pub stick_strength: f32,
    pub neighbor_distance: f32,
    pub neighbor_strength: f32,
}

impl NiObject for BhkLiquidAction {
    fn block_type_name(&self) -> &'static str {
        "bhkLiquidAction"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkLiquidAction {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        stream.skip(12)?; // Unused 01
        let initial_stick_force = stream.read_f32_le()?;
        let stick_strength = stream.read_f32_le()?;
        let neighbor_distance = stream.read_f32_le()?;
        let neighbor_strength = stream.read_f32_le()?;
        Ok(Self {
            initial_stick_force,
            stick_strength,
            neighbor_distance,
            neighbor_strength,
        })
    }
}

/// `bhkOrientHingedBodyAction` — `bhkUnaryAction` that re-orients a
/// rigid body to keep its `Forward LS` axis pointing at a target,
/// pivoting around `Hinge Axis LS`. Used for articulation-driven
/// pieces like pendulums, doors that want to settle open, and some
/// of the Skyrim+ ragdoll "aim" bones. nif.xml line 7035.
///
/// Layout:
/// ```text
///   bhkUnaryAction: Entity(Ptr=4) + Unused 01[8] = 12 B
///   bhkOrientHingedBodyAction:
///     Unused 02[8] + Hinge Axis LS (Vec4) + Forward LS (Vec4)
///     + Strength(f32) + Damping(f32) + Unused 03[8]
///     = 8 + 16 + 16 + 4 + 4 + 8 = 56 B
///   Total = 68 B
/// ```
#[derive(Debug)]
pub struct BhkOrientHingedBodyAction {
    /// Rigid body this action is attached to.
    pub entity_ref: BlockRef,
    /// Local-space axis the body pivots around.
    pub hinge_axis_ls: [f32; 4],
    /// Local-space axis the body tries to keep aimed at the target.
    pub forward_ls: [f32; 4],
    /// Torque multiplier. Larger values re-orient faster.
    pub strength: f32,
    /// Angular damping on the reorientation torque.
    pub damping: f32,
}

impl NiObject for BhkOrientHingedBodyAction {
    fn block_type_name(&self) -> &'static str {
        "bhkOrientHingedBodyAction"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkOrientHingedBodyAction {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let entity_ref = stream.read_block_ref()?;
        stream.skip(8)?; // Unused 01
        stream.skip(8)?; // Unused 02
        let hinge_axis_ls = read_vec4(stream)?;
        let forward_ls = read_vec4(stream)?;
        let strength = stream.read_f32_le()?;
        let damping = stream.read_f32_le()?;
        stream.skip(8)?; // Unused 03
        Ok(Self {
            entity_ref,
            hinge_axis_ls,
            forward_ls,
            strength,
            damping,
        })
    }
}
