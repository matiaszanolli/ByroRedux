//! Throwaway (Oblivion audit dim4): why does NiPSysEmitterCtlr resolve no rate?
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::interpolator::{NiFloatData, NiFloatInterpolator};
use byroredux_nif::blocks::particle::NiPSysEmitterCtlr;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    let mut reasons: BTreeMap<String, u32> = Default::default();
    let mut samples: BTreeMap<String, Vec<String>> = Default::default();

    for name in &files {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        let Some(ctlr) = scene
            .blocks
            .iter()
            .find_map(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlr>())
        else {
            continue;
        };

        let reason: String = match ctlr.interpolator_ref.index() {
            None => "interp_ref NULL".to_string(),
            Some(i) => match scene.blocks.get(i) {
                None => "interp_ref out of range".to_string(),
                Some(b) => {
                    if let Some(interp) = b.as_any().downcast_ref::<NiFloatInterpolator>() {
                        match interp.data_ref.index() {
                            Some(d) => match scene.get_as::<NiFloatData>(d) {
                                Some(fd) => match fd.keys.keys.first() {
                                    Some(k)
                                        if k.value.is_finite()
                                            && k.value > 0.0
                                            && k.value < 3.0e38 =>
                                    {
                                        "OK keyed".to_string()
                                    }
                                    Some(k) => format!(
                                        "first key rejected ({}) nkeys={} const={}",
                                        k.value,
                                        fd.keys.keys.len(),
                                        interp.value
                                    ),
                                    None => format!("NiFloatData 0 keys, const={}", interp.value),
                                },
                                None => format!(
                                    "data_ref -> not NiFloatData ({}), const={}",
                                    scene
                                        .blocks
                                        .get(d)
                                        .map(|x| x.block_type_name())
                                        .unwrap_or("?"),
                                    interp.value
                                ),
                            },
                            None => {
                                if interp.value.is_finite()
                                    && interp.value > 0.0
                                    && interp.value < 3.0e38
                                {
                                    "OK const".to_string()
                                } else {
                                    format!("data NULL + const rejected ({})", interp.value)
                                }
                            }
                        }
                    } else {
                        format!(
                            "interp is {} (not NiFloatInterpolator)",
                            b.block_type_name()
                        )
                    }
                }
            },
        };
        let key = reason.split(" (").next().unwrap().to_string();
        *reasons.entry(key.clone()).or_insert(0) += 1;
        let e = samples.entry(key).or_default();
        if e.len() < 4 {
            e.push(format!("{name} :: {reason}"));
        }
    }
    for (k, v) in &reasons {
        println!("{v:5}  {k}");
        for s in &samples[k] {
            println!("         {s}");
        }
    }
}
