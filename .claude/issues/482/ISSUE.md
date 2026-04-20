# Issue #482

FNV-2-L2: FACT XNAM combat_reaction reads u8 from 4-byte u32 field

---

## Severity: Low (width mismatch)

**Location**: `crates/plugin/src/esm/records/actor.rs:288`

## Problem

```rust
let combat = if sub.data.len() >= 12 { sub.data[8] } else { 0 };
```

Per UESP, FNV XNAM is `u32 other_faction + i32 modifier + u32 combat_reaction` = 12 bytes. The code reads the low byte of the `combat_reaction` u32.

Current values (0–3) fit in one byte, so this is **behaviorally correct**. The mismatch is a type-width bug, not a functional one.

## Impact

None today. Future mod extensions of combat_reaction values >255 would silently truncate.

## Fix

```rust
let combat = if sub.data.len() >= 12 {
    read_u32_at(&sub.data, 8).unwrap_or(0) as u8
} else {
    0
};
```

Read width matches declared field width.

## Completeness Checks

- [ ] **TESTS**: Synthetic XNAM with `combat_reaction = 0x00000002`, assert stored value == 2
- [ ] **SIBLING**: Audit other `sub.data[N]` reads for similar width mismatches

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-2-L2)
