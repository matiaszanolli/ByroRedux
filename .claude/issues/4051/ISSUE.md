# #4051 — REN-2026-09-06-ORCH-01: the emitted shader-recompile instruction does not run

**Labels**: low, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-ORCH-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (documentation / doc-rot)
- **Location**: `crates/renderer/build.rs` — module doc, and the `writeln!`
  that emits the "Then recompile shaders" line; the emitted result lands in
  `crates/renderer/shaders/include/shader_constants.glsl`.
- **Status**: NEW
- **Description**: Both documented invocations write the include path as
  `-I crates/renderer/shaders` — with a space. `glslangValidator` rejects that
  form outright. The emitted copy has a second defect: it drops `-o <output>`,
  so even with the spacing corrected it writes glslang's default output name
  rather than `<shader>.spv`.
- **Evidence**: reproduced against glslang `11:16.2.0`:
  ```
  $ glslangValidator -V -I . triangle.frag -o /tmp/t1.spv
  -I<dir> include path must immediately follow option (no spaces)
  ```
  The working forms both produce a correct 368 324-byte blob: `-I.` (no space),
  or CLAUDE.md's `glslangValidator -V triangle.frag -o triangle.frag.spv` run
  from inside `crates/renderer/shaders/` (relative `#include`s resolve against
  the including file's directory, so no `-I` is needed there at all).
  **CLAUDE.md's documented command is correct and was verified working** — only
  the two `build.rs` copies are broken.
- **Impact**: This is the instruction a contributor reads at the moment of
  maximum relevance — immediately after `cargo build` regenerates
  `shader_constants.glsl` following a constant or GPU-struct field change. That
  is precisely the lockstep chokepoint `feedback_shader_struct_sync.md` names as
  the project's #1 source of silent Rust↔GLSL desync. Severity stays LOW because
  the failure is loud and self-healing: glslang prints a specific diagnostic, and
  the obvious recovery (drop the `-I`) happens to work from inside the shaders
  directory. It is a papercut on a load-bearing path, not a correctness defect —
  and the sweep above proves no `.spv` has actually gone stale in practice.
- **Related**: `feedback_shader_struct_sync.md`; #3846 (`bindings.glsl` citing a
  nonexistent `gpu_material_size_is_396_bytes` pin) — the same class of defect
  in the same lockstep chokepoint, one file over.
- **Suggested Fix**: In `crates/renderer/build.rs`, change the module-doc line
  to `glslangValidator -V -I. <shader> -o <shader>.spv` (run from
  `crates/renderer/shaders/`), and give the emitted line the same treatment plus
  the missing `-o`. Optionally emit the whole-set loop above instead of a
  single-shader template, since a constants change is exactly the case where
  *every* dependent shader needs recompiling, not one.

### Completeness checks
- [ ] **SIBLING**: `docs/engine/shader-pipeline.md` and `CLAUDE.md` both carry
      shader-compile guidance — CLAUDE.md verified correct, shader-pipeline.md
      states only the SPIR-V target version (also verified correct). No other
      copies found.
- [ ] **TESTS**: no test executes a documented command line. A cheap guard is
      the byte-compare loop above as an `#[ignore]`d test, which would pin
      SPIR-V currency and the command line at once.

---

# Recorded regression checks (NOT findings)

Logged so the next sweep does not re-file them. Each was checked and its
guard verified in place.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
