# Issue #481

FNV-2-L1: FACT DATA reads u32 flags from 1-byte FNV field — high-byte garbage

---

## Severity: Low

**Location**: `crates/plugin/src/esm/records/actor.rs:280-282`

## Problem

```rust
b"DATA" if sub.data.len() >= 4 => {
    record.flags = read_u32_at(&sub.data, 0).unwrap_or(0);
}
```

UESP reports FNV FACT DATA as `u8 flags` (1 byte) only — some vanilla records ship variable tails. Reading 4 bytes picks up padding / neighbor bytes into the high 24 bits.

Observable: `NCRFactionNV` reports `flags = 0x00000100` — bit 8 is not a legal FNV faction flag. Authoritative bits are 0 (hidden from PC) / 1 (evil) / 2 (special combat).

## Impact

- Only one vanilla FNV faction has bits above bit-2 set; statistically harmless today.
- Future mod content that extends DATA can flip wrong semantics.
- Cross-game: Skyrim / FO4 extend DATA — needs per-game handling.

## Fix

Match `>= 1` (not `>= 4`) and read `sub.data[0] as u32`. If a future game version extends DATA, add per-`GameKind` arm.

## Completeness Checks

- [ ] **TESTS**: Parse FNV.esm, assert `NCRFactionNV.flags <= 0x07`
- [ ] **SIBLING**: Check other short sub-records read with `>= 4` when actual width is smaller (grep for `read_u32_at.*sub.data, 0`)
- [ ] **DOCS**: UESP reference at the parser

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-2-L1)
