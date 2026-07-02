# SCR-D6-NEW-01: feature-matrix.md understates the CTDA condition catalog (says 7, ships 13)

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1818
**Source report**: docs/audits/AUDIT_SCRIPTING_2026-07-02.md
**Labels**: low, documentation

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (documentation)
- **Untrusted-Input**: No
- **Location**: `docs/feature-matrix.md:137`
- **Status**: NEW

**Description**: The matrix row reads *"CTDA condition evaluation with OR-precedence (M47.1) | ✓ 7 functions"*. `condition.rs` ships **13** catalogued functions (GetDistance, GetActorValue, GetStage, GetStageDone, GetIsClass, GetIsRace, GetIsID, GetFactionRank, GetLevel, HasPerk, GetXPForNextLevel, GetReputation, GetReputationThreshold — the file header table + the `from_index` arms at `condition.rs:143-155`).

**Evidence**: `grep -cE "ConditionFunction::(…)" condition.rs` → `13`; `from_index` maps indices 1/14/58/59/68/69/72/73/80/448-449/533/573/575.

**Impact**: Documentation only; understates shipped capability.

**Related**: the prior "transpiler unstarted" doc-rot (now fixed).

**Suggested Fix**: Update the count to 13 (or drop the count and reference the `condition.rs` header catalog).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
