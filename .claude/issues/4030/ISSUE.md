# #4030 — REN-2026-09-06-D3-06: `MESH_ID_ENCODING_CEILING` is a residual literal copy of the mask `bf8ded3d` consolidated a day earlier

**Labels**: low, renderer, shaders, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (`max_instances_stays_within_mesh_id_encoding_ceiling`)
- **Status**: NEW
- **Description**: `bf8ded3d` (#3881, 2026-09-06) gave the mesh-ID attachment ABI a Rust-side definition — `MESH_ID_NO_HISTORY_BIT` and `MESH_ID_STABLE_MASK` in `shader_constants_data.rs`, the latter expressed as `!MESH_ID_NO_HISTORY_BIT` explicitly *"rather than a fourth copy of `0x7FFFFFFF`"* — and replaced the GLSL literals. One executable copy survives: the ceiling test still declares `const MESH_ID_ENCODING_CEILING: usize = 0x7FFF_FFFF;` locally.
- **Evidence**: `grep -rn "0x7FFF_FFFF\|0x7FFFFFFF" crates/renderer/src/` returns eight hits; seven are doc comments or negative source-scan assertions, and this local `const` is the only remaining code literal.
- **Impact**: The test asserts `MAX_INSTANCES <= MESH_ID_ENCODING_CEILING`. If the no-history bit ever moved, the mask constant would move and this test would keep asserting against the old ceiling — a silently-green guard on the encoding contract it exists to protect. Very low likelihood; the value is a genuine hardening loss rather than a live risk.
- **Related**: #3881 / `bf8ded3d`; #992 (the `R16_UINT` → `R32_UINT` change this test guards).
- **Suggested Fix**: Replace the local `const` with `crate::shader_constants::MESH_ID_STABLE_MASK as usize`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
