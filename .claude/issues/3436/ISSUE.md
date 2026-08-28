# #3436 — TD3-2026-08-27-01: npc-spawn-ai-packages.md — a designated-authoritative cross-cutting trace — is written against a deleted API and cites a file that no longer exists, ten times

Labels: `medium,ai,tech-debt,doc-rot,documentation`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: MEDIUM · **Dimension**: 3 — Stale Documentation & Comments · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD3-2026-08-27-01)

**Location**: `docs/engine/npc-spawn-ai-packages.md` — lines 66, 105, 121, 147-151, 154-161, 169, 211-212, 250, 316-317, 348-349, 372, 391-392, 445, 466

## Description
`_audit-common.md`'s Key Reference Docs table names this file as "Cross-cutting trace #4 … NPC_ spawn → AI package selection → per-procedure runtime" and instructs every audit to *"prefer them over re-deriving facts from source"*. An auditor or contributor who follows that instruction today is handed three separate dead references:

1. **Ten backticked citations of `ai.rs`**, four with line numbers (`ai.rs:20`, `ai.rs:147,159`). No file named `ai.rs` exists anywhere in the tree; the content moved to `crates/plugin/src/esm/records/misc/pack.rs` (and siblings) under #2054.
2. **Eight symbols that no longer exist at all**: `active_sandbox_location`, `active_wander_location`, `active_travel_location`, `active_follow_target`, `active_escort_target`, `active_escort_location`, `active_guard_location`, `active_patrol_location`. All eight are already on the validate gate's `docs/engine` symbol advisory.
3. **Seven names that exist but no longer mean what the doc says**: `active_package_is_sandbox` … `active_package_is_patrol` are now `macro_rules!`-generated **`#[cfg(test)]`-only shims** (`crates/plugin/src/esm/records/misc/pack.rs`), not the production selectors the doc describes as gating behaviour inserts.

The doc's central mechanism paragraph is therefore false at every level: the described functions are gone, the described file is gone, and the described gating no longer happens the way it says.

## Evidence
Verified at publish time (2026-08-28):

```
$ git ls-files | grep -E "(^|/)ai\.rs$" ; echo "exit=$?"
exit=1
$ grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md
10
$ grep -rn "active_sandbox_location" crates byroredux --include='*.rs' | wc -l
0
$ grep -n "active_package_is_sandbox" crates/plugin/src/esm/records/misc/pack.rs | head -1
788:    active_package_is!(active_package_is_sandbox, is_sandbox);   # inside `#[cfg(test)] mod tests`
```

And the doc, unchanged:
```
docs/engine/npc-spawn-ai-packages.md:169
`active_package_is_sandbox`/`active_sandbox_location` (`ai.rs:147,159`)
feed `npc_spawn.rs`, which inserts `SandboxBehavior { search_radius }`
```

The `pack.rs` shims carry their own provenance comment: "#3042 — the seven production `active_package_is_*` wrappers were deleted as dead code (#2031 collapsed the spawn tail onto a single `active_package` resolve and left them unreachable)."

## Impact
Documentation-only, but on the tier audits are explicitly told to trust over source. A reader grepping for any of the eighteen named symbols/paths gets nothing and must reverse-engineer the live path (`active_package` + `PackRecord::is_*`, as `_audit-common.md`'s Sandbox AI row correctly describes) from scratch. Two of this file's rot classes are now tracked independently (#3351 and this issue), which is itself a signal the file needs one consolidating pass rather than three point edits.

## Related
#3351 (OPEN — same file, **disjoint** lines 222-224 / 452-454 / 473-476, claims about spawn-time-only selection and no-pathing; fix both together). #3042 (CLOSED — deleted the code; the doc was not updated with it). #2054 (CLOSED — the `ai.rs` split). The `_audit-validate.sh` bare-basename blind spot filed alongside this report is why the gate did not catch the `ai.rs` half.

## Suggested Fix
Rewrite §4/§5 and the six per-procedure sections against `active_package` + `PackRecord::is_*`, replacing every `ai.rs` with the live `crates/plugin/src/esm/records/misc/pack.rs`; italicise the eight deleted getter names as historical per the path-reference convention, or drop them. Land alongside #3351.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other cross-cutting trace docs in `docs/engine/`, and the per-procedure module docs #3351 names)
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` runs clean on this file afterwards (see the companion bare-basename gate fix)
