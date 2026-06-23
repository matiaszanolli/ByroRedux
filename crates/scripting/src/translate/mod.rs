//! M47.2 — the canonical scripting-translation layer.
//!
//! The scripting analog of NIFAL. A **single boundary**,
//! [`translate_script`], turns a per-game [`ScriptSource`] (Papyrus AST
//! today; `.pex` / Obscript later) + its per-instance properties into a
//! canonical behavior: ECS component(s) + the dispatch systems that
//! already exist (M47.0 hooks, M47.1 conditions, `QuestStageState`,
//! `RecurringUpdate`). Per-game variance is resolved here, behind
//! [`tables`]; nothing downstream of this boundary makes a per-game
//! scripting decision.
//!
//! The boundary runs a chain of *recognizers* (free fns in
//! [`recognizers`]). The first to match wins; a script no recognizer
//! claims returns `None` — a silent miss the caller treats as "no
//! consumer yet", exactly like an M47.0 [`crate::ScriptRegistry`] miss.

pub mod archetype;
pub mod recognizers;
pub mod source;
pub mod tables;

pub use archetype::{RecognizeCtx, Recognized, Recognizer};
pub use source::ScriptSource;
pub use tables::CanonicalEvent;

use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

/// Recognizer chain, in priority order. Per-script recognizers come
/// before the generic ones so a bespoke script isn't swallowed by a
/// family match.
const RECOGNIZERS: &[Recognizer] = &[
    // Per-script (long tail):
    recognizers::rumble::recognize,
    // Generic families (one recognizer covers many scripts):
    recognizers::quest_stage_gate::recognize,
];

/// **THE** scripting translate boundary: per-game source + per-instance
/// binding context → canonical behavior spawn, or `None` (silent miss).
/// Per-game classification happens here and only here.
///
/// `script_instance` is the VMAD-decoded properties for this reference
/// (object/quest refs); `owning_quest` is the alias-owning quest for
/// alias-attached scripts. Both come from the attach context (the cell
/// loader); pass `None` when unavailable (recognizers needing them then
/// decline).
pub fn translate_script(
    source: &ScriptSource<'_>,
    game: GameKind,
    script_instance: Option<&ScriptInstanceData>,
    owning_quest: Option<u32>,
) -> Option<Recognized> {
    let ctx = RecognizeCtx {
        source,
        game,
        script_instance,
        owning_quest,
    };
    RECOGNIZERS.iter().find_map(|recognize| recognize(&ctx))
}

/// Translate a **compiled** Papyrus script (`.pex` bytes) — the
/// vanilla-runtime form shipped in game archives.
///
/// Decompiles the bytecode to the same `byroredux_papyrus` AST a `.psc`
/// parses to (via [`byroredux_pex`]) and runs it through the same
/// [`translate_script`] recognizer chain — so a compiled script and its
/// source decompile to one canonical behavior. A `.pex` that fails to
/// parse or decompile is a silent `None` (logged at debug), treated like
/// any other recognizer miss.
///
/// The decompiled `Script` is owned locally; the returned [`Recognized`]
/// captures only owned constants, so it outlives the borrow.
pub fn translate_pex(
    pex_bytes: &[u8],
    game: GameKind,
    script_instance: Option<&ScriptInstanceData>,
    owning_quest: Option<u32>,
) -> Option<Recognized> {
    let pex = match byroredux_pex::parse(pex_bytes) {
        Ok(p) => p,
        Err(e) => {
            log::debug!("translate_pex: .pex parse failed: {e}");
            return None;
        }
    };
    let script = match byroredux_pex::decompile::decompile_script(&pex) {
        Ok(s) => s,
        Err(e) => {
            log::debug!("translate_pex: decompile failed: {e}");
            return None;
        }
    };
    let source = ScriptSource::PapyrusSource(&script);
    translate_script(&source, game, script_instance, owning_quest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::parse_script;

    #[test]
    fn unrecognized_script_is_a_silent_miss() {
        // A parseable but unrecognized script returns None (no consumer).
        let (script, errors) = parse_script("ScriptName Foo extends ObjectReference\n")
            .expect("trivial script parses");
        assert!(errors.is_empty());
        let src = ScriptSource::PapyrusSource(&script);
        assert!(translate_script(&src, GameKind::Skyrim, None, None).is_none());
    }

    #[test]
    fn translate_pex_on_empty_bytes_is_a_clean_none() {
        // The attach path hands arbitrary archive bytes to translate_pex;
        // an empty / truncated `.pex` must be a graceful None, never a
        // panic (the "no consumer yet" contract for unparseable input).
        assert!(translate_pex(&[], GameKind::Skyrim, None, None).is_none());
    }

    #[test]
    fn translate_pex_on_garbage_bytes_is_a_clean_none() {
        // Bytes with no valid `.pex` magic — parse fails, logged at debug,
        // returns None rather than propagating an error or panicking.
        let garbage = b"this is definitely not compiled papyrus bytecode";
        assert!(translate_pex(garbage, GameKind::Skyrim, None, None).is_none());
    }

    #[test]
    fn translate_pex_on_truncated_after_magic_is_a_clean_none() {
        // Correct LE magic (0xFA57C0DE) but nothing after it — the reader
        // runs off the end mid-header; decode must fail gracefully.
        let truncated = [0xDE, 0xC0, 0x57, 0xFA, 0x00, 0x00];
        assert!(translate_pex(&truncated, GameKind::Skyrim, None, None).is_none());
    }
}
