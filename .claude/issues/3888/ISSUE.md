# #3888: TD8-2026-09-05-05: `ActionState::was_released`'s `#[cfg_attr(not(test), allow(dead_code))]` is stale — `extensions.rs` calls it in production

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3888 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/interaction.rs` (`ActionState::was_released`, allow at line 691)
- **Status**: NEW — regression of **#2981** (`ActionState::is_held`'s test-only allow was redundant; CLOSED), on the sibling method
- **Effort**: trivial (≤30 min)

**Description**
`was_released` sits between `is_held` and `was_pressed`, both of which carry no allow. It gained a production consumer when the SDK/mod-runtime event adapter landed: `byroredux/src/extensions.rs` uses it inside the `InputAction::OBSERVABLE` fan-out that builds `InputActionEvent`s for sandboxed mods:

```rust
let phase = if state.was_pressed(action) {
    InputPhase::Pressed
} else if state.was_released(action) {      // ← extensions.rs:4643, production
    InputPhase::Released
} else { return None; };
```

`extensions.rs`'s `#[cfg(test)]` block spans lines 5930–10652 (verified by brace-depth walk), so line 4643 is production code. The attribute is now a no-op that misdocuments the method as test-only.

**Evidence**
```
$ grep -RIn "was_released" --include="*.rs" byroredux crates tools
  byroredux/src/interaction.rs:692         # definition
  byroredux/src/interaction.rs:1189,1222,1248,1397   # tests (file's cfg(test))
  byroredux/src/commands_tests.rs:118      # test
  byroredux/src/extensions.rs:4643         # PRODUCTION  (cfg(test) mod starts at 5930)
```
Counter-check on its neighbour, so the finding is not over-broad: `ActionBindings::bind_key` (`interaction.rs:180`) carries the same attribute and its only non-`interaction.rs` caller, `extensions.rs:9451`, **is** inside the `cfg(test)` span — that attribute is correct and must stay.

**Impact**
Cosmetic in isolation, but this is the fourth recurrence of the same class in this file's neighbourhood (#2732, #2981, #1632, #1633). Each one costs a future auditor a full call-site trace to disprove.

**Related**: #2981 (the `is_held` twin, CLOSED), #2732 (four allows added to `interaction.rs`, CLOSED), TD8-2026-09-05-04

**Suggested Fix**
Delete the `#[cfg_attr(not(test), allow(dead_code))]` on `was_released`. Leave `bind_key`'s in place.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
