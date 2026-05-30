# #1316 -- TD5-NEW-01: M47.1 condition evaluator 6 stub branches

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: MEDIUM | **Dim 5** — Stub Implementations
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD5-NEW-01)
**Domain**: ecs | **Effort**: small (~2h)

**Location**: `crates/scripting/src/condition.rs` (M47.1 condition evaluator, ~lines 89-220)

**Issue**: The M47.1 condition evaluator has 6 branches that return hardcoded safe-defaults instead of evaluating the authored condition. These are reachable from the M47.1 wiring and any Papyrus gameplay that triggers condition checks. Examples include actor-attribute comparisons that always return `false`, GetGlobalValue that always returns `0.0`, and HasKeyword that always returns `false`. The `papyrus_demo` integration test exercises the condition evaluator path.

**Fix**: Implement the 6 stubbed branches using the existing ECS query infrastructure (the condition type already carries the operands; they need to be evaluated against the ECS world resource). File individual tracking issues per branch if the implementations are non-trivial.

## Completeness Checks
- [ ] **SIBLING**: check remaining condition branches for similar stubs
- [ ] **TESTS**: add a test for each implemented branch
- [ ] **CANONICAL-BOUNDARY**: N/A (ECS/scripting, no material translate path)
- [ ] **UNSAFE**: no unsafe needed
