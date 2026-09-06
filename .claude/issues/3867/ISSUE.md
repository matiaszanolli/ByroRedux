# #3867: TD3-2026-09-05-03: `docs/feature-matrix.md` says the CTDA evaluator covers "13 functions" and `npc-spawn-ai-packages.md` says "~15"; the live `ConditionFunction::CATALOG` holds 19

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,esm-plugin,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3867 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:172` · `docs/engine/npc-spawn-ai-packages.md:134`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: the "13" was itself the fix for #1818 (*"correct CTDA condition function count in feature-matrix.md (7 → 13)"*, commit `1d3190fb`). The catalog has since grown in at least three commits (`e9aece79` → `6df3bad8` → `583a349a`) with no matrix update.
- **Description**: The Scripting (M47) table row reads
  `| CTDA condition evaluation with OR-precedence (M47.1) | ✓ **13 functions** |`.
  The `npc-spawn-ai-packages.md` fail-open paragraph reads *"the M47.1 catalog covers **~15** of Bethesda's ~300 condition functions"*. The live catalog is 19.
- **Evidence**: `crates/scripting/src/condition.rs`:
  ```rust
  pub const CATALOG: [ConditionFunction; 19] = [
      Self::GetDistance, Self::GetActorValue, Self::GetDead, Self::GetStage,
      Self::GetStageDone, Self::GetInCell, Self::GetIsClass, Self::GetIsRace,
      Self::GetIsID, Self::GetFactionRank, Self::GetLevel, Self::GetEquipped,
      Self::HasPerk, Self::GetXPForNextLevel, Self::IsSceneActionComplete,
      Self::HasLoaded3D, Self::GetReputation, Self::GetReputationThreshold,
      Self::GetVMScriptVariable,
  ];
  ```
  The `ConditionFunction` enum has the matching 19 variants. The six additions past 13 —
  `HasPerk`, `GetXPForNextLevel`, `IsSceneActionComplete`, `HasLoaded3D`,
  `GetReputation`/`GetReputationThreshold`/`GetVMScriptVariable` — span the CHARAL,
  SCEN-playback and two-state-activator work.
- **Impact**: `docs/feature-matrix.md` is named in `_audit-common.md`'s Key Reference Docs table as the authority for *"what works at runtime per game"*, and the shared protocol tells auditors to *prefer these docs over re-deriving facts from source*. An auditor sizing the M47.1 gap (or a `/audit-scripting` dimension counting catalog coverage) reads 13 and under-counts by 32%. `docs/audits/AUDIT_SCRIPTING_2026-08-03.md` already shows the drift propagating — it says *"matching the 13 previously-verified catalog functions"* while adding six more in the same sentence. This is the same recurrence pattern as #2417/#2416/#2309/#2253/#2192/#2047, all CLOSED feature-matrix drift.
- **Related**: #1818 (the prior correction of this exact cell, 7→13), #2975, #2417, #2416.
- **Suggested Fix**: `✓ 13 functions` → `✓ 19 functions`; `~15 of Bethesda's ~300` → `19 of Bethesda's ~300`. Better: make the count a one-line assertion — `ConditionFunction::CATALOG.len()` is already a `const`, so a test asserting the documented figure would stop the third recurrence.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
