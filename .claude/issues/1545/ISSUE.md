# Issue #1545: OBL-D2-DOC-01: BSA folder-record size doc comments omit v103

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 2 (BSA v103) — tech-debt / doc rot · **Status**: NEW

**Location**: `crates/bsa/src/archive/open.rs:92`, `crates/bsa/src/archive/open.rs:111`, `crates/bsa/src/archive/extract.rs:127`

## Description
Three comments say "v104" where the behavior (16-byte folder records / zlib) serves both v103 and v104. The live code is correct (`if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`, `open.rs:100`) and other comments in the same files already say "v103/v104" (`open.rs:4, :134`, `extract.rs:4`). Stale text only.

## Evidence
Static read; folder-record size constant verified at `open.rs:100`.

## Impact
None functional. Minor reader confusion / risk of perpetuating the long-dead "v104 = 24 B" misconception.

## Suggested Fix
Reword the three comments to "v103/v104".

## Completeness Checks
- [ ] **SIBLING**: Same "v104"-where-it-means-"v103/v104" wording checked across the rest of `open.rs`/`extract.rs`
- [ ] **TESTS**: N/A (comment-only change)
