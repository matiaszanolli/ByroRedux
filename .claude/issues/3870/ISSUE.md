# #3870: TD3-2026-09-05-06: `CLAUDE.md`'s Quick Reference says `cargo test -p byroredux-core` runs "162 tests"; the crate carries 746 `#[test]` functions

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-06) via `/audit-publish`, 2026-09-05. Labels: `low,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3870 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-06), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `CLAUDE.md:10`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `627b794a` (**2026-04-09**, *"docs: session 6 — N26 closeout"*). **~5 months stale**, never touched since.
- **Description**: The Quick Reference block reads:
  ```
  cargo test -p byroredux-core    # Run ECS/core tests (162 tests)
  ```
  `crates/core` (crate name confirmed `byroredux-core` in its `Cargo.toml`) now holds **746** `#[test]` functions — a 4.6× undercount. `ROADMAP.md`'s own last-verified line puts the whole workspace at 7 185 tests as of 2026-09-03, so the figure is stale against the project's own maintained counter too.
- **Evidence**:
  ```
  $ grep -rn "#\[test\]" --include='*.rs' crates/core | wc -l
  746
  $ grep -n "^name" crates/core/Cargo.toml
  2:name = "byroredux-core"
  $ git log --oneline -S"162 tests" -- CLAUDE.md | tail -1
  627b794a  (2026-04-09)
  ```
- **Impact**: `CLAUDE.md` is loaded at the start of every session by this project's own tooling — the single most-read document in the repo — and its per-command annotations are the first sizing signal an agent or contributor gets. A 4.6× wrong count invites "did I break something?" on a normal run and gives a false baseline to anyone estimating core-crate coverage. This is the identical defect class as TD3-NEW-01 in `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (CLAUDE.md's `Vertex` stated 100 B when a test had pinned 104 B) — that one was found and fixed; the neighbouring line eight rows above it was not checked.
- **Related**: TD3-NEW-01 (`AUDIT_TECH_DEBT_2026-07-25.md` — CLAUDE.md `Vertex` 100 B → 104 B), `ROADMAP.md:15` (the maintained workspace test counter).
- **Suggested Fix**: Either update to the live figure, or — preferably, since it will rot again within a session — drop the parenthetical entirely and let `ROADMAP.md`'s session-close-refreshed counter be the single source of truth for test totals, matching CLAUDE.md's own stated policy: *"Authoritative sources — do not duplicate state into this file."* The `162 tests` annotation is exactly the duplicated state that policy forbids.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
