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
}

/// Decode the first `hkaSkeleton` in a Skyrim-era 64-bit packfile.
pub fn decode_skeleton(bytes: &[u8]) -> Result<HkxSkeleton> {
    let pack = Packfile::parse(bytes)?;
    let object = pack.object("hkaSkeleton")?;
    let name = pack
        .local_target(object + 0x10)
        .map(|offset| pack.cstr(offset, "skeleton name"))
        .transpose()?
        .unwrap_or_default()
        .to_owned();

    let bone_count = pack.u32(object + 0x30, "skeleton bone count")? as usize;
    let parent_count = pack.u32(object + 0x20, "skeleton parent count")? as usize;
    let pose_count = pack.u32(object + 0x40, "skeleton pose count")? as usize;
    if bone_count == 0
        || bone_count > 4096
        || parent_count != bone_count
        || pose_count != bone_count
    {
        return Err(HkxError::InvalidData(
            "skeleton bone/parent/reference-pose counts disagree",
        ));
    }
    let parents_offset = pack
        .local_target(object + 0x18)
        .ok_or(HkxError::InvalidData("skeleton parent array is unbound"))?;
    let bones_offset = pack
        .local_target(object + 0x28)
        .ok_or(HkxError::InvalidData("skeleton bone array is unbound"))?;
    let poses_offset = pack
        .local_target(object + 0x38)
        .ok_or(HkxError::InvalidData("skeleton pose array is unbound"))?;
    let parents = pack.data_slice(parents_offset, bone_count * 2, "skeleton parents")?;
    let poses = pack.data_slice(poses_offset, bone_count * 48, "skeleton poses")?;

    let mut bones = Vec::with_capacity(bone_count);
    for index in 0..bone_count {
        let name_offset = pack
            .local_target(bones_offset + index * 16)
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
    let object = pack.object("hkaSplineCompressedAnimation")?;
    let animation_type = pack.u32(object + 0x10, "animation type")?;
    if animation_type != 5 {
        return Err(HkxError::InvalidData("animation is not spline-compressed"));
    }
    let duration = pack.f32(object + 0x14, "animation duration")?;
    let transform_count = pack.u32(object + 0x18, "transform-track count")? as usize;
    let float_count = pack.u32(object + 0x1c, "float-track count")? as usize;
    let num_frames = pack.u32(object + 0x38, "animation frame count")?;
    let num_blocks = pack.u32(object + 0x3c, "animation block count")? as usize;
    let max_frames_per_block = pack.u32(object + 0x40, "maximum frames per block")?;
    let mask_size = pack.u32(object + 0x44, "mask table size")? as usize;
    let frame_duration = pack.f32(object + 0x50, "animation frame duration")?;
    if !duration.is_finite()
        || duration < 0.0
        || !frame_duration.is_finite()
        || frame_duration <= 0.0
        || transform_count == 0
        || transform_count > 4096
        || float_count > 4096
        || num_frames == 0
        || num_blocks == 0
        || num_blocks > 4096
        || max_frames_per_block < 2
        || mask_size != transform_count * 4 + float_count
    {
        return Err(HkxError::InvalidData("unsupported spline clip dimensions"));
    }
    let block_offsets =
        read_pack_u32_array(&pack, object + 0x58, object + 0x60, "block-offset array")?;
    let float_block_offsets = read_pack_u32_array(
        &pack,
        object + 0x68,
        object + 0x70,
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
        .local_target(object + 0x98)
        .ok_or(HkxError::InvalidData("spline data array is unbound"))?;
    let data_len = pack.u32(object + 0xa0, "spline data size")? as usize;
    let data = pack.data_slice(data_offset, data_len, "spline data")?;
    let mut blocks = Vec::with_capacity(num_blocks);
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

            let translation = read_vector_curve(
                data,
                &mut cursor,
                translation_mask,
                scalar_quantization,
                [0.0; 3],
            )?;
            let rotation =
                read_quaternion_curve(data, &mut cursor, rotation_mask, rotation_quantization)?;
            let scale =
                read_vector_curve(data, &mut cursor, scale_mask, scale_quantization, [1.0; 3])?;
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

    let block_stride = max_frames_per_block - 1;
    let mut tracks = vec![Vec::with_capacity(num_frames as usize); transform_count];
    for frame in 0..num_frames {
        let block_index = ((frame / block_stride) as usize).min(num_blocks - 1);
        let first_frame = block_index as u32 * block_stride;
        let local_frame = frame.saturating_sub(first_frame) as f32;
        for (track_index, track) in blocks[block_index].iter().enumerate() {
            tracks[track_index].push(track.sample(local_frame)?);
        }
    }

    let binding = pack.object("hkaAnimationBinding")?;
    let binding_count = pack.u32(binding + 0x28, "transform binding count")? as usize;
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
            .local_target(binding + 0x20)
            .ok_or(HkxError::InvalidData("transform binding array is unbound"))?;
        let raw = pack.data_slice(offset, binding_count * 2, "transform bindings")?;
        raw.chunks_exact(2)
            .map(|bytes| Ok(u16::from_le_bytes(bytes.try_into().unwrap())))
            .collect::<Result<Vec<_>>>()?
    };

    Ok(HkxAnimation {
        duration,
        num_frames,
        frame_duration,
        tracks,
        track_to_bone,
    })
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
    pointer_field: usize,
    count_field: usize,
    label: &'static str,
) -> Result<Vec<u32>> {
    let count = pack.u32(count_field, label)? as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let offset = pack
        .local_target(pointer_field)
        .ok_or(HkxError::InvalidData("spline offset array is unbound"))?;
    let byte_count = count
        .checked_mul(4)
        .ok_or(HkxError::InvalidData("spline offset array size overflow"))?;
    let raw = pack.data_slice(offset, byte_count, label)?;
    Ok(raw
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect())
}

fn read_vector_curve(
    data: &[u8],
    cursor: &mut usize,
    mask: u8,
    quantization: u8,
    identity: [f32; 3],
) -> Result<VectorCurve> {
    validate_vector_mask(mask)?;
    *cursor = align_up(*cursor, 4)?;
    if mask & 0x70 == 0 {
        return read_static_vector(data, cursor, mask, identity).map(VectorCurve::Static);
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
            if !min.is_finite() || !max.is_finite() || min > max {
                return Err(HkxError::InvalidData("invalid spline scalar bounds"));
            }
            bounds[component] = (min, max);
            components[component] = ScalarCurve::Dynamic(Vec::with_capacity(control_count));
        } else if mask & static_bit != 0 {
            components[component] =
                ScalarCurve::Constant(read_f32(data, cursor, "static spline scalar")?);
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
    if degree == 0 || degree > 8 || control_count <= degree || control_count > 4096 {
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
) -> Result<[f32; 3]> {
    *cursor = align_up(*cursor, 4)?;
    let mut value = identity;
    for (component, output) in value.iter_mut().enumerate() {
        if mask & (1 << component) != 0 {
            let bytes = take(data, cursor, 4, "static spline scalar")?;
            *output = f32::from_le_bytes(bytes.try_into().unwrap());
            if !output.is_finite() {
                return Err(HkxError::InvalidData("non-finite spline scalar"));
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

fn read_f32(data: &[u8], cursor: &mut usize, label: &'static str) -> Result<f32> {
    let bytes = take(data, cursor, 4, label)?;
    let value = f32::from_le_bytes(bytes.try_into().unwrap());
    if !value.is_finite() {
        return Err(HkxError::InvalidData("non-finite spline scalar"));
    }
    Ok(value)
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

    #[test]
    fn skyrim_cart_player_idle_decodes_when_assets_are_available() {
        let data_dir = std::env::var_os("BYROREDUX_SKYRIM_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        let archive_path = data_dir.join("Skyrim - Animations.bsa");
        if !archive_path.is_file() {
            return;
        }
        let archive = byroredux_bsa::BsaArchive::open(archive_path).unwrap();
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

        for path in [
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
        ] {
            let bytes = archive.extract(path).unwrap();
            let dynamic = decode_spline_animation(&bytes)
                .unwrap_or_else(|error| panic!("failed to decode {path}: {error}"));
            assert_eq!(dynamic.tracks.len(), 99, "{path}");
            assert!(dynamic.num_frames > 1, "{path}");
            assert!(dynamic
                .tracks
                .iter()
                .all(|track| track.len() == dynamic.num_frames as usize));
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

    #[test]
    fn bspline_linear_curve_hits_knots_and_midpoint() {
        let values = [0.0, 10.0];
        let knots = [0, 0, 1, 1];
        assert!((sample_bspline(&values, 1, &knots, 0.0).unwrap() - 0.0).abs() < 1e-6);
        assert!((sample_bspline(&values, 1, &knots, 0.5).unwrap() - 5.0).abs() < 1e-6);
        assert!((sample_bspline(&values, 1, &knots, 1.0).unwrap() - 10.0).abs() < 1e-6);
    }
}
