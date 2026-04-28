//! Multi-layer animation stack with blending support.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::{Quat, Vec3};
use crate::string::FixedString;

use super::interpolation::{sample_rotation, sample_scale, sample_translation};
use super::registry::AnimationClipRegistry;
use super::text_events::visit_text_key_events;
use super::types::CycleType;

/// A single animation layer in an AnimationStack.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
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
    /// Previous frame's local_time — used for text key event detection.
    pub prev_time: f32,
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
            prev_time: 0.0,
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
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
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

        // Save prev_time for text key event detection.
        layer.prev_time = layer.local_time;

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

/// Visit every text key event fired across all active layers of a stack
/// between each layer's `prev_time` and `local_time`, deduplicating
/// labels so overlapping layers don't fire the same event twice. The
/// visitor is called once per unique label with `(time, label)`.
///
/// Zero allocations — the caller supplies a `&mut Vec<FixedString>` scratch
/// buffer for the seen-set so the scratch can be reused frame-to-frame.
/// Dedup is integer comparison on the interned symbol (#231 / SI-04). Must
/// be called after `advance_stack()`.
pub fn visit_stack_text_events(
    stack: &AnimationStack,
    registry: &AnimationClipRegistry,
    seen: &mut Vec<FixedString>,
    mut visit: impl FnMut(f32, FixedString),
) {
    seen.clear();
    for layer in &stack.layers {
        if !layer.playing || layer.effective_weight() < 0.001 {
            continue;
        }
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        visit_text_key_events(clip, layer.prev_time, layer.local_time, |time, sym| {
            // Deduplicate labels across layers. Small seen-set (usually
            // 0–3 entries per frame); linear scan on `FixedString` is
            // integer comparison so a Vec is faster than a hash set at
            // this size.
            if seen.contains(&sym) {
                return;
            }
            seen.push(sym);
            visit(time, sym);
        });
    }
}

#[cfg(all(test, feature = "inspect"))]
mod inspect_tests {
    //! #486 sibling check — debug snapshots must preserve
    //! `AnimationLayer.reverse_direction` plus the blend timers
    //! (`blend_in_remaining`, `blend_out_remaining`). Round-trips a
    //! populated `AnimationStack` through JSON and verifies every
    //! per-layer field survives.
    use super::*;

    #[test]
    fn stack_round_trips_reverse_and_blend_state() {
        let mut stack = AnimationStack::new();
        stack.root_entity = Some(17);
        let mut mid_flight = AnimationLayer::new(5).with_blend_in(0.4);
        mid_flight.reverse_direction = true;
        mid_flight.local_time = 0.33;
        mid_flight.prev_time = 0.28;
        mid_flight.blend_in_remaining = 0.1; // mid fade-in
        mid_flight.blend_out_remaining = 0.0;
        stack.layers.push(mid_flight);

        let mut fading_out = AnimationLayer::new(9);
        fading_out.blend_out_remaining = 0.2;
        fading_out.blend_out_total = 0.5;
        fading_out.weight = 0.7;
        stack.layers.push(fading_out);

        let json = serde_json::to_value(&stack).expect("serialize");
        let reloaded: AnimationStack = serde_json::from_value(json).expect("deserialize");

        assert_eq!(reloaded.root_entity, Some(17));
        assert_eq!(reloaded.layers.len(), 2);

        let l0 = &reloaded.layers[0];
        assert_eq!(l0.clip_handle, 5);
        assert!(l0.reverse_direction, "ping-pong direction must survive");
        assert_eq!(l0.local_time, 0.33);
        assert_eq!(l0.prev_time, 0.28);
        assert_eq!(l0.blend_in_remaining, 0.1);
        assert_eq!(l0.blend_in_total, 0.4);

        let l1 = &reloaded.layers[1];
        assert_eq!(l1.clip_handle, 9);
        assert_eq!(l1.blend_out_remaining, 0.2);
        assert_eq!(l1.blend_out_total, 0.5);
        assert_eq!(l1.weight, 0.7);
    }
}

/// Allocation-full wrapper around `visit_stack_text_events` — retained
/// for test ergonomics. Hot paths in `byroredux::systems` should
/// call the visitor form directly and keep `FixedString` symbols.
pub fn collect_stack_text_events(
    stack: &AnimationStack,
    registry: &AnimationClipRegistry,
    pool: &crate::string::StringPool,
) -> Vec<(String, f32)> {
    let mut events = Vec::new();
    let mut seen: Vec<FixedString> = Vec::new();
    visit_stack_text_events(stack, registry, &mut seen, |time, sym| {
        if let Some(s) = pool.resolve(sym) {
            events.push((s.to_owned(), time));
        }
    });
    events
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
    channel_name: FixedString,
) -> Option<(Vec3, Quat, f32)> {
    // Pass 1+2 fused: find max priority AND compute total weight at that
    // priority in a single walk. Running max — when a strictly higher
    // priority appears, reset total_weight to that layer's weight. #288.
    let mut max_priority: Option<u8> = None;
    let mut total_weight = 0.0f32;
    for layer in &stack.layers {
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        // `clip.weight` pre-attenuates the layer per #469.
        let ew = layer.effective_weight() * clip.weight;
        if ew < 0.001 {
            continue;
        }
        let Some(channel) = clip.channels.get(&channel_name) else {
            continue;
        };
        let t = sample_translation(channel, layer.local_time);
        let r = sample_rotation(channel, layer.local_time);
        let s = sample_scale(channel, layer.local_time);
        if t.is_none() && r.is_none() && s.is_none() {
            continue;
        }
        match max_priority {
            None => {
                max_priority = Some(channel.priority);
                total_weight = ew;
            }
            Some(cur) if channel.priority > cur => {
                max_priority = Some(channel.priority);
                total_weight = ew;
            }
            Some(cur) if channel.priority == cur => {
                total_weight += ew;
            }
            _ => {} // lower priority — ignore
        }
    }
    let max_priority = max_priority?;
    if total_weight < 0.001 {
        return None;
    }

    // Pass 3: blend transforms from max_priority layers.
    let mut blended_pos = Vec3::ZERO;
    let mut blended_rot = Quat::IDENTITY;
    let mut blended_scale = 0.0f32;
    let mut accumulated_weight = 0.0f32;

    for layer in &stack.layers {
        let Some(clip) = registry.get(layer.clip_handle) else {
            continue;
        };
        // `clip.weight` pre-attenuates the layer per #469.
        let ew = layer.effective_weight() * clip.weight;
        if ew < 0.001 {
            continue;
        }
        let Some(channel) = clip.channels.get(&channel_name) else {
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
