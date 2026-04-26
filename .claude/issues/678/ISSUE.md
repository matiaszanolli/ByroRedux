# Issue #678: AS-8-6: build_tlas "missing instances" warning miscounts !in_tlas as missing BLAS

**File**: `crates/renderer/src/vulkan/acceleration.rs:1611-1627`
**Dimension**: Acceleration Structures

`let missing = draw_commands.len() - instances.len();` includes draws skipped for `!in_tlas` (legitimate: particles, UI quads — by design they're rasterized but not in the TLAS) along with draws skipped for missing BLAS or missing `instance_map[i]`.

The log line says "{} lack BLAS — no RT shadows for those meshes" which is misleading: a frame with 200 particle draws and 0 missing BLAS will spam the warning suggesting an RT regression every second. Filters firing legitimately should not feed the diagnostic counter.

**Fix**: Count only the two error cases — `in_tlas && (no BLAS for mesh)` and `in_tlas && instance_map[i].is_none()` — and exclude `!in_tlas`. The log message stays accurate, and a real BLAS-miss regression isn't drowned in particle / UI noise.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
