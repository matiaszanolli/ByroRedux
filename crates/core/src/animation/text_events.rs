//! Text key event detection during animation playback.

use super::types::{AnimationClip, CycleType};
use crate::string::{FixedString, StringPool};

/// Visit each text key event crossed between `prev_time` and `curr_time`,
/// passing the event's `(time, label)` to the supplied closure.
///
/// Labels are passed as interned `FixedString` symbols (#231 / SI-04) — the
/// visitor does zero allocations and zero string comparisons. Callers that
/// need a `&str` resolve via `StringPool::resolve(sym)` at the point of use.
///
/// For forward-playing looping animations, wrap-around is handled: a step
/// from 4.8 → 0.3 in a 5-second clip fires events in both [4.8, 5.0] and
/// [0.0, 0.3]. A `CycleType::Loop` step whose delta is an exact multiple of
/// `duration` — landing back on the exact instant it started from — fires
/// every key once rather than the empty window that bare equality would
/// otherwise scan (#3034).
///
/// `reverse_direction` must be the layer/player's ping-pong direction flag
/// (`CycleType::Reverse`). On a backward leg the playhead moves *down*
/// (`curr_time < prev_time`) with no loop wrap — `fold_reverse_time`
/// reflects at the clip ends, it never wraps — so passing `false` here would
/// mis-read the descending step as a loop wrap and fire the complement of
/// the keys actually crossed. Loop/Clamp clips (which never reverse) always
/// pass `false`. See FNV-D6-01 / #2082.
///
/// Prefer this over `collect_text_key_events` in hot per-frame paths — the
/// collecting wrapper allocates even for empty result sets, which fires on
/// every frame for every animated entity whose playhead didn't cross any
/// keys (the overwhelming majority). See #339.
pub fn visit_text_key_events(
    clip: &AnimationClip,
    prev_time: f32,
    curr_time: f32,
    reverse_direction: bool,
    mut visit: impl FnMut(f32, FixedString),
) {
    if clip.text_keys.is_empty() {
        return;
    }

    if reverse_direction {
        // Backward leg of a ping-pong `CycleType::Reverse` clip: the playhead
        // moved DOWN from `prev_time` to `curr_time` (`curr_time < prev_time`)
        // with no wrap. Fire the keys actually crossed on the way down — the
        // closed interval `(curr_time, prev_time]`. Pre-#2082 this hit the
        // loop-wrap branch below (gated on `curr < prev` alone) and fired the
        // *complement*: every key NOT crossed on the leg. FNV-D6-01 / #2082.
        for (t, sym) in &clip.text_keys {
            if *t > curr_time && *t <= prev_time {
                visit(*t, *sym);
            }
        }
    } else if curr_time == prev_time && clip.duration > 0.0 && clip.cycle_type == CycleType::Loop {
        // #3034 — a `CycleType::Loop` clip whose delta this step is an
        // exact multiple of `duration` (one full period, or several) wraps
        // back onto the same instant it started from. The normal forward
        // scan below is the half-open window `(prev, curr]`, which is empty
        // when `prev == curr`, so every text key silently drops instead of
        // firing for the period(s) actually traversed. Fire each key once —
        // the semantically defensible reading for a single frame — rather
        // than falling through to the empty result.
        //
        // `Clamp` and `Reverse` never reach this arm: `Clamp` saturates at
        // `duration` and stays there on every subsequent frame (a *settled*
        // clip, not a wrap — see `clamped_completion_key_fires_once_at_clip_end`,
        // which must stay silent there), and `Reverse`'s ping-pong fold is
        // handled entirely by the `reverse_direction` arm above.
        for (t, sym) in &clip.text_keys {
            visit(*t, *sym);
        }
    } else if curr_time >= prev_time {
        // Normal forward progression (no wrap).
        for (t, sym) in &clip.text_keys {
            if *t > prev_time && *t <= curr_time {
                visit(*t, *sym);
            }
        }
    } else {
        // Forward-playing loop wrap-around: prev_time > curr_time means the
        // playhead wrapped past `duration` back to 0. Fire events in
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
    reverse_direction: bool,
) -> Vec<String> {
    let mut events = Vec::new();
    visit_text_key_events(clip, prev_time, curr_time, reverse_direction, |_, sym| {
        if let Some(s) = pool.resolve(sym) {
            events.push(s.to_owned());
        }
    });
    events
}
