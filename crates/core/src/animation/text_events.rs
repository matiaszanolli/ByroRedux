//! Text key event detection during animation playback.

use super::types::AnimationClip;

/// Collect text key events that were crossed between `prev_time` and `curr_time`.
///
/// For looping animations, handles wrap-around: if time went from 4.8 to 0.3
/// in a 5-second clip, events in [4.8, 5.0] and [0.0, 0.3] both fire.
///
/// Returns the labels of all crossed text keys.
pub fn collect_text_key_events(
    clip: &AnimationClip,
    prev_time: f32,
    curr_time: f32,
) -> Vec<String> {
    if clip.text_keys.is_empty() {
        return Vec::new();
    }

    let mut events = Vec::new();

    if curr_time >= prev_time {
        // Normal forward progression (no wrap).
        for (t, label) in &clip.text_keys {
            if *t > prev_time && *t <= curr_time {
                events.push(label.clone());
            }
        }
    } else {
        // Loop wrap-around: prev_time > curr_time.
        // Fire events in [prev_time, duration] and [0, curr_time].
        for (t, label) in &clip.text_keys {
            if *t > prev_time || *t <= curr_time {
                events.push(label.clone());
            }
        }
    }

    events
}
