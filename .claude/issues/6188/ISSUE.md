# TD7-2026-10-19-02: 13 ESM sub-record decoders pair a bare `len() >= N` literal with hand-indexed `from_le_bytes([sub.data[0], ...])` instead of `SubReader` / `core::bytes::le_*_at`

Labels: low,esm-plugin,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers (ESM sub-record sizes; also Dim 2 duplication, reported here)
- **Location**: `crates/plugin/src/esm/records/misc/challenge.rs:85,95-96` (new, `317320437` 2026-10-18), `misc/music.rs:35`, `weather.rs:1043,1053`, `misc/effects.rs:73`, `scol.rs:163`, `misc/quest.rs:547`, `misc/dialogue.rs:693,735`, `misc/world.rs:1839,1870`, `misc/water.rs:1402`
- **Effort**: trivial
- **Description**: the gate literal (`>= 4`, `== 4`, `>= 2`) and the index literals (`[0]..[3]`) must be edited together; `SubReader::u32()/u16()/f32()` and `byroredux_core::bytes::le_u32_at` (used all over `obscript_vm.rs`) make the length check part of the read. The same files use named sizes elsewhere (`CREATURE_DATA_LEN`, `GRAS_DATA_LEN`, `WIRE_SIZE`), so the convention exists; the 2026-10-18 `challenge`/`music` modules arrived with the hand-indexed form. The prior PROJ finding (#5846) is fixed (`effects.rs` no longer re-slices `SubReader::new(&sub.data[N..])`).
- **Suggested Fix**: replace each with `SubReader::new(&sub.data).u32()` / `le_u32_at(&sub.data, 0)` guarded by `if let Some`; keep a named `*_LEN`/`WIRE_SIZE` only for `DATA` structs that are checked as a whole (`challenge.rs` `DATA` 24).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
