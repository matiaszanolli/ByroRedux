//! The session's plugin profile from the console (PRD legacy-mod-compatibility Phase 1, #6670): `plugins.activation`
//! shows what activation did to the profile (LMC-ID-003, -011) and the archives each plugin brought (LMC-AST-002),
//! `plugins.conflicts` what the fold did with the forms
//! more than one plugin carries (LMC-REC-005, -007). `--plugin-report` prints the same without a window ([`run_report`]).

use super::shared::*;
use crate::cell_loader::load_order::{ActiveLoadOrder, LoadOrder};
use byroredux_plugin::esm::records::Carrier;

/// The override rule the fold applies: the only policy built (LMC-REC-002; `merge`, LMC-REC-003, is not).
const POLICY: &str = "legacy: the last plugin in load order that carries a record wins it whole (LMC-REC-002)";

/// `plugins.activation` — the profile as given, the load order it activated to, every hoisted master and disabled
/// plugin with its reason, and the archives each plugin pairs with.
pub(crate) struct PluginActivationCommand;

impl ConsoleCommand for PluginActivationCommand {
    fn name(&self) -> &str {
        "plugins.activation"
    }

    fn description(&self) -> &str {
        "Show the plugin profile, the load order it activated to, each hoisted master and disabled plugin, and each plugin's paired archives: plugins.activation"
    }

    fn execute(&self, _world: &World, _args: &str) -> CommandOutput {
        match crate::plugin_activation::activation_report() {
            Some(report) => CommandOutput::lines(activation_lines(report)),
            None => CommandOutput::line("no plugin profile: the session was started without --esm"),
        }
    }
}

fn activation_lines(report: &crate::plugin_activation::PluginActivationReport) -> Vec<String> {
    let file = |path: &String| path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
    let profile: Vec<String> = report.profile.iter().map(file).collect();
    let mut lines = vec![format!("profile ({}): {}", profile.len(), profile.join(", "))];
    lines.push(format!("load order ({}):", report.order.len()));
    lines.extend(report.order.iter().enumerate().map(|(position, path)| format!("  {position:3}  {}", file(path))));
    if report.lines.is_empty() {
        lines.push("the profile loads as given: no master hoisted, no plugin disabled".to_owned());
    } else {
        lines.extend(report.lines.iter().cloned());
    }
    let paired = report.paired_lines();
    if paired.is_empty() {
        lines.push("paired archives: none beyond the listed archives".to_owned());
    } else {
        lines.push(format!("paired archives ({}):", report.paired.iter().map(|entry| entry.archives.len()).sum::<usize>()));
        lines.extend(paired.iter().map(|line| format!("  {line}")));
    }
    lines
}

/// `plugins.conflicts [form]` — the forms more than one plugin carries: a summary per plugin and the first of them, or
/// for one form every plugin that carried it in load order, which won and the policy that chose.
pub(crate) struct PluginConflictsCommand;

impl ConsoleCommand for PluginConflictsCommand {
    fn name(&self) -> &str {
        "plugins.conflicts"
    }

    fn description(&self) -> &str {
        "List the forms more than one plugin carries, or one form's carriers, winner, policy and (with `fields`) the sub-records that differ: plugins.conflicts [formid] [fields]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(order) = world.try_resource::<ActiveLoadOrder>().map(|active| std::sync::Arc::clone(&active.0)) else {
            return CommandOutput::error("no load order is active (no cell has been loaded)");
        };
        let record_type = |form_id: u32| {
            world
                .try_resource::<crate::components::LoadedCellIndex>()
                .and_then(|index| index.0.record_types.get(&form_id).copied())
        };
        let mut words = args.split_whitespace();
        match (words.next(), words.next()) {
            (None, _) => CommandOutput::lines(summary_lines(&order, record_type)),
            (Some(text), fields) => match (parse_form_id(text), fields) {
                (Some(form_id), None) => CommandOutput::lines(form_lines(&order, form_id, record_type(form_id))),
                (Some(form_id), Some("fields")) => {
                    let mut lines = form_lines(&order, form_id, record_type(form_id));
                    let paths = world
                        .try_resource::<crate::components::LoadedPluginSet>()
                        .map(|set| set.masters.iter().cloned().chain(std::iter::once(set.esm_path.clone())).collect::<Vec<_>>())
                        .unwrap_or_default();
                    lines.extend(field_lines(&order, &paths, form_id, record_type(form_id)));
                    CommandOutput::lines(lines)
                }
                _ => CommandOutput::error("usage: plugins.conflicts [formid] [fields]  (formid in hex, with or without 0x)"),
            },
        }
    }
}

/// `--plugin-report [formid] [fields]`: the two commands' answers for the session's plugin set, printed without a
/// window or a World (the boot activation has already rewritten the `--master` list), then the process exits.
pub(crate) fn run_report(plugin_paths: &[String], args: &[String]) -> anyhow::Result<()> {
    let refs: Vec<&str> = plugin_paths.iter().map(String::as_str).collect();
    let (index, order) = crate::cell_loader::parse_record_indexes_in_load_order(&refs)?;
    if let Some(report) = crate::plugin_activation::activation_report() {
        activation_lines(report).iter().for_each(|line| println!("{line}"));
    }
    let record_type = |form_id: u32| index.record_types.get(&form_id).copied();
    let form = args.first().filter(|arg| !arg.starts_with("--"));
    let lines = match form.map(|text| parse_form_id(text)) {
        None => summary_lines(&order, record_type),
        Some(None) => anyhow::bail!("--plugin-report takes a hex form ID, got '{}'", form.map_or("", String::as_str)),
        Some(Some(form_id)) => {
            let mut lines = form_lines(&order, form_id, record_type(form_id));
            if args.get(1).is_some_and(|arg| arg == "fields") {
                lines.extend(field_lines(&order, plugin_paths, form_id, record_type(form_id)));
            }
            lines
        }
    };
    lines.iter().for_each(|line| println!("{line}"));
    Ok(())
}

fn parse_form_id(text: &str) -> Option<u32> {
    let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(text);
    u32::from_str_radix(digits, 16).ok()
}

fn type_label(record_type: Option<[u8; 4]>) -> String {
    record_type.map_or("----".to_owned(), |tag| String::from_utf8_lossy(&tag).into_owned())
}

fn plugin_name(order: &LoadOrder, position: usize) -> &str {
    order.get(position).map_or("?", String::as_str)
}

/// The slot owner of `form_id`: the plugin whose ID space it is in.
fn slot_owner(order: &LoadOrder, form_id: u32) -> Option<usize> {
    let slot = byroredux_scripting::load_order::global_slot_of(form_id);
    order.slots().iter().position(|owner| *owner == slot)
}

/// One carrier chain, `A → B → C (wins)`, a deletion marked.
fn chain(order: &LoadOrder, carriers: &[Carrier]) -> String {
    carriers
        .iter()
        .enumerate()
        .map(|(index, carrier)| {
            let mut text = plugin_name(order, carrier.plugin as usize).to_owned();
            if carrier.deletes {
                text.push_str(" [deletes]");
            }
            if index + 1 == carriers.len() {
                text.push_str(" (wins)");
            }
            text
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

const LISTED: usize = 40;

fn summary_lines(order: &LoadOrder, record_type: impl Fn(u32) -> Option<[u8; 4]>) -> Vec<String> {
    let provenance = order.provenance();
    let (overridden, injected) = provenance.counts();
    let mut lines = vec![
        format!("{overridden} forms carried by more than one plugin; {injected} injected into a master's ID space"),
        format!("policy: {POLICY}"),
    ];
    // Per plugin: forms it wins, forms it carried and lost, forms it injected.
    let mut wins = vec![0usize; order.len()];
    let mut losses = vec![0usize; order.len()];
    let mut injects = vec![0usize; order.len()];
    for (_, carriers) in provenance.overridden() {
        if let Some((last, rest)) = carriers.split_last() {
            if let Some(count) = wins.get_mut(last.plugin as usize) {
                *count += 1;
            }
            for carrier in rest {
                if let Some(count) = losses.get_mut(carrier.plugin as usize) {
                    *count += 1;
                }
            }
        }
    }
    for (_, plugin) in provenance.injected() {
        if let Some(count) = injects.get_mut(plugin) {
            *count += 1;
        }
    }
    lines.push("  pos  plugin                                   wins   lost  injected".to_owned());
    for (position, name) in order.iter().enumerate() {
        lines.push(format!("  {position:3}  {name:<40} {:6} {:6} {:9}", wins[position], losses[position], injects[position]));
    }
    let mut forms: Vec<(u32, &[Carrier])> = provenance.overridden().collect();
    forms.sort_unstable_by_key(|(form_id, _)| *form_id);
    if !forms.is_empty() {
        lines.push(format!("first {} of {} (plugins.conflicts <formid> for one):", forms.len().min(LISTED), forms.len()));
    }
    for (form_id, carriers) in forms.into_iter().take(LISTED) {
        lines.push(format!("  {form_id:08X} {}  {}", type_label(record_type(form_id)), chain(order, carriers)));
    }
    lines
}

fn form_lines(order: &LoadOrder, form_id: u32, record_type: Option<[u8; 4]>) -> Vec<String> {
    let provenance = order.provenance();
    let mut lines = vec![format!("{form_id:08X} {}", type_label(record_type))];
    let owner = slot_owner(order, form_id);
    let carriers = provenance.carriers(form_id);
    if carriers.is_empty() {
        match (provenance.winner(form_id), owner) {
            (Some(injector), _) => lines.push(format!(
                "carried by {} alone, injected into {}'s ID space (LMC-REC-005)",
                plugin_name(order, injector),
                owner.map_or("an unloaded plugin", |owner| plugin_name(order, owner))
            )),
            (None, Some(owner)) if record_type.is_some() => {
                lines.push(format!("carried by {} alone: no conflict", plugin_name(order, owner)));
            }
            (None, Some(owner)) => lines.push(format!("no plugin carries it (it is in {}'s ID space)", plugin_name(order, owner))),
            (None, None) => lines.push("no plugin of the load order owns its ID space".to_owned()),
        }
        return lines;
    }
    lines.push(format!("carriers in load order: {}", chain(order, carriers)));
    if let Some(first) = carriers.first()
        && owner != Some(first.plugin as usize)
    {
        lines.push(format!(
            "defined by {} in {}'s ID space (injected, LMC-REC-005)",
            plugin_name(order, first.plugin as usize),
            owner.map_or("an unloaded plugin", |owner| plugin_name(order, owner))
        ));
    }
    let winner = carriers.last().expect("a carrier list is never empty");
    lines.push(format!(
        "winner: {}{}; policy: {POLICY}",
        plugin_name(order, winner.plugin as usize),
        if winner.deletes { ", which deletes it" } else { "" }
    ));
    lines
}

/// What one override did to the record it replaced (LMC-REC-006, -007).
#[derive(Debug, Clone, PartialEq, Eq)]
enum OverrideVerdict {
    /// It changes these sub-records.
    Changes(Vec<byroredux_plugin::esm::raw_record::FieldDiff>),
    /// It equals the record it replaced: an identical override, which changes nothing (xEdit's ITM).
    Identical,
    /// It undoes the change of the carrier before it: equal to the definer's record, unequal to the one it replaced.
    Reverts,
}

/// The verdict on each override of a carrier chain, `records[i]` against `records[i - 1]` for `i` from 1; `records[0]`
/// is the definer's. A `None` record (unreadable) has no verdict, nor has the override after it.
fn chain_verdicts(records: &[Option<byroredux_plugin::esm::raw_record::RawRecord>]) -> Vec<Option<OverrideVerdict>> {
    use byroredux_plugin::esm::raw_record::diff_fields;
    (1..records.len())
        .map(|i| {
            let (current, replaced) = (records[i].as_ref()?, records[i - 1].as_ref()?);
            let diffs = diff_fields(current, replaced);
            Some(if diffs.is_empty() {
                OverrideVerdict::Identical
            } else if i >= 2 && records[0].as_ref().is_some_and(|definer| diff_fields(current, definer).is_empty()) {
                OverrideVerdict::Reverts
            } else {
                OverrideVerdict::Changes(diffs)
            })
        })
        .collect()
}

/// LMC-REC-007's "fields that differ" and LMC-REC-006's identical and reverting overrides: each carrier's record of
/// `form_id`, read back from its plugin file through that plugin's own remap, against the record it replaced in the fold.
/// `paths` are the plugin files in load order (parallel to `order`).
fn field_lines(order: &LoadOrder, paths: &[String], form_id: u32, record_type: Option<[u8; 4]>) -> Vec<String> {
    use byroredux_plugin::esm::raw_record::{find_raw_record, same_form_id_encoding};
    let carriers = order.provenance().carriers(form_id);
    let Some(record_type) = record_type else {
        return vec!["fields: the merged index has no record of this form (deleted, or never carried), so its type is unknown".to_owned()];
    };
    if carriers.is_empty() {
        return vec!["fields: one plugin carries it, so nothing differs".to_owned()];
    }
    if paths.len() != order.len() {
        return vec![format!("fields: the plugin set ({} files) does not match the load order ({} plugins)", paths.len(), order.len())];
    }
    // Each carrier's raw record and its master list (for the byte-comparison caveat), or why it could not be read.
    let read = |position: usize| -> Result<(byroredux_plugin::esm::raw_record::RawRecord, Vec<String>), String> {
        let path = &paths[position];
        let (header, game) = byroredux_plugin::activation::read_plugin_header(std::path::Path::new(path)).map_err(|e| format!("{e:#}"))?;
        let remap = crate::cell_loader::load_order::build_remap_for_plugin(path, &header, game, order.slots()[position], order, order.slots())
            .map_err(|e| format!("{e:#}"))?;
        let record = find_raw_record(std::path::Path::new(path), Some(remap), record_type, form_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| format!("no {} record of it in the file", type_label(Some(record_type))))?;
        let masters: Vec<String> = header.master_files.iter().map(|master| byroredux_core::form_id::plugin_key(master)).collect();
        Ok((record, masters))
    };
    let read: Vec<Result<_, String>> = carriers.iter().map(|carrier| read(carrier.plugin as usize)).collect();
    let records: Vec<_> = read.iter().map(|result| result.as_ref().ok().map(|(record, _)| record.clone())).collect();
    let verdicts = chain_verdicts(&records);
    let name = |index: usize| plugin_name(order, carriers[index].plugin as usize);
    let mut lines = vec!["fields: each override against the record it replaced (sub-records compared by bytes):".to_owned()];
    if let Err(error) = &read[0] {
        lines.push(format!("  {}: could not read it: {error}", name(0)));
    }
    for (index, verdict) in verdicts.into_iter().enumerate().map(|(i, verdict)| (i + 1, verdict)) {
        let head = format!("  {} over {}", name(index), name(index - 1));
        let Some(verdict) = verdict else {
            match &read[index] {
                Err(error) => lines.push(format!("{head}: could not read it: {error}")),
                Ok(_) => lines.push(format!("{head}: not compared, the record it replaced could not be read")),
            }
            continue;
        };
        let text = match verdict {
            OverrideVerdict::Identical => "identical: an override that changes nothing (ITM, LMC-REC-006)".to_owned(),
            OverrideVerdict::Reverts => format!("reverts {}'s change: equal to {}'s record (LMC-REC-006)", name(index - 1), name(0)),
            OverrideVerdict::Changes(diffs) => diffs.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
        };
        let caveat = match (&read[index], &read[index - 1]) {
            (Ok((_, masters)), Ok((_, replaced_masters)))
                if !same_form_id_encoding(masters, &order[carriers[index].plugin as usize], replaced_masters, &order[carriers[index - 1].plugin as usize]) =>
            {
                " (the two files index their masters differently, so a field holding a form ID may differ in bytes only)"
            }
            _ => "",
        };
        lines.push(format!("{head}: {text}{caveat}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::reader::GlobalSlot;

    fn order() -> LoadOrder {
        let mut order = LoadOrder::all_regular(vec!["falloutnv.esm".into(), "deadmoney.esm".into(), "mymod.esp".into()]);
        order.note_record_writer(0x0000_1234, 0);
        order.note_record_writer(0x0000_1234, 1);
        order.note_record_writer(0x0000_1234, 2);
        // MyMod defines a form in FalloutNV's ID space.
        order.note_record_writer(0x0000_F00D, 2);
        order
    }

    /// One form: every carrier in load order, the winner and the policy; an injected form names its injector and the
    /// ID space it went into.
    #[test]
    fn a_form_lists_its_carriers_winner_and_policy() {
        let order = order();
        let lines = form_lines(&order, 0x0000_1234, Some(*b"WEAP")).join("\n");
        assert!(lines.contains("00001234 WEAP"), "{lines}");
        assert!(lines.contains("falloutnv.esm → deadmoney.esm → mymod.esp (wins)"), "{lines}");
        assert!(lines.contains("winner: mymod.esp; policy: legacy"), "{lines}");

        let lines = form_lines(&order, 0x0000_F00D, Some(*b"MISC")).join("\n");
        assert!(lines.contains("carried by mymod.esp alone, injected into falloutnv.esm's ID space"), "{lines}");

        let lines = form_lines(&order, 0x0100_0001, Some(*b"STAT")).join("\n");
        assert!(lines.contains("carried by deadmoney.esm alone: no conflict"), "{lines}");
        assert_eq!(order.slots()[1], GlobalSlot::Regular(1));
    }

    /// The summary counts each plugin's wins, losses and injections and lists the overridden forms.
    #[test]
    fn the_summary_counts_per_plugin_and_lists_the_forms() {
        let order = order();
        let lines = summary_lines(&order, |_| Some(*b"WEAP"));
        assert!(lines[0].starts_with("1 forms carried by more than one plugin; 1 injected"), "{lines:?}");
        let row = |name: &str| lines.iter().find(|line| line.contains(name)).unwrap().split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
        assert_eq!(row("falloutnv.esm"), "0 1 0");
        assert_eq!(row("deadmoney.esm"), "0 1 0");
        assert_eq!(row("mymod.esp"), "1 0 1");
        assert!(lines.iter().any(|line| line.contains("00001234 WEAP  falloutnv.esm → deadmoney.esm → mymod.esp (wins)")));
    }

    /// Real data (ignored: it parses `FalloutNV.esm`): Dead Money's overrides of the base game list both carriers with Dead
    /// Money winning, and reading the two records back from the files finds at least one that changes a sub-record.
    #[test]
    #[ignore = "parses FalloutNV.esm and DeadMoney.esm; run with --ignored"]
    fn dead_money_overrides_name_both_carriers_and_the_fields_they_change() {
        let data = byroredux_plugin::esm::test_paths::fnv_data_dir();
        if !data.is_dir() {
            byroredux_plugin::esm::test_paths::require_or_skip("New Vegas data");
            return;
        }
        let paths: Vec<String> =
            ["FalloutNV.esm", "DeadMoney.esm"].iter().map(|p| data.join(p).to_string_lossy().into_owned()).collect();
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let (index, order) = crate::cell_loader::parse_record_indexes_in_load_order(&refs).expect("parse");
        let (overridden, _) = order.provenance().counts();
        assert!(overridden > 0, "Dead Money overrides base-game forms");
        let summary = summary_lines(&order, |id| index.record_types.get(&id).copied());
        eprintln!("{}", summary[..6.min(summary.len())].join("\n"));
        let mut forms: Vec<u32> = order.provenance().overridden().map(|(id, _)| id).collect();
        forms.sort_unstable();
        let mut changed = 0;
        for &form_id in forms.iter().filter(|id| index.record_types.contains_key(id)).take(25) {
            let record_type = index.record_types.get(&form_id).copied();
            let lines = form_lines(&order, form_id, record_type).join("\n");
            assert!(lines.contains("falloutnv.esm → deadmoney.esm (wins)"), "{lines}");
            let fields = field_lines(&order, &paths, form_id, record_type);
            eprintln!("{form_id:08X} {}: {}", type_label(record_type), fields[1..].join(" | "));
            assert_eq!(fields.len(), 2, "one override, one line: {fields:?}");
            assert!(fields[1].starts_with("  deadmoney.esm over falloutnv.esm: "), "{fields:?}");
            assert!(!fields[1].contains("could not") && !fields[1].contains("not compared"), "both records are found: {fields:?}");
            if !fields[1].contains("identical") {
                changed += 1;
            }
        }
        assert!(changed > 0, "an override that changes nothing in 25 forms would mean the diff reads the same record twice");
    }

    /// LMC-REC-006 — each override is judged against the record it replaced: one equal to it is an identical override;
    /// one equal to the definer's record after an earlier plugin changed it is a revert; otherwise its changes are named.
    #[test]
    fn each_override_is_judged_against_the_record_it_replaced() {
        use byroredux_plugin::esm::raw_record::{FieldDiff, RawRecord};
        let record = |data: u8| Some(RawRecord { record_type: *b"WEAP", flags: 0, subrecords: vec![(*b"DATA", vec![data])] });
        // master 1, dlc 2 (a change), itm 2 (identical), revert 1 (back to the master's), unreadable, after-unreadable
        let verdicts = chain_verdicts(&[record(1), record(2), record(2), record(1), None, record(3)]);
        assert_eq!(
            verdicts,
            vec![
                Some(OverrideVerdict::Changes(vec![FieldDiff::Differs { tag: *b"DATA" }])),
                Some(OverrideVerdict::Identical),
                Some(OverrideVerdict::Reverts),
                None,
                None,
            ]
        );
        assert_eq!(chain_verdicts(&[record(1), record(1)]), vec![Some(OverrideVerdict::Identical)], "an ITM of the master itself");
    }

    #[test]
    fn form_ids_parse_with_or_without_a_prefix() {
        assert_eq!(parse_form_id("0x0001F00D"), Some(0x0001_F00D));
        assert_eq!(parse_form_id("1f00d"), Some(0x0001_F00D));
        assert_eq!(parse_form_id("Skyrim.esm"), None);
    }
}
