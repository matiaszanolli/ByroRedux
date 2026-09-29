# SAFE-D4-2026-09-29-01: Two `unsafe` blocks in `byroredux/src/app_events.rs` (the NVML GPU-name probe) have no SAFETY comment

**Labels**: medium,safety,vulkan,bug

**Source report**: `docs/audits/AUDIT_SAFETY_2026-09-29.md`

- **Severity**: MEDIUM (special-rule floor: an `unsafe` block without a safety comment).
- **Dimension**: Unsafe-Block Discipline
- **Location**: `byroredux/src/app_events.rs:236-241`, in `resumed`. Introduced by 0925f7926 ("feat: extend GPU memory and workload telemetry", 2026-09-26).
- **Status**: NEW. No open or closed issue matches (`app_events unsafe SAFETY`, `NVML`). The skill's census names these blocks as uncommented but no issue was ever filed.
- **Description**: The NVML adapter-matching code reads the selected physical device's properties and wraps `device_name` in a `CStr`. It does this in two bare `unsafe` blocks. Neither has a `// SAFETY:` comment, and the `byroredux` crate has no `undocumented_unsafe_blocks` lint, so nothing flags them. That lint lives only in the renderer; see SAFE-D4-2026-09-29-02 for why even that one is currently inert in CI.
  - The invariants do hold:
    - `ctx` was created a few lines earlier, so `ctx.instance` and `ctx.physical_device` are live;
    - the spec guarantees `VkPhysicalDeviceProperties::deviceName` is a NUL-terminated `char[VK_MAX_PHYSICAL_DEVICE_NAME_SIZE]`.
  - The finding is therefore the missing justification, not a false invariant.
  - The renderer already performs the same two calls with SAFETY comments: `crates/renderer/src/vulkan/device.rs:454-456`.
- **Evidence**:
  ```rust
  let selected_gpu = unsafe {
      ctx.instance
          .get_physical_device_properties(ctx.physical_device)
  };
  let selected_gpu_name =
      unsafe { std::ffi::CStr::from_ptr(selected_gpu.device_name.as_ptr()) }
          .to_string_lossy();
  ```
  The local lint run reports `warning: unsafe block missing a safety comment` at `app_events.rs:236:36` and `:241:21`. A third warning, at `cell_loader/unload.rs:452`, is adjacency only: its SAFETY comment sits above the enclosing `if let`.
- **Impact**: None at runtime today. This is discipline debt on the engine's startup path, and it grows the uncommented tail that only a hand sweep can find.
- **Related**: #1904 (the renderer lint), SAFE-D4-2026-09-29-02.
- **Suggested Fix**: Improve the existing code rather than duplicating it: either
  - have the renderer expose the selected device's `vendor_id` and name, which `device.rs` already reads, on `VulkanContext`; or
  - use ash 0.38's `PhysicalDeviceProperties::device_name_as_c_str()`, which removes the `CStr::from_ptr` block.

  Document whichever `get_physical_device_properties` block remains.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/app_events.rs:236` (`get_physical_device_properties`) and `:241` (`CStr::from_ptr(selected_gpu.device_name.as_ptr())`) are bare `unsafe` blocks with no `// SAFETY:` comment.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
