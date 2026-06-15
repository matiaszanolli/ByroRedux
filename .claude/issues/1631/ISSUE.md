# TD7-002: CNTO subrecord size >= 8 duplicated across two record parsers

_Filed as #1631 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Magic Numbers · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD7-002)
**Status**: NEW

## Description
CNTO (u32 FormID + i32 count = 8 B) is parsed identically in two files, both guarding on the bare `>= 8`. `crates/plugin/src/esm/records/actor.rs:530` comment "CNTO: shared with CONT" acknowledges the coupling but no shared constant exists. The codebase already has the `WIRE_SIZE` convention (`InfoSet::WIRE_SIZE`, `WeatherColor::WIRE_SIZE`).

## Evidence
`container.rs:88` — `b"CNTO" if sub.data.len() >= 8 =>`; `actor.rs:531` — `b"CNTO" if sub.data.len() >= 8 =>` (preceded by `:530 // CNTO: shared with CONT`).

## Impact
A magic `8` is duplicated for the same wire layout in two record parsers; a future CNTO layout assumption must be hand-applied to both sites with nothing tying them together.

## Suggested Fix
Add `const CNTO_WIRE_SIZE: usize = 8;` near the shared inventory-entry types and reference at both gate sites.

## Completeness Checks
- [ ] **SIBLING**: Both gate sites (container.rs / actor.rs) reference the single `CNTO_WIRE_SIZE` const
- [ ] **TESTS**: Existing CNTO parse tests (e.g. `cnto_bytes` fixtures) still pass
