//! `hud.*` — Oblivion MenuXml HUD console control (M48.4).
//!
//! The HUD renderer lives in the frame loop (see `hud.rs`'s module docs);
//! these commands mutate the [`HudControl`] World resource it reads every
//! frame. `hud.status` is the smoke-gate observable: it reports whether a
//! HUD is live even when hidden, because the resource exists only after a
//! successful `--hud` launch.

use super::shared::*;
use crate::hud::HudControl;

/// `hud.on` — show the HUD overlay.
pub(crate) struct HudOnCommand;

impl ConsoleCommand for HudOnCommand {
    fn name(&self) -> &str {
        "hud.on"
    }
    fn description(&self) -> &str {
        "Show the Oblivion MenuXml HUD overlay"
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
        "Hide the Oblivion MenuXml HUD overlay"
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

/// `hud.values <health> <magicka> <fatigue>` — pin bar fractions (0..1);
/// `hud.values auto` returns them to actor-value derivation.
pub(crate) struct HudValuesCommand;

impl ConsoleCommand for HudValuesCommand {
    fn name(&self) -> &str {
        "hud.values"
    }
    fn description(&self) -> &str {
        "Pin HUD bar fractions: hud.values <0-1> <0-1> <0-1> | auto"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<HudControl>() else {
            return CommandOutput::error("hud: not launched (start with --hud)");
        };
        let tokens: Vec<&str> = args.split_whitespace().collect();
        if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("auto") {
            control.health = None;
            control.magicka = None;
            control.fatigue = None;
            return CommandOutput::line("hud: bars follow actor values");
        }
        if tokens.len() != 3 {
            return CommandOutput::error("usage: hud.values <health> <magicka> <fatigue> (0-1) | auto");
        }
        let parse = |t: &str| -> Result<f32, ()> {
            t.parse::<f32>().map(|v| v.clamp(0.0, 1.0)).map_err(|_| ())
        };
        match (parse(tokens[0]), parse(tokens[1]), parse(tokens[2])) {
            (Ok(h), Ok(m), Ok(f)) => {
                control.health = Some(h);
                control.magicka = Some(m);
                control.fatigue = Some(f);
                CommandOutput::line(format!("hud: bars pinned to {h:.2}/{m:.2}/{f:.2}"))
            }
            _ => CommandOutput::error("hud.values: three 0-1 fractions or 'auto'"),
        }
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
        "Report the Oblivion MenuXml HUD state"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource::<HudControl>() {
            Some(control) => {
                let fmt = |v: Option<f32>, live: f32| match v {
                    Some(pinned) => format!("{pinned:.2} (pinned)"),
                    None => format!("{live:.2} (auto)"),
                };
                CommandOutput::line(format!(
                    "hud: launched visible={} health={} magicka={} fatigue={} heading={}",
                    control.visible,
                    fmt(control.health, 1.0),
                    fmt(control.magicka, 1.0),
                    fmt(control.fatigue, 1.0),
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
