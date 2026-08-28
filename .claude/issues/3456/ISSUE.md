# #3456 — TD9-2026-08-27-01: Dim 9 discovery recipe misses the #[ignore = "reason"] form — 29 tests today, a 19% undercount

Labels: `low,tech-debt,test-gap,bug`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 9 — Test Hygiene · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD9-2026-08-27-01)

**Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Phase-1 snapshot recipe (line 120) and Dimension 9's Discovery block (line 384), both `grep -RIn '^\s*#\[ignore\]' --include='*.rs' crates byroredux`

## Description
The pattern requires `#[ignore]` to close immediately after the attribute name, so Rust's documented reason form `#[ignore = "…"]` never matches. 29 such tests exist today and none of them appears in any tech-debt audit's Dim-9 count or triage.

## Evidence
```
$ grep -RIn -E '^[[:space:]]*#\[ignore\]'  --include='*.rs' crates byroredux | wc -l
126     # the SKILL recipe
$ grep -RIn -E '^[[:space:]]*#\[ignore'    --include='*.rs' crates byroredux | wc -l
155     # + the reason form
$ grep -RIn -E '^[[:space:]]*#\[ignore[[:space:]]*=' --include='*.rs' crates byroredux | wc -l
29
```

**Triaged — the substance is clean.** All 29 carry an explicit data/GPU gate and none guards a closed CRITICAL/HIGH fix:
```
crates/scripting/tests/pex_recognize_e2e.rs:37  #[ignore = "needs Skyrim SE game data on disk"]
byroredux/tests/skinning_e2e.rs:151             #[ignore = "requires FNV BSA — opt in with --ignored"]
byroredux/tests/cornell_rt_oracle.rs:26         #[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
byroredux/tests/golden_frames.rs:66             #[ignore = "requires Vulkan device + release build; opt-in via --ignored"]
... (25 more, same three classes)
```

## Impact
No live debt hidden today — but the reason form is the one an author reaches for when the reason is *"blocked on #NNNN"*, which is exactly the case Dim 9's triage rule exists to catch ("referenced issue still open? if it guards a closed CRITICAL/HIGH fix → MEDIUM", SKILL.md:388). The recipe is blind to its own highest-value input class. It also makes every published Dim-9 baseline systematically low (a 19% undercount today), compounding the separate unreproducible-baseline defect filed alongside this report.

## Related
#2262 (CLOSED — same recipe, the whole-repo-textual-scan false regression); the `AUDIT_TECH_DEBT_2026-08-24.md` 171-vs-121 baseline correction filed alongside this report. Three independent defects in one four-token grep argues for fixing it once, properly.

## Suggested Fix
Change both occurrences to `grep -RIn -E '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux` and record in the SKILL that the baseline steps from 126 to 155 at that change, so the next sweep does not read the correction as a regression.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other audit SKILL carrying an `#[ignore]` or attribute-shaped grep recipe)
- [ ] **TESTS**: A regression test pins this specific fix (or, at minimum, the SKILL records the 126 → 155 baseline step so the next diff reads it as the correction it is)
