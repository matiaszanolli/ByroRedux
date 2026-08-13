# REN-D23-05: byro_fsr3_context_destroy leaks the FSR context/wrapper on non-OK destroy result

Labels: low, renderer, bug

## Description

The wrapper is `delete`d and the out-pointer nulled **only inside `if (result == FFX_API_RETURN_OK)`**. On a non-OK return the wrapper, the `ffxContext`, and everything the provider allocated behind it (pipelines, descriptor pool, its own `VkDeviceMemory` — the tens of MB reported as "SDK working memory") stay live with no remaining owner; `Drop for Context` receives the code, `eprintln!`s it, and drops the `NonNull` with no retry. Because `FrameUpscaler::recreate` destroys and rebuilds on **every resize and preset switch**, a persistently-failing destroy compounds per switch. The one place in an otherwise carefully-ordered teardown chain where memory outside gpu-allocator's view can be stranded past `vkDestroyDevice`. Failure-path only; no failure observed.

## Location

`crates/fsr3-sys/native/byro_fsr3.cpp` (`byro_fsr3_context_destroy`); `crates/fsr3-sys/src/lib.rs` (`impl Drop for Context`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-05).

https://github.com/matiaszanolli/ByroRedux/issues/2829
