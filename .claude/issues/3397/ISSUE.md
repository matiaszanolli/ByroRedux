# Issue #3397 — SF-2026-08-27-D7-01: MeshesPatch parse-rate floor is 1.1 points stale — a full revert of #2105 would not trip the gate

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `low,bug,test-gap,nif-parser,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3397 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 7 (real-data validation).

- **Severity**: LOW
- **Location**: `crates/nif/tests/parse_real_nifs.rs:186-192` (docstring) and `:211-212` (the `min_clean` value)
- **Status**: NEW

## Description

The per-archive parse-rate floors carry a documented methodology — "measured minus ~0.5% margin, rounded down to the nearest 0.5%" — and a table refreshed 2026-07-11 under #1900:

```
///   MeshesPatch.ba2     ≥ 98.0% (29 849 NIFs; 98.91% actual; was 97.0%)
```

`98.91%` is the **pre-#2105** figure (29,849 − 325 truncated = 29,524 → 98.91%; see the `BSWeakReferenceNode` 2-byte gap fix `b7e0318f`, which took MeshesPatch 325 → 6 truncations).

Measured today: **99.98%**. The docstring's "actual" column is stale by 1.07 points, and the floor was never re-tightened after the fix it predates, so `min_clean: 0.980` now tolerates **597** truncated files where reality has 6.

## Evidence

Concretely: a change that fully reverted #2105 — restoring all 325 truncations, 98.91% — would leave `parse_rate_starfield_all_meshes` **green**. That is the exact regression this gate exists to catch, and it is the same shape as #2201, which the Meshes02 floor caught only because that archive's floor happened to sit at 99.5%.

Per the file's own stated rule the value should be **0.995**.

Measured this audit (`cargo test --release -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored`):

| Archive | NIFs | Clean | Truncated |
|---|---|---|---|
| Meshes01 | 31,058 | 100.00% | 0 |
| Meshes02 | 7,552 | 100.00% | 0 |
| **MeshesPatch** | 29,849 | **99.98%** | **6** |
| LODMeshes | 19,535 | 100.00% | 0 |
| FaceMeshes | 1,282 | 100.00% | 0 |

### Skepticism check — reasons this might not be a finding, and why they don't hold

- *Maybe MeshesPatch is genuinely noisy across patch levels* — the prior audit measured 99.98% on 2026-08-24 and this pass measured the same 6 files three days later. Stable, not noisy.
- *Maybe the loose floor is deliberate slack for future game patches* — the docstring explicitly states a uniform "measured minus ~0.5%" rule and applies 99.5% to the four archives sitting at 100.00%, so MeshesPatch is the outlier, not the policy.
- *Maybe another test covers the tail* — `per_block_baselines` covers Meshes01 only, and `block_coverage_baselines` gates `NiUnknown`, not truncation.

## Impact

Test-gate only, no runtime defect. The effect is a silent 1.1-point regression window on the one Starfield archive that still has a residual truncation tail — i.e. exactly the archive where a regression is most likely to appear and least likely to be noticed.

## Suggested Fix

Set `min_clean: 0.995` for `Starfield - MeshesPatch.ba2` and refresh the docstring's "actual" column to 99.98% (6 truncated), noting the #2105 fix that moved it.

## Related

#2105 (the `BSWeakReferenceNode` fix whose gain is unguarded), #2201 (the same class of silent regression, caught only by a tighter floor), #1900 (the last floor refresh). Adjacent but distinct: #3369 (`SKY-2026-08-27-D6-03`) covers the Skyrim gate omitting archives entirely, rather than a stale floor value.

## Completeness Checks
- [ ] **SIBLING**: re-check the other four Starfield floors and the other games' tables against currently-measured rates for the same staleness
- [ ] **TESTS**: confirm the tightened floor actually fails on a simulated #2105 revert
