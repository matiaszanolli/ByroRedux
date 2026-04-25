# FO4-DIM2-04: Stale cubemap comment — '0x0800 = cubemap?' (actual bit is 0x1)

**Severity:** LOW | bsa, documentation
**Source:** `docs/audits/AUDIT_FO4_2026-04-23.md` Dim 2
**Location:** `crates/bsa/src/ba2.rs:345`

## Problem
Comment says `// base[22..24] flags (0x0800 = cubemap?)`. Code at line 354 correctly uses `flags & 0x1`. Trap for next contributor.

## Fix
Delete the misleading `0x0800` comment.
