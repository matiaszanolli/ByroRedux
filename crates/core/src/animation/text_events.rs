//! Text key event detection during animation playback.

use super::types::AnimationClip;
use crate::string::{FixedString, StringPool};

/// Visit each text key event crossed between `prev_time` and `curr_time`,
/// passing the event's `(time, label)` to the supplied closure.
///
/// Labels are passed as interned `FixedString` symbols (#231 / SI-04) — the
/// visitor does zero allocations and zero string comparisons. Callers that
/// need a `&str` resolve via `StringPool::resolve(sym)` at the point of use.
///
/// For looping animations, wrap-around is handled: a step from 4.8 → 0.3 in
/// a 5-second clip fires events in both [4.8, 5.0] and [0.0, 0.3].
///
/// Prefer this over `collect_text_key_events` in hot per-frame paths — the
/// collecting wrapper allocates even for empty result sets, which fires on
/// every frame for every animated entity whose playhead didn't cross any
/// keys (the overwhelming majority). See #339.
pub fn visit_text_key_events(
    clip: &AnimationClip,
    prev_time: f32,
    curr_time: f32,
    mut visit: impl FnMut(f32, FixedString),
) {
    if clip.text_keys.is_empty() {
        return;
    }

    if curr_time >= prev_time {
        // Normal forward progression (no wrap).
        for (t, sym) in &clip.text_keys {
            if *t > prev_time && *t <= curr_time {
                visit(*t, *sym);
            }
        }
    } else {
        // Loop wrap-around: prev_time > curr_time. Fire events in
        // [prev_time, duration] and [0, curr_time].
        for (t, sym) in &clip.text_keys {
            if *t > prev_time || *t <= curr_time {
                visit(*t, *sym);
            }
        }
    }
}

/// Collect text key events crossed between `prev_time` and `curr_time` as
/// resolved label strings.
///
/// Allocation-full wrapper around `visit_text_key_events` — kept for test
/// ergonomics (`assert_eq!(events, vec!["hit"])`). Hot per-frame paths in
/// `systems.rs` / `stack.rs` should call `visit_text_key_events` directly
/// and either keep `FixedString` symbols or resolve at the consumer.
pub fn collect_text_key_events(
    clip: &AnimationClip,
    pool: &StringPool,
    prev_time: f32,
    curr_time: f32,
) -> Vec<String> {
    let mut events = Vec::new();
    visit_text_key_events(clip, prev_time, curr_time, |_, sym| {
        if let Some(s) = pool.resolve(sym) {
            events.push(s.to_owned());
        }
    });
    events
}
