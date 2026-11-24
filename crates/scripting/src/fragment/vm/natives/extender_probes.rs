//! `SKSE` / `F4SE`: the script extenders' version probes (LMC-SCR-010, `docs/engine/extender-catalog.md` §7). The
//! engine owns both static classes whole. A mod's installed `SKSE.pex` or `F4SE.pex` has one non-native function,
//! `GetScriptVersionRelease()`, which returns a constant compiled into that file: mods compare it with
//! `GetVersionRelease()` to catch a script/runtime mismatch, so the engine answers it with the same release.
//! `GetPluginVersion` answers -1, the declared "not loaded": the engine loads no extender plugin (LMC-NG-001).

use super::*;
use crate::extender_catalog::probe_answer_for;

/// The probes, lowercase, with how many arguments each takes.
const PROBES: &[(&str, usize)] = &[
    ("getversion", 0),
    ("getversionminor", 0),
    ("getversionbeta", 0),
    ("getversionrelease", 0),
    ("getscriptversionrelease", 0),
    ("getpluginversion", 1),
];

/// The catalog family a probe class answers for.
fn family(class: &str) -> &'static str {
    if class == "skse" { "skse64" } else { "f4se" }
}

impl Vm<'_> {
    /// An `SKSE.<name>()` / `F4SE.<name>()` probe: the answer for the mod the running body's script came from.
    pub(in crate::fragment::vm) fn read_extender_probe(
        &mut self,
        frame: &mut Frame,
        class: &str,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        if name == "getpluginversion" {
            self.eval_args(frame, args)?;
            return Ok(Some(Value::Int(-1)));
        }
        let script = frame.defined_in.as_ref().unwrap_or(&frame.script).name.node.0.clone();
        let answer = probe_answer_for(self.world, family(class), &script);
        let (major, minor, beta) = answer.triple();
        Ok(Some(Value::Int(match name {
            "getversion" => major,
            "getversionminor" => minor,
            "getversionbeta" => beta,
            "getversionrelease" | "getscriptversionrelease" => answer.release,
            _ => return stop(format!("{class}.{name}")),
        })))
    }
}

impl Checker<'_> {
    /// Whether the VM runs `SKSE.<name>` / `F4SE.<name>` with these arguments.
    pub(in crate::fragment::vm) fn check_extender_probe(&mut self, class: &str, name: &str, args: usize) -> Option<Kind> {
        match PROBES.iter().find(|(probe, _)| *probe == name) {
            Some((_, arity)) if *arity == args => Some(Kind::Num),
            _ => self.fail(|| format!("{class}.{name}")),
        }
    }
}
