//! Animation interpolation engine, clip registry, and AnimationPlayer component.
//!
//! Provides keyframe sampling with linear, Hermite (quadratic), and TBC
//! (Kochanek-Bartels) interpolation for position, rotation, and scale channels.

use crate::ecs::resource::Resource;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::{Quat, Vec3};
use std::collections::HashMap;

// ── Re-export NIF animation types ─────────────────────────────────────
// These types are defined in byroredux-nif but we mirror the essential
// ones here so core doesn't depend on nif. The binary crate converts.

/// How the animation behaves when it reaches its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    Clamp,
    Loop,
    Reverse,
}

/// Interpolation type for keyframe data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Linear,
    Quadratic,
    Tbc,
}

/// Translation keyframe.
#[derive(Debug, Clone, Copy)]
pub struct TranslationKey {
    pub time: f32,
    pub value: Vec3,
    pub forward: Vec3,
    pub backward: Vec3,
    pub tbc: Option<[f32; 3]>,
}

/// Rotation keyframe (quaternion).
#[derive(Debug, Clone, Copy)]
pub struct RotationKey {
    pub time: f32,
    pub value: Quat,
    pub tbc: Option<[f32; 3]>,
}

/// Scale keyframe.
#[derive(Debug, Clone, Copy)]
pub struct ScaleKey {
    pub time: f32,
    pub value: f32,
    pub forward: f32,
    pub backward: f32,
    pub tbc: Option<[f32; 3]>,
}

/// A single channel of transform animation for one named node.
#[derive(Debug, Clone)]
pub struct TransformChannel {
    pub translation_keys: Vec<TranslationKey>,
    pub translation_type: KeyType,
    pub rotation_keys: Vec<RotationKey>,
    pub rotation_type: KeyType,
    pub scale_keys: Vec<ScaleKey>,
    pub scale_type: KeyType,
    /// Per-channel priority from ControlledBlock (0 = lowest, 255 = highest).
    pub priority: u8,
}

// ── Non-transform channel types ───────────────────────────────────────

/// What a float channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTarget {
    Alpha,
    UvOffsetU,
    UvOffsetV,
    UvScaleU,
    UvScaleV,
    UvRotation,
    ShaderFloat,
}

/// What a color channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTarget {
    Diffuse,
    Ambient,
    Specular,
    Emissive,
    ShaderColor,
}

/// A float keyframe (alpha, UV params, shader floats).
#[derive(Debug, Clone, Copy)]
pub struct AnimFloatKey {
    pub time: f32,
    pub value: f32,
}

/// A color keyframe (RGB).
#[derive(Debug, Clone, Copy)]
pub struct AnimColorKey {
    pub time: f32,
    pub value: Vec3,
}

/// A bool keyframe (visibility).
#[derive(Debug, Clone, Copy)]
pub struct AnimBoolKey {
    pub time: f32,
    pub value: bool,
}

/// Float animation channel.
#[derive(Debug, Clone)]
pub struct FloatChannel {
    pub target: FloatTarget,
    pub keys: Vec<AnimFloatKey>,
}

/// Color animation channel.
#[derive(Debug, Clone)]
pub struct ColorChannel {
    pub target: ColorTarget,
    pub keys: Vec<AnimColorKey>,
}

/// Visibility animation channel.
#[derive(Debug, Clone)]
pub struct BoolChannel {
    pub keys: Vec<AnimBoolKey>,
}

/// A complete animation clip (one per NiControllerSequence).
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    /// Default weight from NiControllerSequence (0.0–1.0).
    pub weight: f32,
    /// Map from node name to its transform animation channel.
    pub channels: HashMap<String, TransformChannel>,
    /// Float channels: (node_name, channel).
    pub float_channels: Vec<(String, FloatChannel)>,
    /// Color channels: (node_name, channel).
    pub color_channels: Vec<(String, ColorChannel)>,
    /// Bool channels: (node_name, channel).
    pub bool_channels: Vec<(String, BoolChannel)>,
}

// ── AnimationClipRegistry (Resource) ──────────────────────────────────

/// Shared registry of loaded animation clips, indexed by handle.
pub struct AnimationClipRegistry {
    clips: Vec<AnimationClip>,
}

impl Resource for AnimationClipRegistry {}

impl AnimationClipRegistry {
    pub fn new() -> Self {
        Self { clips: Vec::new() }
    }

    pub fn add(&mut self, clip: AnimationClip) -> u32 {
        let handle = self.clips.len() as u32;
        self.clips.push(clip);
        handle
    }

    pub fn get(&self, handle: u32) -> Option<&AnimationClip> {
        self.clips.get(handle as usize)
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

impl Default for AnimationClipRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── AnimationPlayer (Component) ───────────────────────────────────────

/// ECS component that drives animation playback on an entity subtree.
///
/// Attached to the root entity of an animated mesh. The animation system
/// uses the clip's channel map to find child entities by `Name` and
/// update their `Transform` each frame.
pub struct AnimationPlayer {
    pub clip_handle: u32,
    pub local_time: f32,
    pub playing: bool,
    pub speed: f32,
    /// Tracks ping-pong direction for CycleType::Reverse.
    pub reverse_direction: bool,
}

impl AnimationPlayer {
    pub fn new(clip_handle: u32) -> Self {
        Self {
            clip_handle,
            local_time: 0.0,
            playing: true,
            speed: 1.0,
            reverse_direction: false,
        }
    }
}

impl Component for AnimationPlayer {
    type Storage = SparseSetStorage<Self>;
}

// ── Animation Stack (multi-layer blending) ────────────────────────────

/// A single animation layer in an AnimationStack.
#[derive(Debug, Clone)]
pub struct AnimationLayer {
    pub clip_handle: u32,
    pub local_time: f32,
    pub playing: bool,
    pub speed: f32,
    /// Blend weight (0.0–1.0). Used for cross-fade blending between layers.
    pub weight: f32,
    /// Tracks ping-pong direction for CycleType::Reverse.
    pub reverse_direction: bool,
    /// When > 0, this layer is blending in: weight increases from 0 → target over this duration.
    pub blend_in_remaining: f32,
    /// Total blend-in duration (for computing interpolation progress).
    pub blend_in_total: f32,
    /// When > 0, this layer is blending out: weight decreases to 0 over this duration.
    pub blend_out_remaining: f32,
    /// Total blend-out duration.
    pub blend_out_total: f32,
}

impl AnimationLayer {
    pub fn new(clip_handle: u32) -> Self {
        Self {
            clip_handle,
            local_time: 0.0,
            playing: true,
            speed: 1.0,
            weight: 1.0,
            reverse_direction: false,
            blend_in_remaining: 0.0,
            blend_in_total: 0.0,
            blend_out_remaining: 0.0,
            blend_out_total: 0.0,
        }
    }

    /// Create a layer that blends in over `blend_time` seconds.
    pub fn with_blend_in(mut self, blend_time: f32) -> Self {
        self.blend_in_remaining = blend_time;
        self.blend_in_total = blend_time;
        self.weight = 0.0; // Starts at zero, ramps up.
        self
    }

    /// Compute the effective weight after blend-in/out modulation.
    pub fn effective_weight(&self) -> f32 {
        let mut w = self.weight;
        if self.blend_in_total > 0.0 && self.blend_in_remaining > 0.0 {
            let progress = 1.0 - (self.blend_in_remaining / self.blend_in_total);
            w *= progress;
        }
        if self.blend_out_total > 0.0 && self.blend_out_remaining > 0.0 {
            let progress = self.blend_out_remaining / self.blend_out_total;
            w *= progress;
        }
        w
    }
}

/// Multi-layer animation stack. Replaces AnimationPlayer for blended playback.
///
/// Layers are ordered: index 0 is the base layer, higher indices overlay.
/// The system evaluates all layers and blends by weight. Within the same
/// priority level, weighted average is computed. Higher priority overrides lower.
pub struct AnimationStack {
    pub layers: Vec<AnimationLayer>,
}

impl AnimationStack {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Play a clip, optionally cross-fading from the current top layer.
    pub fn play(&mut self, clip_handle: u32, blend_time: f32) {
        // Fade out existing layers.
        if blend_time > 0.0 {
            for layer in &mut self.layers {
                if layer.blend_out_remaining <= 0.0 {
                    layer.blend_out_remaining = blend_time;
                    layer.blend_out_total = blend_time;
                }
            }
        } else {
            self.layers.clear();
        }

        // Add the new layer.
        let new_layer = if blend_time > 0.0 {
            AnimationLayer::new(clip_handle).with_blend_in(blend_time)
        } else {
            AnimationLayer::new(clip_handle)
        };
        self.layers.push(new_layer);
    }

    /// Remove layers whose blend-out has completed (effective weight ≈ 0).
    pub fn cleanup_finished(&mut self) {
        self.layers.retain(|layer| {
            if layer.blend_out_total > 0.0 && layer.blend_out_remaining <= 0.0 {
                return false; // Fully blended out.
            }
            true
        });
    }
}

impl Component for AnimationStack {
    type Storage = SparseSetStorage<Self>;
}

/// Advance all layers in a stack, handling blend-in/out timing.
pub fn advance_stack(
    stack: &mut AnimationStack,
    registry: &AnimationClipRegistry,
    dt: f32,
) {
    for layer in &mut stack.layers {
        if !layer.playing { continue; }

        let Some(clip) = registry.get(layer.clip_handle) else { continue };

        // Advance animation time.
        let delta = dt * layer.speed * clip.frequency;
        match clip.cycle_type {
            CycleType::Clamp => {
                layer.local_time = (layer.local_time + delta).min(clip.duration);
            }
            CycleType::Loop => {
                layer.local_time += delta;
                if clip.duration > 0.0 {
                    layer.local_time %= clip.duration;
                    if layer.local_time < 0.0 {
                        layer.local_time += clip.duration;
                    }
                }
            }
            CycleType::Reverse => {
                if layer.reverse_direction {
                    layer.local_time -= delta;
                    if layer.local_time <= 0.0 {
                        layer.local_time = -layer.local_time;
                        layer.reverse_direction = false;
                    }
                } else {
                    layer.local_time += delta;
                    if layer.local_time >= clip.duration {
                        layer.local_time = 2.0 * clip.duration - layer.local_time;
                        layer.reverse_direction = true;
                    }
                }
            }
        }

        // Advance blend timers.
        if layer.blend_in_remaining > 0.0 {
            layer.blend_in_remaining = (layer.blend_in_remaining - dt).max(0.0);
            if layer.blend_in_remaining <= 0.0 {
                // Blend-in complete — ensure full weight.
                layer.weight = layer.weight.max(1.0);
            }
        }
        if layer.blend_out_remaining > 0.0 {
            layer.blend_out_remaining = (layer.blend_out_remaining - dt).max(0.0);
        }
    }

    stack.cleanup_finished();
}

/// Sample a blended transform from all layers in a stack for a given node.
///
/// Layers with higher priority override lower. Within the same priority,
/// weighted average is used. Returns None if no layer has data for this node.
pub fn sample_blended_transform(
    stack: &AnimationStack,
    registry: &AnimationClipRegistry,
    channel_name: &str,
) -> Option<(Vec3, Quat, f32)> {
    // Collect (priority, weight, translation, rotation, scale) from each layer.
    let mut samples: Vec<(u8, f32, Vec3, Quat, f32)> = Vec::new();

    for layer in &stack.layers {
        let ew = layer.effective_weight();
        if ew < 0.001 { continue; }

        let Some(clip) = registry.get(layer.clip_handle) else { continue };
        let Some(channel) = clip.channels.get(channel_name) else { continue };

        let t = sample_translation(channel, layer.local_time);
        let r = sample_rotation(channel, layer.local_time);
        let s = sample_scale(channel, layer.local_time);

        if t.is_none() && r.is_none() && s.is_none() { continue; }

        samples.push((
            channel.priority,
            ew,
            t.unwrap_or(Vec3::ZERO),
            r.unwrap_or(Quat::IDENTITY),
            s.unwrap_or(1.0),
        ));
    }

    if samples.is_empty() { return None; }

    // Find the highest priority present.
    let max_priority = samples.iter().map(|s| s.0).max().unwrap();

    // Filter to only highest-priority samples and blend by weight.
    let top: Vec<_> = samples.iter().filter(|s| s.0 == max_priority).collect();

    if top.len() == 1 {
        return Some((top[0].2, top[0].3, top[0].4));
    }

    // Weighted blend.
    let total_weight: f32 = top.iter().map(|s| s.1).sum();
    if total_weight < 0.001 { return None; }

    let mut blended_pos = Vec3::ZERO;
    let mut blended_scale = 0.0f32;
    let mut blended_rot = top[0].3; // Start from first, slerp toward others.
    let mut accumulated_weight = 0.0f32;

    for sample in &top {
        let w = sample.1 / total_weight;
        blended_pos += sample.2 * w;
        blended_scale += sample.4 * w;

        // Incremental SLERP for rotation blending.
        if accumulated_weight < 0.001 {
            blended_rot = sample.3;
        } else {
            let t = w / (accumulated_weight + w);
            let q = sample.3;
            blended_rot = blended_rot.slerp(if blended_rot.dot(q) < 0.0 { -q } else { q }, t);
        }
        accumulated_weight += w;
    }

    Some((blended_pos, blended_rot, blended_scale))
}

// ── Keyframe interpolation ────────────────────────────────────────────

/// Binary search for the key pair bracketing `time`.
/// Returns (index_before, index_after, normalized_t).
fn find_key_pair(times: &[f32], time: f32) -> (usize, usize, f32) {
    if times.is_empty() {
        return (0, 0, 0.0);
    }
    if times.len() == 1 || time <= times[0] {
        return (0, 0, 0.0);
    }
    if time >= *times.last().unwrap() {
        let last = times.len() - 1;
        return (last, last, 0.0);
    }

    // Binary search for the interval
    let mut lo = 0;
    let mut hi = times.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if times[mid] <= time {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let dt = times[hi] - times[lo];
    let t = if dt > 0.0 {
        (time - times[lo]) / dt
    } else {
        0.0
    };
    (lo, hi, t)
}

/// Cubic Hermite interpolation: p(t) = h00*p0 + h10*m0 + h01*p1 + h11*m1
fn hermite(t: f32) -> (f32, f32, f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    (h00, h10, h01, h11)
}

/// Compute Kochanek-Bartels tangents for a key given its neighbors.
/// Returns (incoming_tangent, outgoing_tangent).
fn tbc_tangents_f32(
    prev: Option<(f32, f32)>, // (time, value)
    curr: (f32, f32),
    next: Option<(f32, f32)>,
    tension: f32,
    bias: f32,
    continuity: f32,
) -> (f32, f32) {
    let a = (1.0 - tension) * (1.0 + bias) * (1.0 + continuity) / 2.0;
    let b = (1.0 - tension) * (1.0 - bias) * (1.0 - continuity) / 2.0;
    let c = (1.0 - tension) * (1.0 + bias) * (1.0 - continuity) / 2.0;
    let d = (1.0 - tension) * (1.0 - bias) * (1.0 + continuity) / 2.0;

    let (d_in, d_out) = match (prev, next) {
        (Some(p), Some(n)) => {
            let incoming = a * (curr.1 - p.1) + b * (n.1 - curr.1);
            let outgoing = c * (curr.1 - p.1) + d * (n.1 - curr.1);
            (incoming, outgoing)
        }
        (Some(p), None) => {
            let diff = curr.1 - p.1;
            (diff, diff)
        }
        (None, Some(n)) => {
            let diff = n.1 - curr.1;
            (diff, diff)
        }
        (None, None) => (0.0, 0.0),
    };

    (d_in, d_out)
}

/// Sample translation at a given time.
pub fn sample_translation(channel: &TransformChannel, time: f32) -> Option<Vec3> {
    let keys = &channel.translation_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    let k0 = &keys[i0];
    let k1 = &keys[i1];

    match channel.translation_type {
        KeyType::Linear => Some(k0.value.lerp(k1.value, t)),
        KeyType::Quadratic => {
            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(Vec3::new(
                h00 * k0.value.x + h10 * k0.forward.x * dt + h01 * k1.value.x + h11 * k1.backward.x * dt,
                h00 * k0.value.y + h10 * k0.forward.y * dt + h01 * k1.value.y + h11 * k1.backward.y * dt,
                h00 * k0.value.z + h10 * k0.forward.z * dt + h01 * k1.value.z + h11 * k1.backward.z * dt,
            ))
        }
        KeyType::Tbc => {
            // For TBC, compute Hermite tangents from TBC parameters
            let prev = if i0 > 0 { Some(i0 - 1) } else { None };
            let next = if i1 + 1 < keys.len() { Some(i1) } else { None };

            let tbc0 = k0.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let mut m0 = Vec3::ZERO;
            for axis in 0..3 {
                let p = prev.map(|pi| (keys[pi].time, keys[pi].value[axis]));
                let c = (k0.time, k0.value[axis]);
                let n = Some((k1.time, k1.value[axis]));
                let (_, out) = tbc_tangents_f32(p, c, n, tbc0[0], tbc0[1], tbc0[2]);
                m0[axis] = out;
            }

            let tbc1 = k1.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let mut m1 = Vec3::ZERO;
            for axis in 0..3 {
                let p = Some((k0.time, k0.value[axis]));
                let c = (k1.time, k1.value[axis]);
                let n = next.map(|ni| (keys[ni].time, keys[ni].value[axis]));
                let (inc, _) = tbc_tangents_f32(p, c, n, tbc1[0], tbc1[1], tbc1[2]);
                m1[axis] = inc;
            }

            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(Vec3::new(
                h00 * k0.value.x + h10 * m0.x * dt + h01 * k1.value.x + h11 * m1.x * dt,
                h00 * k0.value.y + h10 * m0.y * dt + h01 * k1.value.y + h11 * m1.y * dt,
                h00 * k0.value.z + h10 * m0.z * dt + h01 * k1.value.z + h11 * m1.z * dt,
            ))
        }
    }
}

/// Sample rotation at a given time.
pub fn sample_rotation(channel: &TransformChannel, time: f32) -> Option<Quat> {
    let keys = &channel.rotation_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    // For all rotation key types, use SLERP between the two bracketing keys.
    // TBC rotation uses the same SLERP — TBC tangents affect timing but
    // quaternion interpolation is always spherical.
    let q0 = keys[i0].value;
    let q1 = keys[i1].value;

    // Ensure shortest path
    let q1 = if q0.dot(q1) < 0.0 { -q1 } else { q1 };
    Some(q0.slerp(q1, t))
}

/// Sample scale at a given time.
pub fn sample_scale(channel: &TransformChannel, time: f32) -> Option<f32> {
    let keys = &channel.scale_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    let k0 = &keys[i0];
    let k1 = &keys[i1];

    match channel.scale_type {
        KeyType::Linear => Some(k0.value + (k1.value - k0.value) * t),
        KeyType::Quadratic => {
            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(h00 * k0.value + h10 * k0.forward * dt + h01 * k1.value + h11 * k1.backward * dt)
        }
        KeyType::Tbc => {
            let prev = if i0 > 0 {
                Some((keys[i0 - 1].time, keys[i0 - 1].value))
            } else {
                None
            };
            let next = if i1 + 1 < keys.len() {
                Some((keys[i1 + 1].time, keys[i1 + 1].value))
            } else {
                None
            };

            let tbc0 = k0.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let (_, m0) = tbc_tangents_f32(
                prev,
                (k0.time, k0.value),
                Some((k1.time, k1.value)),
                tbc0[0], tbc0[1], tbc0[2],
            );

            let tbc1 = k1.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let (m1, _) = tbc_tangents_f32(
                Some((k0.time, k0.value)),
                (k1.time, k1.value),
                next,
                tbc1[0], tbc1[1], tbc1[2],
            );

            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(h00 * k0.value + h10 * m0 * dt + h01 * k1.value + h11 * m1 * dt)
        }
    }
}

/// Sample a float channel at a given time.
pub fn sample_float_channel(channel: &FloatChannel, time: f32) -> f32 {
    let keys = &channel.keys;
    if keys.is_empty() { return 0.0; }
    if keys.len() == 1 || time <= keys[0].time { return keys[0].value; }
    if time >= keys.last().unwrap().time { return keys.last().unwrap().value; }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 { return keys[i0].value; }
    keys[i0].value + (keys[i1].value - keys[i0].value) * t
}

/// Sample a color channel at a given time (linear interpolation).
pub fn sample_color_channel(channel: &ColorChannel, time: f32) -> Vec3 {
    let keys = &channel.keys;
    if keys.is_empty() { return Vec3::ONE; }
    if keys.len() == 1 || time <= keys[0].time { return keys[0].value; }
    if time >= keys.last().unwrap().time { return keys.last().unwrap().value; }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 { return keys[i0].value; }
    keys[i0].value.lerp(keys[i1].value, t)
}

/// Sample a bool channel at a given time (step — no interpolation).
pub fn sample_bool_channel(channel: &BoolChannel, time: f32) -> bool {
    let keys = &channel.keys;
    if keys.is_empty() { return true; }
    // Step function: use the last key whose time <= current time.
    let mut result = keys[0].value;
    for key in keys {
        if key.time <= time {
            result = key.value;
        } else {
            break;
        }
    }
    result
}

/// Advance the animation time according to the cycle type.
pub fn advance_time(
    player: &mut AnimationPlayer,
    clip: &AnimationClip,
    dt: f32,
) {
    if !player.playing {
        return;
    }

    let delta = dt * player.speed * clip.frequency;

    match clip.cycle_type {
        CycleType::Clamp => {
            player.local_time = (player.local_time + delta).min(clip.duration);
        }
        CycleType::Loop => {
            player.local_time += delta;
            if clip.duration > 0.0 {
                player.local_time %= clip.duration;
                if player.local_time < 0.0 {
                    player.local_time += clip.duration;
                }
            }
        }
        CycleType::Reverse => {
            if player.reverse_direction {
                player.local_time -= delta;
                if player.local_time <= 0.0 {
                    player.local_time = -player.local_time;
                    player.reverse_direction = false;
                }
            } else {
                player.local_time += delta;
                if player.local_time >= clip.duration {
                    player.local_time = 2.0 * clip.duration - player.local_time;
                    player.reverse_direction = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_linear_translation_channel() -> TransformChannel {
        TransformChannel {
            translation_keys: vec![
                TranslationKey {
                    time: 0.0,
                    value: Vec3::ZERO,
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
                TranslationKey {
                    time: 1.0,
                    value: Vec3::new(10.0, 0.0, 0.0),
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
            ],
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        }
    }

    #[test]
    fn linear_translation_midpoint() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 0.5).unwrap();
        assert!((v.x - 5.0).abs() < 1e-5);
        assert!(v.y.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_at_start() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 0.0).unwrap();
        assert!(v.x.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_at_end() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 1.0).unwrap();
        assert!((v.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn linear_translation_clamp_before() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, -1.0).unwrap();
        assert!(v.x.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_clamp_after() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 2.0).unwrap();
        assert!((v.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn slerp_rotation_midpoint() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc: None,
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    tbc: None,
                },
            ],
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let q = sample_rotation(&ch, 0.5).unwrap();
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        assert!((q.dot(expected)).abs() > 0.999);
    }

    #[test]
    fn empty_channel_returns_none() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        assert!(sample_translation(&ch, 0.0).is_none());
        assert!(sample_rotation(&ch, 0.0).is_none());
        assert!(sample_scale(&ch, 0.0).is_none());
    }

    #[test]
    fn single_key_returns_constant() {
        let ch = TransformChannel {
            translation_keys: vec![TranslationKey {
                time: 0.0,
                value: Vec3::new(5.0, 5.0, 5.0),
                forward: Vec3::ZERO,
                backward: Vec3::ZERO,
                tbc: None,
            }],
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: vec![ScaleKey {
                time: 0.0,
                value: 2.0,
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            }],
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let v = sample_translation(&ch, 99.0).unwrap();
        assert!((v.x - 5.0).abs() < 1e-5);
        let s = sample_scale(&ch, 99.0).unwrap();
        assert!((s - 2.0).abs() < 1e-5);
    }

    #[test]
    fn advance_time_loop() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 0.6);
        assert!((player.local_time - 0.6).abs() < 1e-5);
        advance_time(&mut player, &clip, 0.6);
        // 1.2 % 1.0 = 0.2
        assert!((player.local_time - 0.2).abs() < 1e-4);
    }

    #[test]
    fn advance_time_clamp() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            weight: 1.0,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 2.0);
        assert!((player.local_time - 1.0).abs() < 1e-5);
    }

    #[test]
    fn advance_time_reverse() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Reverse,
            frequency: 1.0,
            weight: 1.0,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 0.8);
        assert!((player.local_time - 0.8).abs() < 1e-5);
        assert!(!player.reverse_direction);

        // Go past the end — should bounce back
        advance_time(&mut player, &clip, 0.4);
        // 0.8 + 0.4 = 1.2 → 2*1.0 - 1.2 = 0.8
        assert!((player.local_time - 0.8).abs() < 1e-4);
        assert!(player.reverse_direction);
    }

    #[test]
    fn clip_registry_add_and_get() {
        let mut reg = AnimationClipRegistry::new();
        let clip = AnimationClip {
            name: "idle".to_string(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
        };
        let handle = reg.add(clip);
        assert_eq!(handle, 0);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get(0).unwrap().name, "idle");
    }

    #[test]
    fn find_key_pair_basic() {
        let times = vec![0.0, 0.5, 1.0];
        let (i0, i1, t) = find_key_pair(&times, 0.25);
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert!((t - 0.5).abs() < 1e-5);
    }

    #[test]
    fn linear_scale_interpolation() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: vec![
                ScaleKey { time: 0.0, value: 1.0, forward: 0.0, backward: 0.0, tbc: None },
                ScaleKey { time: 1.0, value: 3.0, forward: 0.0, backward: 0.0, tbc: None },
            ],
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let s = sample_scale(&ch, 0.5).unwrap();
        assert!((s - 2.0).abs() < 1e-5);
    }
}
