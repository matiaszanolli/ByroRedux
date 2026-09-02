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
/// `applied_delta` must be the time delta the caller's `advance_*` actually
/// applied this tick (`AnimationPlayer::last_delta` / `AnimationLayer::last_delta`),
/// *after* speed/frequency scaling and the non-finite fold. It disambiguates
/// the full-period `Loop` arm from a zero advance, which the `(prev, curr)`
/// pair alone cannot (#3470). Pass `0.0` only to mean "the playhead did not
/// move".
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
    applied_delta: f32,
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
        return;
    }

    // #3034 / #3704 — a `CycleType::Loop` step whose applied delta spans at
    // least one full `duration` traverses one or more complete periods that
    // the ordinary (prev, curr] / wrap scan below can't see on its own — that
    // scan only examines the *final* partial leg, which is empty (`prev ==
    // curr`) for a step landing on an exact multiple of `duration` and
    // otherwise only covers the residual beyond the last full period. Fire
    // every key once first — the semantically defensible reading for
    // "how many periods you can't individually enumerate" a single frame
    // affords — then still fall through to the normal scan below for the
    // final partial crossing.
    //
    // #3034 originally gated this on `curr_time == prev_time` (an exact
    // multiple only); #3704 widens it to `applied_delta.abs() >= duration`
    // so a hitch bigger than one period but NOT an exact multiple (e.g.
    // duration=0.5, delta=1.3 → 2.6 periods, landing on a residual, not
    // back on the start instant) no longer silently drops every key outside
    // that residual window. A key inside the residual window now fires
    // twice this tick — once for the full periods it was genuinely also
    // crossed during, once for the precise final leg — rather than once.
    //
    // #3470 — `applied_delta != 0.0` is still required: `prev == curr` (or,
    // now, a large `|delta|`) on a Loop clip needs a genuine non-zero applied
    // delta to mean "periods elapsed" rather than "the playhead didn't move".
    // `App::resumed` runs the scheduler once with `dt == 0.0` to prime
    // transform state; without this guard every looping clip in the scene
    // would fire ALL of its text keys on that priming tick, before any had
    // been crossed. `AnimationTextKeyEvents` feeds
    // `cinematic_animation_event_system`, which writes `QuestStageState`, so
    // a spurious batch could advance quest state at launch. It also covers
    // the `speed == 0` / `frequency == 0` paused-clip variants, and the
    // worse latent one: `finite_time_delta` folds a non-finite
    // `dt * speed * frequency` to `0.0` (#3258), so a clip reaching the
    // registry with a NaN/inf frequency from a producer other than
    // `anim_convert` would otherwise have fired every key on EVERY frame,
    // forever.
    //
    // `Clamp` and `Reverse` never reach this arm: `Clamp` saturates at
    // `duration` and stays there on every subsequent frame (a *settled*
    // clip, not a wrap — see `clamped_completion_key_fires_once_at_clip_end`,
    // which must stay silent there), and `Reverse`'s ping-pong fold is
    // handled entirely by the `reverse_direction` arm above.
    if applied_delta.abs() >= clip.duration
        && applied_delta != 0.0
        && clip.duration > 0.0
        && clip.cycle_type == CycleType::Loop
    {
        for (t, sym) in &clip.text_keys {
            visit(*t, *sym);
        }
    }

    if curr_time >= prev_time {
        // Normal forward progression (no wrap) — also the final partial
        // leg's window for the widened arm above (empty when `prev ==
        // curr`, matching the pre-#3704 exact-multiple behavior exactly).
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
    applied_delta: f32,
) -> Vec<String> {
    let mut events = Vec::new();
    visit_text_key_events(
        clip,
        prev_time,
        curr_time,
        reverse_direction,
        applied_delta,
        |_, sym| {
            if let Some(s) = pool.resolve(sym) {
                events.push(s.to_owned());
            }
        },
    );
    events
}
