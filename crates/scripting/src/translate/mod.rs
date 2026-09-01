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
pub mod compose;
pub mod effects;
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
///
/// **Fragments dispatch off-chain, by contract — not through this table
/// (SCR-D5-02 / #1739 landed).** The quest-fragment lowerer
/// [`effects::lower_fragment`] is deliberately absent from `RECOGNIZERS`:
/// fragments are invoked by the quest-stage contract (stage N's
/// `Fragment_N` runs when the quest reaches stage N), not by shape
/// recognition. The QUST `VMAD` fragment decoder
/// (`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`,
/// landed) recovers each stage→`Fragment_N` binding, and
/// [`crate::fragment::populate_quest_fragments_from_pex`] feeds each body
/// into `lower_fragment` at cell load, keyed into
/// [`crate::fragment::QuestStageFragments`] for
/// [`crate::quest_fragment_dispatch_system`]. So this recognizer chain
/// stays event-handler-only (the ~22% handler population); the 69.5%
/// fragment population flows through the stage-contract path instead.
const RECOGNIZERS: &[Recognizer] = &[
    // Per-script (long tail):
    recognizers::two_state_activator::recognize,
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
/// parse or decompile — including a decompiler panic (SCR-D5-NEW-02 /
/// #1816) — is a silent `None` (logged at debug), treated like any other
/// recognizer miss.
///
/// The decompiled `Script` is owned locally; the returned [`Recognized`]
/// captures only owned constants, so it outlives the borrow.
pub fn translate_pex(
    pex_bytes: &[u8],
    game: GameKind,
    script_instance: Option<&ScriptInstanceData>,
    owning_quest: Option<u32>,
) -> Option<Recognized> {
    translate_pex_detailed(pex_bytes, game, script_instance, owning_quest).recognized
}

/// Result of one parse/decompile/recognize pass, including compatibility data
/// for an engine-owned load-order registry. This avoids parsing every PEX a
/// second time merely to aggregate extender calls.
pub struct PexTranslation {
    pub recognized: Option<Recognized>,
    pub provider_program: Option<crate::PapyrusProviderProgram>,
    pub provider_error: Option<crate::PapyrusProviderProgramError>,
    pub compatibility: Option<crate::compatibility::CompatibilityReport>,
    pub fingerprint: u64,
}

pub fn translate_pex_detailed(
    pex_bytes: &[u8],
    game: GameKind,
    script_instance: Option<&ScriptInstanceData>,
    owning_quest: Option<u32>,
) -> PexTranslation {
    translate_pex_detailed_with_providers(
        pex_bytes,
        game,
        script_instance,
        owning_quest,
        &crate::PapyrusProviderCatalog::default(),
    )
}

/// Parse and decompile PEX once, producing both existing canonical behavior
/// recognition and manifest-backed provider handlers from the same AST.
pub fn translate_pex_detailed_with_providers(
    pex_bytes: &[u8],
    game: GameKind,
    script_instance: Option<&ScriptInstanceData>,
    owning_quest: Option<u32>,
    providers: &crate::PapyrusProviderCatalog,
) -> PexTranslation {
    let fingerprint = pex_fingerprint(pex_bytes);
    let pex = match byroredux_pex::parse(pex_bytes) {
        Ok(p) => p,
        Err(e) => {
            log::debug!("translate_pex: .pex parse failed: {e}");
            return PexTranslation {
                recognized: None,
                provider_program: None,
                provider_error: None,
                compatibility: None,
                fingerprint,
            };
        }
    };
    let compatibility = crate::compatibility::analyze_pex_compatibility(&pex);
    crate::compatibility::log_compatibility_report(&compatibility);
    let mut provider_program = None;
    let mut provider_error = None;
    let recognized = decompile_catching_panics(|| byroredux_pex::decompile::decompile_script(&pex))
        .and_then(|script| {
            match crate::lower_provider_program(&script, providers) {
                Ok(program) => provider_program = program,
                Err(error) => provider_error = Some(error),
            }
            let source = ScriptSource::PapyrusSource(&script);
            translate_script(&source, game, script_instance, owning_quest)
        });
    PexTranslation {
        recognized,
        provider_program,
        provider_error,
        compatibility: Some(compatibility),
        fingerprint,
    }
}

pub(crate) fn pex_fingerprint(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

/// Run a decompile and flatten both failure modes — a returned `Err` and an
/// unwinding panic — into `None`.
///
/// SCR-D5-NEW-02 / #1816: the decompiler carries internal invariant
/// `.expect()`s that a hostile or corrupt `.pex` can trip; catching that panic
/// here, the same way `pex_corpus_smoke` does, keeps it from escaping through
/// `attach_vmad_scripts` and aborting cell load.
///
/// #3287 — this exists as a named function taking a closure, rather than as an
/// inline `catch_unwind` in [`translate_pex`], so the panic arm is reachable
/// from a test. The obvious guard — feed `translate_pex` a hostile `.pex`
/// crafted to trip one of the cited `.expect()`s — turns out not to be
/// constructible by hand: `cfg.rs`'s `checked_target` already converts every
/// malformed-jump case reachable from bytes into a `DecompileError`, so the
/// surviving `.expect()`s need a genuine fuzzing campaign to reach, not a
/// fixture. Guarding the mechanism is what is actually available, and it is
/// falsifiable: drop the `catch_unwind` and
/// `a_decompile_panic_is_a_silent_none` unwinds instead of passing.
fn decompile_catching_panics<F>(decompile: F) -> Option<byroredux_papyrus::ast::Script>
where
    F: FnOnce() -> Result<byroredux_papyrus::ast::Script, byroredux_pex::decompile::DecompileError>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(decompile)) {
        Ok(Ok(s)) => Some(s),
        Ok(Err(e)) => {
            log::debug!("translate_pex: decompile failed: {e}");
            None
        }
        Err(_) => {
            log::debug!("translate_pex: decompile panicked");
            None
        }
    }
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

    /// #3287 — #1816's `catch_unwind` had no guard at all: the fix was
    /// confirmed present only by reading it, so a refactor that dropped the
    /// wrapper (a `translate_pex` signature change, say) would have gone
    /// unnoticed by CI, and the escaping panic aborts cell load.
    ///
    /// A hostile `.pex` fixture would be the more direct guard, but is not
    /// constructible by hand — see [`decompile_catching_panics`] for why.
    /// This drives the panic arm through the same function production uses,
    /// so deleting the `catch_unwind` makes this test unwind rather than
    /// pass. The panic hook is silenced for the duration so the expected
    /// unwind does not print a scary backtrace in a passing run.
    #[test]
    fn a_decompile_panic_is_a_silent_none() {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let caught = decompile_catching_panics(|| panic!("decompiler invariant tripped"));
        std::panic::set_hook(prev);
        assert!(
            caught.is_none(),
            "a decompiler panic must flatten to None, not escape to the caller"
        );
    }

    /// The sibling arm: a clean `Err` from the decompiler is the same silent
    /// `None`, so the panic guard above is not the only thing keeping this
    /// path quiet.
    #[test]
    fn a_decompile_error_is_a_silent_none() {
        let caught = decompile_catching_panics(|| {
            Err(byroredux_pex::decompile::DecompileError::BadJumpOffset { ip: 0 })
        });
        assert!(caught.is_none());
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
