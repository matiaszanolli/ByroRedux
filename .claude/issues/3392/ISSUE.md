# Issue #3392 — SF-2026-08-27-D1-01: the #2097 LZ4 panic guard is unreachable as built, and powerless in the only configuration where its named panic can occur

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `medium,bug,import-pipeline,safety,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3392 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 1 (BA2 v2/v3 LZ4 block decompression), delta review of `1b521305`.

- **Severity**: MEDIUM
- **Location**: `crates/bsa/src/ba2.rs:755-816` (the `Lz4Block` arm), `crates/bsa/Cargo.toml:9`, `Cargo.toml:140`
- **Status**: NEW (#2097 is closed; #2585 is the adjacent-but-different under-run signalling item)

## Description

`1b521305` wraps `lz4_flex::block::decompress` in `catch_unwind` on the strength of the upstream *"May panic if `min_uncompressed_size` is smaller than the uncompressed data"* doc, and the commit message attributes the observed absence of panics to *"a property of one pinned version"*.

That attribution is wrong, and the mitigation follows the wrong threat. The absence of panics is a property of a **Cargo feature**, not of version 0.11.6 — and in the build where that feature is off, the failure mode is not an unwind at all but an out-of-bounds heap write that `catch_unwind` cannot intercept.

## Evidence

`lz4_flex-0.11.6/src/block/mod.rs:21-25` selects the decoder by feature:

```rust
#[cfg(feature = "safe-decode")]
#[cfg_attr(feature = "safe-decode", forbid(unsafe_code))]
pub(crate) mod decompress_safe;
#[cfg(feature = "safe-decode")]
pub(crate) use decompress_safe as decompress;
#[cfg(not(feature = "safe-decode"))]
pub(crate) mod decompress;
```

**Built path** (`safe-decode`, a default feature) — `decompress_safe.rs:354-360`:

```rust
let mut decompressed: Vec<u8> = vec![0; min_uncompressed_size];
let decomp_len = decompress_internal::<false,_>(input, &mut SliceSink::new(&mut decompressed, 0), b"")?;
decompressed.truncate(decomp_len);
```

`SliceSink` is bounds-checked and the module is `forbid(unsafe_code)`: an undersized hint yields `Err(DecompressError::OutputTooSmall)`. The documented panic is **structurally impossible here**, not merely unobserved in fuzzing — so the `catch_unwind` at `ba2.rs:775` is dead code today.

**Unsafe path** (`safe-decode` off) — `decompress.rs:508-517`:

```rust
let mut vec = Vec::with_capacity(min_uncompressed_size);
let decomp_len = decompress_internal::<true,_>(input, &mut PtrSink::from_vec(&mut vec, 0), b"")?;
```

`PtrSink` writes through raw pointers with no capacity check (only `debug_assert`s, plus optional `checked-decode` guards that are **not** enabled here). An undersized hint on that path is a heap buffer overflow — UB, not an unwind. `catch_unwind` provides zero protection, and this is exactly the "archive bytes are attacker-controlled for modded content" case the commit message cites.

**Feature resolution independently verified by the audit orchestrator:**

```
$ cargo tree -p byroredux-bsa -i lz4_flex -e features
lz4_flex v0.11.6
├── lz4_flex feature "checked-decode"
│   └── lz4_flex feature "default"
│       └── byroredux-bsa v0.1.0
...
├── lz4_flex feature "safe-decode"
│   └── lz4_flex feature "default" (*)
```

Both safety features are reachable **only** via `default`, and `Cargo.toml:140` is a bare `lz4_flex = "0.11"` with no `default-features` / `features` pin. `byroredux-bsa` is the sole dependent, so nothing else re-enables them.

**Corroborating doc drift** inside the same function: `ba2.rs:794` and the test doc at `ba2.rs:1416` both state the `unpacked_size` parameter is *"only a capacity hint (`Vec::with_capacity`)"*. That is the **unsafe** module's implementation; the compiled `safe-decode` module uses `vec![0; n]` and treats the value as a hard output bound. The behavioural claim these comments make (under-run → `Ok` with a shorter buffer) is still correct, but the stated mechanism is the one this workspace does not build.

## Impact

Today: none at runtime — a dead guard plus two comments describing a module that is not compiled.

The exposure is that **nothing pins the feature**. A single `default-features = false` (or a `no_std` / size-motivated edit) on the one `lz4_flex` dependency silently swaps every Starfield v3 texture decode onto a raw-pointer decoder with no bounds check, on fully attacker-controlled archive bytes — and the in-tree defence that exists specifically to cover that scenario would not fire. Nothing in `cargo test`, clippy, or CI would flag the flip; the `lz4_decompress_is_panic_guarded` source-order pin would still pass.

## Suggested Fix

Pin the feature rather than the panic:

```toml
# Cargo.toml:140
lz4_flex = { version = "0.11", default-features = false, features = ["std", "safe-encode", "safe-decode", "frame"] }
```

(optionally adding `checked-decode` as belt-and-braces). Correct `ba2.rs:794` / `ba2.rs:1416` to say `vec![0; n]` + bounds-checked `SliceSink` rather than `Vec::with_capacity`. Keep the `catch_unwind` — it is cheap and still catches any residual panic on the safe path — but re-word its comment so it no longer claims to be the mitigation for the undersized-hint case.

## Related

Closes the loop on #2097 (closed); #2585 (SK-D5-LZ4-LOW-02, adjacent under-run signalling); the standing audit-hygiene rule "verify the audit premise against current code before proposing a fix".

## Completeness Checks
- [ ] **UNSAFE**: the fix removes reliance on an unsafe decoder path; no new `unsafe` should be needed
- [ ] **SIBLING**: check whether any other workspace dependency relies on an unpinned default feature for a memory-safety property
- [ ] **TESTS**: a build-level assertion or test pins that `safe-decode` is active (a source-order pin on `Cargo.toml`, or a `#[cfg]`-guarded compile error)
