# #3996 — REN-2026-09-06-D1-02: `audit-renderer/SKILL.md`'s Dimension-1 checklist carries three stale claims, and one of them has now manufactured a false finding in two consecutive sweeps — the most recent explicitly recommended for GitHub filing

**Labels**: low, renderer, tech-debt, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (audit-tooling doc rot)
- **Location**: `.claude/commands/audit-renderer/SKILL.md`, the Dimension-1
  "LRU/shrink wiring" bullet and the "Transform" bullet. Ground truth:
  `crates/renderer/src/vulkan/context/mod.rs` (`fill_rt_integrity_stats`),
  `byroredux/src/app_events.rs`, `byroredux/src/commands/world_info.rs`
  (`RtIntegrityCommand`), `byroredux/src/commands/mod.rs`,
  `crates/core/src/ecs/resources/mod.rs` (`RtIntegrityStats::verdict`,
  `RtIntegrityStats::machine_line`),
  `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`),
  `crates/renderer/src/vulkan/acceleration/memory.rs`
  (`shrink_tlas_scratch_to_fit`)
- **Status**: NEW (same class as `REN-2026-08-30-D1-02`, a different set of
  claims in the same paragraph; not in the OPEN cache)
- **Description**: Three independent inaccuracies in the Dimension-1 checklist
  the auditor is told to treat as the task list:

  1. **"the three `missing_blas` cause-counters … surface only through the
     rate-limited (once/sec) `log::warn!` … there is NO *mem.stats* command,
     and no registered command reads them (#1228 / REN-LOW L-1)."** The
     *mem.stats* half is still true. The "no registered command reads them" half
     has been false since `9c805cd7` (**2026-08-14**), which added the entire
     chain: `AccelerationManager::integrity_snapshot()` →
     `VulkanContext::fill_rt_integrity_stats` → the `RtIntegrityStats` ECS
     resource, refreshed **every frame** from `byroredux/src/app_events.rs` →
     the registered `rt.integrity` console command. `RtIntegrityStats::verdict`
     implements exactly the positive assertion `TlasIntegritySnapshot`'s own
     docstring promised: `PASS` requires `tlas_emitted == tlas_eligible` **and**
     `missing_skinned_blas == missing_rigid_blas == missing_ssbo_instance == 0`.
  2. **"`TRIANGLE_FACING_CULL_DISABLE` on all instances (two-sided meshes)."**
     The code deliberately does the opposite: `build_tlas_instances` sets the
     flag only when `draw_cmd.two_sided`, because pre-#416 disabling backface
     culling on every instance made shadow/GI rays hit the interior backfaces of
     closed single-sided architecture from outside (~2× ray cost on closed
     meshes). **Here the code is right and the SKILL is wrong**; an auditor
     following the checklist literally would file the correct behaviour as a bug.
  3. **"`shrink_tlas_scratch_to_fit` uses TLAS-calibrated slack matching
     `tlas_instance_should_shrink`."** It calls `tlas_scratch_should_shrink`
     (`TLAS_SCRATCH_SLACK_BYTES` = 256 KB). `tlas_instance_should_shrink`
     (`TLAS_REBUILD_SLACK_BYTES` = 1 MB) is the *instance-buffer* predicate used
     by `shrink_tlas_to_fit`. Both are TLAS-calibrated, so the spirit is right
     and the named symbol is wrong — but a symbol reference in a No-Guessing
     checklist is the one place that distinction has to hold.

  Minor, same paragraph: the `blas_static.rs:228-238` line anchor for the #1793
  `--grid` note has rotted off the text it points at (the note is intact; only
  the range moved). The SKILL's own discipline section says to anchor on
  symbols, not line numbers.
- **Evidence**:
  - `grep -rn "integrity_snapshot\|tlas_integrity\|TlasIntegritySnapshot" --include='*.rs' crates byroredux tools`
    → 7 hits, including `crates/renderer/src/vulkan/context/mod.rs`:
    `.map(super::acceleration::AccelerationManager::integrity_snapshot)`.
  - `git log -S "fill_rt_integrity_stats"` and
    `git log -S "RtIntegrityCommand"` both →
    `9c805cd7`, `2026-08-14 23:26:10 -0300`.
  - `grep -rn "RtIntegrityCommand" byroredux/src/` → registered in
    `byroredux/src/commands/mod.rs`, implemented in
    `byroredux/src/commands/world_info.rs`, exercised in
    `byroredux/src/commands_tests.rs`.
  - **The consequence, twice**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`
    `REN-2026-08-30-D1-01` claims *"`integrity_snapshot()` is a `pub` accessor
    with **zero call sites**"*; `docs/audits/AUDIT_RENDERER_2026-09-05.md`
    `REN-2026-09-05-D1-02` re-verified it and concluded *"Re-verified the premise
    still holds at HEAD … **Recommended as the one finding from this run that
    should actually be filed** — it has now survived two sweeps without an issue
    number."* Both are false, and both were false on the day they were written
    (the consumer predates the 08-30 sweep by 16 days). The mechanism is
    visible in the 09-05 report's own evidence line: it greps
    `tlas_integrity\|TlasIntegritySnapshot` — the *struct* name — which cannot
    match a call to the *method* `integrity_snapshot`. The stale SKILL sentence
    is what told both auditors the answer before they grepped.
- **Impact**: The `/audit-publish` step would have created a GitHub issue
  asserting a telemetry gap that was closed three weeks earlier, against code
  whose maintainer would then have to disprove it. This is the failure mode
  `_audit-common.md`'s "Verify the premise before writing a finding" rule and
  the user's `feedback_audit_findings` memory exist to prevent, and it is now
  measured at 2-for-2 on this one sentence. Claim 2 is the more dangerous of the
  three going forward: it points an auditor at correct code and tells them it is
  wrong.
- **Related**: `REN-2026-08-30-D1-02` (the same file, the previous pair of stale
  Dimension-1 claims), `#1228` (the original telemetry gap, now closed by
  `9c805cd7`), `#416` (the two-sided cull gate), `#1226` (the two TLAS shrink
  predicates), `_audit-common.md` §"Never write an instruction to not look".
- **Suggested Fix**: In `SKILL.md`'s Dimension-1 bullet: (a) replace the "no
  registered command reads them" clause with a regression guard — *"verify the
  `integrity_snapshot()` → `fill_rt_integrity_stats` → `RtIntegrityStats` →
  `rt.integrity` chain still runs per-frame and that `verdict()` still requires
  `emitted == eligible` with all three counters zero"* — and keep only the
  (still-true) "*mem.stats* does not exist" half; (b) change "on all instances"
  to "gated on `draw_cmd.two_sided` (#416) — a blanket enable is the
  regression"; (c) swap `tlas_instance_should_shrink` for
  `tlas_scratch_should_shrink` in the `shrink_tlas_scratch_to_fit` clause;
  (d) drop the `blas_static.rs:228-238` line range. Additionally, add a
  corrective note to `docs/audits/AUDIT_RENDERER_2026-09-05.md` and
  `docs/audits/AUDIT_RENDERER_2026-08-30.md` so the two false findings are not
  picked up by a future `/audit-publish` run over the backlog.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
