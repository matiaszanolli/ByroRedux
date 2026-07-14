**Source:** FNV compatibility audit — Dimension 4 (ESM Record Parser), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** LOW (documentation) · **Status when filed:** NEW, CONFIRMED

## Description
`ROADMAP.md:76` states "73 054 structured records … plus a 5 625-record long-tail bucket" for FNV. The live `parse_rate_fnv_esm` integration run reports `[FNV] total=77828`. The two are roughly consistent (73 054 + 5 625 = 78 679 vs 77 828 — different bucketing, **not** a parse regression), but the headline 73 054 figure lags the current parser.

Separately: the `14 881` figure in the compat matrix (`ROADMAP.md:206`) is the **NIF mesh** parse rate over `Fallout - Meshes.bsa`, **not** an ESM record count — worth a clarifying note so the two are not conflated.

## Evidence
- `cargo test -p byroredux-plugin --test parse_real_esm -- --ignored` → all 7 tests pass, `[FNV] total=77828` (items=2643, NPCs=3816, quests=436, dialogues=18215, SCOL=98, …).
- No code impact — the floor-based real-ESM test is the effective source of truth; only the prose headline is stale.

## Impact
Documentation only. No runtime effect.

## Suggested Fix
Refresh the FNV structured-record total on the next `/session-close`, and optionally annotate `ROADMAP.md:206` that `14 881` is the NIF-mesh parse count, not an ESM record total.

## Completeness Checks
- [ ] **TESTS**: prefer citing the floor-based `parse_real_esm` test as the living source of truth rather than pinning a new hard count in prose
