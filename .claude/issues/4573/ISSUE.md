# ECS-2026-09-21-D5-01: `every_parallel_system_declares_everything_it_acquires` ignores read vs write and matches names by substring

**Labels**: low, ecs, concurrency, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_ECS_2026-09-21.md.

**Severity**: LOW (test gap; the live schedule is clean) · **Dimension**: 5 (declared-access guard)
**Location**: `byroredux/src/boot/schedule/mod.rs`: `acquired_in` (~:616-640, where `types.push(short)` at ~:638 records the short type name only) and `assert_declares_everything_it_acquires` (~:702, where `declared.contains(*ty)` runs over the raw block text returned by `declaration`)
**Verified against**: HEAD `f97775ca8`

## Description

`every_parallel_system_declares_everything_it_acquires` is the mechanical backing for the boot RELEASE assertion `known_conflict_count() == 0` in `install_runtime_registries`. It compares each parallel system's acquisitions against its `add_to_with_access(...)` registration block, and it has three blind spots:

- **Mode.** `acquired_in` records type names with no read/write mode, and the comparison only checks that the name appears somewhere in the registration block. So a parallel system that takes `query_mut::<T>` or `resource_mut::<T>` while declaring only `.reads::<T>()` passes. `analyze_pair` then treats a real write as a read, and a reader in the same stage pairs with it as non-conflicting.
- **Substring.** `declared.contains(ty)` is a plain `str::contains`. An acquisition of `Transform` is satisfied by a block that declares only `GlobalTransform`, and the same holds for any type name that appears inside another declared name.
- **Comments.** `declaration()` returns the whole balanced-paren block, comment lines included. Take the skill's own precedent, `ac1d44f5c`: if `early.rs` dropped only `.writes::<GlobalTransform>()` from the `fly_camera_system` registration, the adjacent comment "`fly_camera_system` publishes its pose to `GlobalTransform`" would keep the test green.

The known forms gap is already documented in the audit-ecs skill: `world.get::<T>` / `get_mut` / `has`, `query_2_mut`, `resource_2_mut`, and acquisitions with no turbofish. These three blind spots are not.

## Evidence

```rust
// acquired_in — no mode recorded
let short = path.rsplit("::").next().unwrap_or(path).to_owned();
if !types.contains(&short) {
    types.push(short);
}

// assert_declares_everything_it_acquires — substring test over raw block text, comments included
let declared = declaration(system);
let missing: Vec<&String> = types.iter().filter(|ty| !declared.contains(*ty)).collect();
```

The audit also ran its own mode-aware version of the check. That version matches exact names, compares read/write, covers the `get`/`has`/`query_2_mut`/`resource_2_mut` forms, and follows same-file callees to depth 3. It finds **0 undeclared and 0 write-declared-as-read** acquisitions across all 9 parallel systems at HEAD.

## Impact

Nothing breaks today. But the next write-for-read slip in a parallel body will ship green, and the boot soundness proof is then silently void.

## Related

- #4064 (closed) was about which systems the table covers, not how the comparison works. That fix is still in place.
- #4404 (closed) was the same comment/substring blind spot in the NIFAL particle completeness guards.
- The same helper backs `papyrus_provider_system_declares_everything_it_acquires` and `legacy_obscript_load_order_system_declares_everything_it_acquires` (#3951), so this fix tightens those tests as well.
- ECS-2026-09-21-D5-02 (#4574) would extend this helper to the P2 exclusives, so fix these holes first.
- ECS-2026-09-21-D2-01 (#4575): the audit-ecs skill's "What the guard cannot see" list omits these three blind spots.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

1. Make `acquired_in` return `(type, mode)`.
2. Strip comments from the registration block, then parse its `.reads` / `.writes` / `.reads_resource` / `.writes_resource::<…>` calls into exact short-name read and write sets.
3. Require every write-mode acquisition to appear in the write set, and every read-mode acquisition to appear in either set.
4. Add negative self-tests, each of which must fail:
   - a body that writes `T` against a block that only reads `T`;
   - a body that acquires `Transform` against a block that only declares `GlobalTransform`;
   - a block whose only mention of the type is in a comment.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D5-01)

## Completeness Checks
- [ ] **SIBLING**: The two exclusive completeness tests that share `assert_declares_everything_it_acquires` (#3951) still pass under the stricter comparison.
- [ ] **TESTS**: Negative self-tests pin all three blind spots (mode, substring, comment text) so the guard cannot silently regress to name-presence matching.
