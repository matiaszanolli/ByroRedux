# Issue #657: AS-8-2: decide_use_update returns (true, true) for empty instance lists — fragile contract

**File**: `crates/renderer/src/vulkan/acceleration.rs:165-183`, `acceleration.rs:1789-1795`, `acceleration.rs:1971-1979`
**Dimension**: Acceleration Structures

For `instances.len() == 0` (empty frame), `current_addresses_scratch` is empty. `decide_use_update(false, last_gen, last_gen, &[], &[])` returns `(true, true)` — the zip-compare considers two empty lists identical and chooses UPDATE. The build then runs `mode=UPDATE, src=dst=tlas.accel, primitiveCount=0`.

Today safe by virtue of `needs_full_rebuild = true` at TLAS creation forcing the very first build to BUILD. Fragile under any future refactor that resets `needs_full_rebuild` after BUILD without checking instance_count > 0.

**Fix**: In `decide_use_update`, treat empty `current_addresses` as "must BUILD" — return `(false, false)` when `current_addresses.is_empty()`. The cost is one extra BUILD on transition between empty-scene frames (no measurable impact); the win is that the helper is correct under any future caller refactor. Add a regression test pinning the new behavior.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
