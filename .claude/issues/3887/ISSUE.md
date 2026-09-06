# #3887: TD8-2026-09-05-04: `SkyParamsRes::texture_indices`'s `#[allow(dead_code)]` is stale — it has a production caller, and the 5-line justification above it is false

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,terrain-exterior,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3887 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/components.rs` (`SkyParamsRes::texture_indices`, allow at line 1305)
- **Status**: NEW (regression of the class closed as #1632 / #1633)
- **Effort**: trivial (≤30 min)

**Description**
The attribute reads:
```rust
#[allow(dead_code)] // release hook not yet built; #1199 is the change that made this worldspace-scoped, not an open gate
pub(crate) fn texture_indices(&self) -> [u32; 5] {
```
and the doc comment above it says *"The matching release will live in a future worldspace-transition hook (door-walking interior↔exterior)"*, with a #3455 sibling-sweep note instructing the reader to **"File one [a tracking issue] before treating this as land-ahead-of-consumer rather than plain dead code."**

The release hook exists. `byroredux/src/scene/world_setup.rs` calls it in production, inside `apply_worldspace_weather`'s prologue:
```rust
let prev_sky_textures = world
    .try_resource::<SkyParamsRes>()
    .map(|s| s.texture_indices());
```
— the `#1339 / #1770` acquire-new-then-release-old handoff on worldspace change, i.e. exactly the hook the doc says is unbuilt. The call is at `world_setup.rs:261`; the file's only `#[cfg(test)]` starts at line 1092, so it is production, not a test.

Rust does not warn about a redundant `#[allow]` by default, which is why this rots silently and why the same class recurs (#1632, #1633, #2981, #1761 — all CLOSED).

**Evidence**
```
$ grep -RIn "texture_indices" --include="*.rs" byroredux/src
  byroredux/src/components.rs:1306                 # definition (with the stale allow)
  byroredux/src/scene/world_setup.rs:196           # doc comment naming it
  byroredux/src/scene/world_setup.rs:261           # THE PRODUCTION CALL
  byroredux/src/cell_loader/sky_params_cleanup_tests.rs:8,10,11,41   # the guard test
$ grep -n "#\[cfg(test)\]" byroredux/src/scene/world_setup.rs
  1092                                             # → line 261 is production
```

**Impact**
A reader auditing sky-texture lifetime is told the release hook does not exist when it does, and is instructed to open a tracking issue for work already shipped. Every future Dim 8 sweep re-triages this attribute from scratch.

**Related**: #1632, #1633, #2981, #1761 (same class, all CLOSED), #1199 / #1339 / #1770 (the changes involved)

**Suggested Fix**
Delete the `#[allow(dead_code)]` and rewrite the doc comment to name `scene/world_setup.rs`'s `apply_worldspace_weather` prologue as the live consumer. Consider enabling `clippy::allow_attributes_without_reason` or a periodic `-W unused` sweep so this class stops recurring.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
