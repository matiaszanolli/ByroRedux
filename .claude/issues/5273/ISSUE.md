# #5273: SAFE-D4-2026-10-05-01: #4895 added a third renderer `CStr::from_ptr(device_name)` block just as #5120 moved the identical byroredux block to ash's bounded `device_name_as_c_str()`

**Labels**: low,safety,renderer,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5273

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` — `SAFE-D4-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW. This is hardening and consistency. The block is commented, and its invariant holds for a conforming driver.
- **Dimension**: Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/device.rs:653`, which is new in `7f6ab8e8f` (#4895). The pre-existing siblings are at `:466` and `:481`.
- **Status**: NEW.
  - #5120 (CLOSED, `b7bc84722`) took SAFE-D4-2026-09-29-01's suggested fix in `byroredux/src/app_events.rs`: `selected_gpu.device_name_as_c_str().unwrap_or_default()`.
  - The renderer kept the raw form and gained one more instance.
- **Description**:
  - `CStr::from_ptr` scans for a NUL with no length bound. The spec guarantees `deviceName` is a NUL-terminated `char[VK_MAX_PHYSICAL_DEVICE_NAME_SIZE]`, so the SAFETY comment is true.
  - But the soundness rests on driver conformance, and a non-terminated name would read past the `properties` struct on the stack.
  - ash 0.38's `PhysicalDeviceProperties::device_name_as_c_str()` performs the same conversion through `CStr::from_bytes_until_nul` over the fixed array. That is bounded and needs no `unsafe`.
  - All three blocks feed only `log::warn!` / `log::info!` formatting.
- **Evidence**:
  ```rust
  // SAFETY: device_name is a fixed-size [c_char; 256] array
  // null-terminated by the Vulkan driver. The pointer remains valid
  // while `properties` is in scope.
  let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) };
  log::warn!("Rejecting GPU {name:?}: missing required Vulkan features {missing:?}");
  ```
- **Impact**: None on conforming drivers. It adds 3 avoidable `unsafe` blocks to the renderer's count, and the codebase now spells one conversion two ways.
- **Related**: #5120, #4895, SAFE-D4-2026-09-29-01.
- **Suggested Fix**: Replace all three with `properties.device_name_as_c_str().unwrap_or_default()`, the spelling #5120 already uses, deleting three `unsafe` blocks and their SAFETY comments. This improves the existing code rather than adding a helper.

## Publisher note

Publisher re-check at HEAD: `crates/renderer/src/vulkan/device.rs` has **four** `CStr::from_ptr(…device_name.as_ptr())` sites, not three — `:466`, `:481`, `:524` (`selected.properties.device_name`) and `:653`. All four can take `device_name_as_c_str().unwrap_or_default()`. (`:388` is the analogous `extension_name` comparison; ash also exposes `extension_name_as_c_str()`.)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
