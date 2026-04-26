# Issue #693: O3-N-05: CELL parser drops XCMT (pre-Skyrim music) and XCCM (Skyrim climate override per cell)

**Severity**: MEDIUM
**File**: `crates/plugin/src/esm/cell.rs:743-922`
**Dimension**: ESM (TES4) — cross-game

Interior cell music selection on Oblivion / FO3 / FNV uses **XCMT** (single-byte enum: 0=Default, 1=Public, 2=Dungeon, 3=None). The walker only handles XCMO (Skyrim+ FormID-style).

Skyrim+ exterior cells override worldspace climate via **XCCM** (FormID → CLMT) — also dropped. The TES4 XCMT byte and Skyrim XCCM both fall to `_ => {}`.

**Impact**:
- All Oblivion/FO3/FNV interior music types lost.
- Skyrim per-cell climate overrides lost.

**Fix**:
```rust
b"XCMT" if sub.data.len() >= 1 => music_type = Some(sub.data[0]),
b"XCCM" if sub.data.len() >= 4 => climate_override = Some(FormId::read(&sub.data[..4])?),
```

Both interior and exterior CELL loops need the additions.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
