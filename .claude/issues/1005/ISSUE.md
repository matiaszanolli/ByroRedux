# #1005 — REN-D3-NEW-03: BLAS scratch buffer not shrunk on swapchain resize

- **Severity**: LOW
- **Domain**: renderer / memory / performance
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1005

## TL;DR
`accel.blas_scratch_buffer` is grow-only by design. Shrunk on cell unload but not on swapchain resize — long session + rare cell crossings keeps the ~80-200 MB scratch resident.

## Fix
Call `shrink_blas_scratch_to_fit` from `recreate_swapchain` after `device_wait_idle`. Resize already pays the device-wait cost.
