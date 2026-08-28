# Issue #3505: REG-2026-08-27-03: regression of #3166 — SCAN_ROOTS still leaves crates/renderer and crates/save unscanned, hiding three production Resource impls from the completeness guard

- **Severity**: LOW
- **Dimension**: Regression / save-load completeness guard
- **Labels**: low, tech-debt, test-gap, bug
- **Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-27.md`
- **Filed**: 2026-08-28

---

## Description

**Regression of #3166** (CLOSED, `medium`) — partial fix.

#3166's title is *"the completeness guard's `SCAN_ROOTS` covers one subdirectory of `crates/core`"*. The fix widened it from 1 root to 6 (`crates/core`, `crates/scripting`, `crates/physics`, `crates/audio`, `crates/plugin`, `byroredux`), which is a real improvement. But the guard's contract is *completeness* — every live ECS `Component`/`Resource` is either registered for save or named in the exclusion ledger — and two workspace crates that declare production `Resource` impls are still outside the scan, with no comment recording the exclusion as deliberate.

## Location

`byroredux/src/save_io/registry_completeness_tests.rs:362-369` (`SCAN_ROOTS`)

## Evidence

```rust
// byroredux/src/save_io/registry_completeness_tests.rs:362-369 — verbatim
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src",
    "../crates/scripting/src",
    "../crates/physics/src",
    "../crates/audio/src",
    "../crates/plugin/src",
    "../byroredux/src",
];
```

Production `impl Resource for` sites outside those roots:

```
crates/renderer/src/vulkan/allocator.rs:49  impl Resource for AllocatorResource {}
crates/renderer/src/vulkan/allocator.rs:70  impl Resource for GpuMemoryBudget {}
crates/save/src/registry.rs:18              impl Resource for SaveRegistry {}
```

(`crates/nif`, `crates/ui`, `crates/bsa`, `crates/papyrus`, `crates/pex`, `crates/debug-server`, `crates/platform`, `crates/sfmaterial`, `crates/bgsm` declare none, so the blind spot is exactly these two crates today.)

## Impact

No live bug — all three are engine machinery (a GPU allocator handle, a VRAM budget probe, and the save registry itself) that must never be serialised, and none carries gameplay state.

The gap is in the guard's reach: a future saveable `Resource` declared in `crates/renderer` or `crates/save` would be silently absent from **both** the registry and the exclusion ledger, which is precisely the failure mode #3166 exists to prevent. A completeness guard that is silently incomplete reports green either way.

## Related

- #3166 — the partially-applied fix (CLOSED)
- #3167 — sibling, *"the rewritten serde guard's file discovery has three residual holes"* (CLOSED)
- #2536 — the earlier `byroredux/src` blind spot in the same guard (CLOSED)

## Suggested Fix

Either add `../crates/renderer/src` and `../crates/save/src` to `SCAN_ROOTS` and enumerate the three types in the existing exclusion table with their one-line justification, or add a comment above `SCAN_ROOTS` stating which crates are deliberately out of scope and why — the current silence is indistinguishable from an oversight.

## Source

`docs/audits/AUDIT_REGRESSION_2026-08-27.md` — REG-2026-08-27-03

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling serde guard (#3167) and any other source-scanning test that hardcodes a root list
- [ ] **TESTS**: A regression test pins this specific fix
