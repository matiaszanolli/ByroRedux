**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads (audit-trail hardening)
**Severity**: LOW (informational — no current bug, contract is correct)

## Observation

`byroredux/src/streaming.rs:131-136` (doc) + lines 96-115 (`PartialNifImport`):

`WorldStreamingState.mat_provider` stays on the main thread (verified — never moved into worker thread). The worker emits `PartialNifImport { scene: NifScene, … }` which is `Send` (the scene contains `Box<dyn NifBlock>` trait objects whose `Send` bound is declared on the trait — verified via single-cell M40 phase 1b tests).

The doc comment at lines 132-136 explicitly states "Worker doesn't touch BGSM."

## Why it's worth tracking

No bug today — the contract is correct. **Risk**: a future contributor adds a non-`Send` field to `NifScene` (e.g. an `Rc<Texture>` for some compositional reason). The channel send breaks at compile time (good), but the compile error is far from the change site and the diagnostic isn't actionable without knowing the worker contract.

## Fix

Add a `static_assertions::assert_impl_all!` near the struct definition so the invariant is compile-checked at its declaration site:

```rust
static_assertions::assert_impl_all!(PartialNifImport: Send);
```

Optionally add `Send + Sync` if the contract calls for that — but for current architecture, `Send` is sufficient (the channel only requires `Send`).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: same assertion for `LoadCellRequest` / `LoadCellPayload` and any other type that crosses the worker channel
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: the `assert_impl_all!` IS the test (compile-fail otherwise)

## Related

- (none — informational follow-up)
