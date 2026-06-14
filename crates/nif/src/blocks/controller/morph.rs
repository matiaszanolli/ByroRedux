//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: NiGeomMorpherController, MorphWeight, MorphTarget, NiMorphData.

use super::*;
use crate::impl_ni_object;
use crate::types::NiPoint3;

/// Morph target controller — drives facial animation and mesh deformation.
///
/// References NiMorphData (vertex deltas per morph target) and an array
/// of interpolators that control the blend weights over time.
#[derive(Debug)]
pub struct NiGeomMorpherController {
    pub base: NiTimeControllerBase,
    pub morpher_flags: u16,
    pub data_ref: BlockRef,
    pub always_update: u8,
    pub interpolator_weights: Vec<MorphWeight>,
}

/// An interpolator reference + weight for morph blending.
#[derive(Debug)]
pub struct MorphWeight {
    pub interpolator_ref: BlockRef,
    pub weight: f32,
}

impl NiGeomMorpherController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiInterpController layer (base + Manager Controlled bool, #1506).
        let base = parse_interp_controller_base(stream)?;
        let morpher_flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let always_update = stream.read_u8()?;
        let num_interpolators = stream.read_u32_le()?;

        // nif.xml NiGeomMorpherController:
        //   "Interpolators" (block refs only): since="10.1.0.106" until="20.0.0.5"
        //   "Interpolator Weights" (ref + f32): since="20.1.0.3"
        //
        // Oblivion (v20.0.0.5) hits the refs-only path — the per-element
        // weight float is absent on disk. Reading it consumed a phantom 4 bytes
        // per morph interpolator, misaligning morph-weight refs and corrupting
        // NiMorphData downstream (facial morphs, animated gates). (#1302)
        let mut interpolator_weights = stream.allocate_vec(num_interpolators)?;
        if stream.version() >= NifVersion::V20_1_0_3 {
            // Since 20.1.0.3: MorphWeight = block_ref(4 B) + weight_f32(4 B)
            for _ in 0..num_interpolators {
                let interpolator_ref = stream.read_block_ref()?;
                let weight = stream.read_f32_le()?;
                interpolator_weights.push(MorphWeight {
                    interpolator_ref,
                    weight,
                });
            }
        } else {
            // Until 20.0.0.5: only block refs on disk; weight defaults to 1.0.
            // Oblivion (v20.0.0.5, bsver=11) takes this path.
            for _ in 0..num_interpolators {
                let interpolator_ref = stream.read_block_ref()?;
                interpolator_weights.push(MorphWeight {
                    interpolator_ref,
                    weight: 1.0,
                });
            }
        }

        // Trailing Num Unknown Ints + Unknown Ints array. nif.xml:
        //   <field name="Num Unknown Ints" type="uint"
        //          since="10.2.0.0" until="20.0.0.5" vercond="#BSVER# #GT# 9" />
        //   <field name="Unknown Ints" type="uint" length="Num Unknown Ints"
        //          since="10.2.0.0" until="20.0.0.5" vercond="#BSVER# #GT# 9" />
        //
        // #1509 / NIF-NEW-04 — the gate was `bsver != 0 && bsver <= 11`,
        // transcribed from a STALE nif.xml revision. The current spec
        // gates on `#BSVER# #GT# 9` (bsver ≥ 10), and the corpus confirms
        // it: `meshes\creatures\dog\doghead.nif` is v10.2.0.0 **bsver 9**,
        // where the old gate wrongly read `num_unknown_ints` (here 5) +
        // its 20-byte array — 24 phantom bytes — so the next block
        // (NiMorphData) started 24 B late and read garbage num_morphs,
        // truncating the file (15 blocks dropped). Oblivion's bsver-11
        // morph rigs (e.g. `meshes\oblivion\gate\obgatemini01.nif`,
        // v20.0.0.4) still read the field — `bsver > 9` keeps them, and
        // the #687 fix that added this read, working. The upper version
        // bound is `until=20.0.0.5` (was 20.1.0.3); FNV/FO3 (20.2.0.7)
        // and Skyrim+ skip it on the version gate as before.
        let version = stream.version();
        let bsver = stream.bsver();
        if version >= NifVersion::V10_2_0_0
            && version <= NifVersion::V20_0_0_5
            && bsver > 9
        {
            let num_unknown_ints = stream.read_u32_le()?;
            // Sanity bound: `num_unknown_ints` is a count that has
            // never been observed > a handful in practice. A drifted
            // u32 here would otherwise allocate gigabytes; the
            // `allocate_vec` cap also bounds it but a tighter early
            // return makes the failure mode obvious if upstream drift
            // ever puts garbage here.
            if num_unknown_ints > 65_536 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "NiGeomMorpherController: implausible \
                         num_unknown_ints={num_unknown_ints} — \
                         upstream drift (Oblivion bsver={bsver})"
                    ),
                ));
            }
            for _ in 0..num_unknown_ints {
                let _ = stream.read_u32_le()?;
            }
        }

        Ok(Self {
            base,
            morpher_flags,
            data_ref,
            always_update,
            interpolator_weights,
        })
    }
}

/// A single morph target: name + vertex deltas.
#[derive(Debug)]
pub struct MorphTarget {
    /// Name of this morph frame (e.g., "Blink", "JawOpen").
    pub name: Option<Arc<str>>,
    /// Vertex position deltas (one per mesh vertex). Stored as
    /// `NiPoint3` rather than `[f32; 3]` so the bulk-read result
    /// from `read_ni_point3_array` is consumed in place — no
    /// throwaway memcpy on the cell-load critical path. Layout is
    /// bitwise identical (`#[repr(C)]` 3×f32, no padding); consumers
    /// access `.x / .y / .z` the same as the rest of the parser. #875.
    pub vectors: Vec<NiPoint3>,
}

/// Morph target data — vertex deltas for facial animation.
#[derive(Debug)]
pub struct NiMorphData {
    pub num_vertices: u32,
    pub relative_targets: u8,
    pub morphs: Vec<MorphTarget>,
}

impl NiMorphData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_morphs = stream.read_u32_le()? as usize;
        let num_vertices = stream.read_u32_le()?;
        let relative_targets = stream.read_u8()?;

        // Sanity cap: a real NIF never has more than a few thousand
        // vertices per morph target (the Oblivion face morph data tops
        // out around 1k verts). If we see something absurd, the block
        // has drifted — bail out rather than allocate several GB. The
        // caller's per-block recovery path will seek past the block.
        if num_morphs > 65_536 || num_vertices > 65_536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NiMorphData: implausible num_morphs={num_morphs} \
                     num_vertices={num_vertices} — block drifted"
                ),
            ));
        }

        // Morph element layout per nif.xml (see <struct name="Morph">):
        //
        //   since 10.1.0.106:          frame_name: string
        //   until 10.1.0.0:            num_keys: u32
        //                              interpolation: KeyType (u32)
        //                              keys: Key<float>[num_keys]
        //   since 10.1.0.104
        //     until 20.1.0.2
        //     && BSVER < 10:           legacy_weight: f32
        //   (always):                  vectors: Vec3[num_vertices]
        //
        // The "until 10.1.0.0" branch is pre-NetImmerse legacy content
        // — NONE of the games Redux targets (Morrowind 4.0.0.0 included)
        // fall into it, because the type was deprecated well before
        // 10.1. The previous implementation read those fields
        // unconditionally, which walked off the end of a valid Oblivion
        // morph and allocated a ~118 GB vector when a garbage num_keys
        // happened to be a huge number.
        //
        // Oblivion (v20.0.0.5, BSVER < 10 — vanilla bsver=11 is
        // correctly excluded here) hits the legacy_weight window;
        // FNV / FO3 (BSVER 34) and everything later do not. Gate
        // matches nif.xml `vercond="#BSVER# #LT# 10"`. See
        // NIF-D1-NEW-02 (audit 2026-05-12).
        let version = stream.version();
        let bsver = stream.bsver();
        let has_keys = version <= NifVersion::V10_1_0_0;
        let has_legacy_weight =
            version >= NifVersion::V10_1_0_104 && version <= NifVersion::V20_1_0_2 && bsver < 10;

        // Already bounded by the 65_536 sanity check above; route
        // through allocate_vec for consistency with #408 sweep.
        let mut morphs = stream.allocate_vec(num_morphs as u32)?;
        for _ in 0..num_morphs {
            // Frame name (string table indexed from 10.1.0.106).
            let name = if version >= NifVersion::V10_1_0_106 {
                stream.read_string()?
            } else {
                None
            };

            if has_keys {
                let num_keys = stream.read_u32_le()? as u64;
                let interpolation = stream.read_u32_le()?;
                let key_size: u64 = match interpolation {
                    1 | 5 => 8, // LINEAR / CONSTANT: time(f32) + value(f32)
                    2 => 16,    // QUADRATIC: time + value + fwd + bwd
                    3 => 20,    // TBC: time + value + tension + bias + continuity
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "NiMorphData: unknown float key interpolation {other} \
                                 with {num_keys} keys — stream position unreliable"
                            ),
                        ));
                    }
                };
                stream.skip(key_size * num_keys)?;
            }

            if has_legacy_weight {
                let _legacy_weight = stream.read_f32_le()?;
            }

            // Vertex deltas — `read_ni_point3_array` validates
            // `num_vertices * 12 <= remaining` via its internal
            // `check_alloc`, which also enforces the hard cap (#408 / #831).
            // Result is moved into the field directly (no axis swap or
            // collect — `MorphTarget.vectors` is `Vec<NiPoint3>`). #875.
            let vectors = stream.read_ni_point3_array(num_vertices as usize)?;

            morphs.push(MorphTarget { name, vectors });
        }

        Ok(Self {
            num_vertices,
            relative_targets,
            morphs,
        })
    }
}

impl_ni_object!(NiGeomMorpherController, NiMorphData,);
