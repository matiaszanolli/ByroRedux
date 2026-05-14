//! Havok constraint variants.
//!
//! BhkConstraint stubs + BhkBreakableConstraint with its inner-wrapped
//! constraint payload (#117 / #557).

use super::super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;


/// Opaque stub for a Havok constraint block.
///
/// Holds just the shared `bhkConstraintCInfo` base (two entity refs
/// + priority); everything else is skipped. The concrete constraint
/// type is preserved in `type_name` so downstream consumers and
/// telemetry can identify it. See #117.
#[derive(Debug)]
pub struct BhkConstraint {
    /// RTTI class name — one of the seven constraint types.
    pub type_name: &'static str,
    pub entity_a: BlockRef,
    pub entity_b: BlockRef,
    pub priority: u32,
}

impl NiObject for BhkConstraint {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
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

    /// Parse a constraint block by type name. On Oblivion, reads the
    /// exact byte layout and returns a `BhkConstraint`. On FO3+, reads
    /// the 16-byte base and returns early; the caller seeks past the
    /// remainder via `block_size`.
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let (entity_a, entity_b, priority) = Self::parse_base(stream)?;

        // Oblivion byte-exact payload sizes (post-base bytes). Derived
        // from nif.xml with `#NI_BS_LTE_16#` active. A zero means
        // "drop through to the FO3+ short-stub path".
        let is_oblivion = stream.version() <= NifVersion::V20_0_0_5;
        if is_oblivion {
            let payload_size: Option<u64> = match type_name {
                // 2 × Vec4
                "bhkBallAndSocketConstraint" => Some(32),
                // 5 × Vec4
                "bhkHingeConstraint" => Some(80),
                // 6 × Vec4 + 6 × f32
                "bhkRagdollConstraint" => Some(120),
                // 7 × Vec4 + 3 × f32
                "bhkLimitedHingeConstraint" => Some(124),
                // 8 × Vec4 + 3 × f32
                "bhkPrismaticConstraint" => Some(140),
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
                });
            }

            if type_name == "bhkMalleableConstraint" {
                // Oblivion layout: type u32 + nested bhkConstraintCInfo
                // (16) + wrapped CInfo + tau f32 + damping f32.
                let wrapped_type = stream.read_u32_le()?;
                let _nested_entities = stream.read_u32_le()?;
                let _nested_a = stream.read_block_ref()?;
                let _nested_b = stream.read_block_ref()?;
                let _nested_priority = stream.read_u32_le()?;
                let inner_size: u64 = match wrapped_type {
                    0 => 32,  // Ball and Socket
                    1 => 80,  // Hinge
                    2 => 124, // Limited Hinge
                    6 => 140, // Prismatic
                    7 => 120, // Ragdoll
                    8 => 36,  // Stiff Spring
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "bhkMalleableConstraint: unknown inner type {other} — \
                                 stream position unreliable"
                            ),
                        ));
                    }
                };
                stream.skip(inner_size)?;
                // Tau + Damping (Oblivion trailer).
                stream.skip(8)?;
                return Ok(Self {
                    type_name,
                    entity_a,
                    entity_b,
                    priority,
                });
            }
        }

        // FO3+ (or unknown pre-Oblivion content): return after the
        // 16-byte base. The outer parse_nif loop seeks past the rest
        // using the header's block_sizes table, which is always present
        // on v >= 20.2.0.7. The stub still preserves the RTTI name
        // for telemetry.
        Ok(Self {
            type_name,
            entity_a,
            entity_b,
            priority,
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
}

impl NiObject for BhkBreakableConstraint {
    fn block_type_name(&self) -> &'static str {
        "bhkBreakableConstraint"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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
        let is_oblivion = stream.version() <= NifVersion::V20_0_0_5;

        // #633: lift the Oblivion-only gate. When the wrapped CInfo size
        // is derivable for the parser's version (Hinge / BallAndSocket /
        // StiffSpring on either; LimitedHinge / Prismatic / Ragdoll on
        // Oblivion), read the trailer fields directly. Pre-#633 every
        // FNV/FO3 instance returned `threshold = 0.0,
        // remove_when_broken = false` even when the bytes were on disk.
        let trailer = if let Some(size) = Self::wrapped_payload_size(wrapped_type, is_oblivion) {
            stream.skip(size)?;
            Some(())
        } else if !is_oblivion {
            // FNV motor-bearing types (LimitedHinge / Prismatic /
            // Ragdoll): consume the fixed prefix + motor inline so the
            // trailer is reachable. Pre-#633 these all hit the short
            // stub and the motor + trailer bytes were skipped via
            // `block_size` recovery.
            if let Some(prefix) = Self::fnv_motor_prefix_size(wrapped_type) {
                stream.skip(prefix)?;
                Self::consume_motor(stream)?;
                Some(())
            } else {
                None
            }
        } else {
            None
        };

        if trailer.is_some() {
            let threshold = stream.read_f32_le()?;
            let remove_when_broken = stream.read_u8()? != 0;
            return Ok(Self {
                entity_a,
                entity_b,
                priority,
                wrapped_type,
                threshold,
                remove_when_broken,
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
        })
    }
}
