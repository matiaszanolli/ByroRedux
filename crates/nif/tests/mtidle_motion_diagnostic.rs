//! Diagnostic test for #794 — does FNV `mtidle.kf` produce visible
//! rotation variation across time when sampled through the same
//! import + sample path the runtime uses?
//!
//! Reproduces the third investigation step in #794: load a real
//! `mtidle.kf` from FNV `Fallout - Meshes.bsa`, extract the
//! TransformChannel for one of the major bones (`Bip01 Spine`,
//! `Bip01 Head`, etc.), and assert that sampling the same channel at
//! several times across the clip duration produces *at least one*
//! quaternion delta above an epsilon.
//!
//! `#[ignore]` because it needs vanilla FNV game data; run with
//! `cargo test -p byroredux-nif --test mtidle_motion_diagnostic
//! -- --ignored --nocapture`.

mod common;

use common::{open_mesh_archive, Game};

use byroredux_nif::anim::{import_kf, RotationKey, TranslationKey};
use byroredux_nif::parse_nif;

const MTIDLE_PATH: &str = r"meshes\characters\_male\locomotion\mtidle.kf";
const SAMPLE_TIMES: &[f32] = &[0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0];
const ROTATION_EPSILON: f32 = 1e-3;

fn quat_diff(a: [f32; 4], b: [f32; 4]) -> f32 {
    // Component-wise max abs delta. Quaternions sit in [-1, 1] so the
    // raw-component metric is fine for "did anything move?" diagnosis.
    let mut max = 0.0f32;
    for i in 0..4 {
        let d = (a[i] - b[i]).abs();
        if d > max {
            max = d;
        }
    }
    max
}

fn vec3_diff(a: [f32; 3], b: [f32; 3]) -> f32 {
    let mut max = 0.0f32;
    for i in 0..3 {
        let d = (a[i] - b[i]).abs();
        if d > max {
            max = d;
        }
    }
    max
}

/// Linear-interp the rotation key list at `time` — mirrors the import-
/// side ordering. Returns `None` when there are no keys.
fn sample_rotation(keys: &[RotationKey], time: f32) -> Option<[f32; 4]> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }
    if time <= keys[0].time {
        return Some(keys[0].value);
    }
    if time >= keys[keys.len() - 1].time {
        return Some(keys[keys.len() - 1].value);
    }
    for w in keys.windows(2) {
        if time >= w[0].time && time <= w[1].time {
            let span = (w[1].time - w[0].time).max(f32::EPSILON);
            let t = (time - w[0].time) / span;
            let mut out = [0.0; 4];
            for i in 0..4 {
                out[i] = (1.0 - t) * w[0].value[i] + t * w[1].value[i];
            }
            return Some(out);
        }
    }
    Some(keys[keys.len() - 1].value)
}

fn sample_translation(keys: &[TranslationKey], time: f32) -> Option<[f32; 3]> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }
    if time <= keys[0].time {
        return Some(keys[0].value);
    }
    if time >= keys[keys.len() - 1].time {
        return Some(keys[keys.len() - 1].value);
    }
    for w in keys.windows(2) {
        if time >= w[0].time && time <= w[1].time {
            let span = (w[1].time - w[0].time).max(f32::EPSILON);
            let t = (time - w[0].time) / span;
            let mut out = [0.0; 3];
            for i in 0..3 {
                out[i] = (1.0 - t) * w[0].value[i] + t * w[1].value[i];
            }
            return Some(out);
        }
    }
    Some(keys[keys.len() - 1].value)
}

#[test]
#[ignore]
fn mtidle_kf_produces_visible_rotation_variation() {
    let Some(archive) = open_mesh_archive(Game::FalloutNV) else {
        return;
    };

    let bytes = archive
        .extract(MTIDLE_PATH)
        .expect("FNV mesh archive must contain mtidle.kf");
    let scene = parse_nif(&bytes).expect("mtidle.kf must parse cleanly");
    let mut clips = import_kf(&scene);
    assert!(
        !clips.is_empty(),
        "import_kf must produce at least one clip from mtidle.kf"
    );
    let clip = clips.remove(0);

    eprintln!(
        "mtidle.kf: clip='{}' duration={:.3}s channels={} freq={}",
        clip.name,
        clip.duration,
        clip.channels.len(),
        clip.frequency,
    );

    let mut max_rot_delta_overall = 0.0f32;
    let mut max_trans_delta_overall = 0.0f32;
    let mut interesting_channel = String::new();
    let mut empty_rot_channels = 0usize;
    let mut single_key_rot_channels = 0usize;
    let mut multi_key_rot_channels = 0usize;

    for (name, channel) in &clip.channels {
        match channel.rotation_keys.len() {
            0 => empty_rot_channels += 1,
            1 => single_key_rot_channels += 1,
            _ => multi_key_rot_channels += 1,
        }

        // Inter-sample rotation delta.
        let mut prev_rot: Option<[f32; 4]> = None;
        let mut max_local_rot = 0.0f32;
        for &t in SAMPLE_TIMES {
            if let Some(q) = sample_rotation(&channel.rotation_keys, t) {
                if let Some(p) = prev_rot {
                    let d = quat_diff(p, q);
                    if d > max_local_rot {
                        max_local_rot = d;
                    }
                }
                prev_rot = Some(q);
            }
        }

        let mut prev_trans: Option<[f32; 3]> = None;
        let mut max_local_trans = 0.0f32;
        for &t in SAMPLE_TIMES {
            if let Some(p) = sample_translation(&channel.translation_keys, t) {
                if let Some(pp) = prev_trans {
                    let d = vec3_diff(pp, p);
                    if d > max_local_trans {
                        max_local_trans = d;
                    }
                }
                prev_trans = Some(p);
            }
        }

        if max_local_rot > max_rot_delta_overall {
            max_rot_delta_overall = max_local_rot;
            interesting_channel = name.to_string();
        }
        if max_local_trans > max_trans_delta_overall {
            max_trans_delta_overall = max_local_trans;
        }
    }

    eprintln!(
        "rotation_keys per channel: empty={} single={} multi={}",
        empty_rot_channels, single_key_rot_channels, multi_key_rot_channels
    );
    eprintln!(
        "max inter-sample rotation delta = {:.6} (channel '{}')",
        max_rot_delta_overall, interesting_channel
    );
    eprintln!(
        "max inter-sample translation delta = {:.6}",
        max_trans_delta_overall
    );

    // Print the first 5 multi-key rotation channels for a closer look.
    let mut shown = 0usize;
    for (name, channel) in &clip.channels {
        if channel.rotation_keys.len() < 2 {
            continue;
        }
        if shown >= 5 {
            break;
        }
        let n = channel.rotation_keys.len();
        eprintln!(
            "  channel '{}': {} rot keys, t0..t-1 = [{:.3}..{:.3}]",
            name,
            n,
            channel.rotation_keys[0].time,
            channel.rotation_keys[n - 1].time,
        );
        eprintln!("    rot[0]   = {:?}", channel.rotation_keys[0].value);
        eprintln!("    rot[n/2] = {:?}", channel.rotation_keys[n / 2].value);
        eprintln!("    rot[n-1] = {:?}", channel.rotation_keys[n - 1].value);
        shown += 1;
    }

    assert!(
        max_rot_delta_overall > ROTATION_EPSILON,
        "mtidle.kf must produce *some* rotation animation across time \
         (max delta {:.6} ≤ {:.6}). If this fails, the B-spline rot \
         decoder is producing constant output — suspect 2 in #794.",
        max_rot_delta_overall,
        ROTATION_EPSILON,
    );
}
