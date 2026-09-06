# #4013 — REN-2026-09-06-D17-04: both #2243/#2244 regression guards named in the Dimension 17 checklist point at the wrong test file

**Labels**: low, renderer, shaders, tech-debt, test-gap, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 17 checklist)
- **Status**: NEW
- **Description**: The checklist cites `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` and `bounded_path_converts_dalc_irradiance_to_environment_radiance` as living in `gpu_instance_layout_tests.rs`. Both are in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`. The path gate cannot catch this: `gpu_instance_layout_tests.rs` **does** exist, so the backtick resolves; only the symbol→file association is wrong. `.claude/issues/2472/ISSUE.md` carries the same wrong anchor with a line number attached (`gpu_instance_layout_tests.rs:1148`).
- **Evidence**: `grep -rn "disney_sheen_keeps_its_relative_weight_in_canonical_direct_path\|bounded_path_converts_dalc_irradiance_to_environment_radiance" .` returns only `shader_contract_tests.rs` (plus the skill, the prior audit report, and the issue file).
- **Impact**: An auditor working the Dimension 17 checklist opens the named file, does not find the guard, and must either conclude the guard was deleted (a false regression report) or re-derive the location. This is the exact TD7-* stale-anchor class `_audit-common.md`'s path-reference convention exists to prevent, in its one form the validate script's advisory cannot flag.
- **Related**: `.claude/commands/_audit-common.md` "Path-Reference Convention (post-#1114)", `.claude/commands/_audit-validate.sh`, #3047 (the same drift in the shader-includes list).
- **Suggested Fix**: Repoint both citations to `shader_contract_tests.rs` in `SKILL.md`, and drop the line number from `.claude/issues/2472/ISSUE.md`. Consider extending `_audit-validate.sh`'s backticked-symbol advisory to also check that a symbol named in the same sentence as a `*_tests.rs` path is actually defined in that file.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
