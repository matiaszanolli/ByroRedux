# SCR-D1-2026-10-17-12: Doc rot from the #5801 split and M47.2 additions

Issue: #5980
Labels: low, scripting, documentation, doc-rot

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-12: Doc rot from the #5801 split and M47.2 additions

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `translate/effects/model.rs:895-914,914-1012`; `prims/world.rs:8-11`; `prims/placement.rs:36-39`; `lower.rs:831-837`; `effects.rs:19-26`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `is_placeholder`'s doc block now sits on `kind()` (the #5807 `kind` doc was spliced under it, `is_placeholder` has none) and the `kind()` match arms are unindented (not rustfmt'd); `prim_add_to_map`'s doc starts with a stale paragraph about `<actor>.SetValue(akAV, afValue)`; `prim_move_to`'s doc repeats the `TryToMoveTo` comment twice; the `lower_3d_loaded_wait` doc paragraph is attached to `lower_wait_while`; the hub doc still says "Scope (this increment) ... conservative set".
- **Evidence**: files above.
- **Impact**: Misleading docs only.
- **Related**: #5801, #5807.
- **Suggested Fix**: Re-attach the docs to the right items and run rustfmt on `model.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
