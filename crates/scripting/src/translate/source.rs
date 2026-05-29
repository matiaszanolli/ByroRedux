//! `ScriptSource` — the per-game input dialects that feed the single
//! [`translate_script`](super::translate_script) boundary.
//!
//! Bethesda scripting spans two compiled languages, and Papyrus ships in
//! two on-disk forms:
//!
//! - **`PapyrusSource`** — Papyrus `.psc` *source* parsed to the M30 AST
//!   (Skyrim / FO4+). The only front-end implemented in the first M47.2
//!   increment. Vanilla game BSAs ship compiled `.pex`, not `.psc`, so at
//!   runtime this front-end serves mod source and the CK `Source/`
//!   folder; vanilla coverage waits on `PapyrusCompiled`.
//! - **`Obscript`** — the pre-Papyrus `SCDA` bytecode of Oblivion / FO3 /
//!   FNV `SCPT` records (the 1257 dormant FO3 scripts). The record is
//!   parsed today and retains its bytecode verbatim; the disassembler
//!   that turns it into recognizable shapes is a later phase.
//!
//! A third form — compiled Papyrus `.pex` (the vanilla-runtime format) —
//! is the next front-end to add; it is intentionally NOT a variant yet
//! because no `.pex` parser exists, and an unconstructable variant would
//! be dead. It joins here when that parser lands.
//!
//! All front-ends translate to the *same* canonical behavior at the
//! boundary; per-game variance is resolved once, in [`super::tables`].

use byroredux_papyrus::ast::Script;
use byroredux_plugin::esm::records::script::ScriptRecord;

/// A script in one of its per-game source dialects, borrowed for the
/// duration of a [`translate_script`](super::translate_script) call.
pub enum ScriptSource<'a> {
    /// Papyrus `.psc` parsed to the M30 AST. Implemented now.
    PapyrusSource(&'a Script),
    /// Oblivion / FO3 / FNV `SCPT` record (Obscript `SCDA` bytecode
    /// retained verbatim). Designed-for; the bytecode disassembler that
    /// feeds the recognizers is a later phase.
    Obscript(&'a ScriptRecord),
}

impl ScriptSource<'_> {
    /// The script's editor-id / name, for diagnostics and registry keys.
    pub fn name(&self) -> &str {
        match self {
            ScriptSource::PapyrusSource(s) => &s.name.node.0,
            ScriptSource::Obscript(r) => &r.editor_id,
        }
    }
}
