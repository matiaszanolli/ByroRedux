//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: NiGeomMorpherController, MorphWeight, MorphTarget, NiMorphData.

use super::*;

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

impl NiObject for NiGeomMorpherController {
    fn block_type_name(&self) -> &'static str {
        "NiGeomMorpherController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiGeomMorpherController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let morpher_flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let always_update = stream.read_u8()?;
        let num_interpolators = stream.read_u32_le()?;

        let mut interpolator_weights = stream.allocate_vec(num_interpolators)?;
        for _ in 0..num_interpolators {
            let interpolator_ref = stream.read_block_ref()?;
            let weight = stream.read_f32_le()?;
            interpolator_weights.push(MorphWeight {
                interpolator_ref,
                weight,
            });
        }

        // Trailing Num Unknown Ints + Unknown Ints array. nif.xml:
        //   <field name="Num Unknown Ints" type="uint"
        //          since="10.2.0.0" until="20.1.0.3"
        //          vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
        //   <field name="Unknown Ints" type="uint"
        //          length="Num Unknown Ints" since="10.2.0.0" until="20.1.0.3"
        //          vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
        // Targets Bethesda content with bsver in 1..=11 — Oblivion
        // (bsver 11) hits this; FNV/FO3 (bsver 24+) and Skyrim+ skip
        // it entirely. Pre-fix the 4-byte u32 (typically 0) was left
        // unread, which misaligned the next block. On `meshes/oblivion/
        // gate/obgatemini01.nif` the trailing bytes were `0x00000000`,
        // so the next block (NiMorphData) read num_morphs from the
        // wrong slot, parsed as a 9-byte stub, and downstream
        // interpolator blocks tripped the alloc cap with billions of
        // ghost morph keys (audit O5-2 / #687).
        let version = stream.version();
        let bsver = stream.bsver();
        if version >= NifVersion(0x0A020000)
            && version <= NifVersion(0x14010003)
            && bsver != 0
            && bsver <= 11
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
    /// Vertex position deltas (one per mesh vertex).
    pub vectors: Vec<[f32; 3]>,
}

/// Morph target data — vertex deltas for facial animation.
#[derive(Debug)]
pub struct NiMorphData {
    pub num_vertices: u32,
    pub relative_targets: u8,
    pub morphs: Vec<MorphTarget>,
}

impl NiObject for NiMorphData {
    fn block_type_name(&self) -> &'static str {
        "NiMorphData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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
        // Oblivion (v20.0.0.5, BSVER in 0..=11) hits the legacy_weight
        // window. FNV / FO3 (BSVER 34) and everything later do not.
        let version = stream.version();
        let bsver = stream.bsver();
        let has_keys = version <= NifVersion(0x0A010000);
        let has_legacy_weight =
            version >= NifVersion(0x0A010068) && version <= NifVersion(0x14010002) && bsver < 10;

        // Already bounded by the 65_536 sanity check above; route
        // through allocate_vec for consistency with #408 sweep.
        let mut morphs = stream.allocate_vec(num_morphs as u32)?;
        for _ in 0..num_morphs {
            // Frame name (string table indexed from 10.1.0.106).
            let name = if version >= NifVersion(0x0A01006A) {
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

            // Vertex deltas — guarded against an absurd num_vertices
            // that would otherwise OOM the process on a corrupt block.
            // The hard cap stays as defensive belt; allocate_vec also
            // bounds against remaining stream bytes (#408).
            stream.allocate_vec::<[f32; 3]>((num_vertices as u32).min(1_000_000))?;
            let points = stream.read_ni_point3_array(num_vertices as usize)?;
            let vectors: Vec<[f32; 3]> = points.into_iter().map(|p| [p.x, p.y, p.z]).collect();

            morphs.push(MorphTarget { name, vectors });
        }

        Ok(Self {
            num_vertices,
            relative_targets,
            morphs,
        })
    }
}
