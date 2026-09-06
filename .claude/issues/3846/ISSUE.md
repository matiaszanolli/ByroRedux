# #3846: TD3-2026-09-05-01: `bindings.glsl` documents `GpuMaterial` as 396 B and points the struct-sync invariant at `gpu_material_size_is_396_bytes` — a test that has never existed (live: 432 B / `_432_`)

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `medium,shaders,renderer,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3846 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/shaders/include/bindings.glsl:99` and `:107-108`
- **Status**: **Regression of #1755** (CLOSED — *"TD3-002: bindings.glsl cites stale `gpu_material_size_is_260_bytes` test (real: `_300_bytes`)"*). Same file, same comment block, same defect, different numbers.
- **Effort**: trivial (≤30 min)
- **Age**: `d9d4a6d7` introduced the "396 B" wording; `ceb69d24` (2026-08-25) grew the struct 396 → 432 B and updated the Rust side but not this GLSL mirror. **11 days stale.**
- **Description**: `bindings.glsl` is the *single* GLSL declaration of `struct GpuMaterial` (lifted out of `triangle.frag` under #1583/#1590) and is therefore the one file whose header comment carries the Rust↔GLSL lockstep contract. Two sentences in that header are wrong:
  1. Line 99 — `// Mirrors the Rust `GpuMaterial` (396 B std430) defined` — the struct is **432 B**.
  2. Lines 107-108 — `// `intern`/encoding sites; the size of this struct (396 B) is pinned by` / `// `gpu_material_size_is_396_bytes` on the Rust side.` — the size is wrong **and** the named test does not exist.
- **Evidence**:
  ```
  $ grep -rn "gpu_material_size_is_396" --include='*.rs' --include='*.glsl' .
  crates/renderer/shaders/include/bindings.glsl:108:// `gpu_material_size_is_396_bytes` on the Rust side.
  ```
  One hit, and it is the comment itself — there is no such test anywhere in the workspace. The live pin is `crates/renderer/src/vulkan/material_tests.rs:62-63`:
  ```rust
  fn gpu_material_size_is_432_bytes() {
      assert_eq!(std::mem::size_of::<GpuMaterial>(), 432);
  ```
  The Rust side already documents this correctly — `crates/renderer/src/vulkan/material.rs:40-46` reads *"std430 GPU-side material record. **432 bytes** per material. … → 396 B (BGEM v21+ glass optics) → 432 B (Bethesda lighting response + canonical mask roles). Pinned by `gpu_material_size_is_432_bytes`."* Only the GLSL mirror lagged.
- **Impact**: This is the highest-value doc site in the renderer for this class. `feedback_shader_struct_sync.md` names `bindings.glsl` as the #1 source of silent GPU-struct desync, and the comment's whole job is to tell a contributor which test to update in lockstep with a field addition. It currently sends them to a dead grep — exactly the failure #1755 was closed to prevent, now recurred one size-bump later. `#[repr(C)]` GPU-struct drift is HIGH per `_audit-severity.md`; a doc that misdirects the guard against it is MEDIUM per the tech-debt promotion table ("stale `GpuMaterial` size in a doc comment — lockstep-drift bait").
- **Related**: #1755 (the identical prior regression, 260→300), #1321 (`GpuMaterial` 260 B in 8 sites), #3830/#3831/#3832 (today's other shader/doc rot — distinct subjects).
- **Suggested Fix**: Two edits: `396 B std430` → `432 B std430` (line 99) and `(396 B) is pinned by` / `gpu_material_size_is_396_bytes` → `(432 B) is pinned by` / `gpu_material_size_is_432_bytes` (lines 107-108). Given this is the second recurrence of the same sentence in the same file, consider making the size a generated value: `crates/renderer/build.rs` already emits `shaders/include/shader_constants.glsl` from `shader_constants_data.rs`, so `GPU_MATERIAL_SIZE_BYTES` could be emitted alongside and the comment could stop hand-carrying a number.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
