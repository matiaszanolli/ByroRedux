//! One entry point for the plugin "open an ESM, print one thing" probes
//! (#5856 folded four one-purpose examples into subcommands; the censuses
//! stay as their own examples).
//!
//! ```text
//! cargo run --release -p byroredux-plugin --example inspect -- base-model <ESM> <form hex>...
//! cargo run --release -p byroredux-plugin --example inspect -- cell-refs-by-base <ESM> <cell editor id> <base hex>...
//! cargo run --release -p byroredux-plugin --example inspect -- sound-names <ESM> < names.txt
//! cargo run --release -p byroredux-plugin --example inspect -- script-cells <ESM> <script>...
//! cargo run --release -p byroredux-plugin --example inspect -- quest-dialogue <ESM> <quest|grep:text|speaker:npc>
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
        "quest-dialogue" => quest_dialogue(&rest),
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

/// `quest-dialogue <ESM> <QUEST EDID|FORMID_HEX>` — one quest's stages (with
/// each stage item's result script), objectives and owned dialogue (INFO
/// conditions, Begin/End result scripts, TCLT/NAME links): the derivation
/// behind a P4 quest fixture on a QSTI-owned (Oblivion / FO3 / FNV) master,
/// where no quest alias binds the speaker and the INFO conditions are the
/// only speaker filter. `grep:<text>` lists quests by EDID/name;
/// `speaker:<NPC_ hex>` lists every INFO whose `GetIsID` names that base.
fn quest_dialogue(args: &[String]) -> anyhow::Result<()> {
    let (Some(esm), Some(wanted)) = (args.first(), args.get(1).cloned()) else {
        anyhow::bail!("usage: quest-dialogue <ESM> <QUEST EDID|FORMID|grep:text|speaker:NPC_>");
    };
    let index = parse_esm(&std::fs::read(esm)?)?;

    if let Some(needle) = wanted.strip_prefix("grep:") {
        let needle = needle.to_ascii_lowercase();
        let mut hits: Vec<_> = index
            .quests
            .values()
            .filter(|q| {
                q.editor_id.to_ascii_lowercase().contains(&needle)
                    || q.full_name.to_ascii_lowercase().contains(&needle)
            })
            .collect();
        hits.sort_by_key(|q| q.form_id);
        for q in hits {
            println!("QUST {:08X} {} {:?}", q.form_id, q.editor_id, q.full_name);
        }
        return Ok(());
    }

    // `speaker:<NPC_ hex>` — every INFO, under any quest, whose conditions
    // name that base as the speaker (`GetIsID`, CTDA function 72).
    if let Some(speaker) = wanted.strip_prefix("speaker:") {
        let speaker = u32::from_str_radix(speaker.trim_start_matches("0x"), 16)?;
        let mut dials: Vec<_> = index.dialogues.values().collect();
        dials.sort_by_key(|d| d.form_id);
        for dial in dials {
            for info in &dial.infos {
                if !info
                    .conditions
                    .iter()
                    .any(|c| c.function_index == 72 && c.param_1 == speaker)
                {
                    continue;
                }
                let quest = index
                    .quests
                    .get(&info.quest)
                    .map(|q| q.editor_id.as_str())
                    .unwrap_or("?");
                println!(
                    "{} {:08X} / INFO {:08X} quest={} {:?} conds={} tclt={}",
                    dial.editor_id,
                    dial.form_id,
                    info.form_id,
                    quest,
                    info.response_text.chars().take(70).collect::<String>(),
                    info.conditions.len(),
                    info.topic_links.len()
                );
            }
        }
        return Ok(());
    }

    let form = u32::from_str_radix(wanted.trim_start_matches("0x"), 16).ok();
    let quest = index
        .quests
        .values()
        .find(|q| Some(q.form_id) == form || q.editor_id.eq_ignore_ascii_case(&wanted))
        .ok_or_else(|| anyhow::anyhow!("no QUST {wanted}"))?;

    let actor_name = |id: u32| {
        index
            .npcs
            .get(&id)
            .or_else(|| index.creatures.get(&id))
            .map(|n| n.editor_id.clone())
    };

    println!(
        "QUST {:08X} {} {:?} flags={:#06x} priority={} script={:08X} delay={:?}",
        quest.form_id,
        quest.editor_id,
        quest.full_name,
        quest.quest_flags,
        quest.priority,
        quest.script_ref,
        quest.quest_delay
    );
    for stage in &quest.stages {
        println!(
            "  STAGE {:>4} flags={:#04x} script={} log={:?}",
            stage.index,
            stage.flags,
            stage.has_script,
            stage.log_text.chars().take(90).collect::<String>()
        );
        for (item, entry) in stage.log_entries.iter().enumerate() {
            if let Some(script) = &entry.result_script {
                println!(
                    "    item {item} conds={} result ({} B, {} refs): {:?}",
                    entry.conditions.len(),
                    script.compiled.len(),
                    script.refs.len(),
                    script.source.as_deref().unwrap_or("").replace("\r\n", " | ")
                );
            }
        }
    }
    for objective in &quest.objectives {
        println!(
            "  OBJECTIVE {:>4} {:?}",
            objective.index,
            objective.text.chars().take(90).collect::<String>()
        );
    }

    let mut topics: Vec<_> = index
        .dialogues
        .values()
        .filter(|d| d.quest_refs.contains(&quest.form_id))
        .collect();
    topics.sort_by_key(|d| d.form_id);
    for dial in topics {
        let infos: Vec<_> = dial
            .infos
            .iter()
            .filter(|i| i.quest == quest.form_id)
            .collect();
        println!(
            "DIAL {:08X} {} {:?} category={:?} flags={:?} owners={} infos(this quest)={}",
            dial.form_id,
            dial.editor_id,
            dial.full_name,
            dial.category,
            dial.data_flags,
            dial.quest_refs.len(),
            infos.len()
        );
        for info in infos {
            println!(
                "  INFO {:08X} {} prev={:08X} data={:?}",
                info.form_id, info.editor_id, info.previous_info, info.data
            );
            println!(
                "    text={:?}",
                info.response_text.chars().take(110).collect::<String>()
            );
            for c in &info.conditions {
                let named = [c.param_1, c.param_2]
                    .iter()
                    .filter_map(|p| actor_name(*p))
                    .collect::<Vec<_>>();
                println!(
                    "    CTDA fn={} {:?} {:?} p1={:08X} p2={:08X} run_on={:?} {}",
                    c.function_index,
                    c.comparator,
                    c.comparand,
                    c.param_1,
                    c.param_2,
                    c.run_on,
                    named.join(",")
                );
            }
            for (label, script) in [("begin", &info.result_script_begin), ("end", &info.result_script_end)] {
                if let Some(script) = script {
                    println!(
                        "    RESULT {label} ({} B): {:?}",
                        script.compiled.len(),
                        script.source.as_deref().unwrap_or("").replace("\r\n", " | ")
                    );
                }
            }
            if !info.topic_links.is_empty() {
                println!("    TCLT {:08X?}", info.topic_links);
            }
            if !info.added_topics.is_empty() {
                println!("    NAME {:08X?}", info.added_topics);
            }
        }
    }
    Ok(())
}
