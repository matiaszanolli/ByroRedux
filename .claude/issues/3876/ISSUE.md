# #3876: TD5-2026-09-05-01: Dim 5's discovery recipe never looks at `tools/` — 4 first-party workspace crates, 4 706 LOC, invisible to four consecutive audits

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD5-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3876 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD5-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Dimension 5
  **Discovery** block (`grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux`)
- **Status**: NEW
- **Age**: the recipe's scope has been `crates byroredux` since the dimension
  was written; the gap *widened* on 2026-08-30 when `tools/byro-launcher` and
  `tools/byro-detect` landed (~2 weeks old at time of audit).
- **Effort**: trivial (≤30 min — one word in the grep + a `nifskope` exclusion)
- **Description**: The Dim 5 recipe greps `crates` and `byroredux`. It does not
  grep `tools/`, which holds **four first-party workspace members** —
  `tools/byro-dbg`, `tools/byro-launcher`, `tools/byro-detect`,
  `tools/texture-upscale` — 17 `.rs` files, 4 706 LOC. These are not vendored:
  all four are listed in the root `Cargo.toml` `[workspace] members` array.
  (`tools/nifskope` is correctly *not* a member and must stay excluded — it is
  vendored reference code, per `_audit-common.md`'s Tools row.)

  A live `// TODO` in the launcher's engine-supervision path or in
  `byro-detect`'s `libraryfolders.vdf` parser would be structurally invisible to
  this dimension, indefinitely.

  The second-order harm is a claim wider than its evidence: the 2026-08-30
  report states *"**Zero live TODO/FIXME/HACK markers in the entire codebase** —
  production and shaders"* and *"There is not one live marker in the codebase."*
  The grep behind that sentence covered `crates` + `byroredux` + shaders, not
  the whole codebase. The conclusion happens to be true — I verified `tools/` is
  marker-free today — but it was **lucky, not measured**, and the next auditor
  inherits an unqualified whole-codebase claim.
- **Evidence**:
  ```
  # root Cargo.toml [workspace] members — all four are first-party:
  "tools/byro-detect", "tools/byro-launcher", "tools/byro-dbg", "tools/texture-upscale"
  # tools/nifskope is absent from members → vendored, correctly out of scope

  $ find tools -name '*.rs' -not -path 'tools/nifskope/*' | wc -l   → 17
  $ find tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | tail -1
                                                                    → 4706 total
  $ grep -RInE '(TODO|FIXME|HACK|XXX)\b' tools --include='*.rs' --include='*.toml' \
      | grep -v nifskope                                            → (empty)
  ```
- **Impact**: No live debt is hidden today (verified). The blast radius is
  future-blindness over the exact code `_audit-common.md`'s un-owned-subsystems
  table calls *"the only path a non-developer reaches the engine through"* —
  the launcher/boot-request/settings-io/detect cluster, which has **no owner
  audit skill at all**. Dim 5 is one of the few generic sweeps that would reach
  it, and it doesn't.
- **Related**: #3456 (CLOSED — the identical recipe-blind-spot finding for
  Dim 9); #2974 (CLOSED — Dim 1's recipe proxy). `_audit-common.md`
  un-owned-subsystems table, "Launcher (boot/settings/detect)" row.
- **Suggested Fix**: Change the Dim 5 discovery command to
  `grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux tools | grep -v nifskope`
  and add a one-line note that `tools/nifskope` is vendored and deliberately
  excluded. Then re-word the "entire codebase" claim in the next report to name
  its actual scope.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
