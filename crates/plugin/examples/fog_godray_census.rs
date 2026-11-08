//! Authored fog-profile and god-ray census for the Volumetric Lighting PRD's Phase 0 items
//! (`docs/prd/volumetric-lighting.md`: VLM-HF-001 context, VLM-HF-003, VLM-HF-010, VLM-HF-011).
//!
//! Part A reads the parsed weathers: authored fog power per game, and the FO4 height-fog
//! extension (`WeatherHeightFog`: near / far height mid and range, high density scale).
//! Part B walks raw records: `VOLI` (Skyrim SE), `GDRY` (FO4), the `WTHR` `HNAM` / `WGDR` slots,
//! `CELL` `XGDR`, `LGTM` / `LIGH` `WGDR`, with the decoded value spread of each god-ray field.
//!
//! Layouts are xEdit's (`wbDefinitionsTES5.pas` VOLI; `wbDefinitionsFO4.pas` GDRY and
//! `wbWeatherGodRays` / `wbWeatherVolumetricLighting` in `wbDefinitionsCommon.pas`).
//!
//! `cargo run --release -p byroredux-plugin --example fog_godray_census -- <master.esm> [-v]`

use byroredux_plugin::esm::reader::{EsmReader, SubRecord};
use byroredux_plugin::esm::records::parse_esm;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Spread {
    n: usize,
    min: f32,
    max: f32,
    sum: f64,
    distinct: BTreeSet<u32>,
}

impl Spread {
    fn add(&mut self, v: f32) {
        if self.n == 0 {
            self.min = v;
            self.max = v;
        }
        self.n += 1;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        self.sum += v as f64;
        self.distinct.insert(v.to_bits());
    }
    fn show(&self) -> String {
        if self.n == 0 {
            return "-".into();
        }
        format!(
            "n={} min={:.4} mean={:.4} max={:.4} distinct={}",
            self.n,
            self.min,
            self.sum / self.n as f64,
            self.max,
            self.distinct.len()
        )
    }
}

fn f32s(sub: &SubRecord) -> Vec<f32> {
    sub.data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn forms(sub: &SubRecord) -> Vec<u32> {
    sub.data
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: fog_godray_census <master.esm> [-v]");
    let verbose = args.next().as_deref() == Some("-v");
    let bytes = std::fs::read(&path)?;
    println!("{path}");

    // ── Part A: parsed weathers ────────────────────────────────────────────────────────────
    let index = parse_esm(&bytes)?;
    let n = index.weathers.len();
    let mut power = Spread::default();
    let mut max_op = Spread::default();
    let mut non_unit_power = 0;
    let mut hf: [Spread; 6] = Default::default();
    let mut with_height = 0;
    let mut tuples: BTreeMap<[u32; 6], Vec<String>> = BTreeMap::new();
    for w in index.weathers.values() {
        for v in [w.fog_day_power, w.fog_night_power] {
            power.add(v);
            if (v - 1.0).abs() > 1e-6 {
                non_unit_power += 1;
            }
        }
        max_op.add(w.fog_day_max);
        if let Some(h) = &w.fog_height {
            with_height += 1;
            let day = [
                h.day_near_height_mid,
                h.day_near_height_range,
                h.day_high_density_scale,
                h.day_far_height_mid,
                h.day_far_height_range,
                h.night_high_density_scale,
            ];
            for (s, v) in hf.iter_mut().zip(day) {
                s.add(v);
            }
            tuples
                .entry(day.map(f32::to_bits))
                .or_default()
                .push(w.editor_id.clone());
        }
    }
    println!("\nPART A  weathers {n}");
    println!("  fog power (day+night values): {}  non-1.0 values {non_unit_power}", power.show());
    println!("  fog max opacity (day): {}", max_op.show());
    println!("  weathers with the FO4 height-fog extension: {with_height}");
    if with_height > 0 {
        let names = [
            "day near height mid",
            "day near height range",
            "day high density scale",
            "day far height mid",
            "day far height range",
            "night high density scale",
        ];
        for (name, s) in names.iter().zip(&hf) {
            println!("    {name:<26} {}", s.show());
        }
        println!("  distinct (day) height tuples: {}", tuples.len());
        let mut rows: Vec<_> = tuples.iter().collect();
        rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        for (t, names) in rows.iter().take(if verbose { 60 } else { 8 }) {
            let v = t.map(f32::from_bits);
            println!(
                "    x{:<3} near({:.0},{:.0}) dens {:.2} far({:.0},{:.0}) e.g. {}",
                names.len(),
                v[0],
                v[1],
                v[2],
                v[3],
                v[4],
                names.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }

    // ── Part B: raw god-ray records ────────────────────────────────────────────────────────
    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut voli_fields: BTreeMap<&'static str, Spread> = BTreeMap::new();
    let mut gdry_fields: BTreeMap<String, Spread> = BTreeMap::new();
    let mut voli_sizes: BTreeMap<String, usize> = BTreeMap::new();
    let mut gdry_sizes: BTreeMap<usize, usize> = BTreeMap::new();
    let mut wthr_hnam_slots: BTreeMap<usize, (usize, usize)> = BTreeMap::new(); // slot → (non-null, total)
    let mut wthr_wgdr_slots: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut wthr_hnam_lens: BTreeMap<usize, usize> = BTreeMap::new();
    let mut wthr_wgdr_lens: BTreeMap<usize, usize> = BTreeMap::new();
    let mut voli_ids: BTreeSet<u32> = BTreeSet::new();
    let mut gdry_ids: BTreeSet<u32> = BTreeSet::new();
    let mut hnam_refs: BTreeSet<u32> = BTreeSet::new();
    let mut wgdr_refs: BTreeSet<u32> = BTreeSet::new();
    let mut other_refs: BTreeSet<u32> = BTreeSet::new();
    let mut total = 0usize;
    reader.for_each_record(bytes.len(), None, |h, subs| {
        total += 1;
        let t = &h.record_type;
        let edid = subs
            .iter()
            .find(|s| &s.sub_type == b"EDID")
            .map(|s| String::from_utf8_lossy(&s.data).trim_end_matches('\0').to_string())
            .unwrap_or_default();
        match t {
            b"VOLI" => {
                *counts.entry("VOLI").or_default() += 1;
                voli_ids.insert(h.form_id);
                let sig = subs
                    .iter()
                    .map(|s| format!("{}:{}", String::from_utf8_lossy(&s.sub_type), s.data.len()))
                    .collect::<Vec<_>>()
                    .join(",");
                *voli_sizes.entry(sig).or_default() += 1;
                let names: [(&[u8; 4], &'static str); 9] = [
                    (b"CNAM", "Intensity"),
                    (b"DNAM", "Custom Color - Contribution"),
                    (b"HNAM", "Density - Contribution"),
                    (b"INAM", "Density - Size"),
                    (b"JNAM", "Density - Wind Speed"),
                    (b"KNAM", "Density - Falling Speed"),
                    (b"LNAM", "Phase Function - Contribution"),
                    (b"MNAM", "Phase Function - Scattering"),
                    (b"NNAM", "Sampling Repartition - Range Factor"),
                ];
                for (tag, label) in names {
                    if let Some(s) = subs.iter().find(|s| &s.sub_type == tag) {
                        if let Some(&v) = f32s(s).first() {
                            voli_fields.entry(label).or_default().add(v);
                        }
                    }
                }
                if verbose {
                    let vals: Vec<String> = names
                        .iter()
                        .map(|(tag, l)| {
                            let v = subs
                                .iter()
                                .find(|s| s.sub_type == **tag)
                                .and_then(|s| f32s(s).first().copied())
                                .map_or("-".into(), |v| format!("{v:.3}"));
                            format!("{}={v}", &l[..l.len().min(9)])
                        })
                        .collect();
                    println!("  VOLI {:08X} {edid}: {}", h.form_id, vals.join(" "));
                }
            }
            b"GDRY" => {
                *counts.entry("GDRY").or_default() += 1;
                gdry_ids.insert(h.form_id);
                if let Some(s) = subs.iter().find(|s| &s.sub_type == b"DATA") {
                    *gdry_sizes.entry(s.data.len()).or_default() += 1;
                    let v = f32s(s);
                    let labels = [
                        "Back Color r", "Back Color g", "Back Color b", "Fwd Color r", "Fwd Color g",
                        "Fwd Color b", "Intensity", "Air Color - Scale", "Back Color - Scale",
                        "Fwd Color - Scale", "Back Phase", "Air Color r", "Air Color g",
                        "Air Color b", "Fwd Phase",
                    ];
                    for (l, x) in labels.iter().zip(&v) {
                        gdry_fields.entry((*l).to_string()).or_default().add(*x);
                    }
                    if verbose {
                        println!("  GDRY {:08X} {edid}: {:.3?}", h.form_id, v);
                    }
                }
            }
            b"WTHR" => {
                *counts.entry("WTHR").or_default() += 1;
                for s in &subs {
                    let slot = if &s.sub_type == b"HNAM" {
                        Some((&mut wthr_hnam_slots, &mut wthr_hnam_lens, &mut hnam_refs))
                    } else if &s.sub_type == b"WGDR" {
                        Some((&mut wthr_wgdr_slots, &mut wthr_wgdr_lens, &mut wgdr_refs))
                    } else {
                        None
                    };
                    if let Some((slots, lens, refs)) = slot {
                        *lens.entry(s.data.len()).or_default() += 1;
                        for (i, f) in forms(s).into_iter().enumerate() {
                            let e = slots.entry(i).or_default();
                            e.1 += 1;
                            if f != 0 {
                                e.0 += 1;
                                refs.insert(f);
                            }
                        }
                    }
                }
            }
            b"CELL" => {
                if let Some(s) = subs.iter().find(|s| &s.sub_type == b"XGDR") {
                    *counts.entry("CELL XGDR").or_default() += 1;
                    other_refs.extend(forms(s).into_iter().filter(|&f| f != 0));
                }
            }
            b"LGTM" => {
                if let Some(s) = subs.iter().find(|s| &s.sub_type == b"WGDR") {
                    *counts.entry("LGTM WGDR").or_default() += 1;
                    other_refs.extend(forms(s).into_iter().filter(|&f| f != 0));
                }
            }
            b"LIGH" => {
                if let Some(s) = subs.iter().find(|s| &s.sub_type == b"WGDR") {
                    *counts.entry("LIGH WGDR").or_default() += 1;
                    other_refs.extend(forms(s).into_iter().filter(|&f| f != 0));
                }
            }
            _ => {}
        }
    })?;

    println!("\nPART B  records walked {total}");
    for (k, v) in &counts {
        println!("  {k:<10} {v}");
    }
    if !voli_ids.is_empty() {
        println!("  VOLI subrecord signatures: {voli_sizes:?}");
        for (k, s) in &voli_fields {
            println!("    {k:<38} {}", s.show());
        }
    }
    if !gdry_ids.is_empty() {
        println!("  GDRY DATA sizes: {gdry_sizes:?}");
        for (k, s) in &gdry_fields {
            println!("    {k:<22} {}", s.show());
        }
    }
    println!("  WTHR HNAM lengths {wthr_hnam_lens:?}; non-null refs per slot (non-null/total): {wthr_hnam_slots:?}");
    println!("  WTHR WGDR lengths {wthr_wgdr_lens:?}; non-null refs per slot (non-null/total): {wthr_wgdr_slots:?}");
    println!(
        "  distinct VOLI referenced by WTHR HNAM {} (resolvable {}); distinct GDRY referenced by WTHR WGDR {} (resolvable {}); by CELL/LGTM/LIGH {} (resolvable {})",
        hnam_refs.len(),
        hnam_refs.iter().filter(|f| voli_ids.contains(f)).count(),
        wgdr_refs.len(),
        wgdr_refs.iter().filter(|f| gdry_ids.contains(f)).count(),
        other_refs.len(),
        other_refs.iter().filter(|f| gdry_ids.contains(f)).count(),
    );
    Ok(())
}
