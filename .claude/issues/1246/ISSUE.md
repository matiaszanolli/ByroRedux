# NIF-D6-NEW-02: read_*_array wrappers + KFM allocate_vec lack #[must_use]

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1246

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 6)
**Severity**: LOW (latent-regression guard)
**Dimension**: Allocation Hygiene

## Description

`read_pod_vec` (the private workhorse at `crates/nif/src/stream.rs:302`) and the 11 public typed wrappers at `stream.rs:337-417` all return an `io::Result<Vec<T>>` containing a populated Vec. The original #831 argument applies symmetrically: a caller that writes `stream.read_u32_array(n)?;` (no binding) silently runs the bulk `read_exact` *and* drops the Vec — that's a `(read_count × sizeof::<T>())`-byte zero-init + read + drop, which is strictly worse than the `allocate_vec` misuse (which would only allocate the empty Vec).

The 13 candidate non-binding sites surfaced by grep all turned out to be expression-position uses (block-yielding `if has_x { stream.read_u16_array(…)? } else { Vec::new() }` shape) — so no current call site triggers the misuse. **The pin holds empirically but is undefended structurally.**

Similarly, KFM's local `allocate_vec` at `crates/nif/src/kfm.rs:686` mirrors `NifStream::allocate_vec` (same bound, same `Vec::with_capacity` return) but doesn't carry the attribute. All 7 KFM call sites bind, so again no current trigger.

## Impact

Latent. A future block parser added for an unfamiliar contributor could write `stream.read_f32_array(count)?;` for its "side effect" of advancing the cursor (since `read_pod_vec` *does* call `read_exact`, the cursor advances even when the Vec is dropped) — perfectly compilable, silently wasteful.

## Suggested Fix

Annotate `read_pod_vec` with:

```rust
#[must_use = "read_pod_vec returns a populated Vec; bind it or use stream.skip() to advance the cursor without reading"]
pub(crate) fn read_pod_vec<T: …>(…) -> io::Result<Vec<T>> { … }
```

Since it's `pub(crate)`, this propagates the lint to call sites of every wrapper that just `self.read_pod_vec::<T>(count)` returns the call. Annotate `kfm.rs:686` similarly. Add a `// must_use propagates from read_pod_vec` comment on each wrapper for future readers.

## Related

- #831 (CLOSED): `#[must_use]` on `allocate_vec` — same architectural intent, only `allocate_vec` was annotated at the time
- #833 (CLOSED): `read_pod_vec` collapse — added the wrappers without the lint-equivalent

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: verify no non-binding call sites exist today by re-running the grep; the annotation should fire zero warnings if all 13 candidates are truly expression-position
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: `cargo check -p byroredux-nif --tests` should be warning-clean post-fix; if not, the offending call site needs a `let _ = …` annotation that documents why the discard is intentional