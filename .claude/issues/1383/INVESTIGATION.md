# Investigation — #1383 FFI-03 panic=unwind required for cxx

**Domain:** cxx (FFI) / build config

## Finding
`byroredux-cxx-bridge` uses the `cxx` crate, which bridges C++ exceptions across
the FFI boundary as Rust `Result`s by relying on the platform unwinder. Setting
`panic = "abort"` would turn any cross-language unwind into an immediate
`abort()`, breaking cxx's exception-safety contract. No `[profile.release]`
existed, so the requirement was implicit (relying on the cargo default).

## Fix (documentation + explicit config; INFO severity)
Added a `[profile.release]` section to the workspace `Cargo.toml` with
`panic = "unwind"` set explicitly (== the cargo default, so zero behavioural
change) plus a DO-NOT-set-abort rationale comment pointing at the cxx bridge.
This makes the contract visible at the edit site and guards against a future
binary-size change silently flipping it. If `panic = "abort"` is ever genuinely
wanted, every C++ function reachable through the bridge must first be `noexcept`.

## Verification
`cargo check` accepts the profile; no behavioural change (unwind is already the
release default). cargo test 2792 passed.

## Completeness
- [x] FFI: no pointer lifetimes touched (build-config + doc only)
- [x] TESTS: N/A — build-profile contract; cargo check is the gate
