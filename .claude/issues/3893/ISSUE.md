# #3893: TD9-2026-09-05-03: Dim 9's own discovery recipe — and the Phase-1 baseline snapshot — are structurally blind to `tools/`

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3893 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `tech-debt`
- **Location**: `/mnt/data/src/gamebyro-redux/.claude/commands/audit-tech-debt/SKILL.md` (the Dimension 9 **Discovery** block, and the Phase-1 *Snapshot totals* block)
- **Status**: NEW (fourth in the #2262 → #3440 → #3456 recipe-accuracy family)
- **Description**: Both recipes scope to `crates byroredux`. The workspace also has `tools/`, which is where `byro-dbg`, `byro-detect`, `byro-launcher` and `texture-upscale` live. `tools/byro-launcher/src/preflight.rs` carries `#[ignore = "needs a Vulkan driver"]`, so the published `#[ignore]` figure is **181 where the tree-wide figure is 182**. The same blind spot applies to every other Phase-1 metric computed with that path pair: TODO/FIXME markers, `allow(dead_code)`, `unimplemented!/todo!()`, and the >2000-LOC file scans.

  The 1-site undercount is immaterial today. The *structural* blindness is not: `tools/byro-launcher` and `tools/byro-detect` landed on 2026-08-30 (~5.8k LOC across them and their two backing crates), `_audit-common.md` lists the launcher stack as one of eight **un-owned subsystems** with no owner audit skill, and it is "the only path a non-developer reaches the engine through". A recipe that cannot see it means no audit dimension will ever report debt there by default.
- **Evidence**:
  ```
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux | wc -l   → 181
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools | wc -l → 182
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' tools
    → tools/byro-launcher/src/preflight.rs:268:    #[ignore = "needs a Vulkan driver"]
  ```
  `_audit-common.md` "Un-owned subsystems" table, row *Launcher (boot/settings/detect)*: `tools/byro-launcher/`, `tools/byro-detect/`, "No owner".
- **Impact**: Audit-infrastructure accuracy. Small today (0.55 % undercount); the cost is that `tools/` debt is invisible by construction, in a tree where four of the ten workspace binaries now live there. The three prior findings in this family each shipped a wrong number into a published audit report before being caught.
- **Related**: #2262, #3440, #3456, #3749 (all recipe/baseline-accuracy findings on this same grep); `_audit-common.md` un-owned-subsystems table.
- **Suggested Fix**: Change `crates byroredux` → `crates byroredux tools` in the Dimension 9 discovery block and in all six Phase-1 snapshot lines, and note the new tree-wide baseline (182) beside the historical one so the next audit's diff is not read as a regression. `tools/nifskope/` is vendored and not a workspace member — exclude it explicitly, as `_audit-common.md` already instructs.
- **Effort**: **Trivial** (two blocks in one skill file, plus one `_audit-validate.sh` run).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
