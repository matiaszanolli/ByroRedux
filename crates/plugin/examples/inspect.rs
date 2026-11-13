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
//! cargo run --release -p byroredux-plugin --example inspect -- wthr-dump <oblivion|fo3nv|skyrim|fo4|fo76|starfield> <ESM>
//! cargo run --release -p byroredux-plugin --example inspect -- cross-space-census <game> <ESM> [<actor REFR hex> <PACK hex>]
//! ```
//!
//! `<game>` is `oblivion`, `fo3` / `fnv` (or `fo3nv`), `skyrim`, `fo4`, `fo76` or `starfield`.

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
        "wthr-dump" => wthr_dump(&rest),
        "cross-space-census" => cross_space_census(&rest),
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

/// `wthr-dump <game> <ESM>` — every decoded `WeatherRecord`, sorted by FormID, as `Debug` text. A
/// refactor pin for `parse_wthr` (#6223): dump a master before and after the change and `diff` the
/// two files; a behaviour-preserving refactor leaves them byte-identical, which the hand-built
/// per-game fixtures cannot show for real content.
fn wthr_dump(args: &[String]) -> anyhow::Result<()> {
    use byroredux_plugin::esm::reader::EsmReader;
    let game = game_kind(&args[0])?;
    let bytes = std::fs::read(&args[1])?;
    let mut reader = EsmReader::new(&bytes);
    reader.read_file_header()?;
    let mut records = Vec::new();
    reader.for_each_record(bytes.len(), Some(b"WTHR"), |header, subs| {
        records.push(byroredux_plugin::esm::records::parse_wthr(header.form_id, &subs, game, &None));
    })?;
    records.sort_by_key(|record| record.form_id);
    println!("# {} WTHR records from {}", records.len(), args[1]);
    for record in &records {
        println!("{record:#?}");
    }
    Ok(())
}

/// A `<game>` argument → the parser's game kind (FO3 and New Vegas share one).
fn game_kind(name: &str) -> anyhow::Result<byroredux_plugin::esm::reader::GameKind> {
    use byroredux_plugin::esm::reader::GameKind;
    Ok(match name {
        "oblivion" => GameKind::Oblivion,
        "fo3" | "fnv" | "fo3nv" => GameKind::Fallout3NV,
        "skyrim" => GameKind::Skyrim,
        "fo4" => GameKind::Fallout4,
        "fo76" => GameKind::Fallout76,
        "starfield" => GameKind::Starfield,
        other => anyhow::bail!("unknown game {other:?}"),
    })
}

/// `cross-space-census <game> <ESM> [<actor REFR hex> <PACK hex>]` — the cross-cell actor travel
/// Phase 0 census (#6851, `docs/prd/cross-cell-actor-travel.md` §8). For every placed actor (a
/// reference whose base is an `NPC_` / `CREA`) it walks the template-resolved `PKID` list and keeps
/// each package whose location names a reference ([`PackRecord::location_reference`]): one (actor,
/// package) pair. A pair is **cross-space** when the destination's space is not the actor's home
/// space; its hop count is the fewest load doors between the two in the engine's own routing graph
/// ([`SpaceGraph`], navmesh door links applied where the game authors them), so the census measures
/// the routes the engine takes. Prints the Markdown table row, ten cross-space pairs spread over the
/// form-sorted list (`SHOW_ALL=1`: all), and the pinned pair when one is given.
fn cross_space_census(args: &[String]) -> anyhow::Result<()> {
    use byroredux_plugin::equip::resolve_inherited_ai_packages;
    use byroredux_plugin::esm::cell::{SpaceGraph, SpaceKey};
    use byroredux_plugin::esm::records::{effective_actor_level, PackRecord};
    use std::collections::HashSet;
    let (Some(game), Some(esm)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: cross-space-census <game> <ESM> [<actor REFR hex> <PACK hex>]");
    };
    let hex = |s: &String| u32::from_str_radix(s.trim_start_matches("0x"), 16);
    let pin = match (args.get(2), args.get(3)) {
        (Some(actor), Some(pack)) => Some((hex(actor)?, hex(pack)?)),
        _ => None,
    };
    let index = parse_esm(&std::fs::read(esm)?)?;
    if index.game != game_kind(game)? {
        eprintln!("warning: {esm} parses as {:?}, not {game}", index.game);
    }
    let cells = &index.cells;
    let links = index.navmesh_door_links();
    let graph = SpaceGraph::build(cells, &links);
    let every_door = SpaceGraph::build(cells, &HashSet::new());
    let name = |key: &SpaceKey| match key {
        SpaceKey::Interior(cell) => cell.clone(),
        SpaceKey::Worldspace(world) => cells.worldspaces.get(world).map_or(world.clone(), |w| w.editor_id.clone()),
    };

    let references = cells
        .cells
        .values()
        .chain(cells.exterior_cells.values().flat_map(|grids| grids.values()))
        .chain(cells.worldspace_persistent_cells.values())
        .flat_map(|cell| cell.references.iter());
    let mut seen = HashSet::new();
    // (actor REFR, base, package, persistent, home space, destination space or None when not placed).
    let mut pairs = Vec::new();
    let (mut actors_with_packages, mut to_player) = (0usize, 0usize);
    for r in references {
        if !seen.insert(r.form_id) {
            continue;
        }
        let Some(npc) = index.npcs.get(&r.base_form_id).or_else(|| index.creatures.get(&r.base_form_id)) else {
            continue;
        };
        let packages = &resolve_inherited_ai_packages(npc, effective_actor_level(npc), &index).ai_packages;
        actors_with_packages += usize::from(!packages.is_empty());
        let Some(home) = cells.space_of_refr(r.form_id) else { continue };
        let mut counted = HashSet::new();
        for pack in packages.iter().filter(|p| counted.insert(**p)).filter_map(|p| index.packages.get(p)) {
            if let Some(destination) = pack.location_reference() {
                let space = cells.space_of_refr(destination);
                // PlayerRef (0x14) is in no cell: a destination that moves with the player.
                to_player += usize::from(space.is_none() && destination == 0x14);
                pairs.push((r.form_id, npc.editor_id.as_str(), pack, r.is_persistent(), home.clone(), space));
            }
        }
    }

    let hops = |from: &SpaceKey, to: &SpaceKey| graph.id(from).zip(graph.id(to)).and_then(|(a, b)| graph.hops(a, b));
    let mut buckets = [0usize; 4]; // 1, 2, 3+, unreachable
    let (mut cross, mut unplaced, mut persistent) = (Vec::new(), 0usize, 0usize);
    for pair in &pairs {
        match &pair.5 {
            None => unplaced += 1,
            Some(destination) if *destination != pair.4 => {
                let distance = hops(&pair.4, destination);
                buckets[distance.map_or(3, |d| d.clamp(1, 3) - 1)] += 1;
                persistent += usize::from(pair.3);
                cross.push((pair, distance));
            }
            Some(_) => {}
        }
    }
    type Pair<'a> = (u32, &'a str, &'a PackRecord, bool, SpaceKey, Option<SpaceKey>);
    let show = |(pair, distance): &(&Pair, Option<usize>)| {
        let (actor, base, pack, _, home, destination) = pair;
        let hops = distance.map_or("unreachable".to_string(), |d| d.to_string());
        let destination = destination.as_ref().map_or("(not placed)".to_string(), name);
        println!("| {actor:08X} {base} | {:08X} {} | {} | {destination} | {hops} |", pack.form_id, pack.editor_id, name(home));
    };
    let linked = every_door.doors().iter().filter(|door| links.contains(&door.door)).count();
    println!(
        "{esm}: {} spaces, {} load doors ({} on a navmesh door link{}), {} space edges",
        graph.space_count(),
        every_door.doors().len(),
        linked,
        if links.is_empty() { "; the game authors none, so every door routes" } else { "" },
        graph.edge_count()
    );
    println!("| game | actors with packages | pairs (ref destination) | destination not placed | cross-space | persistent actor | 1 hop | 2 hops | 3+ hops | unreachable |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    println!(
        "| {game} | {actors_with_packages} | {} | {unplaced} ({to_player} player) | {} | {persistent} | {} | {} | {} | {} |",
        pairs.len(),
        cross.len(),
        buckets[0],
        buckets[1],
        buckets[2],
        buckets[3]
    );
    cross.sort_by_key(|(pair, _)| (pair.0, pair.2.form_id));
    println!("\n| actor REFR / base | package | home space | destination space | hops |\n|---|---|---|---|---|");
    let step = if std::env::var("SHOW_ALL").is_ok() { 1 } else { cross.len().div_ceil(10).max(1) };
    cross.iter().step_by(step).for_each(show);
    if let Some((actor, pack)) = pin {
        match pairs.iter().find(|pair| pair.0 == actor && pair.2.form_id == pack) {
            Some(pair) => {
                let distance = pair.5.as_ref().and_then(|d| if *d == pair.4 { Some(0) } else { hops(&pair.4, d) });
                show(&(pair, distance))
            }
            None => println!("pin {actor:08X} / {pack:08X}: not a counted pair"),
        }
    }
    Ok(())
}
