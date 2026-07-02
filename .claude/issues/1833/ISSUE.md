# RT-3: FNV runtime baseline skin_pool_max is stale (1365 vs live 1364)

**Issue**: #1833
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-07-02.md`
**Labels**: low, tech-debt, bug

**Severity**: LOW
**Dimension**: audit baseline hygiene
**Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv:11`

**Description**: The FNV baseline records `skin_pool_max 1365`; the current run reports 1364, and **all four other baselines** (fo3/oblivion/skyrim_se/fo4) already record 1364. `skin_pool_max` is an "exact match" metric per the `/audit-runtime` SKILL, so the -1 nominally trips the gate, but it is a stale one-off baseline value, not a code regression (the pool cap is uniform 1364 across every game in this run).

**Evidence**: current run reports `skin=686/1364+0`; the four sibling baselines (fo3, oblivion, skyrim_se, fo4) are all pinned at 1364; only `fnv-FreesideAtomicWrangler.tsv` line 11 still says 1365.

**Impact**: Cosmetic — a future exact-match gate on FNV would false-positive on this single stale unit, producing a spurious FAIL unrelated to any code change.

**Related**: `docs/audits/AUDIT_RUNTIME_2026-07-02.md` (finding RT-3).

**Suggested Fix**: `--regen` the FNV baseline (or hand-edit `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` line 11 from `1365` to `1364`) to align it with the other four baselines. No code change required.

## Completeness Checks
- [ ] **TESTS**: N/A — baseline data file only, no code path to regression-test
