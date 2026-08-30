# #3730 — ESM-2026-08-30-D8-02 (INFO): FileHeader::record_count is not a completeness gate — its meaning differs by game

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: INFO · **Dimension**: Real-Data Validation
**Record / Sub-record**: `TES4` / `HEDR`
**Location**: `crates/plugin/src/esm/reader.rs` (`read_file_header`; `FileHeader::record_count`, ~:512)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D8-02)

**This is a recorded fact, not a defect.** Filed so the field is never promoted into a parse-completeness assertion.

## Finding

`FileHeader::record_count` is **not** a completeness gate — its meaning differs by game. Measured with the full file walked to EOF and **zero walker errors in every case**:

- **Oblivion / FO3 / FNV / Skyrim**: `HEDR.count == records + groups` **exactly** (1 252 095 / 808 699 / 542 016 / 920 181 — all four match to the unit).
- **Starfield**: `HEDR.count == records + 1` (3 829 247 vs 3 829 246) — records only, groups excluded.
- **FO4 and FO76**: neither formula, and both short by **exactly 80 196** (1 741 853 vs 1 661 657; 5 839 497 vs 5 759 301). The identical delta across two unrelated files is **unexplained** and recorded here for whoever next reaches for this field.

## Why it matters

Today the field is used only in a `log::info!`, so nothing is wrong. On FO4/FO76 a parse-completeness assertion built on it would fire on clean vanilla data.

This also disposes of the "FO4/FO76 walk drops ~80 k records" hypothesis: the file is walked to EOF with zero errors and two independent walkers agree on the counts. It is `HEDR.count` semantics, not a walk defect.

## Suggested Action

Add a doc comment on `FileHeader::record_count` recording the per-game semantics above and stating that it must not be used as a completeness gate.

## Completeness Checks
- [ ] **SIBLING**: Any existing consumer of `record_count` beyond the `log::info!` audited before the doc lands
- [ ] **TESTS**: n/a (informational)
