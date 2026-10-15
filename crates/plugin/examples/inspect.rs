//! One entry point for the plugin "open an ESM, print one thing" probes
//! (#5856 folded four one-purpose examples into subcommands; the censuses
//! stay as their own examples).
//!
//! ```text
//! cargo run --release -p byroredux-plugin --example inspect -- base-model <ESM> <form hex>...
//! cargo run --release -p byroredux-plugin --example inspect -- cell-refs-by-base <ESM> <cell editor id> <base hex>...
//! cargo run --release -p byroredux-plugin --example inspect -- sound-names <ESM> < names.txt
//! cargo run --release -p byroredux-plugin --example inspect -- script-cells <ESM> <script>...
//! ```

use byroredux_plugin::esm::parse_esm;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().expect("a subcommand (see the module doc)");
    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "base-model" => base_model(&rest),
        "cell-refs-by-base" => cell_refs_by_base(&rest),
        "sound-names" => sound_names(&rest),
        "script-cells" => script_cells(&rest),
        other => anyhow::bail!("unknown subcommand {other:?} (see the module doc)"),
    }
}

/// `base-model <ESM> <form hex>...` — the model path of base records by id.
fn base_model(args: &[String]) -> anyhow::Result<()> {
    let index = parse_esm(&std::fs::read(&args[0])?)?;
    for arg in &args[1..] {
        let form = u32::from_str_radix(arg.trim_start_matches("0x"), 16).expect("hex");
        match index.cells.statics.get(&form) {
            Some(object) => {
                println!("{form:08X} {} {}", object.editor_id, object.model_path);
                for instance in object.script_instance.iter().flat_map(|s| s.scripts.iter()) {
                    println!("    base script {}: {} properties", instance.name, instance.properties.len());
                    for property in &instance.properties {
                        println!("      {} = {:?}", property.name, property.value);
                    }
                }
            }
            None => println!("{form:08X} not a model record"),
        }
    }
    Ok(())
}

/// `cell-refs-by-base <ESM> <cell> <base hex>...` — a cell's references of
/// the given bases with game-space positions (where to put the player).
fn cell_refs_by_base(args: &[String]) -> anyhow::Result<()> {
    let cell = args[1].to_ascii_lowercase();
    let bases: Vec<u32> = args[2..]
        .iter()
        .map(|b| u32::from_str_radix(b.trim_start_matches("0x"), 16).expect("hex"))
        .collect();
    let index = parse_esm(&std::fs::read(&args[0])?)?;
    let data = index.cells.cells.get(&cell).expect("the cell");
    for r in data.references.iter().filter(|r| bases.contains(&r.base_form_id)) {
        let name = index.cells.statics.get(&r.base_form_id).map(|s| s.editor_id.as_str()).unwrap_or("?");
        println!(
            "REFR {:08X} base {:08X} {name} pos {:?} rot {:?}",
            r.form_id, r.base_form_id, r.position, r.rotation
        );
    }
    Ok(())
}

/// `sound-names <ESM> < names.txt` — how many stdin editor ids name a sound
/// record (the check that a graph's `SoundPlay.<name>` notify names one).
fn sound_names(args: &[String]) -> anyhow::Result<()> {
    use std::io::BufRead;
    let index = parse_esm(&std::fs::read(&args[0])?)?;
    let by_edid: std::collections::HashMap<String, &byroredux_plugin::esm::records::SounRecord> =
        index.sounds.values().map(|s| (s.editor_id.to_ascii_lowercase(), s)).collect();
    let (mut found, mut missing, mut files) = (0, Vec::new(), 0);
    for line in std::io::stdin().lock().lines() {
        let name = line?.trim().to_string();
        if name.is_empty() {
            continue;
        }
        match by_edid.get(&name.to_ascii_lowercase()) {
            Some(record) => {
                found += 1;
                files += usize::from(!record.sound_path.is_empty() || !record.variants.is_empty());
            }
            None => missing.push(name),
        }
    }
    println!(
        "{} sound records; {found} found ({files} with a file); {} missing: {missing:?}",
        index.sounds.len(),
        missing.len()
    );
    Ok(())
}

/// `script-cells <ESM> <script>...` — which interior cells place a reference
/// carrying the named script (on the REFR's own VMAD). `SHOW_REFS=1` lists
/// the references and their activate parents.
fn script_cells(args: &[String]) -> anyhow::Result<()> {
    let wanted: Vec<String> = args[1..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let index = parse_esm(&std::fs::read(&args[0])?)?;
    for (cell_key, cell) in &index.cells.cells {
        let mut hits = std::collections::BTreeMap::<String, usize>::new();
        for r in &cell.references {
            let Some(instance) = &r.script_instance else { continue };
            for script in &instance.scripts {
                let name = script.name.to_ascii_lowercase();
                if wanted.contains(&name) {
                    *hits.entry(name.clone()).or_default() += 1;
                    if std::env::var("SHOW_REFS").is_ok() {
                        let parents: Vec<String> =
                            r.activate_parents.iter().map(|p| format!("{:08X}", p.reference)).collect();
                        println!("    {cell_key} {name} REFR {:08X} activate parents {parents:?}", r.form_id);
                    }
                }
            }
        }
        if !hits.is_empty() {
            println!("{cell_key}: {hits:?}");
        }
    }
    Ok(())
}
