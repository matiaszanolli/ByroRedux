//! `vats.*` — VATS console control (#6350): the smoke-gate surface for
//! the targeting mode. `vats.status` is the observable the smoke script
//! asserts on — mode, per-part percentages, AP costs, queue depth — and
//! `vats.enter/queue/confirm/exit` drive the full loop without a UI.

use super::shared::*;
use crate::components::{VatsMode, VatsState};
use crate::systems::vats::{
    vats_active, vats_cancel, vats_confirm, vats_entry_refusal, vats_queue_part, vats_scan,
    VatsQueueRefusal,
};
use byroredux_core::ecs::World;

/// `vats.enter` — enter targeting (the key's gates, [`vats_entry_refusal`]:
/// model, living player in the character mode, enabled control domain).
/// Runs the scan.
pub(crate) struct VatsEnterCommand;

impl ConsoleCommand for VatsEnterCommand {
    fn name(&self) -> &str {
        "vats.enter"
    }
    fn description(&self) -> &str {
        "Enter VATS targeting (runs the limb scan; FO3/FNV freezes the world)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        if world.try_resource::<VatsState>().is_none() {
            return CommandOutput::error("vats: no VATS state (engine boot)");
        }
        if world
            .try_resource::<VatsState>()
            .is_some_and(|vats| vats.mode != VatsMode::None)
        {
            return CommandOutput::line("vats: already active");
        }
        if let Some(why) = vats_entry_refusal(world) {
            return CommandOutput::error(format!("vats: {why}"));
        }
        if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
            vats.mode = VatsMode::TargetSelect;
        }
        vats_scan(world);
        let count = world
            .try_resource::<VatsState>()
            .map(|vats| vats.targets.len())
            .unwrap_or(0);
        CommandOutput::line(format!("vats: entered, {count} target parts"))
    }
}

/// `vats.scan` — re-run the targeting scan against live positions.
pub(crate) struct VatsScanCommand;

impl ConsoleCommand for VatsScanCommand {
    fn name(&self) -> &str {
        "vats.scan"
    }
    fn description(&self) -> &str {
        "Re-run the VATS limb scan against live positions"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        if !vats_active(world) {
            return CommandOutput::error("vats.scan: not in VATS (vats.enter first)");
        }
        vats_scan(world);
        let count = world
            .try_resource::<VatsState>()
            .map(|vats| vats.targets.len())
            .unwrap_or(0);
        CommandOutput::line(format!("vats: {count} target parts"))
    }
}

/// `vats.status` — the smoke observable.
pub(crate) struct VatsStatusCommand;

impl ConsoleCommand for VatsStatusCommand {
    fn name(&self) -> &str {
        "vats.status"
    }
    fn description(&self) -> &str {
        "Report VATS mode, target parts with hit percentages and AP costs, and the queue"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(vats) = world.try_resource::<VatsState>() else {
            return CommandOutput::error("vats: no VATS state (engine boot)");
        };
        let mode = match vats.mode {
            VatsMode::None => "none",
            VatsMode::TargetSelect => "target-select",
            VatsMode::MenuMode => "menu",
            VatsMode::ScanMode => "scan",
            VatsMode::Playback => "playback",
        };
        let mut out = format!(
            "vats: mode={mode} world_time_scale={:.2} parts={} queue={} ap_reserved={:.0}",
            vats.world_time_scale(),
            vats.targets.len(),
            vats.queue.len(),
            vats.ap_reserved
        );
        for (index, part) in vats.targets.iter().take(16).enumerate() {
            out.push_str(&format!(
                "\n  [{index}] entity={:?} part={} hit={:.0}% ap={:.0}",
                part.entity, part.part_name, part.hit_chance, part.ap_cost
            ));
        }
        CommandOutput::line(out)
    }
}

/// Format a [`crate::systems::vats::LimbReport`] and the actor's dismemberment record as `vats.limbs` prints them.
pub(crate) fn format_limbs(
    entity: byroredux_core::ecs::EntityId,
    report: &crate::systems::vats::LimbReport,
    record: Option<byroredux_core::character::DismemberRecord>,
) -> String {
    let mut out = format!("vats.limbs: entity={entity:?} table={} parts={}", report.table, report.rows.len());
    for (index, row) in report.rows.iter().enumerate() {
        let (condition, state) = match row.condition {
            Some(value) => (format!("{value:.0}"), if row.crippled() { "crippled" } else { "healthy" }),
            None => ("-".to_string(), "no-condition"),
        };
        out.push_str(&format!(
            "\n  [{index}] {} loc={} dmg_mult={:.2} condition={condition} {state}",
            row.name, row.body_location, row.damage_mult
        ));
    }
    if let Some(record) = record {
        let gone: Vec<u8> = (0..=byroredux_core::character::MAX_BODY_LOCATION).filter(|&loc| record.is_gone(loc)).collect();
        out.push_str(&format!("\n  gone={gone:?} killing_blow={:?}", record.killing_blow));
    }
    out
}

/// `vats.limbs <entity|.>` - an actor's limb table, the condition value each part names (crippled at or below
/// zero), the parts recorded gone and the killing blow (VATS-LIMB-001, -005, -008). An actor whose limb table did
/// not resolve says so, which is how modded creatures that lose limb targeting are found (PRD §6 row 4).
pub(crate) struct VatsLimbsCommand;

impl ConsoleCommand for VatsLimbsCommand {
    fn name(&self) -> &str {
        "vats.limbs"
    }
    fn description(&self) -> &str {
        "Report an actor's limb table, per-limb condition (crippled or healthy) and dismemberment: vats.limbs <entity|.>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(token) = args.split_whitespace().next() else {
            return CommandOutput::error("usage: vats.limbs <entity|.>");
        };
        let entity = match resolve_console_entity(world, token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("vats.limbs: {message}")),
        };
        let Some(report) = crate::systems::vats::limb_report(world, entity) else {
            return CommandOutput::error(format!("vats.limbs: {entity:?} is not an actor with a form id in a loaded load order"));
        };
        let record = world.get::<byroredux_core::character::DismemberRecord>(entity).map(|record| *record);
        CommandOutput::line(format_limbs(entity, &report, record))
    }
}

/// `vats.queue <part-name>` — queue every scanned part whose name
/// matches (case-insensitive), first match if several.
pub(crate) struct VatsQueueCommand;

impl ConsoleCommand for VatsQueueCommand {
    fn name(&self) -> &str {
        "vats.queue"
    }
    fn description(&self) -> &str {
        "Queue a scanned body part by name (e.g. Torso) for the next confirm"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let name = args.trim();
        if name.is_empty() {
            return CommandOutput::error("vats.queue: part name required (e.g. Torso)");
        }
        let Some(matched) = world
            .try_resource::<VatsState>()
            .and_then(|vats| {
                vats.targets
                    .iter()
                    .find(|part| part.part_name.eq_ignore_ascii_case(name))
                    .map(|part| (part.entity, part.part_index, part.ap_cost))
            })
        else {
            return CommandOutput::error(format!("vats.queue: no scanned part named {name:?}"));
        };
        match vats_queue_part(world, matched.0, matched.1) {
            Ok(()) => CommandOutput::line(format!(
                "vats.queue: queued {} (ap {:.0}, reserved {:.0})",
                name,
                matched.2,
                world
                    .try_resource::<VatsState>()
                    .map(|vats| vats.ap_reserved)
                    .unwrap_or(0.0)
            )),
            Err(VatsQueueRefusal::NotInVats) => {
                CommandOutput::error("vats.queue: not in VATS (vats.enter first)")
            }
            Err(VatsQueueRefusal::PartGone) => {
                CommandOutput::error("vats.queue: part vanished from the scan")
            }
            Err(VatsQueueRefusal::NotEnoughAp { needed, available }) => {
                CommandOutput::error(format!(
                    "vats.queue: not enough AP ({needed:.0} needed, {available:.0} available)"
                ))
            }
        }
    }
}

/// `vats.confirm` — charge the reserved AP, roll and execute the queue,
/// leave VATS.
pub(crate) struct VatsConfirmCommand;

impl ConsoleCommand for VatsConfirmCommand {
    fn name(&self) -> &str {
        "vats.confirm"
    }
    fn description(&self) -> &str {
        "Charge AP, execute the queued VATS attacks through the combat funnel, exit"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        if !vats_active(world) {
            return CommandOutput::error("vats.confirm: not in VATS (vats.enter first)");
        }
        let queued = world
            .try_resource::<VatsState>()
            .map(|vats| vats.queue.len())
            .unwrap_or(0);
        if queued == 0 {
            return CommandOutput::error("vats.confirm: queue is empty (vats.queue <part> first)");
        }
        if vats_confirm(world) {
            CommandOutput::line(format!("vats.confirm: executed {queued} queued attack(s)"))
        } else {
            CommandOutput::error("vats.confirm: execution refused (no player, or the queue costs more AP than the pool holds)")
        }
    }
}

/// `vats.exit` — leave every VATS mode without spending.
pub(crate) struct VatsExitCommand;

impl ConsoleCommand for VatsExitCommand {
    fn name(&self) -> &str {
        "vats.exit"
    }
    fn description(&self) -> &str {
        "Leave VATS without spending the queued AP"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        vats_cancel(world);
        CommandOutput::line("vats: exited")
    }
}
