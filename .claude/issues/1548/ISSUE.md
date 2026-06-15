# Issue #1548: DIM3-01: Oblivion 24-byte CTDA rejected — every Oblivion condition silently dropped

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: HIGH · **Dimension**: 3 (ESM Coverage) — import-pipeline · **Status**: NEW (distinct from #603 — CLOSED/LOW FO4 32-byte stride, which mis-stated FO3/FNV as 24-byte; FO3/FNV are 28-byte, Oblivion is 24-byte)

**Location**: `crates/plugin/src/esm/records/condition.rs:222-229` (`parse_ctda`); consumers `crates/plugin/src/esm/records/misc/ai.rs:259,433` (QUST stages, INFO), `crates/plugin/src/esm/records/misc/magic.rs:435` (MGEF/SPEL)

## Description
`parse_ctda` hard-rejects payloads `< 28` bytes and always reads the FO3+ field map (`function_index` u32 @8, `run_on` u32 @20, `reference_form_id` u32 @24). Oblivion's CTDA is **24 bytes** with a different layout: `type(1)+pad(3) | comparand(4) @4 | function u16 @8 + pad(2) | param1 @12 | param2 @16 | unused @20`. No `GameKind` is plumbed into `parse_ctda`, so there is no path that can accept the Oblivion shape — every Oblivion CTDA returns `None`. (Confirmed by reading the function: the `data.len() < 28` early-return and the `u32::from_le_bytes([data[8..12]])` function-index read.)

## Evidence
- Byte-decode of vanilla Oblivion.esm: **60,115** CTDA tags, size histogram `{24: 60115}` (100%). Decoded as the Oblivion layout, the u16@8 function-index histogram is the known TES4 catalog (72=GetIsID ×19595, 58=GetStage ×12458, 79=GetGlobalValue, …); bytes 10-11 are zero in 60114/60115 (confirms u16, not u32).
- Live parse: `INFO_conditions=0`, `stage_conditions=0` across 19,278 INFOs / 390 quests; contrast FNV `INFO_conditions=59664`. (Verified with a temporary diagnostic test, since reverted; working tree clean.)

## Impact
Every Oblivion dialogue-response, quest-stage, AI-package, and magic-effect condition is lost at parse time. Empty `ConditionList` = "always fires" per Bethesda contract, so the downstream M47 logic will offer wrong dialogue branches, advance/skip quest stages incorrectly, and ignore AI-package guards. Blast radius = all Oblivion gameplay logic that gates on state. Silent — no warn, no test catches it.

## Related
#603 (CLOSED, FO4 stride; wrong FO3/FNV size premise), #1316 (condition evaluator stubs — downstream, unrelated to this parse-layout bug).

## Suggested Fix
Thread `GameKind` (or an explicit `ctda_len`-driven branch) into `parse_ctda`/`parse_condition_list` and add an Oblivion 24-byte arm: `function_index` as u16 @8, `param1 @12`, `param2 @16`, `run_on = Subject`, `reference_form_id = 0`, `extra_data_id = 0`. Keep the 28/32-byte arms unchanged. Add a regression test pinning a real Oblivion INFO's condition count > 0.

## Completeness Checks
- [ ] **SIBLING**: All three consumers (`ai.rs` QUST/INFO, `magic.rs` MGEF/SPEL) exercise the new 24-byte arm; no other CTDA decode site bypasses it
- [ ] **TESTS**: A regression test pins a real Oblivion INFO's condition count > 0 (and the 28/32-byte arms stay green)
