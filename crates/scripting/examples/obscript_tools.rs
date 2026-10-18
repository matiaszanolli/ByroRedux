//! M47.3 (ObScript) triage tools — one example, five subcommands, because every
//! probe starts from the same parsed load order.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example obscript_tools -- <cmd> <args>
//! ```
//!
//! | cmd | args | answers |
//! |-----|------|---------|
//! | `census` | `<ESM>` | which `Begin <block>` types the quest / object / effect scripts use, the authored `QUST` quest-delay spread, and the ids the VM declines |
//! | `names` | `<ESM> [quest\|object\|all] [min-scripts]` | a *name* for each command id, by aligning each script's `SCTX` source with its compiled statements (no opcode table exists locally; this is how the VM's ids were recovered) |
//! | `live` | `<ESM> [rounds]` | what the real `ObScriptCommandHost` does with every scripted quest over a bare world: still-declined ids, objectives reached |
//! | `dump` | `<ESM> <formid\|grep:text\|grepq:text>` | one SCPT: locals, source, compiled bytes |
//! | `walk` | `<ESM> <formid>` | the statement framing of one SCPT, stopping at the first break |
//!
//! Command ids are `0x1000 +` the CTDA function index for every getter; the
//! `names` output is the evidence behind each id in
//! `obscript_quests.rs`.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    match cmd.as_str() {
        "census" => census::run(args),
        "names" => names::run(args),
        "live" => live::run(args),
        "dump" => dump::run(args),
        "walk" => walk::run(args),
        "avif" => avif::run(args),
        "avcodes" => avcodes::run(args),
        "find" => find::run(args),
        _ => {
            eprintln!("usage: obscript_tools <census|names|live|dump|walk> <ESM> ...");
            std::process::exit(2);
        }
    }
}

mod census {

    use std::collections::{BTreeMap, HashMap, HashSet};

    use byroredux_plugin::esm::parse_esm;
    use byroredux_plugin::esm::records::ScriptType;
    use byroredux_scripting::obscript_vm::{
        BlockOutcome, ObScriptHost, ObScriptMessage, ObScriptProgram, ObScriptValue, VmState,
        BLOCK_GAME_MODE, CMD_DECLINED,
    };

    /// `str::strip_prefix` ignoring ASCII case; the rest must start a new word.
    trait StripPrefixCi {
        fn strip_prefix_ci(&self, prefix: &str) -> Option<&str>;
    }

    impl StripPrefixCi for str {
        fn strip_prefix_ci(&self, prefix: &str) -> Option<&str> {
            let head = self.get(..prefix.len())?;
            let rest = &self[prefix.len()..];
            (head.eq_ignore_ascii_case(prefix) && rest.starts_with(char::is_whitespace)).then_some(rest)
        }
    }

    /// Answers every command with a fixed value and records what was asked.
    struct Recorder {
        answer: f32,
        calls: HashMap<u16, u32>,
    }

    impl ObScriptHost for Recorder {
        fn call(&mut self, cmd: u16, args: &[ObScriptValue], _caller: Option<u32>) -> Option<ObScriptValue> {
            let id = if cmd == CMD_DECLINED {
                args.first().map_or(0, |v| v.as_num() as u16)
            } else {
                cmd
            };
            *self.calls.entry(id).or_insert(0) += 1;
            Some(ObScriptValue::Num(self.answer))
        }
        fn message(&mut self, _message: ObScriptMessage) {}
    }

    /// Block types (`Begin <n>`) in a compiled stream, by walking statement
    /// framing: `[op u16][len u16][payload]`, with `0x1c` carrying a 4-byte
    /// caller+command prefix before its length.
    fn block_types(bytes: &[u8]) -> Vec<u16> {
        let mut blocks = Vec::new();
        let mut at = 0usize;
        while at + 4 <= bytes.len() {
            let op = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
            let (payload, end) = if op == 0x1c {
                if at + 8 > bytes.len() {
                    break;
                }
                let len = u16::from_le_bytes([bytes[at + 6], bytes[at + 7]]) as usize;
                (at + 8, at + 8 + len)
            } else {
                let len = u16::from_le_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
                (at + 4, at + 4 + len)
            };
            if op == 0x10 && payload + 2 <= bytes.len() {
                blocks.push(u16::from_le_bytes([bytes[payload], bytes[payload + 1]]));
            }
            if end <= at || end > bytes.len() {
                break;
            }
            at = end;
        }
        blocks
    }

    const KEYWORDS: &[&str] = &[
        "scn", "scriptname", "begin", "end", "if", "elseif", "else", "endif", "set", "to", "short",
        "long", "float", "ref", "return", "gamemode", "menumode", "and", "or", "not", "player",
        "playerref", "messagebox", "message", "setstage", "getstage", "getstagedone",
        "getsecondspassed", "getbuttonpressed", "startquest", "stopquest", "getquestrunning",
    ];

    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
        let esm = a.next().expect("usage: obscript_tools census <ESM>");
        let index = parse_esm(&std::fs::read(&esm)?)?;

        // 1. Block types, by script type.
        let mut blocks: BTreeMap<(&str, u16), usize> = BTreeMap::new();
        let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
        for script in index.scripts.values() {
            let kind = match script.script_type {
                ScriptType::Quest => "quest",
                ScriptType::Object => "object",
                ScriptType::MagicEffect => "effect",
                ScriptType::Unknown(_) => "unknown",
            };
            *totals.entry(kind).or_insert(0) += 1;
            let distinct: HashSet<u16> = block_types(&script.compiled).into_iter().collect();
            for block in distinct {
                *blocks.entry((kind, block)).or_insert(0) += 1;
            }
        }
        println!("== scripts: {totals:?}");
        println!("== block types (scripts containing at least one) ==");
        for ((kind, block), n) in &blocks {
            println!("  {kind:<7} block {block:#06x}: {n}");
        }

        // 1b. A name for each block type: the `Begin <word>` lines of a script's
        // source, in order, against its compiled blocks, in order.
        let mut named: BTreeMap<u16, BTreeMap<String, usize>> = BTreeMap::new();
        for script in index.scripts.values() {
            let Some(source) = &script.source else { continue };
            let words: Vec<String> = source
                .lines()
                .filter_map(|line| line.trim_start().strip_prefix_ci("begin"))
                .filter_map(|rest| rest.split_whitespace().next().map(str::to_ascii_lowercase))
                .collect();
            let compiled = block_types(&script.compiled);
            if words.len() != compiled.len() {
                continue;
            }
            for (word, block) in words.into_iter().zip(compiled) {
                *named.entry(block).or_default().entry(word).or_insert(0) += 1;
            }
        }
        println!("== block names (source `Begin` word per compiled block type) ==");
        for (block, words) in &named {
            let mut words: Vec<_> = words.iter().collect();
            words.sort_by(|a, b| b.1.cmp(a.1));
            let top: Vec<String> = words.iter().take(3).map(|(w, n)| format!("{w} {n}")).collect();
            println!("  block {block:#06x}: {}", top.join(", "));
        }

        // 2. Declined ids on the quest GameMode path, plus 3. source names.
        let handled: HashSet<u16> = [
            0x1039u16, 0x1036, 0x1037, 0x103a, 0x103b, 0x1038, 0x1059, 0x1000, 0x100c, 0x101f, 0x10ce,
        ]
        .into_iter()
        .collect();
        let mut quest_scripts = 0usize;
        let mut declined: BTreeMap<u16, usize> = BTreeMap::new();
        let mut names: BTreeMap<String, usize> = BTreeMap::new();
        for quest in index.quests.values() {
            let Some(script) = index.scripts.get(&quest.script_ref) else {
                continue;
            };
            if script.compiled.is_empty() {
                continue;
            }
            quest_scripts += 1;
            let mut seen: HashSet<u16> = HashSet::new();
            for answer in [0.0f32, 1.0] {
                let mut host = Recorder { answer, calls: HashMap::new() };
                let mut state = VmState::default();
                let outcome = ObScriptProgram::new(script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
                if let BlockOutcome::Malformed(at) = outcome {
                    eprintln!("malformed {:08X} at {at:#x}", script.form_id);
                }
                seen.extend(host.calls.keys().copied().filter(|id| !handled.contains(id)));
            }
            for id in seen {
                *declined.entry(id).or_insert(0) += 1;
            }
            if let Some(source) = &script.source {
                let locals: HashSet<String> =
                    script.locals.iter().map(|l| l.name.to_ascii_lowercase()).collect();
                let mut in_script: HashSet<String> = HashSet::new();
                for line in source.lines() {
                    let line = line.split(';').next().unwrap_or("");
                    for word in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                        let w = word.to_ascii_lowercase();
                        if w.is_empty()
                            || w.as_bytes()[0].is_ascii_digit()
                            || locals.contains(&w)
                            || KEYWORDS.contains(&w.as_str())
                            || index.scripts.values().any(|_| false)
                        {
                            continue;
                        }
                        in_script.insert(w);
                    }
                }
                for w in in_script {
                    *names.entry(w).or_insert(0) += 1;
                }
            }
        }
        let mut delays: BTreeMap<String, usize> = BTreeMap::new();
        for quest in index.quests.values().filter(|q| q.script_ref != 0) {
            let key = quest.quest_delay.map_or("none".to_owned(), |d| format!("{d}"));
            *delays.entry(key).or_insert(0) += 1;
        }
        println!("== authored QUST quest delay, scripted quests: {delays:?}");
        println!("== quest scripts with bytecode: {quest_scripts}");
        println!("== declined command ids on GameMode (scripts) ==");
        let mut rows: Vec<_> = declined.into_iter().collect();
        rows.sort_by_key(|&(id, n)| (std::cmp::Reverse(n), id));
        for (id, n) in rows.iter().take(40) {
            println!("  {id:#06x}: {n}");
        }
        println!("== source identifiers in quest scripts, top 60 (editor ids included) ==");
        let mut rows: Vec<_> = names.into_iter().collect();
        rows.sort_by_key(|(w, n)| (std::cmp::Reverse(*n), w.clone()));
        for (w, n) in rows.iter().take(60) {
            println!("  {w:<28} {n}");
        }
        Ok(())
    }

}

mod names {
    use std::collections::{HashMap, HashSet};

    use byroredux_plugin::esm::parse_esm;
    use byroredux_plugin::esm::records::ScriptType;

    const KW: &[&str] = &[
        "scn", "scriptname", "begin", "end", "if", "elseif", "else", "endif", "set", "to", "return",
        "and", "or", "not", "short", "long", "float", "int", "ref", "reference",
    ];

    struct Stmt {
        ids: Vec<u16>,
    }

    fn u16_at(b: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
    }

    /// Command ids carried by an expression payload (`' X' cmd len args`).
    fn expr_ids(b: &[u8], mut i: usize, end: usize, out: &mut Vec<u16>) {
        while i + 1 < end {
            if b[i] == b' ' {
                match b[i + 1] {
                    b'X' => {
                        let (Some(cmd), Some(len)) = (u16_at(b, i + 2), u16_at(b, i + 4)) else { return };
                        out.push(cmd);
                        // Nested X escapes inside the argument blob.
                        let a_end = (i + 6 + len as usize).min(end);
                        if len >= 2 {
                            nested(b, i + 6, a_end, out);
                        }
                        i = a_end;
                        continue;
                    }
                    b's' | b'f' | b'G' => {
                        i += 4;
                        continue;
                    }
                    b'r' => {
                        i += 4;
                        // `<ref>.<Getter>` in a condition: the X escape follows
                        // the ref directly, with no leading space.
                        if b.get(i) == Some(&b'X') {
                            let (Some(cmd), Some(len)) = (u16_at(b, i + 1), u16_at(b, i + 3)) else { return };
                            out.push(cmd);
                            let a_end = (i + 5 + len as usize).min(end);
                            if len >= 2 {
                                nested(b, i + 5, a_end, out);
                            }
                            i = a_end;
                        }
                        continue;
                    }
                    b'n' => {
                        i += 6;
                        continue;
                    }
                    _ => {}
                }
            }
            i += 1;
        }
    }

    /// Argument blobs hold `X` (no leading space) escapes for nested calls.
    fn nested(b: &[u8], mut i: usize, end: usize, out: &mut Vec<u16>) {
        // Skip the u16 count.
        i += 2;
        while i < end {
            match b[i] {
                b'X' => {
                    let (Some(cmd), Some(len)) = (u16_at(b, i + 1), u16_at(b, i + 3)) else { return };
                    out.push(cmd);
                    let a_end = (i + 5 + len as usize).min(end);
                    nested(b, i + 5, a_end, out);
                    i = a_end;
                }
                b'r' | b's' | b'f' | b'G' => i += 3,
                b'n' => i += 5,
                b'z' => i += 9,
                b'Y' | b'Z' => i += 1,
                _ => i += 2,
            }
        }
    }

    fn statements(b: &[u8]) -> Option<Vec<Stmt>> {
        let mut at = 0usize;
        let mut out = Vec::new();
        while at < b.len() {
            let op = u16_at(b, at)?;
            let mut ids = Vec::new();
            let end;
            if op == 0x1c {
                let cmd = u16_at(b, at + 4)?;
                let len = u16_at(b, at + 6)? as usize;
                end = at + 8 + len;
                ids.push(cmd);
                if end <= b.len() && len >= 2 {
                    nested(b, at + 8, end, &mut ids);
                }
                // The explicit-caller form is itself one statement.
                out.push(Stmt { ids });
                at = end;
                if end > b.len() {
                    return None;
                }
                continue;
            }
            let len = u16_at(b, at + 2)? as usize;
            end = at + 4 + len;
            if end > b.len() {
                return None;
            }
            match op {
                0x16 | 0x18 => {
                    let expr_len = u16_at(b, at + 8 - 2).unwrap_or(0) as usize;
                    // [op][len][u16 cmp][u16 jmp]... real layout: payload =
                    // [jump u16][expr_len u16][expr] after the 2-byte length.
                    let _ = expr_len;
                    let el = u16_at(b, at + 6)? as usize;
                    expr_ids(b, at + 8, (at + 8 + el).min(end), &mut ids);
                }
                0x15 => {
                    // [target escape][u16 expr_len][expr]
                    let tag = *b.get(at + 4)?;
                    let (el_at, ) = match tag {
                        b'r' => (at + 4 + 6,),
                        _ => (at + 4 + 3,),
                    };
                    if let Some(el) = u16_at(b, el_at) {
                        expr_ids(b, el_at + 2, (el_at + 2 + el as usize).min(end), &mut ids);
                    }
                }
                0x10 | 0x11 | 0x17 | 0x19 | 0x1d | 0x1e => {}
                _ => {
                    ids.push(op);
                    if len >= 2 {
                        nested(b, at + 4, end, &mut ids);
                    }
                }
            }
            out.push(Stmt { ids });
            at = end;
        }
        Some(out)
    }

    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
            let esm = a.next().expect("esm");
        let which = a.next().unwrap_or_else(|| "quest".into());
        let min: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(2);
        let index = parse_esm(&std::fs::read(&esm)?)?;

        // id -> the scripts it appears in, the lines it appears on, and how
        // many of those lines carry each word.
        #[derive(Default)]
        struct Votes {
            scripts: HashSet<u32>,
            lines: usize,
            words: HashMap<String, usize>,
        }
        let mut votes: HashMap<u16, Votes> = HashMap::new();
        let (mut aligned, mut total) = (0usize, 0usize);
        for s in index.scripts.values() {
            let kind_ok = matches!(
                (which.as_str(), &s.script_type),
                ("all", _) | ("quest", ScriptType::Quest) | ("object", ScriptType::Object)
            );
            if !kind_ok || s.compiled.is_empty() {
                continue;
            }
            let Some(src) = &s.source else { continue };
            total += 1;
            let Some(stmts) = statements(&s.compiled) else { continue };
            let locals: HashSet<String> = s.locals.iter().map(|l| l.name.to_ascii_lowercase()).collect();
            let mut lines: Vec<String> = Vec::new();
            for raw in src.lines() {
                let l = raw.split(';').next().unwrap_or("").trim();
                if l.is_empty() {
                    continue;
                }
                let first = l.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
                if matches!(first.as_str(), "short" | "long" | "float" | "int" | "ref" | "reference") {
                    continue;
                }
                lines.push(l.to_owned());
            }
            if lines.len() != stmts.len() {
                continue;
            }
            aligned += 1;
            for (line, st) in lines.iter().zip(&stmts) {
                if st.ids.is_empty() {
                    continue;
                }
                let words: HashSet<String> = line
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map(|w| w.to_ascii_lowercase())
                    .filter(|w| !w.is_empty() && !w.as_bytes()[0].is_ascii_digit() && !KW.contains(&w.as_str()) && !locals.contains(w))
                    .collect();
                let uniq: HashSet<u16> = st.ids.iter().copied().collect();
                for id in uniq {
                    let e = votes.entry(id).or_default();
                    e.scripts.insert(s.form_id);
                    e.lines += 1;
                    for w in &words {
                        *e.words.entry(w.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        println!("scripts {total}, aligned {aligned}");
        let fallout = !esm.to_ascii_lowercase().contains("oblivion");
        let (mut total_lines, mut modeled_lines) = (0usize, 0usize);
        let mut rows: Vec<_> = votes.into_iter().filter(|(_, v)| v.scripts.len() >= min).collect();
        rows.sort_by_key(|(id, v)| (std::cmp::Reverse(v.scripts.len()), *id));
        for (id, Votes { scripts, lines, words }) in rows {
            let mut w: Vec<_> = words.into_iter().collect();
            w.sort_by_key(|(n, c)| (std::cmp::Reverse(*c), n.clone()));
            let top: Vec<String> = w.iter().take(3).map(|(n, c)| format!("{n} {}%", c * 100 / lines)).collect();
            let catalog = match id.checked_sub(0x1000) {
                Some(index) => format!("{:?}", byroredux_scripting::condition::ConditionFunction::from_index(u32::from(index))),
                None => String::new(),
            };
            let catalog = if catalog.starts_with("Unknown") { String::new() } else { format!("  [ctda {catalog}]") };
            let modeled = byroredux_scripting::obscript_quests::command_is_modeled(id, fallout);
            let mark = if modeled { "+" } else { " " };
            total_lines += lines;
            if modeled {
                modeled_lines += lines;
            }
            println!("{mark}{id:#06x}  scripts {:>3} lines {:>3}  {}{catalog}", scripts.len(), lines, top.join(", "));
        }
        println!("call-site lines modeled: {modeled_lines} of {total_lines} ({}%)", modeled_lines * 100 / total_lines.max(1));
        Ok(())
    }

}

mod live {
    use std::collections::BTreeMap;

    use byroredux_core::ecs::World;
    use byroredux_plugin::esm::parse_esm;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_scripting::obscript_quests::{
        install_quest_scripts, obscript_quest_tick_system, ObScriptQuestTimers,
    };
    use byroredux_scripting::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState};
    use byroredux_scripting::{ScriptGame, install_start_game_quests};

    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
            let esm = a.next().expect("usage: obscript_live <ESM> [rounds]");
        let rounds: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(6);
        let index = parse_esm(&std::fs::read(&esm)?)?;
        let game = if esm.to_ascii_lowercase().contains("oblivion") { GameKind::Oblivion } else { GameKind::Fallout3NV };

        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(ScriptGame(game));
        world.insert_resource(QuestStageState::default());
        world.insert_resource(QuestObjectiveState::default());
        install_start_game_quests(&mut world, index.quests.values().cloned());
        install_quest_scripts(&mut world, &index.quests, &index.scripts);
        let scripted: Vec<u32> = index
            .quests
            .values()
            .filter(|q| q.script_ref != 0 && index.scripts.get(&q.script_ref).is_some_and(|s| !s.compiled.is_empty()))
            .map(|q| q.form_id)
            .collect();
        {
            let mut stages = world.resource_mut::<QuestStageState>();
            for &q in &scripted {
                stages.start_quest(QuestFormId(q), None);
            }
        }
        // Fixed 1 s steps; the longest authored delay run here is `rounds` seconds.
        for _ in 0..rounds * 5 {
            obscript_quest_tick_system(&world, 1.0);
        }

        let timers = world.resource::<ObScriptQuestTimers>();
        let mut declined: BTreeMap<u16, (usize, u32)> = BTreeMap::new();
        for commands in timers.unknown_commands.values() {
            for (&cmd, &n) in commands {
                let e = declined.entry(cmd).or_default();
                e.0 += 1;
                e.1 += n;
            }
        }
        let total_runs: u32 = timers.runs.values().sum();
        println!("scripted quests {}  runs {}", scripted.len(), total_runs);
        println!("declined command ids: {} distinct, {} total hits", declined.len(), declined.values().map(|v| v.1 as u64).sum::<u64>());
        let mut rows: Vec<_> = declined.into_iter().collect();
        rows.sort_by_key(|(id, (q, h))| (std::cmp::Reverse(*q), std::cmp::Reverse(*h), *id));
        for (id, (quests, hits)) in rows.iter().take(25) {
            println!("  {id:#06x}  quests {quests:>3}  hits {hits:>5}");
        }
        let objectives = world.resource::<QuestObjectiveState>();
        let touched = scripted.iter().filter(|q| objectives.iter_quest(QuestFormId(**q)).next().is_some()).count();
        println!("quests whose scripts reached an objective: {touched}");
        let stages = world.resource::<QuestStageState>();
        let staged = scripted.iter().filter(|q| stages.get_stage(QuestFormId(**q)) != 0).count();
        println!("quests advanced past stage 0 by a script: {staged}");
        Ok(())
    }

}

mod dump {
    use byroredux_plugin::esm::parse_esm;
    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
            let esm = a.next().expect("esm");
        let target = a.next().expect("formid or grep:<text>");
        let index = parse_esm(&std::fs::read(&esm)?)?;
        // The id may be a quest or a script id; `grep:<text>` takes the first
        // script (lowest form id) whose source contains the text.
        let quest_only = target.starts_with("grepq:");
        let script = if let Some(text) = target.strip_prefix("grep:").or_else(|| target.strip_prefix("grepq:")) {
            let needle = text.to_ascii_lowercase();
            let mut hits: Vec<_> = index.scripts.values().filter(|s| (!quest_only || matches!(s.script_type, byroredux_plugin::esm::records::ScriptType::Quest)) && s.source.as_deref().is_some_and(|src| src.to_ascii_lowercase().contains(&needle))).collect();
            hits.sort_by_key(|s| s.form_id);
            hits.first().copied().expect("no script contains that text")
        } else {
            let id = u32::from_str_radix(&target, 16)?;
            index.scripts.get(&id).or_else(|| index.quests.get(&id).and_then(|q| index.scripts.get(&q.script_ref))).expect("no such script")
        };
        println!("script {:08X} {} type {:?} refs {:?}", script.form_id, script.editor_id, script.script_type, script.refs.len());
        for l in &script.locals { println!("  local {} type {} {}", l.index, l.var_type, l.name); }
        println!("{}", script.source.as_deref().unwrap_or("<no source>"));
        for (i, c) in script.compiled.chunks(16).enumerate() {
            println!("{:04x}: {}", i * 16, c.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
        }
        Ok(())
    }

}

mod walk {
    use byroredux_plugin::esm::parse_esm;
    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
            let esm = a.next().expect("esm");
        let id = u32::from_str_radix(&a.next().expect("formid"), 16)?;
        let index = parse_esm(&std::fs::read(&esm)?)?;
        let script = index.scripts.get(&id).or_else(|| index.quests.get(&id).and_then(|q| index.scripts.get(&q.script_ref))).expect("no such script");
        let b = &script.compiled;
        let mut at = 0usize;
        while at + 4 <= b.len() {
            let op = u16::from_le_bytes([b[at], b[at + 1]]);
            let end = if op == 0x1c {
                at + 8 + u16::from_le_bytes([b[at + 6], b[at + 7]]) as usize
            } else {
                at + 4 + u16::from_le_bytes([b[at + 2], b[at + 3]]) as usize
            };
            let show = end.min(b.len()).min(at + 24);
            println!("{at:04x} op {op:#06x} end {end:04x}  {}", b[at..show].iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "));
            if end > b.len() { println!("!! runs past end ({})", b.len()); break; }
            at = end;
        }
        Ok(())
    }

}

mod avif {
    use byroredux_plugin::esm::parse_esm;
    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
        let esm = a.next().expect("esm");
        let index = parse_esm(&std::fs::read(&esm)?)?;
        let mut rows: Vec<_> = index.actor_values.values().collect();
        rows.sort_by_key(|r| r.form_id);
        for r in rows {
            println!("{:08X} {:<28} {}", r.form_id, r.editor_id, r.full_name);
        }
        Ok(())
    }
}

/// Which actor value each compiled argument code names: every call of an
/// actor-value command in a script whose source lines align with its compiled
/// statements, as (code, the word after the command in the source line).
mod avcodes {
    use std::collections::{BTreeMap, HashSet};

    use byroredux_plugin::esm::parse_esm;

    /// `GetAV`, `SetAV`, `ModAV`, `ForceAV`, `DamageAV`, `RestoreAV`, `GetBaseAV`,
    /// `GetPermanentActorValue`, `ForceActorValue`, ... by the `names` census.
    const COMMANDS: &[(u16, &str)] = &[
        (0x100e, "getav"), (0x100f, "setav"), (0x1010, "modav"), (0x110e, "forceav"),
        (0x1181, "damageav"), (0x1182, "restoreav"), (0x11ef, "getpermanentactorvalue"),
        (0x1100, "getgs"),
    ];

    fn u16_at(b: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
    }

    /// `(code literal)` of the first argument of the blob at `at`: `[count u16]`
    /// then a u16 integer, or an `'n'` i32.
    fn first_code(b: &[u8], at: usize, end: usize) -> Option<u32> {
        let count = u16_at(b, at)?;
        if count == 0 || at + 3 > end {
            return None;
        }
        match *b.get(at + 2)? {
            b'n' => Some(i32::from_le_bytes([*b.get(at + 3)?, *b.get(at + 4)?, *b.get(at + 5)?, *b.get(at + 6)?]) as u32),
            b'r' | b's' | b'f' | b'G' | b'z' | b'X' | b'Y' | b'Z' => None,
            _ => u16_at(b, at + 2).map(u32::from),
        }
    }

    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
        let esm = a.next().expect("esm");
        let index = parse_esm(&std::fs::read(&esm)?)?;
        let ids: HashSet<u16> = COMMANDS.iter().map(|c| c.0).collect();
        let mut table: BTreeMap<(u16, u32), BTreeMap<String, usize>> = BTreeMap::new();
        let mut unaligned = 0usize;
        for s in index.scripts.values() {
            let Some(src) = &s.source else { continue };
            let b = &s.compiled;
            // Statement ranges.
            let mut stmts: Vec<(usize, usize)> = Vec::new();
            let mut at = 0usize;
            let mut ok = true;
            while at + 4 <= b.len() {
                let op = u16_at(b, at).unwrap();
                let end = if op == 0x1c { at + 8 + u16_at(b, at + 6).unwrap() as usize } else { at + 4 + u16_at(b, at + 2).unwrap() as usize };
                if end > b.len() { ok = false; break; }
                stmts.push((at, end));
                at = end;
            }
            if !ok { continue; }
            let locals: HashSet<String> = s.locals.iter().map(|l| l.name.to_ascii_lowercase()).collect();
            let lines: Vec<String> = src
                .lines()
                .map(|r| r.split(';').next().unwrap_or("").trim().to_owned())
                .filter(|l| !l.is_empty())
                .filter(|l| !matches!(l.split_whitespace().next().unwrap_or("").to_ascii_lowercase().as_str(), "short" | "long" | "float" | "int" | "ref" | "reference"))
                .collect();
            let _ = &locals;
            if lines.len() != stmts.len() { unaligned += 1; continue; }
            for (line, &(st, en)) in lines.iter().zip(&stmts) {
                let words: Vec<String> = line
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|w| !w.is_empty())
                    .map(|w| w.to_ascii_lowercase())
                    .collect();
                // Occurrences in the statement: implicit op, explicit caller, expression escapes.
                let op = u16_at(b, st).unwrap();
                let mut found: Vec<(u16, u32)> = Vec::new();
                if op == 0x1c {
                    let cmd = u16_at(b, st + 4).unwrap();
                    if ids.contains(&cmd) { if let Some(c) = first_code(b, st + 8, en) { found.push((cmd, c)); } }
                } else if ids.contains(&op) {
                    if let Some(c) = first_code(b, st + 4, en) { found.push((op, c)); }
                } else if matches!(op, 0x15 | 0x16 | 0x18) {
                    let mut i = st + 4;
                    while i + 5 < en {
                        if b[i] == b'X' {
                            let cmd = u16_at(b, i + 1).unwrap();
                            let len = u16_at(b, i + 3).unwrap() as usize;
                            if ids.contains(&cmd) && i + 5 + len <= en {
                                if let Some(c) = first_code(b, i + 5, i + 5 + len) { found.push((cmd, c)); }
                            }
                        }
                        i += 1;
                    }
                }
                // Source order of the command words pairs with byte order of the calls.
                let mut occ: Vec<(usize, &str)> = Vec::new();
                for (i, w) in words.iter().enumerate() {
                    for &(_, name) in COMMANDS {
                        if w == name { occ.push((i, name)); }
                    }
                }
                for (k, (cmd, code)) in found.iter().enumerate() {
                    let Some(&(i, name)) = occ.get(k) else { continue };
                    if COMMANDS.iter().find(|c| c.0 == *cmd).map(|c| c.1) != Some(name) { continue; }
                    let word = words.get(i + 1).cloned().unwrap_or_default();
                    *table.entry((*cmd, *code)).or_default().entry(word).or_insert(0) += 1;
                }
            }
        }
        println!("unaligned scripts skipped: {unaligned}");
        for ((cmd, code), words) in table {
            let mut w: Vec<_> = words.into_iter().collect();
            w.sort_by_key(|(n, c)| (std::cmp::Reverse(*c), n.clone()));
            let top: Vec<String> = w.iter().take(4).map(|(n, c)| format!("{n} {c}")).collect();
            println!("{cmd:#06x} code {code:>3}: {}", top.join(", "));
        }
        Ok(())
    }
}

/// `find <ESM> <needle> [limit]` — the scripts whose source contains `needle`.
mod find {
    use byroredux_plugin::esm::parse_esm;
    pub fn run(mut a: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
        let esm = a.next().expect("esm");
        let needle = a.next().expect("needle").to_ascii_lowercase();
        let limit: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(10);
        let index = parse_esm(&std::fs::read(&esm)?)?;
        let mut found: Vec<_> = index
            .scripts
            .values()
            .filter(|s| s.source.as_deref().is_some_and(|src| src.to_ascii_lowercase().contains(&needle)))
            .collect();
        found.sort_by_key(|s| (s.compiled.len(), s.form_id));
        println!("{} scripts match", found.len());
        for s in found.into_iter().take(limit) {
            println!("{:08X} {} ({:?}, {} bytes)", s.form_id, s.editor_id, s.script_type, s.compiled.len());
        }
        Ok(())
    }
}
