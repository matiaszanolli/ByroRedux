//! Multi-layer animation stack with blending support.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::{Quat, Vec3};

use super::interpolation::{sample_rotation, sample_scale, sample_translation};
use super::registry::AnimationClipRegistry;
use super::types::CycleType;

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
    /// Root entity of the subtree to animate (scoped name lookup).
    pub root_entity: Option<EntityId>,
}

impl AnimationStack {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            root_entity: None,
        }
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
pub fn advance_stack(stack: &mut AnimationStack, registry: &AnimationClipRegistry, dt: f32) {
    for layer in &mut stack.layers {
        if !layer.playing {
            continue;
        }

        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };

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
///
/// Zero-allocation: uses inline iteration instead of collecting into Vecs.
pub fn sample_blended_transform(
    stack: &AnimationStack,
    registry: &AnimationClipRegistry,
    channel_name: &str,
) -> Option<(Vec3, Quat, f32)> {
    // Pass 1: find max priority among layers that have data for this channel.
    let mut max_priority: Option<u8> = None;
    for layer in &stack.layers {
        if layer.effective_weight() < 0.001 {
            continue;
        }
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        let Some(channel) = clip.channels.get(channel_name) else {
            continue;
        };
        let t = sample_translation(channel, layer.local_time);
        let r = sample_rotation(channel, layer.local_time);
        let s = sample_scale(channel, layer.local_time);
        if t.is_none() && r.is_none() && s.is_none() {
            continue;
        }
        max_priority = Some(max_priority.map_or(channel.priority, |p: u8| p.max(channel.priority)));
    }
    let max_priority = max_priority?;

    // Pass 2: compute total weight for layers at max_priority.
    let mut total_weight = 0.0f32;
    for layer in &stack.layers {
        let ew = layer.effective_weight();
        if ew < 0.001 {
            continue;
        }
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        let Some(channel) = clip.channels.get(channel_name) else {
            continue;
        };
        if channel.priority != max_priority {
            continue;
        }
        total_weight += ew;
    }
    if total_weight < 0.001 {
        return None;
    }

    // Pass 3: blend transforms from max_priority layers.
    let mut blended_pos = Vec3::ZERO;
    let mut blended_rot = Quat::IDENTITY;
    let mut blended_scale = 0.0f32;
    let mut accumulated_weight = 0.0f32;

    for layer in &stack.layers {
        let ew = layer.effective_weight();
        if ew < 0.001 {
            continue;
        }
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        let Some(channel) = clip.channels.get(channel_name) else {
            continue;
        };
        if channel.priority != max_priority {
            continue;
        }

        let t = sample_translation(channel, layer.local_time).unwrap_or(Vec3::ZERO);
        let r = sample_rotation(channel, layer.local_time).unwrap_or(Quat::IDENTITY);
        let s = sample_scale(channel, layer.local_time).unwrap_or(1.0);

        let w = ew / total_weight;
        blended_pos += t * w;
        blended_scale += s * w;

        // Incremental SLERP for rotation blending.
        if accumulated_weight < 0.001 {
            blended_rot = r;
        } else {
            let interp = w / (accumulated_weight + w);
            blended_rot = blended_rot.slerp(if blended_rot.dot(r) < 0.0 { -r } else { r }, interp);
        }
        accumulated_weight += w;
    }

    Some((blended_pos, blended_rot, blended_scale))
}
