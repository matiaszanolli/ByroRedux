use crate::packfile::Packfile;
use crate::{HkxError, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HkxTransform {
    pub translation: [f32; 3],
    /// Havok-native quaternion order `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl HkxTransform {
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct HkxBone {
    pub name: String,
    pub parent_index: i16,
    pub reference_pose: HkxTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HkxSkeleton {
    pub name: String,
    pub bones: Vec<HkxBone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HkxAnimation {
    pub duration: f32,
    pub num_frames: u32,
    pub frame_duration: f32,
    /// Transform-track-major local-space samples. Every track contains
    /// `num_frames` poses sampled at the clip's authored frame rate.
    pub tracks: Vec<Vec<HkxTransform>>,
    /// Transform-track index → skeleton bone index. Empty bindings in the
    /// packfile are expanded to the Havok identity mapping here.
    pub track_to_bone: Vec<u16>,
    /// Authored Havok annotation events across every annotation track.
    pub annotations: Vec<HkxAnnotation>,
    /// Skeleton bone indices whose transform track was dropped at decode
    /// because its translation or scale curve stores a non-finite scalar
    /// (#5662). Those tracks are absent from `tracks` / `track_to_bone`, so
    /// the bone keeps its bind pose; a consumer reports the list with the
    /// clip path, which this decoder never sees.
    pub dropped_bones: Vec<u16>,
    /// The clip's extracted motion (`hkaDefaultAnimatedReferenceFrame`), when it has
    /// one: Fallout 4 authors its locomotion clips in place and keeps the travel here,
    /// where Skyrim keeps it in the root bone's own track.
    pub reference_frame: Option<ReferenceFrame>,
}

/// The motion an animation extracts from its pose: the root's path through the world over
/// the clip, sampled evenly from time 0 to `duration`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceFrame {
    pub duration: f32,
    /// `(x, y, z, yaw)` per sample, in the clip's own space (Z up): the root's offset
    /// from where the clip began, and its turn about the up axis in radians.
    pub samples: Vec<[f32; 4]>,
}

/// Bound the decoded transform sample matrix before allocating one `Vec` per
/// track. A hostile packfile can otherwise provide a tiny set of blocks but a
/// `num_frames` value large enough to make `Vec::with_capacity` abort the
/// process (#3011).
///
/// #5316 (PAR-D1-2026-10-05-02) — 16 M was 128× the vanilla maximum: the
/// SE Animations.bsa census (`skyrim_se_spline_dimensions_census_stays_
/// under_the_gate_ceilings`) measures the largest real clip at 124,821
/// samples (`paired_dlc1seranaholdsvyrthur.hkx`, 201 tracks × 621 frames),
/// and #5006's mask-table cost already ties output to file bytes at ~64
/// samples per file byte — so the only work this backstop did was admit
/// the audit probe's 279 KB file at 15.7 M samples / 597 MiB. Now set
/// from the census at 8× the measured maximum.
const MAX_TRANSFORM_SAMPLES: usize = 1_000_000;

// ── #5847 — the hostile-input plausibility ceilings that used to be five
// anonymous `4096`s. None is a format limit; each is a backstop far above
// the vanilla census maximum (SE Animations.bsa, #5316: 201 tracks, 101
// skeleton bones, the largest spline ~4k control points). Named so the
// census tests can cite them and a future Starfield/FO76 rig that trips
// one gets an explained error, and so the skeleton ceiling is visibly
// looser than behavior.rs's MAX_BONES = 1024 (a skeleton this reader
// accepts may still be unusable by the graph reader — both gates stand).
/// Skeleton bone count (`hkaSkeleton` bones array).
const MAX_SKELETON_BONES: usize = 4096;
/// Tracks in one `hkaInterleavedUncompressedAnimation` transform array.
const MAX_TRANSFORM_TRACKS: usize = 4096;
/// Float channels in the same block.
const MAX_FLOAT_TRACKS: usize = 4096;
/// `hkaSplineCompressedAnimation` blocks.
const MAX_SPLINE_BLOCKS: usize = 4096;
/// Control points in one spline track.
const MAX_SPLINE_CONTROL_POINTS: usize = 4096;

/// #5006 (PAR-D1-2026-09-29-01) — ceiling on a clip's declared
/// `max_frames_per_block`. 256 is the Havok compressor's default and the
/// measured vanilla maximum (SE Animations.bsa census,
/// `skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`).
/// #4655's `num_frames <= num_blocks * (max_frames_per_block - 1) + 1`
/// tie is sound only if a block's frame count is itself plausible: with
/// all-static masks a block costs `transform_count * 4` mask bytes
/// WHATEVER its frame count, so a hand-built clip with
/// `max_frames_per_block ≈ num_frames` (one block, everything static)
/// slipped under every #4655 check and still expanded a 17 KB file into
/// 16 M samples / 610 MiB. Capping the compressor parameter at its
/// vanilla ceiling closes that: `num_blocks <= 4096` then bounds
/// `num_frames` at ~1.04 M, and each block must still carry its full
/// mask table, so decoded output stays proportional to file bytes.
const MAX_FRAMES_PER_BLOCK: usize = 256;

/// An `hkArray` field: the data pointer, then the `u32` element count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArrayField {
    pointer: usize,
    count: usize,
}

/// Walks a class's fields in declaration order, applying the packfile's
/// pointer width and C++ alignment, so one field list yields the offsets of
/// both the 32-bit (2011 Skyrim) and the 64-bit (Special Edition) layout.
struct Fields {
    pointer: usize,
    offset: usize,
    align: usize,
}

impl Fields {
    fn plain(pointer: usize) -> Self {
        Self {
            pointer,
            offset: 0,
            align: 1,
        }
    }

    /// Start of a class deriving from `hkReferencedObject`: vtable pointer,
    /// `hkUint16 m_memSizeAndFlags`, `hkInt16 m_referenceCount`, padded to
    /// the base class's pointer alignment.
    fn referenced_object(pointer: usize) -> Self {
        let mut fields = Self::plain(pointer);
        fields.pointer();
        fields.take(4, 4);
        fields.end_base_class();
        fields
    }

    fn take(&mut self, size: usize, align: usize) -> usize {
        self.align = self.align.max(align);
        let at = pad(self.offset, align);
        self.offset = at + size;
        at
    }

    /// `int`, `hkReal` or an `hkInt32`-backed enum.
    fn int(&mut self) -> usize {
        self.take(4, 4)
    }

    fn byte(&mut self) -> usize {
        self.take(1, 1)
    }

    /// A raw pointer, `hkRefPtr` or `hkStringPtr`.
    fn pointer(&mut self) -> usize {
        self.take(self.pointer, self.pointer)
    }

    /// `hkArray<T>`: pointer, `int m_size`, `int m_capacityAndFlags`.
    fn array(&mut self) -> ArrayField {
        let pointer = self.take(self.pointer + 8, self.pointer);
        ArrayField {
            pointer,
            count: pointer + self.pointer,
        }
    }

    fn end_base_class(&mut self) {
        self.offset = pad(self.offset, self.align);
    }

    fn size(&self) -> usize {
        pad(self.offset, self.align)
    }
}

const fn pad(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// Field offsets of every Havok class this crate reads, for one pointer
/// width. Field order is Havok's own declaration order (`hkaSkeleton.h`,
/// `hkaBone.h`, `hkaAnimation.h`, `hkaSplineCompressedAnimation.h`,
/// `hkaAnimationBinding.h`, `hkaAnnotationTrack.h`); fields this crate never
/// reads are still walked so the ones after them land correctly.
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    skeleton_name: usize,
    skeleton_parents: ArrayField,
    skeleton_bones: ArrayField,
    skeleton_pose: ArrayField,
    bone_stride: usize,
    animation_type: usize,
    animation_duration: usize,
    transform_tracks: usize,
    float_tracks: usize,
    annotation_tracks: ArrayField,
    num_frames: usize,
    num_blocks: usize,
    max_frames_per_block: usize,
    mask_size: usize,
    frame_duration: usize,
    block_offsets: ArrayField,
    float_block_offsets: ArrayField,
    spline_data: ArrayField,
    binding_track_to_bone: ArrayField,
    track_name: usize,
    track_annotations: ArrayField,
    track_stride: usize,
    annotation_time: usize,
    annotation_text: usize,
    annotation_stride: usize,
}

impl Layout {
    fn new(pointer: usize) -> Self {
        let mut skeleton = Fields::referenced_object(pointer);
        let skeleton_name = skeleton.pointer();
        let skeleton_parents = skeleton.array();
        let skeleton_bones = skeleton.array();
        let skeleton_pose = skeleton.array();

        let mut bone = Fields::plain(pointer);
        bone.pointer(); // m_name
        bone.byte(); // m_lockTranslation

        let mut clip = Fields::referenced_object(pointer);
        let animation_type = clip.int();
        let animation_duration = clip.int();
        let transform_tracks = clip.int();
        let float_tracks = clip.int();
        clip.pointer(); // m_extractedMotion
        let annotation_tracks = clip.array();
        clip.end_base_class(); // hkaAnimation → hkaSplineCompressedAnimation
        let num_frames = clip.int();
        let num_blocks = clip.int();
        let max_frames_per_block = clip.int();
        let mask_size = clip.int();
        clip.int(); // m_blockDuration
        clip.int(); // m_blockInverseDuration
        let frame_duration = clip.int();
        let block_offsets = clip.array();
        let float_block_offsets = clip.array();
        clip.array(); // m_transformOffsets
        clip.array(); // m_floatOffsets
        let spline_data = clip.array();

        let mut binding = Fields::referenced_object(pointer);
        binding.pointer(); // m_originalSkeletonName
        binding.pointer(); // m_animation
        let binding_track_to_bone = binding.array();

        let mut track = Fields::plain(pointer);
        let track_name = track.pointer();
        let track_annotations = track.array();

        let mut annotation = Fields::plain(pointer);
        let annotation_time = annotation.int();
        let annotation_text = annotation.pointer();

        Self {
            skeleton_name,
            skeleton_parents,
            skeleton_bones,
            skeleton_pose,
            bone_stride: bone.size(),
            animation_type,
            animation_duration,
            transform_tracks,
            float_tracks,
            annotation_tracks,
            num_frames,
            num_blocks,
            max_frames_per_block,
            mask_size,
            frame_duration,
            block_offsets,
            float_block_offsets,
            spline_data,
            binding_track_to_bone,
            track_name,
            track_annotations,
            track_stride: track.size(),
            annotation_time,
            annotation_text,
            annotation_stride: annotation.size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HkxAnnotation {
    pub track_name: String,
    pub time: f32,
    pub text: String,
}

/// Decode the first `hkaSkeleton` in a Skyrim-era packfile (32- or 64-bit).
pub fn decode_skeleton(bytes: &[u8]) -> Result<HkxSkeleton> {
    let pack = Packfile::parse(bytes)?;
    let layout = Layout::new(pack.pointer_size());
    let object = pack.object("hkaSkeleton")?;
    decode_skeleton_at(&pack, &layout, object)
}

/// Decode **every** `hkaSkeleton` in a packfile, in file order. A Skyrim
/// character's `skeleton.hkx` carries two: the animation rig and, second, the
/// ragdoll's own skeleton (#5015) — the bone list the `XRGD` pose entries
/// index.
pub fn decode_skeletons(bytes: &[u8]) -> Result<Vec<HkxSkeleton>> {
    let pack = Packfile::parse(bytes)?;
    let layout = Layout::new(pack.pointer_size());
    pack.objects_of("hkaSkeleton")
        .into_iter()
        .map(|object| decode_skeleton_at(&pack, &layout, object))
        .collect()
}

fn decode_skeleton_at(pack: &Packfile<'_>, layout: &Layout, object: usize) -> Result<HkxSkeleton> {
    let name = pack
        .local_target(object + layout.skeleton_name)
        .map(|offset| pack.cstr(offset, "skeleton name"))
        .transpose()?
        .unwrap_or_default()
        .to_owned();

    let bone_count =
        pack.u32(object + layout.skeleton_bones.count, "skeleton bone count")? as usize;
    let parent_count = pack.u32(
        object + layout.skeleton_parents.count,
        "skeleton parent count",
    )? as usize;
    let pose_count = pack.u32(object + layout.skeleton_pose.count, "skeleton pose count")? as usize;
    if bone_count == 0
        || bone_count > MAX_SKELETON_BONES
        || parent_count != bone_count
        || pose_count != bone_count
    {
        return Err(HkxError::InvalidData(
            "skeleton bone/parent/reference-pose counts disagree",
        ));
    }
    let parents_offset = pack
        .local_target(object + layout.skeleton_parents.pointer)
        .ok_or(HkxError::InvalidData("skeleton parent array is unbound"))?;
    let bones_offset = pack
        .local_target(object + layout.skeleton_bones.pointer)
        .ok_or(HkxError::InvalidData("skeleton bone array is unbound"))?;
    let poses_offset = pack
        .local_target(object + layout.skeleton_pose.pointer)
        .ok_or(HkxError::InvalidData("skeleton pose array is unbound"))?;
    let parents = pack.data_slice(parents_offset, bone_count * 2, "skeleton parents")?;
    let poses = pack.data_slice(poses_offset, bone_count * 48, "skeleton poses")?;

    let mut bones = Vec::with_capacity(bone_count);
    for index in 0..bone_count {
        let name_offset = pack
            .local_target(bones_offset + index * layout.bone_stride)
            .ok_or(HkxError::InvalidData("skeleton bone name is unbound"))?;
        let name = pack.cstr(name_offset, "skeleton bone name")?.to_owned();
        let parent_index =
            i16::from_le_bytes(parents[index * 2..index * 2 + 2].try_into().unwrap());
        if parent_index >= index as i16 {
            return Err(HkxError::InvalidData("skeleton parent is not before child"));
        }
        let pose = &poses[index * 48..(index + 1) * 48];
        bones.push(HkxBone {
            name,
            parent_index,
            reference_pose: read_qs_transform(pose)?,
        });
    }
    Ok(HkxSkeleton { name, bones })
}

/// Decode an `hkaSplineCompressedAnimation` from a Skyrim-era packfile.
///
/// Havok stores each block as per-track static values or quantized B-spline
/// control points. The decoder expands those blocks to authored-frame samples;
/// the engine's animation sampler then interpolates between those keys.
pub fn decode_spline_animation(bytes: &[u8]) -> Result<HkxAnimation> {
    let pack = Packfile::parse(bytes)?;
    let layout = Layout::new(pack.pointer_size());
    let object = pack.object("hkaSplineCompressedAnimation")?;
    let animation_type = pack.u32(object + layout.animation_type, "animation type")?;
    // `hkaAnimation::AnimationType::HK_SPLINE_COMPRESSED_ANIMATION`: 5 in Havok 2010
    // (Skyrim), 3 in Havok 2014 (Fallout 4), which dropped the delta and wavelet
    // types from the enum.
    let spline_type = if pack.version() == 11 { 3 } else { 5 };
    if animation_type != spline_type {
        return Err(HkxError::InvalidData("animation is not spline-compressed"));
    }
    let duration = pack.f32(object + layout.animation_duration, "animation duration")?;
    let transform_count =
        pack.u32(object + layout.transform_tracks, "transform-track count")? as usize;
    let float_count = pack.u32(object + layout.float_tracks, "float-track count")? as usize;
    let num_frames = pack.u32(object + layout.num_frames, "animation frame count")?;
    let num_blocks = pack.u32(object + layout.num_blocks, "animation block count")? as usize;
    let max_frames_per_block = pack.u32(
        object + layout.max_frames_per_block,
        "maximum frames per block",
    )?;
    let mask_size = pack.u32(object + layout.mask_size, "mask table size")? as usize;
    let frame_duration = pack.f32(object + layout.frame_duration, "animation frame duration")?;
    let frame_count = usize::try_from(num_frames)
        .map_err(|_| HkxError::InvalidData("animation frame count overflows usize"))?;
    let sample_count = transform_count
        .checked_mul(frame_count)
        .ok_or(HkxError::InvalidData(
            "animation sample count overflows usize",
        ))?;
    if !duration.is_finite()
        || duration < 0.0
        || !frame_duration.is_finite()
        || frame_duration <= 0.0
        || transform_count == 0
        || transform_count > MAX_TRANSFORM_TRACKS
        || float_count > MAX_FLOAT_TRACKS
        || num_frames == 0
        || sample_count > MAX_TRANSFORM_SAMPLES
        || num_blocks == 0
        || num_blocks > MAX_SPLINE_BLOCKS
        || !(2..=MAX_FRAMES_PER_BLOCK as u32).contains(&max_frames_per_block)
        // #4655 (PAR-D1-2026-09-21-02) — frames must fit in the blocks the
        // file ACTUALLY carries: each block's frames cost real bytes on
        // disk, so `num_frames <= num_blocks * (max_frames_per_block - 1)
        // + 1` ties decoded output size to file size. The absolute
        // MAX_TRANSFORM_SAMPLES cap stays as the final backstop; alone it
        // admitted a 17 KB file claiming 4096 tracks x 3906 frames
        // (610 MiB decoded, ~2 GB retained as keys).
        // #5006 (PAR-D1-2026-09-29-01) — that tie is bypassable with ONE
        // block whose `max_frames_per_block ≈ num_frames` (all-static
        // masks cost `transform_count * 4` bytes per block regardless of
        // frames); the MAX_FRAMES_PER_BLOCK cap above closes it.
        || num_frames as u64 > num_blocks as u64 * (max_frames_per_block as u64 - 1) + 1
        // #5659 (PAR-D1-2026-10-12-02) — Havok stores the mask table size
        // padded to 4 bytes: 4 per transform track plus 1 per float track,
        // rounded up. The unpadded equality rejected every vanilla clip
        // whose float-track count is not a multiple of 4 (229 SE / 167 LE,
        // all 117 Falmer clips among them); the block walk below already
        // starts the curve data at `align_up(mask_end, 4)`. Measured, not
        // sourced: every one of those clips stores exactly this value and
        // decodes byte-exactly to its float block. Both counts are ≤ 4096
        // by the clauses above, so the sum cannot overflow.
        || mask_size != pad(transform_count * 4 + float_count, 4)
    {
        return Err(HkxError::InvalidData("unsupported spline clip dimensions"));
    }
    let block_offsets =
        read_pack_u32_array(&pack, object, layout.block_offsets, "block-offset array")?;
    let float_block_offsets = read_pack_u32_array(
        &pack,
        object,
        layout.float_block_offsets,
        "float block-offset array",
    )?;
    if block_offsets.len() != num_blocks || float_block_offsets.len() != num_blocks {
        return Err(HkxError::InvalidData(
            "spline block-offset counts disagree with block count",
        ));
    }
    if block_offsets.first().copied() != Some(0) {
        return Err(HkxError::InvalidData(
            "first spline block does not start at zero",
        ));
    }
    let data_offset = pack
        .local_target(object + layout.spline_data.pointer)
        .ok_or(HkxError::InvalidData("spline data array is unbound"))?;
    let data_len = pack.u32(object + layout.spline_data.count, "spline data size")? as usize;
    let data = pack.data_slice(data_offset, data_len, "spline data")?;
    let mut blocks = Vec::with_capacity(num_blocks);
    // #5662 — tracks whose translation or scale stores a non-finite scalar
    // in any block; dropped from the clip below instead of failing it.
    let mut non_finite_tracks = vec![false; transform_count];
    for block_index in 0..num_blocks {
        let block_start = block_offsets[block_index] as usize;
        let block_end = block_offsets
            .get(block_index + 1)
            .map(|offset| *offset as usize)
            .unwrap_or(data.len());
        let float_start = block_start
            .checked_add(float_block_offsets[block_index] as usize)
            .ok_or(HkxError::InvalidData("float block offset overflow"))?;
        if block_start > float_start || float_start > block_end || block_end > data.len() {
            return Err(HkxError::InvalidData(
                "spline block offsets are out of order",
            ));
        }
        let mask_end = block_start
            .checked_add(mask_size)
            .ok_or(HkxError::InvalidData("spline mask offset overflow"))?;
        if mask_end > float_start {
            return Err(HkxError::Truncated("spline mask table"));
        }
        let masks = &data[block_start..block_start + transform_count * 4];
        let mut cursor = align_up(mask_end, 4)?;
        let mut block_tracks = Vec::with_capacity(transform_count);
        for track in 0..transform_count {
            let quantization = masks[track * 4];
            let translation_mask = masks[track * 4 + 1];
            let rotation_mask = masks[track * 4 + 2];
            let scale_mask = masks[track * 4 + 3];
            let scalar_quantization = quantization & 0x03;
            let rotation_quantization = (quantization >> 2) & 0x0f;
            let scale_quantization = (quantization >> 6) & 0x03;
            if scalar_quantization > 1 || scale_quantization > 1 || rotation_quantization > 5 {
                return Err(HkxError::InvalidData("invalid spline quantization type"));
            }

            let non_finite = &mut non_finite_tracks[track];
            let translation = read_vector_curve(
                data,
                &mut cursor,
                translation_mask,
                scalar_quantization,
                [0.0; 3],
                non_finite,
            )?;
            let rotation =
                read_quaternion_curve(data, &mut cursor, rotation_mask, rotation_quantization)?;
            let scale = read_vector_curve(
                data,
                &mut cursor,
                scale_mask,
                scale_quantization,
                [1.0; 3],
                non_finite,
            )?;
            block_tracks.push(BlockTrack {
                translation,
                rotation,
                scale,
            });
        }
        if cursor > float_start {
            return Err(HkxError::InvalidData(
                "transform spline data overlaps float tracks",
            ));
        }
        blocks.push(block_tracks);
    }
    if non_finite_tracks.iter().all(|bad| *bad) {
        return Err(HkxError::InvalidData(
            "every transform track stores a non-finite spline scalar",
        ));
    }

    let block_stride = max_frames_per_block - 1;
    let mut tracks = vec![Vec::with_capacity(frame_count); transform_count];
    for frame in 0..num_frames {
        let block_index = ((frame / block_stride) as usize).min(num_blocks - 1);
        let first_frame = block_index as u32 * block_stride;
        let local_frame = frame.saturating_sub(first_frame) as f32;
        for (track_index, track) in blocks[block_index].iter().enumerate() {
            if !non_finite_tracks[track_index] {
                tracks[track_index].push(track.sample(local_frame)?);
            }
        }
    }

    let binding = pack.object("hkaAnimationBinding")?;
    let binding_count = pack.u32(
        binding + layout.binding_track_to_bone.count,
        "transform binding count",
    )? as usize;
    let track_to_bone = if binding_count == 0 {
        (0..transform_count)
            .map(|index| {
                u16::try_from(index).map_err(|_| HkxError::InvalidData("bone index overflow"))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        if binding_count != transform_count {
            return Err(HkxError::InvalidData(
                "transform binding count disagrees with tracks",
            ));
        }
        let offset = pack
            .local_target(binding + layout.binding_track_to_bone.pointer)
            .ok_or(HkxError::InvalidData("transform binding array is unbound"))?;
        let raw = pack.data_slice(offset, binding_count * 2, "transform bindings")?;
        raw.as_chunks::<2>().0
            .iter()
            .map(|bytes| Ok(u16::from_le_bytes(*bytes)))
            .collect::<Result<Vec<_>>>()?
    };
    let annotations = read_annotations(&pack, &layout, object, duration)?;

    // #5662 — drop the flagged tracks (samples were never taken for them)
    // and report their bones, so the clip keeps every valid track.
    let mut dropped_bones = Vec::new();
    let mut kept_tracks = Vec::with_capacity(tracks.len());
    let mut kept_bones = Vec::with_capacity(track_to_bone.len());
    for ((samples, bone), bad) in tracks
        .into_iter()
        .zip(track_to_bone)
        .zip(&non_finite_tracks)
    {
        if *bad {
            dropped_bones.push(bone);
        } else {
            kept_tracks.push(samples);
            kept_bones.push(bone);
        }
    }

    Ok(HkxAnimation {
        duration,
        num_frames,
        frame_duration,
        tracks: kept_tracks,
        track_to_bone: kept_bones,
        annotations,
        dropped_bones,
        reference_frame: read_reference_frame(&pack),
    })
}

/// `hkaDefaultAnimatedReferenceFrame`: Havok 2014 (Fallout 4) layout - the duration and the
/// `hkArray<hkVector4>` of samples follow the up and forward vectors. A Havok 2010 packfile
/// carries no such object in the vanilla clips, so none is read there.
fn read_reference_frame(pack: &Packfile<'_>) -> Option<ReferenceFrame> {
    const MAX_SAMPLES: usize = 65_536;
    const DURATION: usize = 0x40;
    const SAMPLES: usize = 0x48;
    if pack.version() != 11 {
        return None;
    }
    let frame = pack.object("hkaDefaultAnimatedReferenceFrame").ok()?;
    let duration = pack.f32(frame + DURATION, "reference frame duration").ok()?;
    let count = pack.u32(frame + SAMPLES + 8, "reference frame samples").ok()? as usize;
    if count == 0 || count > MAX_SAMPLES || !duration.is_finite() || duration <= 0.0 {
        return None;
    }
    let data = pack.local_target(frame + SAMPLES)?;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let at = data + index * 16;
        let sample = [
            pack.f32(at, "reference frame x").ok()?,
            pack.f32(at + 4, "reference frame y").ok()?,
            pack.f32(at + 8, "reference frame z").ok()?,
            pack.f32(at + 12, "reference frame yaw").ok()?,
        ];
        if !sample.iter().all(|value| value.is_finite()) {
            return None;
        }
        samples.push(sample);
    }
    Some(ReferenceFrame { duration, samples })
}

fn read_annotations(
    pack: &Packfile<'_>,
    layout: &Layout,
    animation: usize,
    duration: f32,
) -> Result<Vec<HkxAnnotation>> {
    const MAX_TRACKS: usize = 4096;
    const MAX_ANNOTATIONS: usize = 65_536;

    let track_count = pack.u32(
        animation + layout.annotation_tracks.count,
        "annotation-track count",
    )? as usize;
    if track_count > MAX_TRACKS {
        return Err(HkxError::InvalidData("implausible annotation-track count"));
    }
    if track_count == 0 {
        return Ok(Vec::new());
    }
    let tracks = pack
        .local_target(animation + layout.annotation_tracks.pointer)
        .ok_or(HkxError::InvalidData("annotation-track array is unbound"))?;
    let tracks_size = track_count
        .checked_mul(layout.track_stride)
        .ok_or(HkxError::InvalidData("annotation-track size overflow"))?;
    pack.data_slice(tracks, tracks_size, "annotation tracks")?;

    let mut result = Vec::new();
    let mut total_annotations = 0usize;
    for track_index in 0..track_count {
        let track = tracks + track_index * layout.track_stride;
        let track_name = pack
            .local_target(track + layout.track_name)
            .map(|offset| pack.cstr(offset, "annotation-track name"))
            .transpose()?
            .unwrap_or_default()
            .to_owned();
        let annotation_count =
            pack.u32(track + layout.track_annotations.count, "annotation count")? as usize;
        total_annotations = total_annotations
            .checked_add(annotation_count)
            .ok_or(HkxError::InvalidData("annotation count overflow"))?;
        if total_annotations > MAX_ANNOTATIONS {
            return Err(HkxError::InvalidData("implausible annotation count"));
        }
        if annotation_count == 0 {
            continue;
        }
        let annotations = pack
            .local_target(track + layout.track_annotations.pointer)
            .ok_or(HkxError::InvalidData("annotation array is unbound"))?;
        let annotations_size = annotation_count
            .checked_mul(layout.annotation_stride)
            .ok_or(HkxError::InvalidData("annotation array size overflow"))?;
        pack.data_slice(annotations, annotations_size, "annotations")?;

        for annotation_index in 0..annotation_count {
            let annotation = annotations + annotation_index * layout.annotation_stride;
            let time = pack.f32(annotation + layout.annotation_time, "annotation time")?;
            // #3018 (SCR-D8-2026-08-16-03) — an out-of-range annotation
            // timestamp used to hard-fail the whole clip decode, discarding
            // every transform track over one bad piece of text-event
            // metadata. The accepted-range path a few lines below already
            // clamps `time` to `duration` anyway, so the two policies
            // disagreed about what "out of range" means. Skip just this
            // annotation and keep decoding — `InvalidData` is reserved for
            // structural corruption (unbound pointers, size overflow),
            // not one anomalous float.
            if !time.is_finite() || time < 0.0 || time > duration + 0.001 {
                log::warn!(
                    "hkx: skipping out-of-range annotation time {time} (clip duration \
                     {duration}) in track {track_name:?} — clip decode continues"
                );
                continue;
            }
            let text_offset = pack
                .local_target(annotation + layout.annotation_text)
                .ok_or(HkxError::InvalidData("annotation text is unbound"))?;
            let text = pack.cstr(text_offset, "annotation text")?;
            if !text.is_empty() {
                result.push(HkxAnnotation {
                    track_name: track_name.clone(),
                    time: time.min(duration),
                    text: text.to_owned(),
                });
            }
        }
    }
    result.sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok(result)
}

#[derive(Debug, Clone)]
struct BlockTrack {
    translation: VectorCurve,
    rotation: QuaternionCurve,
    scale: VectorCurve,
}

impl BlockTrack {
    fn sample(&self, time: f32) -> Result<HkxTransform> {
        Ok(HkxTransform {
            translation: self.translation.sample(time)?,
            rotation: self.rotation.sample(time)?,
            scale: self.scale.sample(time)?,
        })
    }
}

#[derive(Debug, Clone)]
enum VectorCurve {
    Static([f32; 3]),
    Spline {
        degree: usize,
        knots: Vec<u8>,
        components: [ScalarCurve; 3],
    },
}

impl VectorCurve {
    fn sample(&self, time: f32) -> Result<[f32; 3]> {
        match self {
            Self::Static(value) => Ok(*value),
            Self::Spline {
                degree,
                knots,
                components,
            } => {
                let mut output = [0.0; 3];
                for (value, component) in output.iter_mut().zip(components) {
                    *value = component.sample(*degree, knots, time)?;
                }
                Ok(output)
            }
        }
    }
}

#[derive(Debug, Clone)]
enum ScalarCurve {
    Constant(f32),
    Dynamic(Vec<f32>),
}

impl ScalarCurve {
    fn sample(&self, degree: usize, knots: &[u8], time: f32) -> Result<f32> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Dynamic(values) => sample_bspline(values, degree, knots, time),
        }
    }
}

#[derive(Debug, Clone)]
enum QuaternionCurve {
    Static([f32; 4]),
    Spline {
        degree: usize,
        knots: Vec<u8>,
        values: Vec<[f32; 4]>,
    },
}

impl QuaternionCurve {
    fn sample(&self, time: f32) -> Result<[f32; 4]> {
        match self {
            Self::Static(value) => Ok(*value),
            Self::Spline {
                degree,
                knots,
                values,
            } => {
                let weights = bspline_weights(values.len(), *degree, knots, time)?;
                let mut output = [0.0; 4];
                for (index, weight) in weights {
                    for (component, value) in output.iter_mut().zip(values[index]) {
                        *component += value * weight;
                    }
                }
                normalize_quaternion(output)
            }
        }
    }
}

fn read_pack_u32_array(
    pack: &Packfile<'_>,
    object: usize,
    field: ArrayField,
    label: &'static str,
) -> Result<Vec<u32>> {
    let count = pack.u32(object + field.count, label)? as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let offset = pack
        .local_target(object + field.pointer)
        .ok_or(HkxError::InvalidData("spline offset array is unbound"))?;
    let byte_count = count
        .checked_mul(4)
        .ok_or(HkxError::InvalidData("spline offset array size overflow"))?;
    let raw = pack.data_slice(offset, byte_count, label)?;
    Ok(raw
        .as_chunks::<4>().0
        .iter()
        .map(|bytes| u32::from_le_bytes(*bytes))
        .collect())
}

/// Read one translation or scale curve. A non-finite stored scalar (a
/// static value or a dynamic component's min/max), or finite bounds whose
/// span overflows f32 (#5691), does not fail the read: it sets
/// `non_finite`, the curve keeps a finite placeholder, and every
/// byte is still consumed so the tracks after it land correctly (#5662).
/// Vanilla Skyrim stores NaN bounds on one prop-attachment bone
/// (`AnimObjectR` / `AnimObjectL`) in 59 SE clips whose other tracks are
/// valid; the caller drops the flagged track instead of the whole clip.
fn read_vector_curve(
    data: &[u8],
    cursor: &mut usize,
    mask: u8,
    quantization: u8,
    identity: [f32; 3],
    non_finite: &mut bool,
) -> Result<VectorCurve> {
    validate_vector_mask(mask)?;
    *cursor = align_up(*cursor, 4)?;
    if mask & 0x70 == 0 {
        return read_static_vector(data, cursor, mask, identity, non_finite)
            .map(VectorCurve::Static);
    }

    let (degree, knots, control_count) = read_spline_header(data, cursor)?;
    *cursor = align_up(*cursor, 4)?;
    let mut components = std::array::from_fn(|index| ScalarCurve::Constant(identity[index]));
    let mut bounds = [(0.0f32, 0.0f32); 3];
    for component in 0..3 {
        let static_bit = 1 << component;
        let dynamic_bit = 1 << (component + 4);
        if mask & dynamic_bit != 0 {
            let min = read_f32(data, cursor, "spline scalar minimum")?;
            let max = read_f32(data, cursor, "spline scalar maximum")?;
            if !min.is_finite() || !max.is_finite() {
                // Placeholder bounds keep the decoded control points finite;
                // the flagged track is dropped before anything samples it.
                *non_finite = true;
            } else if min > max {
                return Err(HkxError::InvalidData("invalid spline scalar bounds"));
            } else if !(max - min).is_finite() {
                // #5691 — finite bounds whose span overflows f32 (±3.4e38
                // each way) would push ±inf/NaN control points through
                // `min + quantized * (max - min)`. Same policy as
                // non-finite bounds: flag the track, keep the (0, 0)
                // placeholder so every byte is still consumed.
                *non_finite = true;
            } else {
                bounds[component] = (min, max);
            }
            components[component] = ScalarCurve::Dynamic(Vec::with_capacity(control_count));
        } else if mask & static_bit != 0 {
            let value = read_f32(data, cursor, "static spline scalar")?;
            if !value.is_finite() {
                *non_finite = true;
            }
            components[component] = ScalarCurve::Constant(if value.is_finite() {
                value
            } else {
                identity[component]
            });
        }
    }
    for _ in 0..control_count {
        for component in 0..3 {
            if let ScalarCurve::Dynamic(values) = &mut components[component] {
                let quantized = match quantization {
                    0 => f32::from(take(data, cursor, 1, "8-bit spline control point")?[0]) / 255.0,
                    1 => {
                        let raw = take(data, cursor, 2, "16-bit spline control point")?;
                        f32::from(u16::from_le_bytes(raw.try_into().unwrap())) / 65535.0
                    }
                    _ => return Err(HkxError::InvalidData("invalid scalar quantization type")),
                };
                let (min, max) = bounds[component];
                values.push(min + quantized * (max - min));
            }
        }
    }
    *cursor = align_up(*cursor, 4)?;
    Ok(VectorCurve::Spline {
        degree,
        knots,
        components,
    })
}

fn read_quaternion_curve(
    data: &[u8],
    cursor: &mut usize,
    mask: u8,
    quantization: u8,
) -> Result<QuaternionCurve> {
    validate_quaternion_mask(mask)?;
    if mask & 0xf0 == 0 {
        return read_static_quaternion(data, cursor, mask, quantization)
            .map(QuaternionCurve::Static);
    }

    let (degree, knots, control_count) = read_spline_header(data, cursor)?;
    let (alignment, byte_count) = quaternion_layout(quantization)?;
    *cursor = align_up(*cursor, alignment)?;
    let mut values = Vec::with_capacity(control_count);
    for _ in 0..control_count {
        let bytes = take(data, cursor, byte_count, "spline quaternion control point")?;
        values.push(decode_quaternion(bytes, quantization)?);
    }
    *cursor = align_up(*cursor, 4)?;
    Ok(QuaternionCurve::Spline {
        degree,
        knots,
        values,
    })
}

fn read_spline_header(data: &[u8], cursor: &mut usize) -> Result<(usize, Vec<u8>, usize)> {
    let raw_count = take(data, cursor, 2, "spline control-point count")?;
    let last_control = u16::from_le_bytes(raw_count.try_into().unwrap()) as usize;
    let degree = take(data, cursor, 1, "spline degree")?[0] as usize;
    let control_count = last_control
        .checked_add(1)
        .ok_or(HkxError::InvalidData("spline control-point count overflow"))?;
    if degree == 0 || degree > 8 || control_count <= degree || control_count > MAX_SPLINE_CONTROL_POINTS {
        return Err(HkxError::InvalidData(
            "invalid spline degree or control-point count",
        ));
    }
    let knot_count = control_count
        .checked_add(degree + 1)
        .ok_or(HkxError::InvalidData("spline knot count overflow"))?;
    let knots = take(data, cursor, knot_count, "spline knots")?.to_vec();
    if knots.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(HkxError::InvalidData("spline knots are not monotonic"));
    }
    Ok((degree, knots, control_count))
}

fn read_static_vector(
    data: &[u8],
    cursor: &mut usize,
    mask: u8,
    identity: [f32; 3],
    non_finite: &mut bool,
) -> Result<[f32; 3]> {
    *cursor = align_up(*cursor, 4)?;
    let mut value = identity;
    for (component, output) in value.iter_mut().enumerate() {
        if mask & (1 << component) != 0 {
            let stored = read_f32(data, cursor, "static spline scalar")?;
            if stored.is_finite() {
                *output = stored;
            } else {
                *non_finite = true;
            }
        }
    }
    Ok(value)
}

fn read_static_quaternion(
    data: &[u8],
    cursor: &mut usize,
    mask: u8,
    quantization: u8,
) -> Result<[f32; 4]> {
    if mask == 0 {
        return Ok([0.0, 0.0, 0.0, 1.0]);
    }
    let (alignment, byte_count) = quaternion_layout(quantization)?;
    *cursor = align_up(*cursor, alignment)?;
    let bytes = take(data, cursor, byte_count, "static spline quaternion")?;
    let packed = decode_quaternion(bytes, quantization)?;
    let identity = [0.0, 0.0, 0.0, 1.0];
    let mut output = identity;
    for component in 0..4 {
        if mask & (1 << component) != 0 {
            output[component] = packed[component];
        }
    }
    normalize_quaternion(output)
}

fn quaternion_layout(quantization: u8) -> Result<(usize, usize)> {
    match quantization {
        0 => Ok((4, 4)),
        1 => Ok((1, 5)),
        2 => Ok((2, 6)),
        3 => Ok((1, 3)),
        4 => Ok((2, 2)),
        5 => Ok((4, 16)),
        _ => Err(HkxError::InvalidData("invalid quaternion quantization")),
    }
}

fn decode_quaternion(bytes: &[u8], quantization: u8) -> Result<[f32; 4]> {
    match quantization {
        1 => decode_three_component_40(bytes),
        2 => decode_three_component_48(bytes),
        5 => {
            if bytes.len() != 16 {
                return Err(HkxError::Truncated("uncompressed quaternion"));
            }
            let mut q = [0.0; 4];
            for index in 0..4 {
                q[index] = f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
            }
            normalize_quaternion(q)
        }
        _ => Err(HkxError::UnsupportedLayout(
            "spline track uses an unsupported quaternion encoding",
        )),
    }
}

fn validate_vector_mask(mask: u8) -> Result<()> {
    if mask & 0x88 != 0 {
        return Err(HkxError::InvalidData(
            "vector spline mask contains a W component",
        ));
    }
    for component in 0..3 {
        if mask & (1 << component) != 0 && mask & (1 << (component + 4)) != 0 {
            return Err(HkxError::InvalidData(
                "spline component is both static and dynamic",
            ));
        }
    }
    Ok(())
}

fn validate_quaternion_mask(mask: u8) -> Result<()> {
    for component in 0..4 {
        if mask & (1 << component) != 0 && mask & (1 << (component + 4)) != 0 {
            return Err(HkxError::InvalidData(
                "quaternion component is both static and dynamic",
            ));
        }
    }
    Ok(())
}

/// One stored `f32`, as written. Finiteness is the caller's policy: the
/// vector-curve readers flag a non-finite value per track (#5662).
fn read_f32(data: &[u8], cursor: &mut usize, label: &'static str) -> Result<f32> {
    let bytes = take(data, cursor, 4, label)?;
    Ok(f32::from_le_bytes(bytes.try_into().unwrap()))
}

fn sample_bspline(values: &[f32], degree: usize, knots: &[u8], time: f32) -> Result<f32> {
    let weights = bspline_weights(values.len(), degree, knots, time)?;
    Ok(weights
        .into_iter()
        .map(|(index, weight)| values[index] * weight)
        .sum())
}

fn bspline_weights(
    control_count: usize,
    degree: usize,
    knots: &[u8],
    time: f32,
) -> Result<Vec<(usize, f32)>> {
    if control_count <= degree || knots.len() != control_count + degree + 1 || !time.is_finite() {
        return Err(HkxError::InvalidData("invalid B-spline dimensions"));
    }
    let last_control = control_count - 1;
    let low_knot = f32::from(knots[degree]);
    let high_knot = f32::from(knots[control_count]);
    let time = time.clamp(low_knot, high_knot);
    let span = if time >= high_knot {
        last_control
    } else if time <= low_knot {
        degree
    } else {
        let mut low = degree;
        let mut high = control_count;
        let mut middle = (low + high) / 2;
        while time < f32::from(knots[middle]) || time >= f32::from(knots[middle + 1]) {
            if time < f32::from(knots[middle]) {
                high = middle;
            } else {
                low = middle;
            }
            middle = (low + high) / 2;
        }
        middle
    };

    // Cox-de Boor basis evaluation for the non-zero basis functions in this span.
    let mut basis = vec![0.0f32; degree + 1];
    let mut left = vec![0.0f32; degree + 1];
    let mut right = vec![0.0f32; degree + 1];
    basis[0] = 1.0;
    for order in 1..=degree {
        left[order] = time - f32::from(knots[span + 1 - order]);
        right[order] = f32::from(knots[span + order]) - time;
        let mut saved = 0.0;
        for index in 0..order {
            let denominator = right[index + 1] + left[order - index];
            let term = if denominator.abs() <= f32::EPSILON {
                0.0
            } else {
                basis[index] / denominator
            };
            basis[index] = saved + right[index + 1] * term;
            saved = left[order - index] * term;
        }
        basis[order] = saved;
    }
    let first = span - degree;
    Ok(basis
        .into_iter()
        .enumerate()
        .map(|(index, weight)| (first + index, weight))
        .collect())
}

/// Havok THREECOMP40: three signed 12-bit values plus a two-bit omitted
/// component index and one sign bit. The stored values cover
/// `[-1/sqrt(2), +1/sqrt(2)]`; the omitted largest component is recovered
/// from the unit-length constraint.
/// `THREECOMP48`: the 40-bit "smallest three" with 15-bit components. Three
/// little-endian words; each holds one component in its low 15 bits, the omitted
/// component's index sits in the top bits of the first two words and the sign of
/// the omitted component in the top bit of the third.
fn decode_three_component_48(bytes: &[u8]) -> Result<[f32; 4]> {
    if bytes.len() != 6 {
        return Err(HkxError::Truncated("48-bit quaternion"));
    }
    let words = [
        u16::from_le_bytes([bytes[0], bytes[1]]),
        u16::from_le_bytes([bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
    ];
    let factor = 1.0 / (16383.0 * std::f32::consts::SQRT_2);
    let stored = words.map(|word| ((word & 0x7fff) as i32 - 16383) as f32 * factor);
    let omitted = usize::from(words[0] >> 15) | usize::from(words[1] >> 15) << 1;
    let mut missing = (1.0 - stored.iter().map(|value| value * value).sum::<f32>())
        .max(0.0)
        .sqrt();
    if words[2] & 0x8000 != 0 {
        missing = -missing;
    }
    let mut output = [0.0; 4];
    let mut source = 0;
    for (index, component) in output.iter_mut().enumerate() {
        if index == omitted {
            *component = missing;
        } else {
            *component = stored[source];
            source += 1;
        }
    }
    normalize_quaternion(output)
}

fn decode_three_component_40(bytes: &[u8]) -> Result<[f32; 4]> {
    if bytes.len() != 5 {
        return Err(HkxError::Truncated("40-bit quaternion"));
    }
    let mut packed = 0u64;
    for (index, byte) in bytes.iter().enumerate() {
        packed |= u64::from(*byte) << (index * 8);
    }
    let factor = 1.0 / (2047.0 * std::f32::consts::SQRT_2);
    let stored = [
        ((packed & 0x0fff) as i32 - 2047) as f32 * factor,
        (((packed >> 12) & 0x0fff) as i32 - 2047) as f32 * factor,
        (((packed >> 24) & 0x0fff) as i32 - 2047) as f32 * factor,
    ];
    let omitted = ((packed >> 36) & 0x03) as usize;
    let mut missing = (1.0 - stored.iter().map(|value| value * value).sum::<f32>())
        .max(0.0)
        .sqrt();
    if packed & (1 << 38) != 0 {
        missing = -missing;
    }
    let mut output = [0.0; 4];
    let mut source = 0;
    for (index, component) in output.iter_mut().enumerate() {
        if index == omitted {
            *component = missing;
        } else {
            *component = stored[source];
            source += 1;
        }
    }
    normalize_quaternion(output)
}

fn read_qs_transform(bytes: &[u8]) -> Result<HkxTransform> {
    if bytes.len() != 48 {
        return Err(HkxError::Truncated("hkQsTransform"));
    }
    let read = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let translation = [read(0), read(4), read(8)];
    let rotation = normalize_quaternion([read(16), read(20), read(24), read(28)])?;
    let scale = [read(32), read(36), read(40)];
    if translation
        .iter()
        .chain(scale.iter())
        .any(|value| !value.is_finite())
    {
        return Err(HkxError::InvalidData("non-finite hkQsTransform"));
    }
    Ok(HkxTransform {
        translation,
        rotation,
        scale,
    })
}

fn normalize_quaternion(mut value: [f32; 4]) -> Result<[f32; 4]> {
    if value.iter().any(|component| !component.is_finite()) {
        return Err(HkxError::InvalidData("non-finite quaternion"));
    }
    let length_squared = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    // #4396 — individually-finite components past ~1.84e19 square past
    // f32::MAX, so `length_squared` overflows to inf and `recip` yields 0:
    // every component becomes `finite * 0.0 == 0.0` and this returned
    // Ok([0,0,0,0]) — the zero quaternion — for the same input the NIF
    // paths reject. Reject it like the zero-length case.
    if !length_squared.is_finite() {
        return Err(HkxError::InvalidData("quaternion length overflow"));
    }
    if length_squared <= f32::EPSILON {
        return Err(HkxError::InvalidData("zero-length quaternion"));
    }
    let inverse = length_squared.sqrt().recip();
    for component in &mut value {
        *component *= inverse;
    }
    Ok(value)
}

fn take<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    label: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(HkxError::InvalidData("spline cursor overflow"))?;
    let bytes = data.get(*cursor..end).ok_or(HkxError::Truncated(label))?;
    *cursor = end;
    Ok(bytes)
}

fn align_up(value: usize, alignment: usize) -> Result<usize> {
    if !alignment.is_power_of_two() {
        return Err(HkxError::InvalidData("non-power-of-two alignment"));
    }
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or(HkxError::InvalidData("alignment overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4396 (NIFAL-D7-2026-09-14-01) — individually-finite components
    /// past ~1.84e19 square past f32::MAX: `length_squared` overflows to
    /// inf, `recip()` yields 0, and this function used to return
    /// Ok([0,0,0,0]) — the zero quaternion — for the same input the NIF
    /// paths reject. The overflow arm now errors like the zero-length and
    /// non-finite arms.
    #[test]
    fn normalize_quaternion_rejects_length_overflow() {
        let huge = 2.0e19f32;
        assert!((huge * huge).is_infinite(), "fixture sanity: squaring overflows");
        assert!(
            normalize_quaternion([huge, 0.0, 0.0, 0.0]).is_err(),
            "an overflowing len_sq must error, not yield the zero quaternion (#4396)"
        );
        // The pre-existing arms stay: non-finite and zero-length reject,
        // an ordinary non-unit quaternion still normalizes.
        assert!(normalize_quaternion([f32::NAN, 0.0, 0.0, 0.0]).is_err());
        assert!(normalize_quaternion([0.0, 0.0, 0.0, 0.0]).is_err());
        let q = normalize_quaternion([0.0, 3.0, 4.0, 0.0]).expect("ordinary survives");
        let len_sq: f32 = q.iter().map(|c| c * c).sum();
        assert!((len_sq - 1.0).abs() < 1e-5);
    }

    fn pack_three_component_40(values: [u16; 3], omitted: u8, negative: bool) -> [u8; 5] {
        let mut packed = u64::from(values[0])
            | (u64::from(values[1]) << 12)
            | (u64::from(values[2]) << 24)
            | (u64::from(omitted) << 36);
        if negative {
            packed |= 1 << 38;
        }
        let bytes = packed.to_le_bytes();
        bytes[..5].try_into().unwrap()
    }

    #[test]
    fn three_component_40_decodes_identity() {
        let bytes = pack_three_component_40([2047; 3], 3, false);
        let q = decode_three_component_40(&bytes).unwrap();
        assert!(q[0].abs() < 1e-6 && q[1].abs() < 1e-6 && q[2].abs() < 1e-6);
        assert!((q[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn three_component_40_retains_omitted_component_and_sign() {
        let bytes = pack_three_component_40([2047; 3], 1, true);
        let q = decode_three_component_40(&bytes).unwrap();
        assert!(q[0].abs() < 1e-6 && q[2].abs() < 1e-6 && q[3].abs() < 1e-6);
        assert!((q[1] + 1.0).abs() < 1e-6);
    }

    const SKELETON: &str = r"meshes\actors\character\character assets\skeleton.hkx";
    const CART_PLAYER_IDLE: &str = r"meshes\actors\character\animations\carttravelplayeridle.hkx";
    /// The MQ101 cart family beyond the player idle, shared by both releases.
    const CART_CLIPS: [&str; 15] = [
        r"meshes\actors\character\animations\carttraveldriveridle.hkx",
        r"meshes\actors\character\animations\cartdriveridlesway.hkx",
        r"meshes\actors\character\animations\cartprisonerasway.hkx",
        r"meshes\actors\character\animations\cartprisonerbsway.hkx",
        r"meshes\actors\character\animations\cartprisonercsway.hkx",
        r"meshes\actors\character\animations\cartprisonerdsway.hkx",
        r"meshes\actors\character\animations\cartprisoneraidle.hkx",
        r"meshes\actors\character\animations\cartprisonerbidle.hkx",
        r"meshes\actors\character\animations\cartprisonercidle.hkx",
        r"meshes\actors\character\animations\cartprisonerdidle.hkx",
        r"meshes\actors\character\animations\cartdriverexit.hkx",
        r"meshes\actors\character\animations\cartprisoneraexit.hkx",
        r"meshes\actors\character\animations\cartprisonerbexit.hkx",
        r"meshes\actors\character\animations\cartprisonercexit.hkx",
        r"meshes\actors\character\animations\cartprisonerdexit.hkx",
    ];

    /// #4660 — resolves like the other real-data harnesses: a set `env`
    /// override is binding (#3850) instead of silently falling back, and
    /// under `BYROREDUX_REQUIRE_GAME_DATA` an absent archive fails, naming
    /// the path, instead of skipping green.
    fn animations_archive(env: &str, default: &str) -> Option<byroredux_bsa::BsaArchive> {
        let data_dir = match std::env::var(env).ok().filter(|s| !s.is_empty()) {
            Some(v) => {
                let p = std::path::PathBuf::from(&v);
                assert!(p.is_dir(), "{env} points to {v:?}, which is not a directory");
                p
            }
            None => std::path::PathBuf::from(default),
        };
        let archive_path = data_dir.join("Skyrim - Animations.bsa");
        if !archive_path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!(
                    "BYROREDUX_REQUIRE_GAME_DATA is set, but {archive_path:?} is not a file \
                     ({env} unset, or pointing at the wrong install)"
                );
            }
            eprintln!("SKIP: {} not found (no game data?)", archive_path.display());
            return None;
        }
        Some(byroredux_bsa::BsaArchive::open(archive_path).unwrap())
    }

    /// #5006 (PAR-D1-2026-09-29-01) — vanilla census of the spline
    /// compressor's `max_frames_per_block` across every `.hkx` in the SE
    /// Animations archive. This is what measured the ceiling the
    /// dimension gate now enforces (256, the Havok compressor default —
    /// vanilla never exceeds it), and it doubles as the guard: a future
    /// re-export or toolchain change that exceeds the gate's ceiling
    /// fails here first, on real data, instead of as silent rejects in
    /// the decoder.
    /// `cargo test -p byroredux-hkx -- --ignored --nocapture`
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings() {
        let Some(archive) = animations_archive(
            byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV,
            byroredux_plugin::esm::test_paths::SKYRIM_SE_DEFAULT,
        ) else {
            return;
        };
        let mut max_mfpb = 0u32;
        let mut max_frames = 0u32;
        let mut max_blocks = 0u32;
        let mut max_samples: u64 = 0;
        let mut clips = 0usize;
        let mut skipped = 0usize;
        // #5326 — clips the REAL decoder rejects (the `mask_size` tie,
        // block-chain and dimension gates the raw-field reads below never
        // run). Vanilla must have none.
        let mut rejects: Vec<String> = Vec::new();
        // #5662 — clips that decode with a non-finite track dropped.
        let mut partial = 0usize;
        let mut worst = String::new();
        for path in archive.list_files() {
            if !path.ends_with(".hkx") {
                continue;
            }
            let bytes = match archive.extract(path) {
                Ok(b) => b,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let pack = match Packfile::parse(&bytes) {
                Ok(p) => p,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let object = match pack.object("hkaSplineCompressedAnimation") {
                Ok(o) => o,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let layout = Layout::new(pack.pointer_size());
            let mfpb = pack
                .u32(object + layout.max_frames_per_block, "census mfpb")
                .unwrap();
            let frames = pack.u32(object + layout.num_frames, "census frames").unwrap();
            let blocks = pack.u32(object + layout.num_blocks, "census blocks").unwrap();
            let tracks = pack
                .u32(object + layout.transform_tracks, "census tracks")
                .unwrap();
            clips += 1;
            match decode_spline_animation(&bytes) {
                Ok(animation) if !animation.dropped_bones.is_empty() => partial += 1,
                Ok(_) => {}
                Err(error) => rejects.push(format!("{path}: {error:?}")),
            }
            max_mfpb = max_mfpb.max(mfpb);
            max_frames = max_frames.max(frames);
            max_blocks = max_blocks.max(blocks);
            let samples = tracks as u64 * frames as u64;
            if samples > max_samples {
                max_samples = samples;
                worst = format!("{path}: {tracks} tracks x {frames} frames");
            }
        }
        eprintln!(
            "census: {clips} clips, {skipped} non-clip/undecodable files; \
             max mfpb={max_mfpb}, max frames={max_frames}, max blocks={max_blocks}, \
             max samples={max_samples} ({worst})"
        );
        // #5326 — the census must not pass vacuously: a `Packfile::parse` /
        // extract regression used to count every file as "skipped" and
        // leave `max_mfpb = 0`. Measured on the shipped SE archive
        // (2026-10-05): 6,126 clips, 1,573 non-clip / non-spline files.
        // #5671 — every count below is EXACT, not an upper bound: a
        // one-sided `<=` stays green through a regression that changes
        // WHICH clips decode, which is the drift this census exists to
        // catch.
        assert_eq!(clips, 6_126, "vanilla SE spline-clip count (a parse regression shows here)");
        assert_eq!(
            skipped, 1_573,
            "files skipped — a parse/extract regression turns clips into skips"
        );
        // The real decoder rejected 288 of the 6,126 vanilla clips on
        // 2026-10-11: 229 on the unpadded `mask_size` equality (#5659 —
        // Havok pads the mask table to 4 bytes) and 59 on NaN translation/
        // scale bounds on one prop-attachment bone (#5662 — that track is
        // now dropped, the clip kept). Every vanilla clip decodes since
        // (measured 2026-10-12) — the empty set, listed in full on any
        // failure (#5671: a count could hide a swap of which clips fail).
        assert!(
            rejects.is_empty(),
            "{} vanilla clips rejected by decode_spline_animation: {:#?}",
            rejects.len(),
            rejects
        );
        eprintln!(
            "census: {} clips rejected by the real decoder, {partial} decoded with a \
             non-finite track dropped",
            rejects.len()
        );
        // #5662 — measured 2026-10-12: the 59 clips that used to be rejected
        // (one `AnimObjectR`/`AnimObjectL` track each, or its paired-rig
        // slot). More means a decoder change started flagging valid tracks;
        // fewer means the NaN-bounds detection stopped firing on them (#5671
        // made this two-sided for the same reason).
        assert_eq!(
            partial, 59,
            "vanilla clips decoding with a dropped track (baseline 59)"
        );
        // The dimension maxima, pinned exactly against the measured
        // vanilla shape (frames 1,471, blocks 15). #5671 — this replaces
        // the old `div_ceil(frames) >= blocks || blocks <= 64` clause,
        // whose consistency half was false on vanilla (1,471/256 = 6 < 15)
        // and whose `||` arm made it pass for any plausible value — an
        // inert assert blessing exactly the regressions it described.
        assert!(max_frames > 0 && max_blocks > 0, "census read no dimensions");
        assert_eq!(
            max_frames, 1_471,
            "vanilla max frame count moved — re-measure the census"
        );
        assert_eq!(
            max_blocks, 15,
            "vanilla max block count moved — re-measure the census"
        );
        assert!(
            max_mfpb <= MAX_FRAMES_PER_BLOCK as u32,
            "vanilla max_frames_per_block {max_mfpb} exceeds the gate ceiling \
             {MAX_FRAMES_PER_BLOCK} — re-measure and raise the ceiling"
        );
        // #5316 (PAR-D1-2026-10-05-02) — the absolute sample backstop is
        // census-derived too: a vanilla re-export that grows past it must
        // trip this gate, not silently start paying the 597 MiB decode.
        assert!(
            max_samples <= MAX_TRANSFORM_SAMPLES as u64,
            "vanilla max samples {max_samples} exceeds the gate ceiling \
             {MAX_TRANSFORM_SAMPLES} — re-measure and raise the ceiling"
        );
    }

    /// The layout walk must reproduce, for 8-byte pointers, exactly the
    /// offsets this decoder hard-coded while it read Skyrim SE only — those
    /// were validated against the shipped SE assets, so they pin the field
    /// order and alignment rules the 32-bit layout is derived from.
    #[test]
    fn layout_walk_reproduces_the_skyrim_se_offsets() {
        let array = |pointer, count| ArrayField { pointer, count };
        let se = Layout::new(8);
        assert_eq!(
            se,
            Layout {
                skeleton_name: 0x10,
                skeleton_parents: array(0x18, 0x20),
                skeleton_bones: array(0x28, 0x30),
                skeleton_pose: array(0x38, 0x40),
                bone_stride: 16,
                animation_type: 0x10,
                animation_duration: 0x14,
                transform_tracks: 0x18,
                float_tracks: 0x1c,
                annotation_tracks: array(0x28, 0x30),
                num_frames: 0x38,
                num_blocks: 0x3c,
                max_frames_per_block: 0x40,
                mask_size: 0x44,
                frame_duration: 0x50,
                block_offsets: array(0x58, 0x60),
                float_block_offsets: array(0x68, 0x70),
                spline_data: array(0x98, 0xa0),
                binding_track_to_bone: array(0x20, 0x28),
                track_name: 0,
                track_annotations: array(0x08, 0x10),
                track_stride: 0x18,
                annotation_time: 0,
                annotation_text: 0x08,
                annotation_stride: 0x10,
            }
        );
    }

    /// The 2011 release's 32-bit packfiles must decode to the same rig and the
    /// same clips as Special Edition's 64-bit re-export of that content. The
    /// 32-bit offsets are derived, not measured one by one, so agreement with
    /// the SE decode on real files is what verifies them.
    ///
    /// `cargo test -p byroredux-hkx -- --ignored --nocapture`
    #[test]
    #[ignore = "needs Skyrim LE and Skyrim SE game data on disk"]
    fn skyrim_le_packfiles_decode_to_the_same_rig_and_clips_as_se() {
        let (Some(le), Some(se)) = (
            animations_archive(
                "BYROREDUX_SKYRIMLE_DATA",
                "/home/matias/Games/skyrim-original/drive_c/Program Files (x86)/The Elder Scrolls V Skyrim/Data",
            ),
            animations_archive(
                byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV,
                byroredux_plugin::esm::test_paths::SKYRIM_SE_DEFAULT,
            ),
        ) else {
            return;
        };

        let le_skeleton = decode_skeleton(&le.extract(SKELETON).unwrap()).unwrap();
        let se_skeleton = decode_skeleton(&se.extract(SKELETON).unwrap()).unwrap();
        assert_eq!(le_skeleton.name, se_skeleton.name);
        assert_eq!(le_skeleton.bones.len(), se_skeleton.bones.len());
        for (le_bone, se_bone) in le_skeleton.bones.iter().zip(&se_skeleton.bones) {
            assert_eq!(le_bone.name, se_bone.name);
            assert_eq!(
                le_bone.parent_index, se_bone.parent_index,
                "{}",
                le_bone.name
            );
            assert_transforms_match(
                &le_bone.reference_pose,
                &se_bone.reference_pose,
                &le_bone.name,
            );
        }

        for path in std::iter::once(CART_PLAYER_IDLE).chain(CART_CLIPS) {
            let le_clip = decode_spline_animation(&le.extract(path).unwrap())
                .unwrap_or_else(|error| panic!("LE {path}: {error}"));
            let se_clip = decode_spline_animation(&se.extract(path).unwrap()).unwrap();
            assert_eq!(le_clip.num_frames, se_clip.num_frames, "{path}");
            assert_eq!(le_clip.duration, se_clip.duration, "{path}");
            assert_eq!(le_clip.frame_duration, se_clip.frame_duration, "{path}");
            assert_eq!(le_clip.track_to_bone, se_clip.track_to_bone, "{path}");
            assert_eq!(le_clip.annotations, se_clip.annotations, "{path}");
            assert_eq!(le_clip.tracks.len(), se_clip.tracks.len(), "{path}");
            for (le_track, se_track) in le_clip.tracks.iter().zip(&se_clip.tracks) {
                for (le_sample, se_sample) in le_track.iter().zip(se_track) {
                    assert_transforms_match(le_sample, se_sample, path);
                }
            }
        }
    }

    fn assert_transforms_match(le: &HkxTransform, se: &HkxTransform, context: &str) {
        let pairs = le
            .translation
            .iter()
            .zip(&se.translation)
            .chain(le.rotation.iter().zip(&se.rotation))
            .chain(le.scale.iter().zip(&se.scale));
        for (a, b) in pairs {
            assert!((a - b).abs() <= 1e-4, "{context}: LE {le:?} vs SE {se:?}");
        }
    }

    /// #3014 (SCR-D8-2026-08-16-04) — this test needs the Skyrim SE
    /// animation archive on disk, so it never runs in CI. `#[ignore]`
    /// makes a skipped run show as `ignored`, not `ok` — pre-fix this was
    /// a plain `return`, so the crate's only integration test reported
    /// green on every machine without the archive (including CI), leaving
    /// zero real coverage indistinguishable from a passing run. Run
    /// locally with:
    /// `cargo test -p byroredux-hkx -- --ignored --nocapture`
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn skyrim_cart_player_idle_decodes_when_assets_are_available() {
        let Some(archive) = animations_archive(
            byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV,
            byroredux_plugin::esm::test_paths::SKYRIM_SE_DEFAULT,
        ) else {
            return;
        };
        let skeleton = archive
            .extract(r"meshes\actors\character\character assets\skeleton.hkx")
            .unwrap();
        let clip = archive
            .extract(r"meshes\actors\character\animations\carttravelplayeridle.hkx")
            .unwrap();
        let skeleton = decode_skeleton(&skeleton).unwrap();
        let clip = decode_spline_animation(&clip).unwrap();
        assert_eq!(skeleton.bones.len(), 99);
        assert_eq!(clip.tracks.len(), 99);
        assert_eq!(clip.track_to_bone.len(), 99);
        assert_eq!(clip.num_frames, 20);
        assert!(clip
            .tracks
            .iter()
            .all(|track| track.len() == clip.num_frames as usize));
        assert!((clip.duration - 19.0 / 30.0).abs() < 1e-4);
        assert!(clip.tracks.iter().flatten().all(|sample| {
            sample.translation.iter().all(|value| value.is_finite())
                && sample.rotation.iter().all(|value| value.is_finite())
                && sample.scale.iter().all(|value| value.is_finite())
        }));

        for path in CART_CLIPS {
            let bytes = archive.extract(path).unwrap();
            let dynamic = decode_spline_animation(&bytes)
                .unwrap_or_else(|error| panic!("failed to decode {path}: {error}"));
            assert_eq!(dynamic.tracks.len(), 99, "{path}");
            assert!(dynamic.num_frames > 1, "{path}");
            assert!(dynamic
                .tracks
                .iter()
                .all(|track| track.len() == dynamic.num_frames as usize));
            if path.ends_with("exit.hkx") {
                let com_track = dynamic
                    .track_to_bone
                    .iter()
                    .position(|bone| {
                        skeleton.bones[usize::from(*bone)]
                            .name
                            .eq_ignore_ascii_case("NPC COM [COM ]")
                    })
                    .unwrap();
                let first = dynamic.tracks[com_track].first().unwrap().translation;
                let last = dynamic.tracks[com_track].last().unwrap().translation;
                let horizontal_travel =
                    ((last[0] - first[0]).powi(2) + (last[1] - first[1]).powi(2)).sqrt();
                assert!(
                    horizontal_travel > 10.0,
                    "{path} has no authored COM exit trajectory"
                );
            }
            assert!(
                dynamic.tracks.iter().any(|track| {
                    track.windows(2).any(|pair| {
                        pair[0]
                            .translation
                            .iter()
                            .zip(pair[1].translation)
                            .any(|(left, right)| (left - right).abs() > 1e-5)
                            || pair[0]
                                .rotation
                                .iter()
                                .zip(pair[1].rotation)
                                .any(|(left, right)| (left - right).abs() > 1e-5)
                    })
                }),
                "{path} decoded without any time-varying pose samples"
            );
        }
    }

    /// #3014 (SCR-D8-2026-08-16-04) — the crate had zero negative/malformed-
    /// input coverage; the only integration test needed real game data and
    /// silently passed without it. Checked-in fixture built with
    /// `crate::packfile::fixtures::PackfileBuilder` (promoted out of
    /// `packfile.rs`'s own tests for exactly this — #3018), no game data
    /// required.
    ///
    /// Pins #3011's bound: `transform_count * num_frames` past
    /// `MAX_TRANSFORM_SAMPLES` must be rejected before any `Vec::with_
    /// capacity` allocation, not after. `num_blocks`/`max_frames_per_block`/
    /// `mask_size` are left zeroed — the dimension check's `||` chain short-
    /// circuits on the sample-count clause before reaching theirs, so this
    /// fixture never needs valid spline block data at all.
    #[test]
    fn decode_spline_animation_rejects_a_sample_count_bomb() {
        use crate::packfile::fixtures::PackfileBuilder;

        // float_count (0x1c..0x20) is left at 0.
        let mut data = vec![0u8; 0x60];
        data[0x10..0x14].copy_from_slice(&5u32.to_le_bytes()); // animation_type = spline-compressed
        data[0x14..0x18].copy_from_slice(&1.0f32.to_le_bytes()); // duration
        data[0x18..0x1c].copy_from_slice(&4096u32.to_le_bytes()); // transform_count (max allowed)
        data[0x38..0x3c].copy_from_slice(&5000u32.to_le_bytes()); // num_frames: transform_count * num_frames > MAX_TRANSFORM_SAMPLES
        data[0x50..0x54].copy_from_slice(&(1.0f32 / 30.0).to_le_bytes()); // frame_duration

        let mut builder = PackfileBuilder {
            data,
            ..Default::default()
        };
        let class = builder.class("hkaSplineCompressedAnimation");
        builder.virtual_fixups.push((0, 0, class));
        let bytes = builder.build();

        let err = decode_spline_animation(&bytes).expect_err(
            "a transform_count * num_frames product past MAX_TRANSFORM_SAMPLES must be \
             rejected, not handed to Vec::with_capacity (#3011)",
        );
        assert_eq!(
            err,
            HkxError::InvalidData("unsupported spline clip dimensions")
        );
    }

    /// #4655 (PAR-D1-2026-09-21-02) — the sample cap is now RELATIVE to
    /// the blocks the file carries, not only absolute. A 17 KB clip
    /// claiming 4096 tracks x 3906 frames (under the old absolute cap at
    /// 15,998,976 samples) decoded to 610 MiB transient and ~2 GB of
    /// retained keys; with num_blocks small the relative check rejects
    /// it because the frames do not fit the block data on disk.
    #[test]
    fn decode_spline_animation_rejects_frames_beyond_the_declared_blocks() {
        use crate::packfile::fixtures::PackfileBuilder;

        let mut data = vec![0u8; 0x60];
        data[0x10..0x14].copy_from_slice(&5u32.to_le_bytes()); // spline-compressed
        data[0x14..0x18].copy_from_slice(&1.0f32.to_le_bytes()); // duration
        data[0x18..0x1c].copy_from_slice(&8u32.to_le_bytes()); // transform_count
        data[0x38..0x3c].copy_from_slice(&3906u32.to_le_bytes()); // num_frames
        data[0x3c..0x40].copy_from_slice(&2u32.to_le_bytes()); // num_blocks (tiny)
        data[0x40..0x44].copy_from_slice(&16u32.to_le_bytes()); // max_frames_per_block
        data[0x50..0x54].copy_from_slice(&(1.0f32 / 30.0).to_le_bytes()); // frame_duration

        let mut builder = PackfileBuilder {
            data,
            ..Default::default()
        };
        let class = builder.class("hkaSplineCompressedAnimation");
        builder.virtual_fixups.push((0, 0, class));
        let bytes = builder.build();

        // 8 tracks x 3906 frames = 31,248 samples: far below the absolute
        // cap, so ONLY the relative check can reject it — 2 blocks x 15
        // frames cannot carry 3906 frames.
        let err = decode_spline_animation(&bytes)
            .expect_err("frames beyond the declared block capacity must be rejected");
        assert_eq!(
            err,
            HkxError::InvalidData("unsupported spline clip dimensions")
        );
    }

    /// #5316 (PAR-D1-2026-10-05-02) — the audit probe shape: a 279 KB
    /// hand-built spline clip with 15 tracks, 4096 blocks, the vanilla
    /// mfpb ceiling of 256, and `num_frames = 4096*255 + 1` satisfies
    /// every relative check (#4655's frames-vs-blocks tie exactly,
    /// #5006's mfpb ceiling at equality) and decoded to 15,667,215
    /// samples / 597 MiB — only the absolute MAX_TRANSFORM_SAMPLES
    /// backstop can reject it. The backstop is now census-derived
    /// (1 M = 8× the vanilla maximum of 124,821) instead of 16 M.
    #[test]
    fn decode_spline_animation_rejects_the_279kb_spline_probe() {
        use crate::packfile::fixtures::PackfileBuilder;

        let mut data = vec![0u8; 0x60];
        data[0x10..0x14].copy_from_slice(&5u32.to_le_bytes()); // spline-compressed
        data[0x14..0x18].copy_from_slice(&1.0f32.to_le_bytes()); // duration
        data[0x18..0x1c].copy_from_slice(&15u32.to_le_bytes()); // transform_count
        data[0x38..0x3c].copy_from_slice(&(4096u32 * 255 + 1).to_le_bytes()); // num_frames
        data[0x3c..0x40].copy_from_slice(&4096u32.to_le_bytes()); // num_blocks
        data[0x40..0x44].copy_from_slice(&256u32.to_le_bytes()); // mfpb = ceiling
        data[0x44..0x48].copy_from_slice(&(15u32 * 4).to_le_bytes()); // mask_size = tracks*4
        data[0x50..0x54].copy_from_slice(&(1.0f32 / 30.0).to_le_bytes()); // frame_duration

        let mut builder = PackfileBuilder {
            data,
            ..Default::default()
        };
        let class = builder.class("hkaSplineCompressedAnimation");
        builder.virtual_fixups.push((0, 0, class));
        let bytes = builder.build();

        // 15 tracks x 1,044,481 frames = 15,667,215 samples: every
        // relative check passes, so only the census-derived absolute cap
        // can reject this (597 MiB of decode pre-fix).
        let err = decode_spline_animation(&bytes).expect_err(
            "the 279 KB probe's 15.7M-sample expansion must exceed the census-derived cap",
        );
        assert_eq!(
            err,
            HkxError::InvalidData("unsupported spline clip dimensions")
        );
    }

    /// #5006 (PAR-D1-2026-09-29-01) — #4655's frames-vs-blocks tie is
    /// bypassable with ONE block whose `max_frames_per_block` ≈
    /// `num_frames`: all-static masks cost `transform_count * 4` bytes
    /// per block whatever the frame count, so a 17 KB file claiming 4096
    /// tracks x 3906 frames (15,998,976 samples, 610 MiB decoded) passed
    /// every #4655 check. The vanilla-measured 256 ceiling on the
    /// compressor parameter rejects it before any allocation.
    #[test]
    fn decode_spline_animation_rejects_a_block_claiming_its_whole_clip() {
        use crate::packfile::fixtures::PackfileBuilder;

        let mut data = vec![0u8; 0x60];
        data[0x10..0x14].copy_from_slice(&5u32.to_le_bytes()); // spline-compressed
        data[0x14..0x18].copy_from_slice(&1.0f32.to_le_bytes()); // duration
        data[0x18..0x1c].copy_from_slice(&4096u32.to_le_bytes()); // transform_count (max)
        data[0x38..0x3c].copy_from_slice(&3906u32.to_le_bytes()); // num_frames
        data[0x3c..0x40].copy_from_slice(&1u32.to_le_bytes()); // num_blocks
        data[0x40..0x44].copy_from_slice(&3907u32.to_le_bytes()); // mfpb = num_frames + 1
        data[0x50..0x54].copy_from_slice(&(1.0f32 / 30.0).to_le_bytes()); // frame_duration

        let mut builder = PackfileBuilder {
            data,
            ..Default::default()
        };
        let class = builder.class("hkaSplineCompressedAnimation");
        builder.virtual_fixups.push((0, 0, class));
        let bytes = builder.build();

        // 1 block x 3906 declared frames satisfies #4655's
        // `num_frames <= num_blocks * (mfpb - 1) + 1` exactly, and the
        // sample product sits just under MAX_TRANSFORM_SAMPLES — only the
        // MAX_FRAMES_PER_BLOCK ceiling can reject it.
        let err = decode_spline_animation(&bytes).expect_err(
            "max_frames_per_block beyond the vanilla ceiling must be rejected (#5006)",
        );
        assert_eq!(
            err,
            HkxError::InvalidData("unsupported spline clip dimensions")
        );

        // The 99-track skeleton-binding form (blocks=40, mfpb=4096,
        // frames=161,616 → 15,999,984 samples from a 16 KB file) dies on
        // the same ceiling.
        let mut data = vec![0u8; 0x60];
        data[0x10..0x14].copy_from_slice(&5u32.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&1.0f32.to_le_bytes());
        data[0x18..0x1c].copy_from_slice(&99u32.to_le_bytes());
        data[0x38..0x3c].copy_from_slice(&161_616u32.to_le_bytes());
        data[0x3c..0x40].copy_from_slice(&40u32.to_le_bytes());
        data[0x40..0x44].copy_from_slice(&4096u32.to_le_bytes());
        data[0x50..0x54].copy_from_slice(&(1.0f32 / 30.0).to_le_bytes());
        let mut builder = PackfileBuilder {
            data,
            ..Default::default()
        };
        let class = builder.class("hkaSplineCompressedAnimation");
        builder.virtual_fixups.push((0, 0, class));
        let err = decode_spline_animation(&builder.build())
            .expect_err("the 99-track mfpb=4096 form must be rejected on the same ceiling");
        assert_eq!(
            err,
            HkxError::InvalidData("unsupported spline clip dimensions")
        );
    }

    /// A one-transform-track spline clip in the 64-bit (SE) layout that
    /// fully decodes: one block whose mask table (4 bytes for the track,
    /// then one byte per float track, then padding) occupies `mask_size`
    /// bytes, followed at the next 4-byte boundary by a static translation.
    /// Float-track data is empty — the decoder never reads it.
    fn synthetic_static_clip(float_count: u32, mask_size: u32, translation: [f32; 3]) -> Vec<u8> {
        let mut curves = Vec::new();
        for component in translation {
            curves.extend_from_slice(&component.to_le_bytes());
        }
        // 8-bit scalars; static x/y/z translation, identity rotation/scale.
        synthetic_clip(float_count, mask_size, &[[0, 0x07, 0, 0]], &curves)
    }

    /// The general form of [`synthetic_static_clip`]: one block, one track
    /// per `masks` entry (quantization, translation, rotation, scale), and
    /// `curves` — every track's curve bytes, laid out by the caller exactly
    /// as the decoder walks them — starting at the 4-byte boundary after
    /// the `mask_size`-byte mask table.
    fn synthetic_clip(
        float_count: u32,
        mask_size: u32,
        masks: &[[u8; 4]],
        curves: &[u8],
    ) -> Vec<u8> {
        use crate::packfile::fixtures::PackfileBuilder;

        const CLIP: usize = 0;
        const BINDING: usize = 0x100;
        const BLOCKS: usize = 0x140;
        const FLOAT_BLOCKS: usize = 0x148;
        const SPLINE: usize = 0x150;
        let layout = Layout::new(8);
        assert!(
            layout.spline_data.count + 4 <= BINDING,
            "clip object overlaps the binding"
        );

        let mut spline: Vec<u8> = masks.iter().flatten().copied().collect();
        spline.resize(spline.len() + float_count as usize, 0); // float-track masks
        spline.resize(pad(mask_size as usize, 4), 0); // padding up to the curve data
        spline.extend_from_slice(curves);
        let float_start = spline.len() as u32;

        let mut data = vec![0u8; SPLINE + spline.len()];
        let put_u32 = |data: &mut Vec<u8>, at: usize, value: u32| {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put_u32(&mut data, CLIP + layout.animation_type, 5); // spline-compressed
        data[CLIP + layout.animation_duration..CLIP + layout.animation_duration + 4]
            .copy_from_slice(&(1.0f32 / 30.0).to_le_bytes());
        put_u32(
            &mut data,
            CLIP + layout.transform_tracks,
            masks.len() as u32,
        );
        put_u32(&mut data, CLIP + layout.float_tracks, float_count);
        put_u32(&mut data, CLIP + layout.num_frames, 2);
        put_u32(&mut data, CLIP + layout.num_blocks, 1);
        put_u32(&mut data, CLIP + layout.max_frames_per_block, 2);
        put_u32(&mut data, CLIP + layout.mask_size, mask_size);
        data[CLIP + layout.frame_duration..CLIP + layout.frame_duration + 4]
            .copy_from_slice(&(1.0f32 / 30.0).to_le_bytes());
        put_u32(&mut data, CLIP + layout.block_offsets.count, 1);
        put_u32(&mut data, CLIP + layout.float_block_offsets.count, 1);
        put_u32(
            &mut data,
            CLIP + layout.spline_data.count,
            spline.len() as u32,
        );
        put_u32(&mut data, BLOCKS, 0);
        put_u32(&mut data, FLOAT_BLOCKS, float_start);
        data[SPLINE..].copy_from_slice(&spline);

        let mut local = vec![
            ((CLIP + layout.block_offsets.pointer) as u32, BLOCKS as u32),
            (
                (CLIP + layout.float_block_offsets.pointer) as u32,
                FLOAT_BLOCKS as u32,
            ),
            ((CLIP + layout.spline_data.pointer) as u32, SPLINE as u32),
        ];
        local.sort_unstable();
        let mut builder = PackfileBuilder {
            data,
            local,
            ..Default::default()
        };
        let clip_class = builder.class("hkaSplineCompressedAnimation");
        let binding_class = builder.class("hkaAnimationBinding");
        builder.virtual_fixups.push((CLIP as u32, 0, clip_class));
        builder
            .virtual_fixups
            .push((BINDING as u32, 0, binding_class));
        builder.build()
    }

    /// #5659 (PAR-D1-2026-10-12-02) — Havok writes `mask_size` padded to 4
    /// bytes. A clip with one float track (the Falmer shape: 4·tc + 1,
    /// stored rounded up) must decode, and the curve data after the padding
    /// must land byte-exactly — the static translation proves the cursor
    /// starts at the padded boundary. The unpadded equality rejected 229
    /// vanilla SE clips with exactly this shape.
    #[test]
    fn decode_spline_animation_accepts_a_padded_mask_table() {
        let translation = [1.5, -2.0, 3.25];
        for float_count in [1u32, 2, 3, 4, 5] {
            let padded = pad(4 + float_count as usize, 4) as u32;
            let clip =
                decode_spline_animation(&synthetic_static_clip(float_count, padded, translation))
                    .unwrap_or_else(|e| panic!("fc={float_count}, mask_size={padded}: {e:?}"));
            assert_eq!(clip.tracks.len(), 1);
            assert_eq!(clip.track_to_bone, vec![0]);
            for pose in &clip.tracks[0] {
                assert_eq!(pose.translation, translation, "fc={float_count}");
                assert_eq!(pose.rotation, [0.0, 0.0, 0.0, 1.0]);
                assert_eq!(pose.scale, [1.0; 3]);
            }
        }
    }

    /// #5659 — the gate stays an exact tie to the file's track counts, so
    /// #5006's file-bytes argument still holds: neither the unpadded value
    /// (never written by Havok when it differs) nor an over-padded one is
    /// accepted.
    #[test]
    fn decode_spline_animation_rejects_a_mask_size_off_the_padded_value() {
        for mask_size in [5u32, 12] {
            let err = decode_spline_animation(&synthetic_static_clip(1, mask_size, [0.0; 3]))
                .expect_err("mask_size must equal the padded track-mask size");
            assert_eq!(
                err,
                HkxError::InvalidData("unsupported spline clip dimensions"),
                "mask_size={mask_size}"
            );
        }
    }

    /// Curve bytes for a translation whose X component is dynamic with the
    /// given bounds: spline header (2 control points, degree 1, 4 knots),
    /// padding, min/max, two 8-bit control points, padding — the vanilla
    /// `AnimObjectR` shape when `bounds` are NaN.
    fn dynamic_x_translation(bounds: (f32, f32)) -> Vec<u8> {
        let mut curve = vec![1, 0, 1, 0, 0, 1, 1, 0]; // last=1, degree=1, knots, pad
        curve.extend_from_slice(&bounds.0.to_le_bytes());
        curve.extend_from_slice(&bounds.1.to_le_bytes());
        curve.extend_from_slice(&[0, 255, 0, 0]); // control points, pad
        curve
    }

    /// #5662 (PAR-D2-2026-10-12-02) — vanilla Skyrim stores NaN translation
    /// bounds on one prop-attachment bone in 59 SE clips whose other tracks
    /// are valid. The decoder must drop that one track (reporting its bone)
    /// and keep the clip, and the tracks after it must still land, which
    /// proves the NaN curve's bytes are consumed in full.
    #[test]
    fn non_finite_translation_bounds_drop_only_that_track() {
        let finite = [1.0f32, 2.0, 3.0];
        let mut curves = Vec::new();
        curves.extend_from_slice(&dynamic_x_translation((f32::NAN, f32::NAN)));
        for component in finite {
            curves.extend_from_slice(&component.to_le_bytes());
        }
        let clip = decode_spline_animation(&synthetic_clip(
            0,
            8,
            &[[0, 0x10, 0, 0], [0, 0x07, 0, 0]],
            &curves,
        ))
        .expect("one bad prop-bone channel must not reject the clip");
        assert_eq!(clip.dropped_bones, vec![0]);
        assert_eq!(
            clip.track_to_bone,
            vec![1],
            "the valid track keeps its binding"
        );
        assert_eq!(clip.tracks.len(), 1);
        assert!(clip.tracks[0].iter().all(|pose| pose.translation == finite));

        // A static NaN component is the same class and gets the same policy.
        let mut curves = Vec::new();
        for component in finite {
            curves.extend_from_slice(&component.to_le_bytes());
        }
        for component in [0.5f32, f32::INFINITY, 0.5] {
            curves.extend_from_slice(&component.to_le_bytes());
        }
        let clip = decode_spline_animation(&synthetic_clip(
            0,
            8,
            &[[0, 0x07, 0, 0], [0, 0x07, 0, 0]],
            &curves,
        ))
        .expect("a static non-finite scalar drops its track, not the clip");
        assert_eq!(clip.dropped_bones, vec![1]);
        assert_eq!(clip.track_to_bone, vec![0]);
    }

    /// #5662 — a clip with nothing usable is still an error, and finite but
    /// inverted bounds stay a format error rather than a dropped track.
    #[test]
    fn non_finite_scalars_on_every_track_or_inverted_bounds_still_fail() {
        let all_bad = decode_spline_animation(&synthetic_clip(
            0,
            4,
            &[[0, 0x10, 0, 0]],
            &dynamic_x_translation((f32::NAN, f32::NAN)),
        ))
        .expect_err("a clip whose every track is non-finite has nothing to keep");
        assert_eq!(
            all_bad,
            HkxError::InvalidData("every transform track stores a non-finite spline scalar")
        );
        let inverted = decode_spline_animation(&synthetic_clip(
            0,
            4,
            &[[0, 0x10, 0, 0]],
            &dynamic_x_translation((2.0, 1.0)),
        ))
        .expect_err("min > max is a malformed curve");
        assert_eq!(
            inverted,
            HkxError::InvalidData("invalid spline scalar bounds")
        );
    }

    /// #5691 — #5662's gate covered the stored min/max only. Finite bounds
    /// whose *span* overflows f32 (`-3.0e38 .. 3.0e38` — both pass
    /// `is_finite`, `min <= max`) made `min + quantized * (max - min)`
    /// evaluate to NaN (quantized 0) and ±inf (quantized 1), so a hostile
    /// or corrupt mod clip decoded `Ok` with non-finite samples. The span
    /// check now flags the track — same drop-the-track policy as NaN
    /// bounds — and the all-bad clip still has nothing to keep.
    #[test]
    fn overflowing_finite_span_drops_only_that_track() {
        let finite = [1.0f32, 2.0, 3.0];
        let mut curves = Vec::new();
        curves.extend_from_slice(&dynamic_x_translation((-3.0e38, 3.0e38)));
        for component in finite {
            curves.extend_from_slice(&component.to_le_bytes());
        }
        let clip = decode_spline_animation(&synthetic_clip(
            0,
            8,
            &[[0, 0x10, 0, 0], [0, 0x07, 0, 0]],
            &curves,
        ))
        .expect("an overflowing span drops the track, not the clip");
        assert_eq!(clip.dropped_bones, vec![0]);
        assert_eq!(
            clip.track_to_bone,
            vec![1],
            "the valid track keeps its binding"
        );
        assert_eq!(clip.tracks.len(), 1);
        assert!(clip.tracks[0]
            .iter()
            .all(|pose| pose.translation.iter().all(|v| v.is_finite())));

        let all_bad = decode_spline_animation(&synthetic_clip(
            0,
            4,
            &[[0, 0x10, 0, 0]],
            &dynamic_x_translation((-3.0e38, 3.0e38)),
        ))
        .expect_err("a clip whose every track overflows the span has nothing to keep");
        assert_eq!(
            all_bad,
            HkxError::InvalidData("every transform track stores a non-finite spline scalar")
        );
    }

    /// #3018 (SCR-D8-2026-08-16-03) — an out-of-range annotation timestamp
    /// used to hard-fail the whole clip decode via `read_annotations`'s `?`
    /// propagation, discarding every transform track over one bad piece of
    /// metadata. Pins the fixed policy: skip just the offending annotation
    /// and keep the rest, using the same clamp-on-accept semantics the
    /// accepted-range path already had.
    #[test]
    fn read_annotations_skips_an_out_of_range_time_and_keeps_the_rest() {
        use crate::packfile::fixtures::PackfileBuilder;
        use crate::packfile::Packfile;

        const TRACKS: usize = 0x40;
        const ANNOTATIONS: usize = 0x60;
        const ANNOTATION_SIZE: usize = 0x10;
        const TEXT_BAD: usize = 0x90;
        const TEXT_GOOD: usize = 0x98;

        // ann[0]'s time is out of range (negative) and must be skipped, not
        // fail the clip; ann[1]'s is in range and must survive, clamped to
        // `duration` as before.
        let mut data = vec![0u8; 0xa0];
        data[0x30..0x34].copy_from_slice(&1u32.to_le_bytes()); // annotation-track count
        data[TRACKS + 0x10..TRACKS + 0x14].copy_from_slice(&2u32.to_le_bytes()); // annotation count
        data[ANNOTATIONS..ANNOTATIONS + 4].copy_from_slice(&(-5.0f32).to_le_bytes()); // ann[0].time
        data[ANNOTATIONS + ANNOTATION_SIZE..ANNOTATIONS + ANNOTATION_SIZE + 4]
            .copy_from_slice(&0.1f32.to_le_bytes()); // ann[1].time
        data[TEXT_BAD..TEXT_BAD + 4].copy_from_slice(b"Bad\0");
        data[TEXT_GOOD..TEXT_GOOD + 5].copy_from_slice(b"Good\0");

        let builder = PackfileBuilder {
            data,
            local: vec![
                (0x28, TRACKS as u32),
                ((TRACKS + 0x08) as u32, ANNOTATIONS as u32),
                ((ANNOTATIONS + 0x08) as u32, TEXT_BAD as u32),
                (
                    (ANNOTATIONS + ANNOTATION_SIZE + 0x08) as u32,
                    TEXT_GOOD as u32,
                ),
            ],
            ..Default::default()
        };
        let bytes = builder.build();
        let pack = Packfile::parse(&bytes).unwrap();

        // The fixture uses Skyrim SE's 64-bit annotation layout.
        let annotations = read_annotations(&pack, &Layout::new(8), 0, 1.0).expect(
            "an out-of-range annotation must be skipped, not fail the whole decode (#3018)",
        );
        assert_eq!(
            annotations,
            vec![HkxAnnotation {
                track_name: String::new(),
                time: 0.1,
                text: "Good".to_string(),
            }],
            "only the in-range annotation should survive, clamped as before"
        );
    }

    #[test]
    fn bspline_linear_curve_hits_knots_and_midpoint() {
        let values = [0.0, 10.0];
        let knots = [0, 0, 1, 1];
        assert!((sample_bspline(&values, 1, &knots, 0.0).unwrap() - 0.0).abs() < 1e-6);
        assert!((sample_bspline(&values, 1, &knots, 0.5).unwrap() - 5.0).abs() < 1e-6);
        assert!((sample_bspline(&values, 1, &knots, 1.0).unwrap() - 10.0).abs() < 1e-6);
    }

    /// Fallout 4 ships Havok 2014 packfiles (version 11, a predicate area before the
    /// section table, spline type 3, 48-bit rotations): the player's skeleton and
    /// the one-handed walk decode to a consistent rig and clip.
    /// `cargo test -p byroredux-hkx -- --ignored`
    #[test]
    #[ignore = "needs Fallout 4 game data on disk"]
    fn fallout_4_skeleton_and_clip_decode() {
        let env = byroredux_plugin::esm::test_paths::FO4_ENV;
        let dir = std::env::var(env)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| byroredux_plugin::esm::test_paths::FO4_DEFAULT.to_owned());
        let path = std::path::Path::new(&dir).join("Fallout4 - Animations.ba2");
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
            }
            eprintln!("SKIP: {} not found (no game data?)", path.display());
            return;
        }
        let archive = byroredux_bsa::Ba2Archive::open(path).unwrap();
        let skeleton = decode_skeleton(
            &archive.extract(r"meshes\actors\character\characterassets\skeleton.hkx").unwrap(),
        )
        .unwrap();
        eprintln!("skeleton {:?}: {} bones, first {:?}", skeleton.name, skeleton.bones.len(), skeleton.bones.iter().take(4).map(|b| &b.name).collect::<Vec<_>>());
        assert!(skeleton.bones.len() > 50);
        let clip = decode_spline_animation(
            &archive.extract(r"meshes\actors\character\animations\1hm\walkforward.hkx").unwrap(),
        )
        .unwrap();
        eprintln!("clip: {} s, {} frames, {} tracks", clip.duration, clip.num_frames, clip.tracks.len());
        assert!(clip.duration > 0.1 && clip.num_frames > 2);
        assert_eq!(clip.tracks.len(), clip.track_to_bone.len());
        assert!(clip.track_to_bone.iter().all(|&bone| usize::from(bone) < skeleton.bones.len()));
        for track in &clip.tracks {
            assert_eq!(track.len(), clip.num_frames as usize);
            for pose in track {
                let q = pose.rotation;
                let length = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                assert!((length - 1.0).abs() < 1e-3, "{q:?}");
                assert!(pose.translation.iter().all(|v| v.is_finite()));
            }
        }
    }
}
