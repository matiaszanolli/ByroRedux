//! Text key event detection during animation playback.

use super::types::AnimationClip;

/// Visit each text key event crossed between `prev_time` and `curr_time`,
/// passing the event's `(time, label)` to the supplied closure.
///
/// Zero allocations — no Vec, no String clones. The label is borrowed
/// from the clip for the duration of the visitor. For looping
/// animations, wrap-around is handled: a step from 4.8 → 0.3 in a
/// 5-second clip fires events in both [4.8, 5.0] and [0.0, 0.3].
///
/// Prefer this over `collect_text_key_events` in hot per-frame paths —
/// the collecting wrapper allocates even for empty result sets, which
/// fires on every frame for every animated entity whose playhead
/// didn't cross any keys (the overwhelming majority). See #339.
pub fn visit_text_key_events(
    clip: &AnimationClip,
    prev_time: f32,
    curr_time: f32,
    mut visit: impl FnMut(f32, &str),
) {
    if clip.text_keys.is_empty() {
        return;
    }

    if curr_time >= prev_time {
        // Normal forward progression (no wrap).
        for (t, label) in &clip.text_keys {
            if *t > prev_time && *t <= curr_time {
                visit(*t, label);
            }
        }
    } else {
        // Loop wrap-around: prev_time > curr_time. Fire events in
        // [prev_time, duration] and [0, curr_time].
        for (t, label) in &clip.text_keys {
            if *t > prev_time || *t <= curr_time {
                visit(*t, label);
            }
        }
    }
}

/// Collect text key events crossed between `prev_time` and `curr_time`.
///
/// Allocation-full wrapper around `visit_text_key_events` — kept for
/// test ergonomics (`.contains(&"...".to_string())` assertions). Hot
/// per-frame paths in `systems.rs` / `stack.rs` should call
/// `visit_text_key_events` directly.
pub fn collect_text_key_events(
    clip: &AnimationClip,
    prev_time: f32,
    curr_time: f32,
) -> Vec<String> {
    let mut events = Vec::new();
    visit_text_key_events(clip, prev_time, curr_time, |_, label| {
        events.push(label.to_owned());
    });
    events
}
