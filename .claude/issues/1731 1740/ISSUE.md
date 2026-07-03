# #1731: LC-D7-02: VWD / "Has Distant LOD" record-header flag (0x00010000) not parsed

**Severity**: LOW
**Location**: `crates/plugin/src/esm/reader.rs`

The base-record header "Visible When Distant" flag (`0x00010000`) was stored
in `header.flags` (never dropped) but had no named constant or accessor — the
record-header flag decoder only ever masked `FLAG_COMPRESSED`, plus the
TES4-file-header Localized (0x80) and Light-Master (0x0200) bits.
`docs/engine/exal.md` §5.4 names this flag as the small parser gap blocking
proper full-model VWD culling (a separate, larger follow-up: LC-D7-01).

## Fix
Added `pub const FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x00010000;` beside
`FLAG_COMPRESSED` (made `pub`, unlike the private `FLAG_COMPRESSED`, so the
eventual LC-D7-01 LOD-spawn consumer can reference it by name), plus a
`RecordHeader::is_visible_when_distant()` accessor. Scope intentionally
stops at parsing/exposing the flag — wiring it into the LOD spawn path to
actually gate full-model culling is LC-D7-01's job (explicitly named in the
issue as a separate follow-up; the LOD pipeline currently distance-gates by
other means, so this is a refinement, not a load-bearing parse step).

## Completeness Checks
- [x] **SIBLING**: `FLAG_VISIBLE_WHEN_DISTANT` sits beside `FLAG_COMPRESSED`;
      added a test (`vwd_flag_is_distinct_from_deleted_refr_flag`) proving it
      isn't confused with the unrelated `0x20` deleted-REFR tombstone flag
      (SKY-D4-01 / #1660).
- [x] **TESTS**: 4 regression tests: flag surfaced when set, false when unset,
      distinct from the deleted-REFR flag, and coexists correctly with
      `FLAG_COMPRESSED` on the same header.

---

# #1740: SCR-D5-03: no decompiled-.pex parity test for DA10

**Severity**: LOW
**Location**: `crates/scripting/tests/pex_recognize_e2e.rs`

The byte-equality fidelity gate
(`quest_stage_gate.rs::recognizes_da10_and_reproduces_hand_builder`) only ran
`.psc` → AST → recognizer. No test took a real DA10 `.pex`, ran
`translate_pex`, and asserted the same hand-builder equality — the
decompiler→recognizer fidelity loop wasn't closed by CI (the existing
`pex_recognize_e2e.rs::da10_pex_is_recognized_as_a_quest_stage_gate` only
checked the archetype name, not field-level value fidelity; the corpus smoke
example is panic-only).

## Fix
Located the real, unmodified `scripts\da10maindoorscript.pex` inside the
user's local `Skyrim - Misc.bsa` (confirmed present via a throwaway
BSA-listing probe, since deleted — not part of this change). Added
`da10_pex_reproduces_hand_builder_byte_for_byte` to the existing
`#[ignore]`-gated `pex_recognize_e2e.rs` (needs Skyrim SE game data on disk,
consistent with every other real-game-data test in this codebase — never
embeds the compiled binary asset in the repo). It runs the real `.pex`
through `translate_pex`, spawns the resulting `QuestAdvanceOnActivate`, and
asserts every field (`owning_quest`, `target_stage`, and each condition's
`function_index`/`param_1`/`param_2`/`comparand`) equals
`da10_main_door(QuestFormId(DA10_QUEST))` — the same hand-builder the `.psc`
path is checked against. Verified passing against the real archive.

## Completeness Checks
- [x] **TESTS**: New `translate_pex` DA10 parity test asserts byte-equality
      against the same `da10_main_door` hand-builder, closing the
      decompiler→recognizer loop. Verified locally against the real
      `Skyrim - Misc.bsa` (`cargo test -p byroredux-scripting --test
      pex_recognize_e2e -- --ignored`, 3 passed).
