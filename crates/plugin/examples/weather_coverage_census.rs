//! WTHR cloud-coverage census (light & shadow campaign W3.14).
//!
//! `env_translate::fog_coverage_from_weather` maps WTHR classification
//! flags onto a fixed procedural-sky cloud coverage (0.86 rain / 0.80 snow
//! / 0.70 cloudy / 0.40 pleasant / 0.55 unclassified) that skyal.md flags
//! as uncited. This census measures the closest thing the records author
//! to a coverage signal — Skyrim's JNAM cloud-layer alpha tables (four
//! layers × four TOD samples) — per classification bucket, so the
//! constants can be re-ground in measured values or confirmed as-is.
//!
//! JNAM is a Skyrim+ sub-record; non-Skyrim masters carry the default
//! 1.0 alphas and contribute nothing (the census reports them as such).
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example weather_coverage_census -- <master.esm>...

use std::collections::BTreeMap;

use byroredux_plugin::esm::records::weather::{
    WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
};

fn class_name(classification: u8) -> String {
    let mut parts = Vec::new();
    if classification & WTHR_PLEASANT != 0 {
        parts.push("pleasant");
    }
    if classification & WTHR_CLOUDY != 0 {
        parts.push("cloudy");
    }
    if classification & WTHR_RAINY != 0 {
        parts.push("rainy");
    }
    if classification & WTHR_SNOW != 0 {
        parts.push("snow");
    }
    if parts.is_empty() {
        "unclassified".to_string()
    } else {
        parts.join("+")
    }
}

fn main() {
    let masters: Vec<String> = std::env::args().skip(1).collect();
    assert!(!masters.is_empty(), "usage: weather_coverage_census <master.esm>...");

    for path in &masters {
        let bytes = std::fs::read(path).expect("read master");
        let index = byroredux_plugin::esm::records::parse_esm(&bytes).expect("parse esm");
        println!("=== {path} ({} weathers) ===", index.weathers.len());

        // bucket -> (count, summed max-layer alpha, summed layer-0 alpha, jnam-authored count)
        let mut buckets: BTreeMap<String, (usize, f64, f64, usize)> = BTreeMap::new();
        for w in index.weathers.values() {
            let key = class_name(w.classification);
            let entry = buckets.entry(key).or_insert((0, 0.0, 0.0, 0));
            entry.0 += 1;
            // The 2D engine's visible cloud coverage per layer is its alpha
            // (how opaque the cloud texture sits over the sky). Take, per
            // TOD slot, the MAX alpha across the four rendered layers (the
            // heaviest band dominates the sky) and average the slots.
            let mut max_per_slot = [0.0f32; 4];
            for layer in &w.cloud_layer_alphas {
                for (slot, a) in layer.iter().enumerate() {
                    max_per_slot[slot] = max_per_slot[slot].max(*a);
                }
            }
            let mean_max = max_per_slot.iter().sum::<f32>() / 4.0;
            entry.1 += mean_max as f64;
            entry.2 += (w.cloud_layer_alphas[0].iter().sum::<f32>() / 4.0) as f64;
            // JNAM present? Default construction is exactly 1.0 everywhere;
            // any deviation means the record authored the table.
            let authored = w
                .cloud_layer_alphas
                .iter()
                .any(|layer| layer.iter().any(|a| (a - 1.0).abs() > f32::EPSILON));
            if authored {
                entry.3 += 1;
            }
        }

        println!(
            "{:18} {:>6} {:>10} {:>12} {:>10} {:>10}",
            "classification", "n", "jnam", "mean max-a", "mean L0-a", "engine cov"
        );
        for (key, (n, sum_max, sum_l0, jnam)) in &buckets {
            let engine = match key.as_str() {
                "rainy" => Some(0.86),
                "snow" => Some(0.80),
                "cloudy" => Some(0.70),
                "pleasant" => Some(0.40),
                "unclassified" => Some(0.55),
                _ => None,
            };
            println!(
                "{key:18} {n:>6} {jnam:>10} {:>12.4} {:>10.4} {:>10}",
                sum_max / *n as f64,
                sum_l0 / *n as f64,
                engine.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            );
        }
    }
}
