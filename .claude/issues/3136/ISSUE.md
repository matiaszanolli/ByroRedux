# SAFE-2026-08-20-03: six unsafe blocks carry no SAFETY comment — two in the water delta, four in the new compute_blas_budget probe

**Issue**: #3136 — https://github.com/matiaszanolli/ByroRedux/issues/3136
**Finding**: `SAFE-2026-08-20-03`
**Labels**: bug, medium, safety
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_SAFETY_2026-08-20.md` — Dimension 4 (unsafe-block discipline)
**Severity**: MEDIUM · **Status**: NEW

## Location
- `crates/renderer/src/vulkan/water.rs:448-453`, `:466`
- `crates/renderer/src/vulkan/acceleration/predicates.rs:679-683`, `:684`, `:685`, `:687`

## Description
A mechanised sweep of every `.rs` file under `crates/`, `byroredux/` and `tools/` finds **699 `unsafe {` blocks**. 693 carry a SAFETY comment either immediately above or as the first line inside the block (the house convention). Six do not.

All six are correct as written — none is unsound — but per `_audit-severity`'s Special Rules an `unsafe` block without a safety comment is a MEDIUM regardless, and both clusters are inconsistent with their *immediate* neighbours, which is what makes them look like oversights rather than a deliberate style choice.

In `water.rs`, the partial-init cleanup at `:422` (twenty-six lines earlier) carries a full one-line SAFETY rationale and the near-identical cleanup at `:448` carries none; the `update_descriptor_sets` at `:466` has none while the byte-identical call at `:509` does.

In `predicates.rs`, `compute_blas_budget` (added by #3043, after the last sweep) is a four-call `unsafe` sequence — create a probe buffer, query its memory requirements, destroy it, query physical-device memory properties — with no comment anywhere in the function.

## Evidence
```
$ python3 sweep.py   # unsafe { blocks vs SAFETY comment, 14 lines before / 25 inside
total unsafe blocks: 699
missing SAFETY: 9
```
Three of the nine are false positives on manual read and are **not** part of this finding: `crates/renderer/src/vulkan/buffer.rs:1093` and `crates/nif/src/stream.rs:467` both carry SAFETY comments longer than the window (17 and 20 lines respectively), and `crates/renderer/src/vulkan/context/draw.rs:3556` is the word `unsafe` in prose.

The six real sites, verified at HEAD:
```rust
// water.rs:445-454 — no SAFETY, unlike the identical cleanup at :422
for buffer in &mut param_buffers { buffer.destroy(device, allocator); }
unsafe {
    device.destroy_pipeline(pipeline, None);
    device.destroy_pipeline_layout(pipeline_layout, None);
    device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
    device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
}
```
```rust
// water.rs:466 — no SAFETY, unlike the identical call at :509
unsafe { device.update_descriptor_sets(&[write], &[]) };
```
```rust
// acceleration/predicates.rs:679-687 — four bare unsafe blocks, no comment in the fn
let probe = unsafe { device.create_buffer(&create_info, None)…? };
let requirements = unsafe { device.get_buffer_memory_requirements(probe) };
unsafe { device.destroy_buffer(probe, None) };
let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
```

## Impact
No runtime impact — every invariant holds. The cost is that the house convention is what makes the *next* audit's mechanised sweep meaningful: #2692 already retired one phantom "SAFETY gap" work item, and the value of the remaining sweep depends on the miss list being short enough that each entry is worth reading.

The `predicates.rs` cluster is the one with a real (if small) invariant worth stating: the probe buffer must be destroyed before the function returns on every path, and it currently leaks on the `?` at `:681`.

## Related
#2683 / #2684 / #2692 (all CLOSED) were the previous rounds of this same sweep. The prior report's count was 683/683; the six new misses arrived with the water UBO (`ed3570ad`) and the BLAS-budget probe (#3043).

## Suggested fix
Add the four missing comments. While in `compute_blas_budget`, note that the `?` on `create_buffer`'s `.context(...)` at `:681` is fine (nothing allocated yet) but a future fallible call added between `:679` and `:685` would leak the probe — worth stating in the comment so the shape is deliberate.

## Completeness Checks
- [ ] **UNSAFE**: Each added SAFETY comment states the upheld invariant, not just that the call is FFI
- [ ] **SIBLING**: Re-run the sweep after the fix — the miss list must return to zero, and the three long-comment false positives should stay recognised as such
