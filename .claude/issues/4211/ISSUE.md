# TD3-001: `GpuMaterial` 432->428 B shrink (#3909) never propagated past the pinning test and shader docs

Labels: medium,tech-debt,doc-rot,documentation,renderer

**Description**: `GpuMaterial` is pinned at 428 B by `crates/renderer/src/vulkan/material_tests.rs`'s `gpu_material_size_is_428_bytes` (107 flat scalar fields x 4 B, confirmed live). The shader side (`bindings.glsl`, `triangle.frag`) already documents this correctly, including the #3909 shrink narrative. ~24 sites across the Rust doc comments, two test-file doc comments, two skill files, and six `docs/engine/*.md` files still assert 432 B and/or cite the dead test name `gpu_material_size_is_432_bytes` (confirmed present in `material.rs:43,48-49,1047,1321,1363,1380`). One site (`material.rs:392`) has an independent arithmetic error: `// offset 424 -> total 432` should read 428 regardless of the historical narrative. `material.rs:1047`'s "108 live scalar fields" is also one off (current count: 107). This is the third or fourth recurrence of this exact class for this struct (#1321/#1755, #3846, #3414/#3240).

**Evidence**:
`material.rs:43` — "432 bytes"; `:48-49` — "-> 432 B"; `:49` — "Pinned by `gpu_material_size_is_432_bytes`"; `:392` — `// offset 424 -> total 432`; `:1047` (x2) — "432-byte struct" / "108 live scalar fields". Also stale in `material_tests.rs`, `scene_buffer/constants.rs:174,177`, `shader_contract_tests.rs:2214`, `byroredux/src/material_translate.rs:104,107`, `byroredux/src/render/static_meshes.rs:1108`, `.claude/commands/audit-safety/SKILL.md:261,269,282`, `.claude/commands/_audit-common.md:101`, `docs/engine/material-abstraction.md:27`, `docs/engine/nifal.md:85`, `docs/engine/memory-budget.md:95`, `docs/engine/shader-pipeline.md:370,493`, `docs/engine/rt-lighting-material-recovery.md:38,115,641`, `docs/engine/renderer.md:136,539`. Same-day corroboration in `docs/audits/AUDIT_RENDERER_2026-09-11.md` (REN-2026-09-11-D3-01/D7-01).

**Impact**: A GPU-struct-layout doc claim that's wrong is exactly the class of drift the project's own tooling exists to catch (see `_audit-common.md`'s symbol-advisory convention) — recurring a fourth time on the same struct suggests the manual-sweep approach doesn't hold.

**Related**: #1321/#1755, #3846, #3414/#3240 (prior recurrences of this exact doc-rot class); #3909 (the shrink that wasn't propagated); `docs/audits/AUDIT_RENDERER_2026-09-11.md` REN-2026-09-11-D3-01/D7-01 (same-day corroboration).

**Suggested Fix**: Mechanical find/replace at every site (`432`->`428`, test name update, MB-derived figures in `constants.rs:174` and `memory-budget.md:95`), fix the two numeric errors, extend each "growth chain" narrative with the #3909 link, and add a `collect_stale_gpu_material_size_claims` source-scanning test to `byroredux/src/workspace_hygiene_tests.rs` (mirroring the existing `classify_pbr` self-enforcing test) so a fifth recurrence fails the build instead of requiring another manual sweep.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
