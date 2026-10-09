# #5417: CONC-D3-2026-10-08-04: At HEAD the ABBA lane and the main test job are red on a documentation-count test

**Labels**: low,tech-debt,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5417

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D3-2026-10-08-04` (HEAD `00f580e09`)

- **Severity**: LOW (trivial to fix and HEAD-only; graded under the baseline's CONC-D3-2026-10-05-01 reasoning — a red lane masks the next real cycle — but that one persisted five days, this one is one commit old).
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `crates/debug-server/src/registration.rs:633-651` (`debug_cli_component_counts_match_the_registry`); `docs/engine/debug-cli.md:186` and `:1256` (both still say "67 components"); the registrations `00f580e09` added to `registration.rs`.
- **Status**: NEW (the guard is #4756's; this is fresh drift). Likely also reported by the tooling audit.
- **Description**: `00f580e09` registered `EatBehavior`, `SleepBehavior` and `EatSleepState` (registry count 67 → 70) without touching `debug-cli.md`. CI run 37848932617: the "ABBA lock-order detector" job (workspace-wide `--no-fail-fast`) and "Test + Check + Clippy" both fail on that one test; no `lock-order cycle` panic appears in the ABBA log. Reproduced locally (`cargo test -p byroredux-debug-server --lib -- roster`).
- **Impact**: Until fixed, a real cycle in the next commit would be hidden behind an already-red lane.
- **Suggested Fix**: Update the two counts in `docs/engine/debug-cli.md` to 70.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both counts in `docs/engine/debug-cli.md` (`:186` and `:1256`))
