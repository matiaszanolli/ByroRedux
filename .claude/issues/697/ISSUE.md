# Issue #697: O5-4: NiTransformData shows 20 partial-unknown blocks — regression signal R3 (upstream drift, not parser bug)

**Severity**: MEDIUM
**File**: `crates/nif/src/blocks/animation.rs` (NiTransformData parser) + upstream drift sources
**Dimension**: Real-Data Validation

Top regression signal in the 2026-04-25 sweep histogram: `NiTransformData` parses cleanly 3,487 times but lands on `NiUnknown` 20 times (e.g. `unknown KeyType: 0`, `unknown KeyType: 4286578687`).

**04-17 H-3 attribution**: upstream stream-position drift (the named block is a victim, not the perpetrator). The bogus `KeyType` u32 is being read from a position that the previous block under-consumed.

**Fix**:
1. Run `crates/nif/examples/recovery_trace.rs` on the 20 failing files.
2. Confirm the drift originates in a sibling block.
3. Cross-link with #687 (alloc-cap drift) — likely the same root cause.

Pairs with #687 + #688 — all three are downstream of the same upstream parser drift class.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
