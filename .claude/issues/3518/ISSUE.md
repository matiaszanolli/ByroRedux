# OBL-2026-08-27-03

Issue: #3518 — https://github.com/matiaszanolli/ByroRedux/issues/3518
Filed: 2026-08-27 by /audit-publish from docs/audits/AUDIT_OBLIVION_2026-08-27.md

Source: `docs/audits/AUDIT_OBLIVION_2026-08-27.md` — finding `OBL-2026-08-27-03`

- **Severity**: MEDIUM
- **Dimension**: 7 — Exterior Blocker Chain & Game-Specific Quirks
- **Location**: `byroredux/src/cell_loader/placement_lod.rs:119-122` (`parse_placement_lod`)

## Description

The Oblivion-only `DistantLOD\*.lod` reader takes its group count straight from the first four bytes of an archive file and hands it to `Vec::with_capacity` before any validation:

```rust
// placement_lod.rs:119-122
pub(crate) fn parse_placement_lod(bytes: &[u8]) -> io::Result<Vec<PlacementGroup>> {
    let num_groups = u32_at(bytes, 0)?;
    let mut off = 4usize;
    let mut groups = Vec::with_capacity(num_groups as usize);
```

`PlacementGroup` is `{ base_form_id: u32, placements: Vec<Placement> }` (>= 32 B with padding), so a header word of `0xFFFFFFFF` requests roughly 137 GB in one allocation. Rust's allocation-failure path is `handle_alloc_error` → **`abort`**, which is neither the `Err` this function's own doc comment promises nor an unwind the caller could contain:

```
/// Errors (rather than panics) on any out-of-bounds read, so a malformed /
/// degenerate file (e.g. `toddland`) is skipped by the caller rather than
/// crashing.
```

The per-group `Vec::with_capacity(count)` further down is **fine** — it sits after the `end > bytes.len()` bounds check. Only the outer count is unguarded.

## Evidence

The surrounding code has an explicit, documented doctrine for exactly this, and this is the site that doesn't follow it:

- `crates/bsa/src/archive/open.rs:50-56` — "Cap folder / file counts before the downstream `Vec::with_capacity` / `HashMap::with_capacity` allocations … catches the `u32::MAX` attack from a single corrupted header word. See #586" → `checked_entry_count`.
- `crates/bsa/src/ba2.rs:180-181` — same cap for BA2 `file_count`.
- `crates/nif/src/stream.rs:270-283` (`allocate_vec`) and `:321-323` (`allocate_vec_sized`, #2523) — bound the count against remaining bytes and `MAX_SINGLE_ALLOC_BYTES` before allocating.

Reachability: `spawn_placement_lod_cell` pulls the bytes with `tex_provider.extract_mesh(&lod_path)` from whatever BSA set is open, so any installed Oblivion mod archive containing a `distantlod\<world>_<x>_<y>.lod` entry reaches this parser during exterior streaming. The scheme is Oblivion-only (`placement_lod_supported`) — no other title has this exposure.

## Impact

A single corrupt or hostile 4-byte word in a mod-supplied `.lod` aborts the process during exterior streaming, bypassing the module's own documented "skip the file" recovery. Vanilla is unaffected (the reader is validated against all 9889 real files). MEDIUM: recoverable path with missing error handling, in an Oblivion-only module, on attacker/mod-controlled input.

## Related

- `#586` (the BSA/BA2 count caps this mirrors)
- `#2523` / PERF-D8-NEW-01 (`allocate_vec_sized`)
- Not covered by `#3150`.

## Suggested Fix

Bound `num_groups` by the file's own smallest legal encoding before allocating — each group costs at least 8 bytes (`base_form_id` + `count`), so `num_groups > (bytes.len() - 4) / 8` is provably corrupt and should return `Err`. One line, plus a synthetic `u32::MAX`-header unit test alongside the existing `parse_placement_lod` tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other untrusted-count allocation sites)
- [ ] **TESTS**: A regression test pins this specific fix
