**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads / BSA (cross-cutting)
**Severity**: LOW

## Observation

`byroredux/src/streaming.rs:474-515` (`pre_parse_cell`) wraps each per-NIF parse in `catch_unwind(AssertUnwindSafe(...))`. `extract_mesh` reads `BsaArchive` / `Ba2Archive` whose backing `File` is behind a `Mutex<File>`. If a panic fires while that mutex is held (e.g. a length-prefix overflow inside decompression), the mutex is poisoned.

Confirmed pattern: `crates/bsa/src/ba2.rs:367`:

```rust
let mut file = self.file.lock().expect("BA2 file mutex poisoned");
```

`.expect("BA2 file mutex poisoned")` panics on `PoisonError`. The per-NIF guard catches the panic and converts it to `None`, **but the mutex stays poisoned for every subsequent extract**.

## Why it's a bug

One parser panic → archive mutex poisoned → every subsequent `extract_mesh` call (from main thread OR worker) panics. The per-NIF guard transforms one panic into N panics. Worker keeps recovering with `None`, but user-visible cells start failing to load — symptom looks like "cells silently empty after one bad NIF".

## Trigger Conditions

A panic inside `parse_nif` / `import_nif_lights` / `extract_bsx_flags` / decompression that occurs while a `BsaArchive`'s inner `Mutex<File>` is held. The `AssertUnwindSafe` claim made by `pre_parse_cell` is technically false for code paths that mutate state inside the mutex.

## Fix

In `BsaArchive::extract` / `Ba2Archive::extract`, recover from poison via `into_inner()` — file-position state is reset on each `seek` anyway, so poison is recoverable:

```rust
let mut file = match self.file.lock() {
    Ok(g) => g,
    Err(poisoned) => {
        log::warn!("BSA file mutex poisoned — recovering");
        poisoned.into_inner()
    }
};
```

Or wrap in a helper that converts poison into a parse error (richer telemetry, parser sees a clean `Err` instead of an unwound panic).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: sweep `crates/bsa/` for every `.lock().unwrap()` / `.lock().expect(...)` on Mutex types; same pattern likely in BA2 v1/v2/v3/v7/v8 paths
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test that explicitly panics inside an extract closure and asserts subsequent extracts succeed (poison recovery)

## Related

- #877 — NIF-PERF-13: `pre_parse_cell` BSA mutex serializes I/O across workers (performance angle on the same mutex; this issue is the correctness angle on its poison handling)
