# NIF-D3-2026-09-21-02: per_block_baseline_skyrim_se and _fallout_76 are red on the current installs from game-data drift, not parser loss

**Issue**: #4628
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 3 (Tooling / CI gates)
**Location**: `crates/nif/tests/data/per_block_baselines/{skyrim_se,fallout_76}.tsv`; `crates/nif/tests/common/mod.rs:999-1033`; `parse_real_nifs.rs` FO76 archive list
**Status**: NEW
**Game Affected**: Skyrim SE, Fallout 76

## Description
`per_block_baseline_skyrim_se` and `per_block_baseline_fallout_76` are red on the current local installs, but this is game-data drift on the audited machine, not a parser regression.
- **Skyrim SE**: baseline TSV (`crates/nif/tests/data/per_block_baselines/skyrim_se.tsv`, dated 2026-08-30, confirmed unchanged in the working tree) records `BSDynamicTriShape 21140` / `BSTriShape 71303`. A parser-independent header recount on the current archives shows an exact 1:1 swap: `BSDynamicTriShape` 21140 → 21054 (−86), `BSTriShape` 71303 → 71389 (+86); the corpus total is identical at 856,103. `Skyrim Special Edition - Meshes.bsa` was rewritten 2026-09-02, after the baseline was captured.
- **FO76**: three `NiPSys*Ctlr` types shrank in count. `SeventySix - Meshes.ba2` was rewritten 2026-09-20 (NIF count 58,469 → 63,305 — a large enough jump to explain per-type count drift without any parse loss).
- The comparator ignores the TSV's own `total=` header line, so an install-driven redistribution reads identically to a real filter-or-dispatch loss — there's no signal in the diff output to distinguish the two.
- Separately, the FO76 all-meshes list in `parse_real_nifs.rs` names 16 `NNUpdateMain.ba2` archives the patched install no longer has, and misses `Startup.ba2` (12 NIFs) — a second, independent source of the same "install moved, fixture didn't" symptom.

## Evidence
Baseline TSV file timestamps (2026-08-30 for `skyrim_se.tsv`/`fallout_76.tsv`) predate the archive rewrites (2026-09-02 Skyrim SE, 2026-09-20 FO76). Header-only recount reproduces the swap/shrink exactly, with corpus totals matching the game-file counts, not the parser's block-type distribution.

## Impact
Two of the harness's own gates are false-red, and (per NIF-D3-2026-09-21-01, the nightly lane never running) nobody is notified either way. Wastes triage time on future runs until regenerated, and — until the comparator names corpus-vs-parser drift explicitly — a real per-type block loss on these two titles would be indistinguishable from this noise.

## Related
NIF-D3-2026-09-21-01 (the nightly lane that would otherwise catch and explain this drift promptly)

## Suggested Fix
Regenerate `skyrim_se.tsv` and `fallout_76.tsv` against the current installs; update the FO76 all-meshes archive list to match the current install's archive set (drop `NNUpdateMain.ba2` entries no longer present, add `Startup.ba2`). Teach the comparator to check the TSV's `total=` header against the corpus's actual NIF count first, so a corpus-size change is reported distinctly from a same-size per-type redistribution.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D3-2026-09-21-02)

## Completeness Checks
- [ ] **TESTS**: Regenerated baselines are pinned and the comparator gains a corpus-size-drift check
