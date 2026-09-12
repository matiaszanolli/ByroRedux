# SCR-D3-2026-09-11-01: control_flow.rs module doc still says the residual ||-shape branch is "advanced past" instead of declined

URL: https://github.com/matiaszanolli/ByroRedux/issues/4115
Labels: documentation, low, scripting, doc-rot

- **Severity**: LOW (documentation only; the code path itself is correct and tested — `Err(self.fail())`, not a fallthrough)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower (audit-scripting)
- **Location**: `crates/pex/src/decompile/control_flow.rs:27-29`
- **Status**: NEW (a defect first raised by `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`'s re-adjudication table but never filed as its own issue; carried forward unfixed across three intervening fix cycles — `f1a9b984` and `5d0d96ef` cleared every other stale-prose item that pass re-checked but not this line)

**Description**

The doc comment reads "…only fires for the `||`-shapes the boolean pass declined to collapse — see SCR-D3-01/#1732 for that tail" and describes the branch as "advanced past." The actual arm (`:210-220`) is `return Err(self.fail())` — a hard decline, not an advance. The bare `SCR-D3-01` citation is also now ambiguous against this report series' dated finding-ID convention.

**Impact**

None functional; doc-rot risk only — a reader trusting the comment over the code would mis-describe the failure mode when extending this branch.

**Related**

`docs/audits/AUDIT_SCRIPTING_2026-09-06.md`, "The two documented Champollion departures" table, row 2; #1732

**Suggested Fix**

Reword to "declined, not advanced past" and drop the bare `SCR-D3-01` shorthand in favor of the issue number (`#1732`) only.

## Completeness Checks
- [ ] **SIBLING**: Check the `boolean.rs` no-debug-line guard's own doc comment for the same "advanced past" phrasing before closing

Source: `docs/audits/AUDIT_SCRIPTING_2026-09-11.md`
