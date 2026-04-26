# Issue #692: O3-N-04: CELL parser drops XOWN / XRNK / XGLB ownership tuple — global gameplay gap

**Severity**: MEDIUM
**File**: `crates/plugin/src/esm/cell.rs:743-922` (interior CELL sub-record loop), `cell.rs:1457-1493` (exterior CELL), `cell.rs:1020-1175` (REFR sub-record loop)
**Dimension**: ESM (TES4) — cross-game

The interior CELL match arm enumerates `EDID / DATA / XCLW / XCIM / XCWT / XCAS / XCMO / XLCN / XCLR / XCLL`, then `_ => {}`. Same pattern at the exterior walker and at REFR.

**Missing entirely**:
- **XOWN** — cell ownership FormID (references NPC_/FACT)
- **XRNK** — faction-rank gate, i32
- **XGLB** — global var FormID controlling ownership

These live on both CELL and REFR.

**Impact**:
- Stealing/property crime detection unwirable.
- No rendering impact.
- Blocks the eventual ECS gameplay layer that #519 (AVIF) and #443 (SCPT) feed into.

Cross-game (FO3/FNV/Skyrim use these too).

**Fix**: Add `b"XOWN" / b"XRNK" / b"XGLB"` arms producing an `Ownership { owner_form_id, faction_rank, global_var_form_id }` field on `CellData` and `PlacedRef`.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
