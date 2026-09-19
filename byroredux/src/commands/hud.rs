//! `hud.*` — MenuXml HUD console control (M48.4 Oblivion / M48.5 FO3).
//!
//! The HUD renderer lives in the frame loop (see `hud.rs`'s module docs);
//! these commands mutate the [`HudControl`] World resource it reads every
//! frame. `hud.status` is the smoke-gate observable: it reports whether a
//! HUD is live even when hidden, because the resource exists only after a
//! successful `--hud` launch. Bar count and labels are the launched
//! game's (3 for Oblivion, 2 for FO3/FNV).

use super::shared::*;
use crate::hud::HudControl;

/// `hud.on` — show the HUD overlay.
pub(crate) struct HudOnCommand;

impl ConsoleCommand for HudOnCommand {
    fn name(&self) -> &str {
        "hud.on"
    }
    fn description(&self) -> &str {
        "Show the MenuXml HUD overlay"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource_mut::<HudControl>() {
            Some(mut control) => {
                control.visible = true;
                CommandOutput::line("hud: visible")
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}

/// `hud.off` — hide the HUD overlay (stops the UI quad entirely).
pub(crate) struct HudOffCommand;

impl ConsoleCommand for HudOffCommand {
    fn name(&self) -> &str {
        "hud.off"
    }
    fn description(&self) -> &str {
        "Hide the MenuXml HUD overlay"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource_mut::<HudControl>() {
            Some(mut control) => {
                control.visible = false;
                CommandOutput::line("hud: hidden")
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}

/// `hud.values <f0> [f1] [f2]` — pin bar fractions (0..1), one per the
/// game's bars; `hud.values auto` returns them to actor-value
/// derivation.
pub(crate) struct HudValuesCommand;

impl ConsoleCommand for HudValuesCommand {
    fn name(&self) -> &str {
        "hud.values"
    }
    fn description(&self) -> &str {
        "Pin HUD bar fractions: hud.values <0-1>... | auto"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<HudControl>() else {
            return CommandOutput::error("hud: not launched (start with --hud)");
        };
        let count = control.bar_count as usize;
        let labels = control.bar_labels;
        let tokens: Vec<&str> = args.split_whitespace().collect();
        if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("auto") {
            control.bars = [None; 3];
            return CommandOutput::line("hud: bars follow actor values");
        }
        if tokens.len() != count {
            let usage = labels[..count].join(" ");
            return CommandOutput::error(format!(
                "usage: hud.values <{usage}> (one 0-1 fraction per bar) | auto"
            ));
        }
        let parse = |t: &str| -> Result<f32, ()> {
            t.parse::<f32>().map(|v| v.clamp(0.0, 1.0)).map_err(|_| ())
        };
        let parsed: Vec<Result<f32, ()>> = tokens.iter().map(|t| parse(t)).collect();
        if parsed.iter().any(|p| p.is_err()) {
            return CommandOutput::error("hud.values: 0-1 fractions or 'auto'");
        }
        let values: Vec<f32> = parsed.into_iter().map(|p| p.unwrap()).collect();
        for (slot, value) in values.iter().enumerate() {
            control.bars[slot] = Some(*value);
        }
        let joined = values
            .iter()
            .map(|v| format!("{v:.2}"))
            .collect::<Vec<_>>()
            .join("/");
        CommandOutput::line(format!("hud: bars pinned to {joined}"))
    }
}

/// `hud.heading <degrees>` — pin the compass; `hud.heading auto` follows
/// the camera.
pub(crate) struct HudHeadingCommand;

impl ConsoleCommand for HudHeadingCommand {
    fn name(&self) -> &str {
        "hud.heading"
    }
    fn description(&self) -> &str {
        "Pin the compass heading: hud.heading <0-360> | auto"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<HudControl>() else {
            return CommandOutput::error("hud: not launched (start with --hud)");
        };
        let tokens: Vec<&str> = args.split_whitespace().collect();
        let Some(first) = tokens.first() else {
            return CommandOutput::error("usage: hud.heading <0-360> | auto");
        };
        if first.eq_ignore_ascii_case("auto") {
            control.heading = None;
            return CommandOutput::line("hud: compass follows the camera");
        }
        match first.parse::<f32>() {
            Ok(deg) => {
                let deg = deg.rem_euclid(360.0);
                control.heading = Some(deg);
                CommandOutput::line(format!("hud: compass pinned to {deg:.0}°"))
            }
            Err(_) => CommandOutput::error("hud.heading: degrees 0-360 or 'auto'"),
        }
    }
}

/// `hud.status` — the smoke-gate observable.
pub(crate) struct HudStatusCommand;

impl ConsoleCommand for HudStatusCommand {
    fn name(&self) -> &str {
        "hud.status"
    }
    fn description(&self) -> &str {
        "Report the MenuXml HUD state"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource::<HudControl>() {
            Some(control) => {
                let fmt = |v: Option<f32>| match v {
                    Some(pinned) => format!("{pinned:.2} (pinned)"),
                    None => "1.00 (auto)".to_string(),
                };
                let bars = (0..control.bar_count as usize)
                    .map(|i| format!("{}={}", control.bar_labels[i], fmt(control.bars[i])))
                    .collect::<Vec<_>>()
                    .join(" ");
                CommandOutput::line(format!(
                    "hud: launched visible={} {bars} heading={}",
                    control.visible,
                    match control.heading {
                        Some(deg) => format!("{deg:.0} (pinned)"),
                        None => "auto".to_string(),
                    },
                ))
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}
