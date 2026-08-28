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
//! 8       2     function_index (u16 in Oblivion through Skyrim+)
//! 10      2     unused
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

use crate::esm::reader::{FormIdRemap, SubRecord};

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

/// Stable, case-insensitive identity for a CTDA string parameter.
///
/// Skyrim stores condition string parameters in `CIS1` / `CIS2`
/// subrecords adjacent to the owning `CTDA`; the numeric parameter slot in
/// the CTDA itself is not a persistent string identifier. Keeping a compact
/// hash here preserves [`Condition`]'s cheap `Copy` representation while
/// allowing the runtime to address Papyrus variables by their authored name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConditionStringId(pub u64);

impl ConditionStringId {
    pub fn from_text(text: &str) -> Self {
        // FNV-1a over ASCII-folded bytes. Papyrus identifiers are
        // case-insensitive; non-ASCII bytes pass through unchanged.
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in text.bytes() {
            hash ^= u64::from(byte.to_ascii_lowercase());
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        Self(hash)
    }
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
    /// `CIS1` string parameter, when authored for this CTDA.
    pub param_1_text: Option<ConditionStringId>,
    /// `CIS2` string parameter, notably the variable name for Skyrim
    /// `GetVMScriptVariable`.
    pub param_2_text: Option<ConditionStringId>,
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
/// Accepts the 20-byte (FNV short form), 24-byte (Oblivion / TES4), 28-byte
/// (FO3 / FNV), and 32-byte (Skyrim+) layouts. Anything shorter than 20
/// (truncated CTDA, malformed plugin) returns `None`.
pub fn parse_ctda(sub: &SubRecord) -> Option<Condition> {
    if sub.sub_type != *b"CTDA" {
        return None;
    }
    let data = &sub.data;
    // Layout by length (#1548, #3350): the canonical prefix is 20 bytes —
    // type@0, comparand@4, function u16@8 (+ unused@10), param1@12, param2@16
    // — and every known layout is that prefix plus an optional tail:
    //
    //   20  FNV short form: prefix only, no run_on/reference tail
    //   24  Oblivion (TES4): prefix + 4 unused bytes
    //   28  FO3 / FNV:       prefix + run_on@20 + reference@24
    //   32  Skyrim+:         the above + extra_data_id@28
    //
    // The 20-byte form is real, vanilla FNV content, not corruption. A CTDA
    // size histogram over the whole of FalloutNV.esm reads
    // `{28: 67880, 20: 123, 24: 2}`; the 20-byte rows are 24 PACK conditions
    // (the two Patrol packages `0x26d86 mvsRaiderTowerPatrolA` and
    // `0x26d88 mvsRaiderTowerPatrolB`, each a twelve-leaf OR-chain of
    // hour-of-day windows), 98 IDLE conditions and 1 QUST condition. The
    // sub-record size field reads `0x0014` verbatim in the file — this is the
    // authored length, not a walker artefact.
    //
    // Pre-#3350 the `< 24` reject dropped all of them with only a debug log,
    // exactly as the earlier `< 28` reject had dropped every Oblivion
    // condition before #1548. That was invisible rather than harmless: the
    // conditions never reached `PackRecord.conditions`, so an empty list made
    // the packages unconditionally active, and the day `ConditionFunction`
    // learns function 18 those two Patrol packages would *still* be
    // unconditionally active — a wrong result arriving via a change in an
    // unrelated file, with no test to catch it.
    if data.len() < 20 {
        log::debug!(
            "CTDA payload {} bytes < 20 (shortest known prefix) — dropping condition",
            data.len()
        );
        return None;
    }
    // Defense-in-depth (#1550): a CTDA is exactly 20, 24, 28 or 32 bytes.
    // Anything else is parsed best-effort against the 20-byte prefix but is a
    // layout signal worth surfacing rather than silently absorbing — this is
    // the trap that hid the Oblivion 24-byte case (#1548) for so long.
    if !matches!(data.len(), 20 | 24 | 28 | 32) {
        log::debug!(
            "CTDA unexpected payload length {} (expected 20/24/28/32) — \
             parsing against the 20-byte prefix; possible per-game layout drift",
            data.len()
        );
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

    // xEdit defines this as `itU16` in TES4, FO3, FNV, and TES5. Bytes
    // 10..12 are explicitly unused and are not reliably zero in vanilla
    // Skyrim SCEN conditions (e.g. `3A 00 53 00` is GetStage, function 58,
    // not function 0x0053_003A). Keep the public field widened to u32 for
    // catalog ergonomics, but decode only the authored low word.
    let function_index = u16::from_le_bytes([data[8], data[9]]) as u32;
    let param_1 = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let param_2 = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    // run_on / reference exist only on the 28+ byte (FO3+) layout; on the
    // 20-byte FNV short form and Oblivion's 24-byte records they are absent
    // → default Subject / 0. `param_1`/`param_2` above need no length gate:
    // they live at 12..20, entirely inside the shortest accepted form.
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
        param_1_text: None,
        param_2_text: None,
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
    let mut out = Vec::new();
    for sub in subs {
        push_ctda(sub, &None, &mut out);
    }
    out
}

/// Parse one `CTDA` sub-record, remap its FormIDs, and append it to `out`.
///
/// The three-statement `parse_ctda` → [`remap_condition_form_ids`] → `push`
/// triplet was copy-pasted at five sites across four record walkers
/// (`quest.rs` ×2, `dialogue.rs`, `magic.rs`, `pack.rs`) — TD2-111 / #2070.
/// Every one of them owns a `remap`, and dropping the remap step is the
/// multi-plugin false-positive landmine described on
/// [`remap_condition_form_ids`], so keeping the three steps welded together
/// here is the point of the helper, not just the line saving.
///
/// Undecodable CTDAs are skipped (`parse_ctda` returns `None`); order is
/// preserved, which the OR-precedence evaluator requires.
pub fn push_ctda(sub: &SubRecord, remap: &Option<FormIdRemap>, out: &mut ConditionList) {
    match &sub.sub_type {
        b"CTDA" => {
            if let Some(mut cond) = parse_ctda(sub) {
                remap_condition_form_ids(&mut cond, remap);
                out.push(cond);
            }
        }
        b"CIS1" | b"CIS2" => {
            let Some(condition) = out.last_mut() else {
                return;
            };
            let end = sub
                .data
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(sub.data.len());
            let text = String::from_utf8_lossy(&sub.data[..end]);
            let id = ConditionStringId::from_text(&text);
            if sub.sub_type == *b"CIS1" {
                condition.param_1_text = Some(id);
            } else {
                condition.param_2_text = Some(id);
            }
        }
        _ => {}
    }
}

/// Does `param_1` of the given condition function carry a FormID?
///
/// `parse_ctda` is deliberately decoupled from the function catalog, so it
/// can't know which params are FormIDs vs literals. This is the minimal
/// slice of catalog knowledge the remap pass needs: the M47.1 functions
/// whose first parameter is a FormID (and so must be load-order remapped).
/// `param_2` is a literal in every one of them (`GetStageDone`'s stage
/// index), so only `param_1` is ever promoted. Indices mirror the
/// `ConditionFunction` catalog in `byroredux_scripting::condition`.
fn param1_is_form_id(function_index: u32) -> bool {
    // CTDA function indices per TES5Edit `wbDefinitions*.pas` (FO3 == FNV).
    matches!(
        function_index,
        1   // GetDistance    — target FormID
        | 14  // GetActorValue — AVIF FormID
        | 58  // GetStage      — quest FormID
        | 59  // GetStageDone  — quest FormID (param_2 = stage, a literal)
        | 67  // GetInCell     — CELL FormID
        | 68  // GetIsClass    — CLAS FormID
        | 69  // GetIsRace     — RACE FormID
        | 72  // GetIsID       — base FormID
        | 73  // GetFactionRank — faction FormID
        | 182 // GetEquipped — inventory object FormID
        | 448 | 449 // HasPerk — perk FormID (448 Skyrim, 449 FO3/FNV)
        | 550 // IsSceneActionComplete — SCEN FormID (param_2 = action index)
        | 573 // GetReputation — REPU FormID (param_2 = axis, a literal)
        | 575 // GetReputationThreshold — REPU FormID (param_2 = axis)
        | 630 // GetVMScriptVariable — object-reference FormID (CIS2 = variable)
    )
}

/// Rewrite a condition's FormID-bearing fields from plugin-local space into
/// global load-order space, using the owning plugin's [`FormIdRemap`].
///
/// CTDA params are parsed raw (`parse_ctda` is decoupled from the function
/// catalog); this pass — run by each record walker that owns a `remap` —
/// promotes them so the downstream evaluator (`byroredux_scripting`) can
/// compare `param_1` directly against an entity's global `FormIdComponent`.
/// Without it, `GetIsID` / `HasPerk` would test a plugin-local id against a
/// global one — the multi-plugin false-positive landmine called out in #1666.
///
/// Always remaps `reference_form_id` (the `RunOn::Reference` target) and a
/// `Use Global` comparand (a GLOB FormID); remaps `param_1` only for the
/// [`param1_is_form_id`] catalog. Null (`0`) FormIDs are left untouched — a
/// remap would otherwise compose them onto the owning plugin's top byte and
/// fabricate a non-null id. A `None` remap (single standalone plugin / unit
/// tests) is identity.
pub fn remap_condition_form_ids(cond: &mut Condition, remap: &Option<FormIdRemap>) {
    let Some(remap) = remap.as_ref() else {
        return;
    };
    if cond.reference_form_id != 0 {
        cond.reference_form_id = remap.remap(cond.reference_form_id);
    }
    if let ConditionValue::Global(fid) = cond.comparand {
        if fid != 0 {
            cond.comparand = ConditionValue::Global(remap.remap(fid));
        }
    }
    if cond.param_1 != 0 && param1_is_form_id(cond.function_index) {
        cond.param_1 = remap.remap(cond.param_1);
    }
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

    /// #1550 — an unexpected length (>= 24 but not 24/28/32) is parsed
    /// best-effort against the 24-byte prefix and logged, NOT silently
    /// dropped. This pins that the length gate no longer hides layout drift
    /// the way it hid the Oblivion 24-byte case.
    #[test]
    fn parse_ctda_unexpected_length_parses_best_effort() {
        let mut sub = make_ctda_24(0x00, 2.0_f32.to_le_bytes(), 58, 0x11, 0x22);
        sub.data.extend_from_slice(&[0u8, 0u8]); // 26 bytes — not 24/28/32
        assert_eq!(sub.data.len(), 26);
        let cond = parse_ctda(&sub).expect("26-byte CTDA must still parse, not drop");
        assert_eq!(cond.function_index, 58);
        assert_eq!(cond.param_1, 0x11);
    }

    /// #3350 — a payload shorter than the 20-byte canonical prefix is still
    /// rejected. The floor moved from 24 to 20 (real FNV content authors the
    /// short form), not to zero: below 20 the `param_2` read at 16..20 would
    /// be out of bounds.
    #[test]
    fn parse_ctda_under_20_bytes_returns_none() {
        for len in [0usize, 8, 19] {
            let sub = SubRecord {
                sub_type: *b"CTDA",
                data: vec![0u8; len],
            };
            assert!(
                parse_ctda(&sub).is_none(),
                "{len}-byte CTDA is below the 20-byte prefix and must be rejected"
            );
        }
    }

    /// #3350 — the 20-byte CTDA form is real vanilla FNV content and must
    /// parse. Pre-fix the `< 24` guard dropped it with only a debug log,
    /// taking 123 conditions on `FalloutNV.esm` with it (24 PACK, 98 IDLE,
    /// 1 QUST — verified against the file: the sub-record size field reads
    /// `0x0014` verbatim, so this is the authored length, not a walker
    /// artefact).
    ///
    /// Bytes are the first two CTDAs of `0x26d88 mvsRaiderTowerPatrolB`,
    /// transcribed from a raw record dump. It is a twelve-leaf OR-chain of
    /// hour-of-day windows for a tower patrol: function 18, comparand
    /// stepping 2.0 -> 4.0 -> 6.0 ..., and the `0x01` OR bit set on
    /// alternating rows.
    #[test]
    fn parse_fnv_20_byte_ctda_from_patrol_package() {
        // 60 00 00 00 | 00 00 00 40 | 12 00 | 00 00 | 00000000 | 00000000
        let first = SubRecord {
            sub_type: *b"CTDA",
            data: vec![
                0x60, 0x00, 0x00, 0x00, // type: comparator bits, OR bit clear
                0x00, 0x00, 0x00, 0x40, // comparand f32 = 2.0
                0x12, 0x00, // function index u16 = 18
                0x00, 0x00, // unused
                0x00, 0x00, 0x00, 0x00, // param_1
                0x00, 0x00, 0x00, 0x00, // param_2
            ],
        };
        assert_eq!(first.data.len(), 20);
        let cond = parse_ctda(&first).expect("20-byte FNV CTDA must parse, not drop");
        assert_eq!(cond.function_index, 18);
        assert_eq!(cond.comparand, ConditionValue::Literal(2.0));
        assert!(!cond.or_next, "OR bit is clear on this row (type 0x60)");
        // The run_on / reference tail is absent on the short form.
        assert_eq!(cond.run_on, RunOn::Subject);
        assert_eq!(cond.reference_form_id, 0);

        // 81 00 00 00 | 00 00 80 40 | 12 00 | ... — OR bit set, comparand 4.0.
        let second = SubRecord {
            sub_type: *b"CTDA",
            data: vec![
                0x81, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x40, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        };
        let cond = parse_ctda(&second).expect("20-byte FNV CTDA must parse, not drop");
        assert_eq!(cond.function_index, 18);
        assert_eq!(cond.comparand, ConditionValue::Literal(4.0));
        assert!(
            cond.or_next,
            "type 0x81 has the 0x01 OR bit set — this is an OR-chain leaf"
        );
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
    fn condition_list_attaches_cis2_to_the_preceding_ctda() {
        let ctda = make_ctda_28(0, 1.0_f32.to_le_bytes(), 630, 0x0009_0A05, 0, 0, 0);
        let cis2 = SubRecord {
            sub_type: *b"CIS2",
            data: b"::isOpen_var\0".to_vec(),
        };

        let conditions = parse_condition_list(&[ctda, cis2]);

        assert_eq!(conditions.len(), 1);
        assert_eq!(
            conditions[0].param_2_text,
            Some(ConditionStringId::from_text("::ISOPEN_VAR"))
        );
    }

    #[test]
    fn parse_ctda_ignores_nonzero_unused_bytes_after_u16_function() {
        let mut sub = make_ctda_28(0, 1.0_f32.to_le_bytes(), 58, 0, 0, 0, 0);
        sub.data[10..12].copy_from_slice(&0x3053u16.to_le_bytes());

        let cond = parse_ctda(&sub).expect("valid CTDA");

        assert_eq!(cond.function_index, 58);
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

    /// #3350 — the accepted floor is the 20-byte canonical prefix. This test
    /// used a 20-byte payload with the comment "< 28 bytes", which had been
    /// stale since #1548 lowered the guard to 24 and was wrong outright once
    /// real FNV 20-byte CTDAs were found. Use a genuinely truncated payload.
    /// See `parse_ctda_under_20_bytes_returns_none` for the boundary sweep.
    #[test]
    fn parse_ctda_rejects_too_short() {
        let sub = SubRecord {
            sub_type: *b"CTDA",
            data: vec![0; 19], // one byte under the 20-byte prefix
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

    // ── remap_condition_form_ids (#1666) ────────────────────────────────

    #[test]
    fn remap_promotes_param1_for_form_id_functions() {
        // Plugin at global slot 2, master at slot 0. A self-referenced form
        // (top byte == master count == 1) gets the plugin's own slot (2).
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 72, // GetIsID — param_1 is a FormID
            param_1: 0x0100_0ABC,
            ..Default::default()
        };
        remap_condition_form_ids(&mut cond, &Some(remap));
        assert_eq!(
            cond.param_1, 0x0200_0ABC,
            "GetIsID param_1 promoted to slot 2"
        );
    }

    #[test]
    fn remap_skips_param1_for_non_form_id_functions() {
        // A non-catalog function: param_1 is a literal, never remapped.
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 0xDEAD,
            param_1: 0x0100_002A,
            ..Default::default()
        };
        remap_condition_form_ids(&mut cond, &Some(remap));
        assert_eq!(cond.param_1, 0x0100_002A, "non-form-id param_1 untouched");
    }

    #[test]
    fn remap_leaves_param2_literal_untouched() {
        // GetStageDone (59): param_1 is a quest FormID (remapped) but param_2
        // is the stage index (a literal) — only param_1 is ever promoted.
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 59,
            param_1: 0x0100_0001,
            param_2: 0x0BAD_F00D, // stage index garbage-shaped value — a literal
            ..Default::default()
        };
        remap_condition_form_ids(&mut cond, &Some(remap));
        assert_eq!(cond.param_1, 0x0200_0001, "quest param_1 remapped");
        assert_eq!(cond.param_2, 0x0BAD_F00D, "param_2 is a literal, untouched");
    }

    #[test]
    fn remap_scene_action_completion_scene_but_not_action_index() {
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 550,
            param_1: 0x0100_0001,
            param_2: 21,
            ..Default::default()
        };

        remap_condition_form_ids(&mut cond, &Some(remap));

        assert_eq!(cond.param_1, 0x0200_0001, "SCEN FormID remapped");
        assert_eq!(cond.param_2, 21, "action index remains a literal");
    }

    #[test]
    fn remap_vm_script_variable_object_reference() {
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 630,
            param_1: 0x0100_0A05,
            param_2_text: Some(ConditionStringId::from_text("::isOpen_var")),
            ..Default::default()
        };

        remap_condition_form_ids(&mut cond, &Some(remap));

        assert_eq!(cond.param_1, 0x0200_0A05);
        assert_eq!(
            cond.param_2_text,
            Some(ConditionStringId::from_text("::isOpen_var"))
        );
    }

    #[test]
    fn remap_get_equipped_inventory_object() {
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 182,
            param_1: 0x0100_0042,
            ..Default::default()
        };

        remap_condition_form_ids(&mut cond, &Some(remap));

        assert_eq!(cond.param_1, 0x0200_0042);
    }

    #[test]
    fn remap_get_in_cell_target_cell() {
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 67,
            param_1: 0x0100_00A5,
            ..Default::default()
        };

        remap_condition_form_ids(&mut cond, &Some(remap));

        assert_eq!(cond.param_1, 0x0200_00A5);
    }

    #[test]
    fn remap_promotes_reference_and_global_comparand_but_not_null() {
        let remap = FormIdRemap::regular(2, vec![0]);
        let mut cond = Condition {
            function_index: 72,
            reference_form_id: 0x0100_0005,
            comparand: ConditionValue::Global(0x0100_0006),
            param_1: 0, // null param_1 — must stay null, not compose onto slot 2
            ..Default::default()
        };
        remap_condition_form_ids(&mut cond, &Some(remap));
        assert_eq!(cond.reference_form_id, 0x0200_0005);
        assert_eq!(cond.comparand, ConditionValue::Global(0x0200_0006));
        assert_eq!(cond.param_1, 0, "null param_1 left untouched");
    }

    #[test]
    fn remap_none_is_identity() {
        let mut cond = Condition {
            function_index: 72,
            param_1: 0x0100_0ABC,
            reference_form_id: 0x0100_0005,
            ..Default::default()
        };
        let before = cond;
        remap_condition_form_ids(&mut cond, &None);
        assert_eq!(cond, before, "no remap = identity");
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
