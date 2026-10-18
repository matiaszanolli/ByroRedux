# SCR-D7-2026-10-17-08: `CompatibilityRegistry` bounds script count but not bytes; the ObScript source path has no per-script finding budget and truncation is silent in the log

Issue: #6024
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-08: `CompatibilityRegistry` bounds script count but not bytes; the ObScript source path has no per-script finding budget and truncation is silent in the log

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/compatibility.rs:30, 61-90` (registry), `compatibility.rs:166-250` (`analyze_obscript_compatibility` / bytecode path), caller `cell_loader/references/attach.rs:305-335`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The PEX path is budgeted (`ScanBudget`, #4317). The ObScript source path is not: every recognized extender command in an SCTX produces a `CompatibilityFinding` that owns ~6 heap strings, so a ~64 KiB script of repeated `IsModLoaded` yields ~5k findings (~2 MB). Retention is capped only at 65 536 scripts. When the cap is hit `record` sets `truncated = true` and returns `false` with no log line; the flag is visible only to an operator who runs `sdk.compat`.
- **Evidence**: no byte/finding cap in `analyze_obscript_compatibility`; `record` has no `log::warn!`. Amplification bounded by input size x ~35, and only for scripts attached to loaded refs, hence LOW.
- **Impact**: Memory amplification from a hostile plugin; silent evidence loss after 65 536 scripts.
- **Suggested Fix**: Charge ObScript findings against the same style of per-script budget (or cap findings per script and set a per-report `truncated`), and `warn!` once when the registry first truncates.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
