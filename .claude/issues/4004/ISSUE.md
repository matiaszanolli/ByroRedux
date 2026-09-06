# #4004 — REN-2026-09-06-D11-02: the pipeline-cache header gate accepts a `headerSize` larger than the file it validated

**Labels**: low, pipeline, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D11-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`validate_pipeline_cache_header`)
- **Status**: NEW
- **Description**: The SAFE-11 / #91 gate exists so "a bad header never
  reaches the driver" — an explicit defence-in-depth argument about a cache
  file "dropped next to the binary by a process with filesystem write
  access". It rejects `len < 32`, `headerSize < 32`, `headerVersion != 1`,
  and vendor/device/UUID mismatch, but deliberately does not upper-bound
  `headerSize` ("a future version might legitimately grow the prefix"). The
  cheap and version-agnostic bound is missing: `headerSize` must not exceed
  the length of the file it describes. A 32-byte file claiming
  `headerSize = 0xFFFF_FFFF` passes every check and is handed to
  `vkCreatePipelineCache` with the correct vendor/device/UUID, which is the
  one shape the gate's own threat model names.
- **Evidence**: the `if header_size < 32 { return false; }` check with no
  companion `header_size as usize > initial_data.len()` arm; the six
  `pipeline_cache_header_tests` cases cover short files, bad version, and the
  three identity fields, not an over-large `headerSize`.
- **Impact**: Defence-in-depth only — the driver re-validates independently
  and a well-behaved one rejects it. The pre-condition (write access to the
  executable's directory) is already a strong position for an attacker.
  Filed because the gate's stated purpose is exactly to catch this, and the
  fix is one comparison plus one test.
- **Related**: SAFE-11 / #91.
- **Suggested Fix**: Add `if header_size as usize > initial_data.len() { return false; }`
  alongside the `< 32` check, and a `pipeline_cache_header_tests` case for it.
  This is compatible with the "future version might grow the prefix" comment
  — a grown prefix still fits inside its own file.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
