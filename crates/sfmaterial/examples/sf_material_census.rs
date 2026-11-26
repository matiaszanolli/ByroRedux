//! Starfield material-map census (SMM-OBS-004 / -005, #7420): for the
//! material paths Starfield models name, what the base Component Database
//! authors for opacity, roughness, metalness and ambient occlusion, under
//! the one-layer rules of `docs/prd/starfield-material-maps.md`
//! (SMM-SRC-010, SMM-OPA-005). It reports the database side only: an
//! Override and the canonical material are the engine's reach test
//! (SMM-OBS-006).
//!
//! With `--textures`, it also reads the DX10 record headers (nothing is
//! extracted) for each property's texture formats and added bytes.
//!
//! The path list comes from `byroredux-nif`'s `sf_matpath_dump`:
//!
//! ```text
//! cargo run --release -p byroredux-nif --example sf_matpath_dump -- --limit 1000000 \
//!     "<Data>/Starfield - Meshes01.ba2" "<Data>/Starfield - Meshes02.ba2" \
//!     "<Data>/Starfield - MeshesPatch.ba2" > paths.txt
//! cargo run --release -p byroredux-sfmaterial --example sf_material_census -- \
//!     "<Data>/Starfield - Materials.ba2" paths.txt \
//!     --textures "<Data>/Starfield - Textures01.ba2" … "<Data>/Starfield - TexturesPatch02.ba2"
//! ```
//!
//! List texture archives in load order; a later one wins a path.

use byroredux_bsa::{Ba2Archive, Dx10Layout};
use byroredux_sfmaterial::{
    material_layer_number, CdbLayeredMaterial, CdbMaterial, CdbSlotSet, MaterialIndex,
    SLOT_AMBIENT_OCCLUSION, SLOT_COLOR, SLOT_EMISSIVE, SLOT_METALNESS, SLOT_NORMAL, SLOT_OPACITY,
    SLOT_ROUGHNESS,
};
use std::collections::{BTreeMap, BTreeSet};

const PROPERTIES: [(&str, u8); 4] = [
    ("opacity", SLOT_OPACITY),
    ("roughness", SLOT_ROUGHNESS),
    ("metalness", SLOT_METALNESS),
    ("ambient occlusion", SLOT_AMBIENT_OCCLUSION),
];

/// What one chosen layer holds for a slot.
#[derive(Default)]
struct Holds {
    texture_only: usize,
    replacement_only: usize,
    both: usize,
    neither: usize,
}

impl Holds {
    fn add(&mut self, set: Option<&CdbSlotSet>, slot: u8) {
        let texture = set.and_then(|s| s.texture(slot)).is_some();
        let replacement = set.and_then(|s| s.replacement(slot)).is_some();
        match (texture, replacement) {
            (true, false) => self.texture_only += 1,
            (false, true) => self.replacement_only += 1,
            (true, true) => self.both += 1,
            (false, false) => self.neither += 1,
        }
    }

    fn row(&self, label: &str) -> String {
        format!(
            "| {label} | {} | {} | {} | {} |",
            self.texture_only, self.replacement_only, self.both, self.neither
        )
    }
}

#[derive(Default)]
struct SlotStats {
    /// SMM-SRC-010's layer (roughness / metalness / occlusion).
    chosen: Holds,
    /// SMM-OBS-005: more than one layer supplies the slot.
    multi_layer: usize,
    /// The SMM-SRC-010 layer is not the material's first layer.
    chosen_is_later: usize,
    /// `lookup` would take the texture and the replacement from two layers.
    lookup_mixes: usize,
    /// Replacement colours on the chosen layer: grey (r = g = b) or not.
    replacement_grey: usize,
    replacement_coloured: usize,
    /// Red channel of those replacements, as an 8-bit value.
    replacement_red: BTreeMap<u8, usize>,
}

impl SlotStats {
    fn add_replacement(&mut self, color: [f32; 4]) {
        let [r, g, b, _] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        if r == g && g == b {
            self.replacement_grey += 1;
        } else {
            self.replacement_coloured += 1;
        }
        *self.replacement_red.entry(r).or_default() += 1;
    }
}

/// How `OpacitySourceLayer` resolves for one material.
enum Named<'a> {
    Layer(&'a CdbSlotSet),
    /// Layer 0 with no numbered layers: only a root-named texture set.
    RootOnly(&'a CdbSlotSet),
    Unresolved,
}

fn named_layer(material: &CdbLayeredMaterial) -> (u8, Named<'_>) {
    // Unauthored is the class default, layer zero (`libfo76utils`
    // `material.cpp`, `alphaSourceLayer(0)`).
    let number = match &material.opacity_source_layer {
        None => Some(0),
        Some(name) => material_layer_number(name),
    };
    let Some(number) = number else {
        return (u8::MAX, Named::Unresolved);
    };
    let named = match material.layer(number) {
        Some(set) => Named::Layer(set),
        None if number == 0 && material.layers.is_empty() => Named::RootOnly(&material.root_sets),
        None => Named::Unresolved,
    };
    (number, named)
}

/// An effect material as `libfo76utils` routes one (`renderer.cpp`,
/// `isEffect`): effect settings and a shader route other than the default
/// `Deferred`.
fn is_effect(material: &CdbLayeredMaterial) -> bool {
    material.has_effect_settings
        && material
            .shader_route
            .as_deref()
            .is_some_and(|r| r != "Deferred")
}

#[derive(Default)]
struct Opacity {
    flag_on: usize,
    flag_on_effect: usize,
    flag_on_holds: Holds,
    flag_on_root_only: usize,
    flag_on_unresolved: usize,
    named_nonzero: usize,
    named_nonzero_resolved: usize,
    named_strings: BTreeMap<String, usize>,
    threshold_positive: usize,
    threshold_zero: usize,
    threshold_absent: usize,
    flag_off_with_slot_deferred: usize,
    flag_off_with_slot_effect: usize,
    flag_off_false: usize,
    flag_off_absent: usize,
}

impl Opacity {
    fn add(&mut self, merged: &CdbMaterial, material: &CdbLayeredMaterial) {
        let has_slot = material
            .sets_in_order()
            .any(|(_, s)| s.supplies(SLOT_OPACITY));
        if merged.has_opacity != Some(true) {
            if has_slot {
                if is_effect(material) {
                    self.flag_off_with_slot_effect += 1;
                } else {
                    self.flag_off_with_slot_deferred += 1;
                }
                match merged.has_opacity {
                    Some(_) => self.flag_off_false += 1,
                    None => self.flag_off_absent += 1,
                }
            }
            return;
        }
        self.flag_on += 1;
        self.flag_on_effect += usize::from(is_effect(material));
        match merged.alpha_test_threshold {
            Some(t) if t > 0.0 => self.threshold_positive += 1,
            Some(_) => self.threshold_zero += 1,
            None => self.threshold_absent += 1,
        }
        let authored = material.opacity_source_layer.clone();
        *self
            .named_strings
            .entry(authored.unwrap_or_else(|| "(not authored)".to_string()))
            .or_default() += 1;
        let (number, named) = named_layer(material);
        if number != 0 {
            self.named_nonzero += 1;
            self.named_nonzero_resolved += usize::from(!matches!(named, Named::Unresolved));
        }
        match named {
            Named::Layer(set) => self.flag_on_holds.add(Some(set), SLOT_OPACITY),
            Named::RootOnly(set) => {
                self.flag_on_root_only += 1;
                self.flag_on_holds.add(Some(set), SLOT_OPACITY);
            }
            Named::Unresolved => self.flag_on_unresolved += 1,
        }
    }
}

/// The texture the rules would bind for each property of one material.
fn chosen_textures<'a>(
    merged: &CdbMaterial,
    material: &'a CdbLayeredMaterial,
) -> [Option<&'a str>; 4] {
    PROPERTIES.map(|(_, slot)| {
        if slot != SLOT_OPACITY {
            return material.first_supplying(slot)?.1.texture(slot);
        }
        // SMM-OPA-005 / -006: the named layer, only with the flag on.
        if merged.has_opacity != Some(true) {
            return None;
        }
        match named_layer(material).1 {
            Named::Layer(set) | Named::RootOnly(set) => set.texture(slot),
            Named::Unresolved => None,
        }
    })
}

/// A database texture path as the BA2 name table spells it.
fn archive_path(path: &str) -> String {
    let lower = path.replace('/', "\\").to_ascii_lowercase();
    lower
        .strip_prefix("data\\")
        .map(str::to_string)
        .unwrap_or(lower)
}

fn format_name(dxgi: u8) -> String {
    let name = match dxgi {
        28 => "R8G8B8A8_UNORM",
        29 => "R8G8B8A8_UNORM_SRGB",
        61 => "R8_UNORM",
        71 => "BC1_UNORM",
        72 => "BC1_UNORM_SRGB",
        77 => "BC3_UNORM",
        78 => "BC3_UNORM_SRGB",
        80 => "BC4_UNORM",
        81 => "BC4_SNORM",
        83 => "BC5_UNORM",
        84 => "BC5_SNORM",
        87 => "B8G8R8A8_UNORM",
        91 => "B8G8R8A8_UNORM_SRGB",
        98 => "BC7_UNORM",
        99 => "BC7_UNORM_SRGB",
        other => return format!("DXGI {other}"),
    };
    name.to_string()
}

#[derive(Default)]
struct TextureBytes {
    textures: usize,
    missing: usize,
    all_levels: u64,
    /// Records in several chunks: the floor is the last chunk
    /// (`docs/engine/texture-streaming.md` §3.3).
    multi_chunk: usize,
    multi_chunk_floor: u64,
    /// One-chunk records: the floor is all of the record or its levels at
    /// or below 256 px, so their whole size is its upper bound.
    one_chunk_bytes: u64,
}

impl TextureBytes {
    fn add(&mut self, layout: Option<&Dx10Layout>) {
        self.textures += 1;
        let Some(layout) = layout else {
            self.missing += 1;
            return;
        };
        let bytes: u64 = layout
            .chunks
            .iter()
            .map(|c| u64::from(c.unpacked_size))
            .sum();
        self.all_levels += bytes;
        match layout.chunks.iter().max_by_key(|c| c.start_mip) {
            Some(last) if layout.chunks.len() > 1 => {
                self.multi_chunk += 1;
                self.multi_chunk_floor += u64::from(last.unpacked_size);
            }
            _ => self.one_chunk_bytes += bytes,
        }
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn texture_report(
    archives: &[Ba2Archive],
    bound: &[BTreeSet<String>; 4],
    today: &BTreeSet<String>,
) {
    let layout = |path: &str| archives.iter().rev().find_map(|a| a.dx10_layout(path));

    println!("\n## Texture formats of the chosen textures (SMM-SRC-003)\n");
    println!("| Property | Format | Textures |\n|---|---|---:|");
    for ((name, _), paths) in PROPERTIES.iter().zip(bound) {
        let mut formats: BTreeMap<String, usize> = BTreeMap::new();
        for path in paths {
            let format =
                layout(path).map_or("(not found)".to_string(), |l| format_name(l.dxgi_format));
            *formats.entry(format).or_default() += 1;
        }
        for (format, count) in formats {
            println!("| {name} | {format} | {count} |");
        }
    }

    println!("\n## Added texture bytes (PRD §8)\n");
    println!(
        "Distinct textures each property would bind that no material in the population binds as \
         colour, normal or emissive today. Sizes are unpacked bytes from the record headers.\n"
    );
    println!(
        "| Property | Added textures | Not found | All levels, MiB | Floor, MiB: multi-chunk records \
         | Floor, MiB: one-chunk records, at most | One-chunk records |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    let mut union: BTreeSet<&String> = BTreeSet::new();
    let row = |name: &str, paths: &mut dyn Iterator<Item = &String>| {
        let mut total = TextureBytes::default();
        for path in paths {
            total.add(layout(path).as_ref());
        }
        println!(
            "| {name} | {} | {} | {:.1} | {:.1} | {:.1} | {} |",
            total.textures,
            total.missing,
            mib(total.all_levels),
            mib(total.multi_chunk_floor),
            mib(total.one_chunk_bytes),
            total.textures - total.multi_chunk - total.missing,
        );
    };
    for ((name, _), paths) in PROPERTIES.iter().zip(bound) {
        row(name, &mut paths.difference(today));
        union.extend(paths.difference(today));
    }
    row("all four, each texture once", &mut union.into_iter());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let textures = match args.iter().position(|a| a == "--textures") {
        Some(at) => args.split_off(at).split_off(1),
        None => Vec::new(),
    };
    let [materials, paths] = args.as_slice() else {
        return Err(
            "usage: sf_material_census <materials.ba2> <paths.txt> [--textures <ba2>…]".into(),
        );
    };

    let cdb = Ba2Archive::open(materials)?.extract("materials\\materialsbeta.cdb")?;
    let index = MaterialIndex::build(&cdb)?;
    drop(cdb);
    let list = std::fs::read_to_string(paths)?;

    let mut named = 0usize;
    let mut resolved = 0usize;
    let mut with_layers: BTreeMap<usize, usize> = BTreeMap::new();
    let mut root_named = 0usize;
    let mut slots: [SlotStats; 4] = Default::default();
    let mut opacity = Opacity::default();
    let mut bound: [BTreeSet<String>; 4] = Default::default();
    let mut today: BTreeSet<String> = BTreeSet::new();

    for path in list.lines().filter(|l| !l.is_empty()) {
        named += 1;
        let (Some(merged), Some(material)) = (index.lookup(path), index.layered(path)) else {
            continue;
        };
        resolved += 1;
        *with_layers.entry(material.layers.len()).or_default() += 1;
        root_named += usize::from(material.root_sets != CdbSlotSet::default());

        for (stats, (_, slot)) in slots.iter_mut().zip(PROPERTIES) {
            let suppliers = material
                .sets_in_order()
                .filter(|(_, s)| s.supplies(slot))
                .count();
            stats.multi_layer += usize::from(suppliers > 1);
            let chosen = material.first_supplying(slot);
            stats.chosen.add(chosen.map(|(_, set)| set), slot);
            let first = material.sets_in_order().next().map(|(n, _)| n);
            stats.chosen_is_later += usize::from(chosen.is_some_and(|(n, _)| Some(n) != first));
            let texture_at = material
                .sets_in_order()
                .position(|(_, s)| s.texture(slot).is_some());
            let replacement_at = material
                .sets_in_order()
                .position(|(_, s)| s.replacement(slot).is_some());
            stats.lookup_mixes += usize::from(matches!(
                (texture_at, replacement_at),
                (Some(t), Some(r)) if t != r
            ));
            if let Some(color) = chosen.and_then(|(_, set)| set.replacement(slot)) {
                stats.add_replacement(color);
            }
        }
        opacity.add(&merged, &material);

        for (paths, texture) in bound.iter_mut().zip(chosen_textures(&merged, &material)) {
            if let Some(texture) = texture {
                paths.insert(archive_path(texture));
            }
        }
        for (slot, texture) in &merged.textures {
            if [SLOT_COLOR, SLOT_NORMAL, SLOT_EMISSIVE].contains(slot) {
                today.insert(archive_path(texture));
            }
        }
    }

    println!("## Population\n");
    println!("| Measure | Materials |\n|---|---:|");
    println!("| Paths named | {named} |\n| Resolved in the base database | {resolved} |");
    println!("| With a texture set named by the root object | {root_named} |");
    for (layers, count) in &with_layers {
        println!("| With {layers} numbered layer(s) | {count} |");
    }

    println!("\n## Roughness, metalness and occlusion: the SMM-SRC-010 layer\n");
    println!("| Property | Texture only | Replacement only | Both | Neither |\n|---|---:|---:|---:|---:|");
    for (stats, (name, _)) in slots.iter().zip(PROPERTIES).skip(1) {
        println!("{}", stats.chosen.row(name));
    }

    println!("\n## Layers (SMM-OBS-005)\n");
    println!(
        "| Property | More than one layer supplies it | SMM-SRC-010 layer is not the first \
         | `lookup` takes texture and replacement from two layers |\n|---|---:|---:|---:|"
    );
    for (stats, (name, _)) in slots.iter().zip(PROPERTIES) {
        println!(
            "| {name} | {} | {} | {} |",
            stats.multi_layer, stats.chosen_is_later, stats.lookup_mixes
        );
    }

    println!("\n## Replacement colours on the SMM-SRC-010 layer (supports SMM-SRC-004)\n");
    println!("| Property | Grey (r = g = b) | Not grey | Commonest red values (value × count) |");
    println!("|---|---:|---:|---|");
    for (stats, (name, _)) in slots.iter().zip(PROPERTIES) {
        let mut reds: Vec<(&u8, &usize)> = stats.replacement_red.iter().collect();
        reds.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let top: Vec<String> = reds
            .iter()
            .take(6)
            .map(|(v, n)| format!("{v} × {n}"))
            .collect();
        println!(
            "| {name} | {} | {} | {} |",
            stats.replacement_grey,
            stats.replacement_coloured,
            top.join(", ")
        );
    }

    println!("\n## Opacity with `HasOpacity` true (SMM-OPA-004, -005)\n");
    println!("| Measure | Materials |\n|---|---:|");
    println!("| `HasOpacity` true | {} |", opacity.flag_on);
    println!(
        "| …of which effect materials | {} |",
        opacity.flag_on_effect
    );
    println!("| Threshold above zero | {} |", opacity.threshold_positive);
    println!(
        "| Threshold authored as zero or less | {} |",
        opacity.threshold_zero
    );
    println!(
        "| No threshold on the chain | {} |",
        opacity.threshold_absent
    );
    println!("| Named layer is not layer 0 | {} |", opacity.named_nonzero);
    println!(
        "| …of which the index has a layer with that number | {} |",
        opacity.named_nonzero_resolved
    );
    println!(
        "| Named layer unresolved | {} |",
        opacity.flag_on_unresolved
    );
    println!(
        "| Layer 0 with only a root-named set | {} |",
        opacity.flag_on_root_only
    );
    for (name, count) in &opacity.named_strings {
        println!("| `OpacitySourceLayer` = {name} | {count} |");
    }
    println!("\n| Slot 2 on the named layer | Texture only | Replacement only | Both | Neither |");
    println!("|---|---:|---:|---:|---:|");
    println!(
        "{}",
        opacity
            .flag_on_holds
            .row("`HasOpacity` true, layer resolved")
    );

    println!("\n## Slot-2 data without `HasOpacity` true (SMM-OPA-006)\n");
    println!("| Measure | Materials |\n|---|---:|");
    println!(
        "| Effect materials | {} |",
        opacity.flag_off_with_slot_effect
    );
    println!(
        "| Not effect materials | {} |",
        opacity.flag_off_with_slot_deferred
    );
    println!(
        "| `HasOpacity` authored false | {} |",
        opacity.flag_off_false
    );
    println!(
        "| `HasOpacity` not on the chain | {} |",
        opacity.flag_off_absent
    );

    if !textures.is_empty() {
        let archives = textures
            .iter()
            .map(Ba2Archive::open)
            .collect::<Result<Vec<_>, _>>()?;
        texture_report(&archives, &bound, &today);
    }
    Ok(())
}
