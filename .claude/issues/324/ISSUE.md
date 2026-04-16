# M2: Oblivion synthetic skip-table to prevent cascading parse failure

## Finding: Dim 5 cascading-failure (MEDIUM)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md`
**Dimension**: Coverage / Stream Position
**Games Affected**: Oblivion (v20.0.0.5 — no `block_sizes` table)
**Location**: `crates/nif/src/lib.rs` parse loop

## Description

For FO3+ NIFs (`block_sizes` present), the parse loop has a guardrail at `lib.rs:179-230` that snaps the cursor to `start_pos + block_size` on both Ok and Err branches — any per-block bug is contained.

Oblivion v20.0.0.5 has **no `block_sizes` table**. A single mis-read in any dispatched block poisons the rest of the file. Audit output `oblivion_stats.txt:20-21` shows a `NiFloatData` "unknown KeyType" aborting and discarding 7 subsequent blocks from a vanilla DLCHorseArmor NIF.

## Impact

The "100% dispatch coverage" Oblivion headline figure is deceptive. Every block-parser addition is a regression risk on Oblivion because any sub-field mis-read cascades. The real Oblivion risk is that *any* mis-read inside a dispatched block behaves like a coverage gap.

## Suggested Fix

Add a pre-scan pass over the raw NIF that builds a synthetic skip-table:
- Record offset of each block's type-name string (from header block_types table + per-block type_index).
- Build `block_start_offsets[N]` equivalent to the missing `block_sizes`.
- On parse error in v20.0.0.5, seek to `block_start_offsets[i+1]` instead of discarding remaining blocks.

Architectural companion to closed #104 / #224.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
