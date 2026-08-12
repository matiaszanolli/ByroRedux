# #2669: SCR-D5-NEW11-04: two_state_activator::vmad_bool silently falls back to the script default on a present-but-non-Bool VMAD value

**Severity**: LOW
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/translate/recognizers/two_state_activator.rs:20-28` (consumption) and `:66-72` (`vmad_bool`)
**Status**: NEW

## Description

`vmad_bool` returns `Option<bool>` and collapses two distinct cases into the same `None`: "no such property" and "property present but its `PropertyValue` is not `Bool`". The caller then turns that `None` into the `.psc`-authored default via `.or(bool_prop(script, ...)?).unwrap_or(false)`.

That is exactly the two-case collapse #2023 fixed for `bool_arg` and #1909 fixed for `rumble::bool_prop`: a property whose real value is present but unreadable must **decline**, not silently adopt the authored default.

The recognizer is otherwise disciplined -- `bool_prop` correctly returns `None` (making the whole recognizer decline via `?`) on a non-literal property initializer.

## Evidence

`crates/scripting/src/translate/recognizers/two_state_activator.rs:66-72`:

```rust
match property.value { PropertyValue::Bool(v) => Some(v), _ => None }
```

consumed at `:20-28` as:

```rust
vmad_bool(ctx, "isOpen").or(bool_prop(script, "isOpen")?).unwrap_or(false)
```

The three-case `Option<Option<bool>>` contract mirroring `bool_arg` (`crates/scripting/src/translate/effects.rs:1098-1103`) is the established pattern in this crate.

## Impact

A `default2StateActivator` instance whose VMAD carries `isOpen` / `isAnimating` / `doOnce` under a non-`Bool` type tag spawns with the script default instead of declining -- a door or gate that starts in the wrong state, with no diagnostic.

Low likelihood (the Creation Kit writes bool properties as type 5) and a small blast radius, hence LOW -- but it is a decline-invariant deviation in a recognizer, which is the pattern this domain treats as load-bearing.

## Related

#2023 and #1909 (the same two-case collapse, already fixed in `bool_arg` and `rumble::bool_prop`); #2289 (decline-path test coverage)

## Suggested Fix

Give `vmad_bool` the `Option<Option<bool>>` three-case contract used by `bool_arg` -- `None` for absent, `Some(None)` for present-but-unreadable, `Some(Some(v))` for a real value -- and propagate the present-but-unreadable case as a recognizer decline.

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: The recognizer still declines on every unmodeled term -- a partial lowering is worse than none
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
