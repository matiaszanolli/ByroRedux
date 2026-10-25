# TD3-2026-10-25-11: ROADMAP Known Issues cite closed issues as the tracker of still-unbuilt work

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6382
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `ROADMAP.md:677` ("takes effect on the next cell load only (#5065)... live consumer needs per-placement GPU/physics teardown"): #5065 was closed 2026-10-12 by `f579785e3`, a docs-only commit that made ROADMAP itself the tracker. `ROADMAP.md:~668`-area text names #4213 (NiStencilProperty "waits for a consumer", closed 2026-09-15) and "emissive is parked (#5503)" (closed 2026-10-11). 16 of 18 issue numbers cited in Known Issues are closed; most are history (fine), but these three are the *only* record of open work.
- **Fix**: Open one forward issue per item (live Enable()/Disable() consumer; NiStencilProperty consumer; Starfield emissive) and cite those, or mark the ROADMAP lines "tracked here only".


#### Dimension 4 — Audit-Finding Rot (5)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-11`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
