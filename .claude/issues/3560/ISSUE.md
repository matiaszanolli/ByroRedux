# RT-14: the runtime audit harness silently mis-attributes telemetry between games — `kill -INT` on the `xvfb-run` wrapper leaves the engine alive on port 9876

**Issue**: #3560
**Labels**: bug, medium, tech-debt
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-14. **Reproduced live during this sweep.**

## Description

The `/audit-runtime` skill's documented teardown (`kill -INT $PID` on the backgrounded `xvfb-run ...` job) kills the **wrapper**, not the engine. `xvfb-run` execs the binary as a child, so the engine keeps running and keeps holding port 9876.

The FNV run that followed Oblivion therefore connected to the **still-live Oblivion engine** and captured Oblivion's numbers — `Entities: 718`, Oblivion's exact 8-path `tex.missing` list — under the FNV filename. The only tell was `dbg up at 1s`, impossible for a cell that takes ~40 s to load.

## Impact

This is exactly the RT-1/#1619 mis-attribution the skill warns about, but reached through **teardown failure rather than parallelism** — so running serially, as the skill instructs, does **not** prevent it. Any past `--game all` sweep using the documented teardown may carry silently shifted telemetry, including baselines regenerated from such a sweep.

## Fix applied for this audit (recommend folding into the skill)

1. Pre-flight assert `pgrep -x byroredux` is empty and port 9876 is unbound. **Note**: `pgrep -f 'target/release/byroredux'` self-matches the harness shell — use `pgrep -x`.
2. Resolve the real engine PID with `pgrep -x byroredux` **after launch**, and sweep any survivor after teardown.
3. **Cross-check** `Entities:` from the `byro-dbg` `stats` line against `entities=` on the `bench:` line, and hard-fail on divergence.

All five captured runs in the 2026-08-30 report pass that cross-check; the one run that failed it (the first FNV attempt) was discarded and re-run, not reported.

## Suggested Fix

Fold the three steps above into `.claude/commands/audit-runtime/SKILL.md` as mandatory pre-flight / post-flight / per-capture assertions, replacing the current `kill -INT $PID` teardown text.

## Completeness Checks
- [ ] **SIBLING**: Every other smoke test / harness that backgrounds `xvfb-run` and later signals `$!` audited for the same wrapper-vs-child gap (`docs/smoke-tests/*.sh`)
- [ ] **TESTS**: The cross-check (item 3) is what makes the failure *visible* rather than silent — it must be in the harness, not just the skill prose
