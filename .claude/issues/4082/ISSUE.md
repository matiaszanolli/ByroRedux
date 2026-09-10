# #4082 — ESM-2026-09-09-D7-07

`resolve_winner` breaks an equal-depth tie by candidate slice order and still labels it `DepthResolved`

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4082 --json state`).

---

- **Severity**: LOW
- **Dimension**: ESM→ECS Handoff (Redux-native tier)
- **Record / Sub-record**: —
- **Location**: `crates/plugin/src/resolver.rs:73-101`; doc
  `docs/engine/plugin-loading.md:176-184`
- **Status**: NEW — re-verified against **code** at HEAD (source
  `AUDIT_ESM_2026-08-13.md` ESM-D7-04, pre-2026-06-07); no issue filed.
  *Scope note: item 5 of this dimension's checklist scopes the Redux-native tier to
  rot only. This is a logic defect, not rot; included for continuity because it was
  found once, never filed, and is unchanged — flag it as scope-adjacent when triaging.*
- **Description**: `best` is seeded with the first candidate and replaced only on
  strictly greater overlap, so among candidates tied at the maximum overlap the one
  earliest in the `plugins` slice wins. That slice is plugin registration order —
  precisely the load-order dependence this tier exists to remove. The
  `ConflictResolution::TieBreak { winner }` arm (`min(PluginId)`, order-independent)
  fires only when the maximum overlap across the whole set is `0`, so a genuine
  tie at depth 1 is reported as `DepthResolved` and never surfaces for review.
- **Evidence**:
  ```rust
  // crates/plugin/src/resolver.rs:82-100
              if let Some((_, best_overlap)) = best {
                  if overlap > best_overlap {
                      best = Some((candidate, overlap));
                  }
              } else {
                  best = Some((candidate, overlap));
              }
          }

          let (winner, overlap) = best.unwrap();

          if overlap > 0 {
              // Winner depends on at least one other plugin in the set —
              // this is an intentional override.
              (winner, ConflictResolution::DepthResolved { winner })
          } else {
              // No dependency relationship — deterministic tiebreak.
              let winner = *plugins.iter().min().unwrap();
              (winner, ConflictResolution::TieBreak { winner })
          }
  ```
- **Impact**: none today (no live caller). Latent: two mods that both depend on the
  same base master and both edit one record are a diamond with overlap 1 each; the
  winner flips with registration order and the `Conflict` record claims the DAG
  decided it. The doc's algorithm summary (`plugin-loading.md:180-184`) describes the
  same behaviour without noting the tie case, so it does not flag the gap either.
- **Related**: `AUDIT_ESM_2026-08-13.md` ESM-D7-04.
- **Suggested Fix**: collect all candidates at the maximum overlap; if more than one,
  break by `min(PluginId)` and report `TieBreak` (or a new `AmbiguousDepth` variant)
  rather than `DepthResolved`, so the ambiguity is visible in `conflicts`.

---
