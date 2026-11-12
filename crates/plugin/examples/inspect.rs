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
/// Phase 0 census (#6851, `docs/prd/cross-cell-actor-travel.md` §2 / §8). For every placed actor
/// (a reference whose base is an `NPC_` / `CREA`) it walks the template-resolved `PKID` list and
/// keeps each package whose location is a **Near Reference** (`PLDT` type 0; on Skyrim+ the first
/// such `Location` data input): one (actor, package) pair. A pair is **cross-space** when
/// `EsmCellIndex::locate_refr` puts the destination in another space than the actor's home — an
/// interior cell, or a whole worldspace — and its hops are the BFS distance over load doors (a
/// placed `XTEL` door in space A whose destination door lies in space B is an edge A→B). Prints one
/// Markdown table row, ten cross-space pairs spread over the form-sorted list (`SHOW_ALL=1`: all),
/// and, given a REFR and a PACK, that pair (the FNV pin is `fnv … 104E85 10A216`, SunnyRef's
/// VCG02SunnyTravelOutside: 1 hop).
fn cross_space_census(args: &[String]) -> anyhow::Result<()> {
    use byroredux_plugin::equip::resolve_inherited_ai_packages;
    use byroredux_plugin::esm::cell::CellRef;
    use byroredux_plugin::esm::records::{effective_actor_level, PackDataValue, PackLocationTarget, PackRecord};
    use std::collections::{HashMap, HashSet, VecDeque};
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
    // `locate_refr` names an interior by its EDID, which FO4 leaves empty on some cells: those
    // fall back to the index's own `cell_<FORMID>` key so two of them stay two spaces.
    let unnamed: HashMap<u32, &str> = cells
        .cells
        .iter()
        .filter(|(_, cell)| cell.editor_id.is_empty())
        .flat_map(|(key, cell)| cell.references.iter().map(move |r| (r.form_id, key.as_str())))
        .collect();
    let (mut names, mut ids) = (Vec::<String>::new(), HashMap::<(bool, String), usize>::new());
    let mut space_of = |form: u32| -> Option<usize> {
        let (key, name) = match cells.locate_refr(form)?.0 {
            CellRef::Interior { editor_id: "" } => {
                let key = unnamed.get(&form)?;
                ((true, key.to_string()), key.to_string())
            }
            CellRef::Interior { editor_id } => ((true, editor_id.to_ascii_lowercase()), editor_id.to_string()),
            CellRef::Exterior { worldspace, .. } => (
                (false, worldspace.to_string()),
                cells.worldspaces.get(worldspace).map_or(worldspace.to_string(), |w| w.editor_id.clone()),
            ),
        };
        Some(*ids.entry(key).or_insert_with(|| {
            names.push(name);
            names.len() - 1
        }))
    };
    let near_reference = |pack: &PackRecord| {
        let inputs = pack.data_inputs.iter().filter_map(|input| match &input.value {
            PackDataValue::Location(location) => Some(location),
            _ => None,
        });
        pack.location.iter().chain(inputs).find_map(|location| match location.target {
            PackLocationTarget::NearReference(form) if form != 0 => Some(form),
            _ => None,
        })
    };

    let references = cells
        .cells
        .values()
        .chain(cells.exterior_cells.values().flat_map(|grids| grids.values()))
        .chain(cells.worldspace_persistent_cells.values())
        .flat_map(|cell| cell.references.iter());
    let mut edges = HashMap::<usize, HashSet<usize>>::new();
    let (mut doors, mut seen) = (0usize, HashSet::new());
    // (actor REFR, base, package, home space, destination space or None when not placed here).
    let mut pairs = Vec::new();
    let (mut actors_with_packages, mut to_player) = (0usize, 0usize);
    for r in references {
        if !seen.insert(r.form_id) {
            continue;
        }
        if let Some(teleport) = &r.teleport {
            doors += 1;
            if let (Some(a), Some(b)) = (space_of(r.form_id), space_of(teleport.destination)) {
                if a != b {
                    edges.entry(a).or_default().insert(b);
                }
            }
        }
        let Some(npc) = index.npcs.get(&r.base_form_id).or_else(|| index.creatures.get(&r.base_form_id)) else {
            continue;
        };
        let packages = &resolve_inherited_ai_packages(npc, effective_actor_level(npc), &index).ai_packages;
        actors_with_packages += usize::from(!packages.is_empty());
        let Some(home) = space_of(r.form_id) else { continue };
        let mut counted = HashSet::new();
        for pack in packages.iter().filter(|p| counted.insert(**p)).filter_map(|p| index.packages.get(p)) {
            if let Some(destination) = near_reference(pack) {
                let space = space_of(destination);
                // PlayerRef (0x14) is in no cell: a destination that moves with the player.
                to_player += usize::from(space.is_none() && destination == 0x14);
                pairs.push((r.form_id, npc.editor_id.as_str(), pack, home, space));
            }
        }
    }

    let hops = |from: usize, to: usize| -> Option<usize> {
        let (mut distance, mut queue) = (HashMap::from([(from, 0usize)]), VecDeque::from([from]));
        while let Some(at) = queue.pop_front() {
            if at == to {
                return distance.get(&at).copied();
            }
            for &next in edges.get(&at).into_iter().flatten() {
                if !distance.contains_key(&next) {
                    distance.insert(next, distance[&at] + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    };
    let mut buckets = [0usize; 4]; // 1, 2, 3+, unreachable
    let (mut cross, mut unplaced) = (Vec::new(), 0usize);
    for pair in &pairs {
        match pair.4 {
            None => unplaced += 1,
            Some(destination) if destination != pair.3 => {
                let distance = hops(pair.3, destination);
                buckets[distance.map_or(3, |d| d.clamp(1, 3) - 1)] += 1;
                cross.push((pair, distance));
            }
            Some(_) => {}
        }
    }
    let show = |(pair, distance): &(&(u32, &str, &PackRecord, usize, Option<usize>), Option<usize>)| {
        let (actor, base, pack, home, destination) = pair;
        let hops = distance.map_or("unreachable".to_string(), |d| d.to_string());
        let destination = destination.map_or("(not placed)", |d| names[d].as_str());
        println!(
            "| {actor:08X} {base} | {:08X} {} | {} | {destination} | {hops} |",
            pack.form_id, pack.editor_id, names[*home]
        );
    };
    println!("{esm}: {} spaces, {doors} load doors, {} space edges", names.len(), edges.values().map(HashSet::len).sum::<usize>());
    println!("| game | actors with packages | pairs (ref destination) | destination not placed | cross-space | 1 hop | 2 hops | 3+ hops | unreachable |");
    println!("|---|---|---|---|---|---|---|---|---|");
    println!(
        "| {game} | {actors_with_packages} | {} | {unplaced} ({to_player} player) | {} | {} | {} | {} | {} |",
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
            Some(pair) => show(&(pair, pair.4.and_then(|d| if d == pair.3 { Some(0) } else { hops(pair.3, d) }))),
            None => println!("pin {actor:08X} / {pack:08X}: not a counted pair"),
        }
    }
    Ok(())
}
