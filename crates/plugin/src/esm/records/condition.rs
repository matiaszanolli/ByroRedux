//! `CTDA` (Condition) sub-record parser + data model.
//!
//! M47.1 Phase 1 — conditions are the universal predicate system in
//! Creation Engine. A `CTDA` sub-record appears on perks, dialogue
//! INFOs, quest stages, AI packages, magic effects, and idle anims.
//! Each `CTDA` is one boolean test against engine state; multiple
//! `CTDA`s on the same record form a list combined with AND / OR.
//!
//! The OR-precedence quirk is the most important spec detail:
//! consecutive ORs form a block that binds tighter than AND.
//! `A AND B OR C AND D` evaluates as `A AND (B OR C) AND D`, NOT
//! `(A AND B) OR (C AND D)`. See [`evaluate`] for the implementation.
//!
//! ## Wire layout (FO3 / FNV — 28 bytes)
//!
//! ```text
//! offset  size  field
//! 0       1     type_byte (comparator + flags)
//! 1       3     pad (ignored)
//! 4       4     comparand (f32 literal, or u32 Global FormID when
//!               type_byte bit 2 = "Use Global" is set)
//! 8       4     function_index (u32 — Oblivion used u16, FO3+ widened)
//! 12      4     param_1 (function-specific — often a FormID)
//! 16      4     param_2 (function-specific)
//! 20      4     run_on_type
//! 24      4     reference_form_id (only meaningful when run_on=Reference)
//! ```
//!
//! Skyrim+ extends to 32 bytes (adds 4 bytes for alias/package/event
//! data ID). Both layouts parse here; the trailing 4-byte field is
//! captured into `extra_data_id` when present.
//!
//! ## Type byte bit layout
//!
//! ```text
//! bit 0:    OR flag (1 = OR with next condition; default AND)
//! bit 1:    Parameters use FormIDs (informational)
//! bit 2:    Use Global (comparand is FormID, not literal)
//! bit 3:    reserved
//! bit 4:    reserved
//! bits 5-7: Comparator
//!   0 = ==, 1 = !=, 2 = >, 3 = >=, 4 = <, 5 = <=
//! ```

use crate::esm::reader::SubRecord;

/// Comparison operator applied to (function_result, comparand).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComparisonOp {
    #[default]
    /// `function_result == comparand`
    Eq,
    /// `function_result != comparand`
    Ne,
    /// `function_result > comparand`
    Gt,
    /// `function_result >= comparand`
    Ge,
    /// `function_result < comparand`
    Lt,
    /// `function_result <= comparand`
    Le,
}

impl ComparisonOp {
    fn from_type_byte(type_byte: u8) -> Self {
        match type_byte >> 5 {
            0 => Self::Eq,
            1 => Self::Ne,
            2 => Self::Gt,
            3 => Self::Ge,
            4 => Self::Lt,
            5 => Self::Le,
            // 6, 7 reserved — fall back to Eq rather than panic; the
            // evaluator emits a debug log for unknown comparators
            // upstream so malformed plugins surface without breaking
            // cell load.
            _ => Self::Eq,
        }
    }

    /// Apply the comparator to a pair of values. `function_result`
    /// is what the condition function returned (Run On's evaluation);
    /// `comparand` is the right-hand-side value the condition was
    /// authored against.
    pub fn apply(self, function_result: f32, comparand: f32) -> bool {
        match self {
            Self::Eq => function_result == comparand,
            Self::Ne => function_result != comparand,
            Self::Gt => function_result > comparand,
            Self::Ge => function_result >= comparand,
            Self::Lt => function_result < comparand,
            Self::Le => function_result <= comparand,
        }
    }
}

/// Who the condition function evaluates against.
///
/// Authored on each `CTDA`; the consumer (perk dispatch, dialogue
/// gate, AI package head) is responsible for resolving the abstract
/// targets (`Subject` / `Target` / `CombatTarget` / …) to concrete
/// entity ids at evaluation time. The condition list itself only
/// stores the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunOn {
    /// Speaker for dialogue, player for quest targets, caster for
    /// magic effects. The Papyrus `Self` analogue in most contexts.
    #[default]
    Subject,
    /// Spoken-to for dialogue, package target, effect target.
    Target,
    /// Specific REFR pointed at by [`Condition.reference_form_id`].
    Reference,
    /// Subject's current combat target.
    CombatTarget,
    /// Subject's linked reference chain head.
    LinkedReference,
    /// Quest alias slot (alias id = `extra_data_id` on Skyrim+,
    /// `reference_form_id` on FO3 / FNV depending on plugin shape).
    QuestAlias,
    /// Package data ref (packages / procedures only).
    PackageData,
    /// Radiant quest event data ref.
    EventData,
}

impl RunOn {
    fn from_u32(value: u32) -> Self {
        match value {
            0 => Self::Subject,
            1 => Self::Target,
            2 => Self::Reference,
            3 => Self::CombatTarget,
            4 => Self::LinkedReference,
            5 => Self::QuestAlias,
            6 => Self::PackageData,
            7 => Self::EventData,
            // Unknown run-on falls back to Subject. The function will
            // evaluate against the wrong target but won't crash; mod
            // authoring errors surface as gameplay-visible behaviour
            // bugs rather than parse panics.
            _ => Self::Subject,
        }
    }
}

/// Right-hand side of the comparison.
///
/// Bethesda distinguishes literal numeric comparands from "Use Global"
/// comparands (which point at a GLOB record). The runtime resolves
/// the Global to its current numeric value at evaluation time. M47.1
/// Phase 3 resolves via `EsmIndex.globals[fid].value`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConditionValue {
    /// Literal f32 authored directly in the CTDA.
    Literal(f32),
    /// GLOB FormID — the evaluator looks up the current value from
    /// `EsmIndex.globals` at evaluation time.
    Global(u32),
}

impl Default for ConditionValue {
    fn default() -> Self {
        Self::Literal(0.0)
    }
}

/// One condition (one CTDA sub-record).
///
/// Multiple `Condition`s on the same record form a [`ConditionList`]
/// evaluated with the OR-precedence rule. See [`evaluate`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Condition {
    /// Function index (Bethesda's `~300` catalog; see `ConditionFunction`
    /// enum in `byroredux_scripting::condition`). Raw u32 here keeps
    /// the parser decoupled from the function catalog — the evaluator
    /// is the one that maps index → ECS query.
    pub function_index: u32,
    /// Comparator applied to `(function_result, comparand)`.
    pub comparator: ComparisonOp,
    /// Right-hand-side comparand.
    pub comparand: ConditionValue,
    /// First function-specific parameter. Common cases: FormID of an
    /// ActorValue (for `GetActorValue`), FormID of a faction (for
    /// `GetInFaction`), stage index (for `GetStage`). Function-
    /// specific interpretation lives in `byroredux_scripting`.
    pub param_1: u32,
    /// Second function-specific parameter. Often unused — many
    /// functions take only one arg.
    pub param_2: u32,
    /// Who the function evaluates against.
    pub run_on: RunOn,
    /// Specific REFR FormID — only meaningful when [`run_on`] is
    /// [`RunOn::Reference`]; zero otherwise.
    pub reference_form_id: u32,
    /// Skyrim+ trailing 4-byte field (alias id / package data id /
    /// event data id, depending on `run_on`). Zero on FO3 / FNV
    /// 28-byte layouts.
    pub extra_data_id: u32,
    /// If `true`, this condition is OR-combined with the NEXT
    /// condition in the list (forming an OR group that binds tighter
    /// than the surrounding AND chain). See [`evaluate`].
    pub or_next: bool,
}

/// A list of conditions, evaluated with OR-precedence (see [`evaluate`]).
///
/// Owned by the record carrying the condition list (perks have one,
/// AI packages have several, dialogue INFOs have one, etc.). Empty
/// lists are treated as unconditionally-true by [`evaluate`] — matches
/// Bethesda's "no conditions = always fires" contract.
pub type ConditionList = Vec<Condition>;

/// Parse one CTDA sub-record into a `Condition`. Returns `None` when
/// the payload is too short to extract the minimum fields; the
/// caller (a record walker) skips the condition silently in that
/// case rather than failing the entire record.
///
/// Accepts the 24-byte (Oblivion / TES4), 28-byte (FO3 / FNV), and
/// 32-byte (Skyrim+) layouts. Anything shorter than 24 (truncated CTDA,
/// malformed plugin) returns `None`.
pub fn parse_ctda(sub: &SubRecord) -> Option<Condition> {
    if sub.sub_type != *b"CTDA" {
        return None;
    }
    let data = &sub.data;
    // Layout by length (#1548): Oblivion (TES4) CTDA is 24 bytes; FO3 / FNV
    // are 28; Skyrim+ is 32. Offsets 0-19 (type, comparand, function@8,
    // param1@12, param2@16) are byte-identical across all three — the
    // Oblivion 24-byte record simply lacks the run_on@20 / reference@24
    // tail (its bytes 20-23 are unused). Pre-fix the hard `< 28` reject
    // dropped every Oblivion condition silently.
    if data.len() < 24 {
        return None;
    }

    let type_byte = data[0];
    let or_next = (type_byte & 0x01) != 0;
    let use_global = (type_byte & 0x04) != 0;
    let comparator = ComparisonOp::from_type_byte(type_byte);

    let comparand_bytes = [data[4], data[5], data[6], data[7]];
    let comparand = if use_global {
        ConditionValue::Global(u32::from_le_bytes(comparand_bytes))
    } else {
        ConditionValue::Literal(f32::from_le_bytes(comparand_bytes))
    };

    let function_index = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let param_1 = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let param_2 = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    // run_on / reference exist only on the 28+ byte (FO3+) layout; on
    // Oblivion 24-byte records they are absent → default Subject / 0.
    let (run_on, reference_form_id) = if data.len() >= 28 {
        let run_on_raw = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        (
            RunOn::from_u32(run_on_raw),
            u32::from_le_bytes([data[24], data[25], data[26], data[27]]),
        )
    } else {
        (RunOn::from_u32(0), 0)
    };

    // Skyrim+ trailing 4-byte field (alias id / package data id /
    // event data id, depending on `run_on`). Optional — FO3 / FNV
    // CTDAs are exactly 28 bytes and leave this at zero.
    let extra_data_id = if data.len() >= 32 {
        u32::from_le_bytes([data[28], data[29], data[30], data[31]])
    } else {
        0
    };

    Some(Condition {
        function_index,
        comparator,
        comparand,
        param_1,
        param_2,
        run_on,
        reference_form_id,
        extra_data_id,
        or_next,
    })
}

/// Walk a `SubRecord` slice extracting every CTDA into a [`ConditionList`].
///
/// Non-CTDA sub-records are silently ignored. Order is preserved —
/// the OR-precedence evaluator at `byroredux_scripting::condition::evaluate`
/// requires sequential order to correctly chunk OR groups.
pub fn parse_condition_list(subs: &[SubRecord]) -> ConditionList {
    subs.iter().filter_map(parse_ctda).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    /// Build a synthetic CTDA payload at the FO3 / FNV 28-byte layout.
    fn make_ctda_28(
        type_byte: u8,
        comparand_bytes: [u8; 4],
        function_index: u32,
        param_1: u32,
        param_2: u32,
        run_on: u32,
        reference_form_id: u32,
    ) -> SubRecord {
        let mut data = vec![type_byte, 0, 0, 0]; // type + 3 pad
        data.extend_from_slice(&comparand_bytes);
        data.extend_from_slice(&function_index.to_le_bytes());
        data.extend_from_slice(&param_1.to_le_bytes());
        data.extend_from_slice(&param_2.to_le_bytes());
        data.extend_from_slice(&run_on.to_le_bytes());
        data.extend_from_slice(&reference_form_id.to_le_bytes());
        SubRecord {
            sub_type: *b"CTDA",
            data,
        }
    }

    /// Build a synthetic CTDA payload at the Oblivion / TES4 24-byte layout
    /// (function@8, param1@12, param2@16, then 4 unused bytes — no run_on /
    /// reference tail).
    fn make_ctda_24(
        type_byte: u8,
        comparand_bytes: [u8; 4],
        function_index: u32,
        param_1: u32,
        param_2: u32,
    ) -> SubRecord {
        let mut data = vec![type_byte, 0, 0, 0]; // type + 3 pad
        data.extend_from_slice(&comparand_bytes);
        data.extend_from_slice(&function_index.to_le_bytes());
        data.extend_from_slice(&param_1.to_le_bytes());
        data.extend_from_slice(&param_2.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]); // unused @20
        assert_eq!(data.len(), 24);
        SubRecord {
            sub_type: *b"CTDA",
            data,
        }
    }

    /// #1548 — Oblivion's 24-byte CTDA must parse, not be silently rejected
    /// by the old `< 28` guard. run_on defaults to Subject and reference to 0
    /// (those fields are absent on the TES4 layout).
    #[test]
    fn parse_oblivion_24_byte_ctda() {
        // 72 = GetIsID, the most common TES4 condition function.
        let sub = make_ctda_24(0x00, 1.0_f32.to_le_bytes(), 72, 0xDEAD, 0xBEEF);
        let cond = parse_ctda(&sub).expect("Oblivion 24-byte CTDA must parse");
        assert_eq!(cond.function_index, 72);
        assert_eq!(cond.comparator, ComparisonOp::Eq);
        assert_eq!(cond.comparand, ConditionValue::Literal(1.0));
        assert_eq!(cond.param_1, 0xDEAD);
        assert_eq!(cond.param_2, 0xBEEF);
        assert_eq!(cond.run_on, RunOn::Subject);
        assert_eq!(cond.reference_form_id, 0);
    }

    /// A payload shorter than the Oblivion minimum (24) is still rejected.
    #[test]
    fn parse_ctda_under_24_bytes_returns_none() {
        let sub = SubRecord {
            sub_type: *b"CTDA",
            data: vec![0u8; 20],
        };
        assert!(parse_ctda(&sub).is_none());
    }

    #[test]
    fn parse_ctda_eq_literal_no_or() {
        // type_byte: comparator=Eq (0 << 5), no flags set → 0
        let sub = make_ctda_28(0, 3.5_f32.to_le_bytes(), 58, 0xCAFE, 0, 0, 0);
        let cond = parse_ctda(&sub).expect("valid 28-byte CTDA must parse");
        assert_eq!(cond.function_index, 58); // GetStage
        assert_eq!(cond.comparator, ComparisonOp::Eq);
        assert_eq!(cond.comparand, ConditionValue::Literal(3.5));
        assert_eq!(cond.param_1, 0xCAFE);
        assert_eq!(cond.run_on, RunOn::Subject);
        assert!(!cond.or_next);
    }

    #[test]
    fn parse_ctda_or_flag_decoded() {
        // Type byte bit 0 = OR. comparator stays Eq (high 3 bits = 0).
        let sub = make_ctda_28(0x01, 1.0_f32.to_le_bytes(), 9, 0, 0, 0, 0);
        let cond = parse_ctda(&sub).unwrap();
        assert!(cond.or_next, "type_byte bit 0 must decode as or_next");
    }

    #[test]
    fn parse_ctda_use_global_switches_comparand_kind() {
        // Type byte bit 2 = Use Global → comparand is FormID, not f32.
        let global_fid: u32 = 0x0001_2345;
        let sub = make_ctda_28(0x04, global_fid.to_le_bytes(), 14, 0, 0, 0, 0);
        let cond = parse_ctda(&sub).unwrap();
        assert_eq!(cond.comparand, ConditionValue::Global(global_fid));
    }

    #[test]
    fn parse_ctda_comparators_round_trip() {
        // Walk every comparator. Type byte top 3 bits encode it.
        let cases = [
            (0 << 5, ComparisonOp::Eq),
            (1 << 5, ComparisonOp::Ne),
            (2 << 5, ComparisonOp::Gt),
            (3 << 5, ComparisonOp::Ge),
            (4 << 5, ComparisonOp::Lt),
            (5 << 5, ComparisonOp::Le),
        ];
        for (type_byte, expected) in cases {
            let sub = make_ctda_28(type_byte, 0_f32.to_le_bytes(), 0, 0, 0, 0, 0);
            let cond = parse_ctda(&sub).unwrap();
            assert_eq!(cond.comparator, expected, "type_byte {type_byte:#x}");
        }
    }

    #[test]
    fn parse_ctda_run_on_variants_decoded() {
        let cases = [
            (0u32, RunOn::Subject),
            (1, RunOn::Target),
            (2, RunOn::Reference),
            (3, RunOn::CombatTarget),
            (4, RunOn::LinkedReference),
            (5, RunOn::QuestAlias),
            (6, RunOn::PackageData),
            (7, RunOn::EventData),
            (42, RunOn::Subject), // unknown → Subject fallback
        ];
        for (raw, expected) in cases {
            let sub = make_ctda_28(0, 0_f32.to_le_bytes(), 0, 0, 0, raw, 0);
            let cond = parse_ctda(&sub).unwrap();
            assert_eq!(cond.run_on, expected, "run_on raw {raw}");
        }
    }

    #[test]
    fn parse_ctda_skyrim_32_byte_layout_captures_extra_data_id() {
        let mut data = vec![0u8, 0, 0, 0]; // type + 3 pad
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&58u32.to_le_bytes()); // function
        data.extend_from_slice(&0xCAFEu32.to_le_bytes()); // param 1
        data.extend_from_slice(&0u32.to_le_bytes()); // param 2
        data.extend_from_slice(&5u32.to_le_bytes()); // run_on = QuestAlias
        data.extend_from_slice(&0u32.to_le_bytes()); // reference (unused for QuestAlias)
        data.extend_from_slice(&0xABCDu32.to_le_bytes()); // extra_data_id (alias id)
        let sub = SubRecord {
            sub_type: *b"CTDA",
            data,
        };
        let cond = parse_ctda(&sub).unwrap();
        assert_eq!(cond.run_on, RunOn::QuestAlias);
        assert_eq!(cond.extra_data_id, 0xABCD);
    }

    #[test]
    fn parse_ctda_rejects_too_short() {
        let sub = SubRecord {
            sub_type: *b"CTDA",
            data: vec![0; 20], // < 28 bytes
        };
        assert!(parse_ctda(&sub).is_none());
    }

    #[test]
    fn parse_ctda_rejects_non_ctda_subrecord() {
        let sub = SubRecord {
            sub_type: *b"EDID",
            data: vec![0; 28],
        };
        assert!(parse_ctda(&sub).is_none());
    }

    #[test]
    fn parse_condition_list_extracts_all_ctdas_in_order() {
        let mixed = vec![
            SubRecord {
                sub_type: *b"EDID",
                data: b"PerkQuest\0".to_vec(),
            },
            make_ctda_28(0, 1_f32.to_le_bytes(), 58, 0xAA, 0, 0, 0),
            make_ctda_28(0x01, 2_f32.to_le_bytes(), 9, 0xBB, 0, 0, 0), // OR flag
            make_ctda_28(0, 3_f32.to_le_bytes(), 71, 0xCC, 0, 0, 0),
        ];
        let list = parse_condition_list(&mixed);
        assert_eq!(list.len(), 3, "only the 3 CTDAs make it through");
        assert_eq!(list[0].function_index, 58);
        assert_eq!(list[1].function_index, 9);
        assert!(list[1].or_next);
        assert_eq!(list[2].function_index, 71);
        assert!(!list[2].or_next);
    }

    #[test]
    fn comparison_op_apply_eq_and_ne() {
        assert!(ComparisonOp::Eq.apply(1.0, 1.0));
        assert!(!ComparisonOp::Eq.apply(1.0, 2.0));
        assert!(ComparisonOp::Ne.apply(1.0, 2.0));
        assert!(!ComparisonOp::Ne.apply(1.0, 1.0));
    }

    #[test]
    fn comparison_op_apply_ordering_operators() {
        assert!(ComparisonOp::Gt.apply(2.0, 1.0));
        assert!(!ComparisonOp::Gt.apply(1.0, 1.0));
        assert!(ComparisonOp::Ge.apply(1.0, 1.0));
        assert!(ComparisonOp::Lt.apply(1.0, 2.0));
        assert!(!ComparisonOp::Lt.apply(1.0, 1.0));
        assert!(ComparisonOp::Le.apply(1.0, 1.0));
    }
}
