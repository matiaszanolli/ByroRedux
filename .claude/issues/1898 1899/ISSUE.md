# #1898: NIF-D2-06 — Max Filepath BSStreamHeader gate uses raw magic literal 103

**Severity**: low
**Dimension**: NIF audit 2026-07-06 (nif-deep suite)
**Location**: `crates/nif/src/header.rs:158`

## Description
`if user_version_2 >= 103 { ... }` gates the BSStreamHeader `Max Filepath`
field. Every other `bsver` comparison in the crate routes through a named
constant in `version::bsver`; the two adjacent gates on the same field
already do (`bsver::FALLOUT4`). The value 103 is correct (nif.xml
`#BS_GTE_103#`); only the naming violates convention.

## Suggested Fix
Add `pub const MAX_FILEPATH: u32 = 103;` to `version::bsver` and reference
it in `header.rs`, matching the two adjacent gates.

---

# #1899: NIF-D3-01 — Oblivion per-block TSV baseline is stale-high on NiUnknown

**Severity**: low
**Dimension**: NIF audit 2026-07-06 (nif-deep suite; corrects prior AUDIT_NIF_2026-07-02 NIF-D3-01)
**Location**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv`

## Description
`oblivion.tsv` (last regenerated 2026-06-15) recorded `NiMaterialProperty`
and `NiTexturingProperty` with a residual `NiUnknown` count of 1 each. The
live parser emits 0 NiUnknown across the entire Oblivion mesh archive.
Because `compare_histograms` only fails on unknown-growth or parsed-
shrinkage, a dropped unknown count doesn't trip the gate and never self-
corrects. The #1840/#1841 pass regenerated the other five game TSVs but
left `oblivion.tsv` and `starfield.tsv` untouched.

## Suggested Fix
Regenerate `oblivion.tsv` via `BYROREDUX_REGEN_BASELINES=1 cargo test -p
byroredux-nif --test per_block_baselines -- --ignored`; verify the diff is
unknown-shrink only (no parsed shrinkage); commit. Same treatment for
`starfield.tsv`.
