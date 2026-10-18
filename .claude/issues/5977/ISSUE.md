# SCR-D1-2026-10-17-09: The arity-bound guard is a substring check that a lower bound (or a comment) satisfies

Issue: #5977
Labels: low, scripting, bug, test-gap

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-09: The arity-bound guard is a substring check that a lower bound (or a comment) satisfies

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/tests.rs:4381-4418`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `every_effect_primitive_bounds_its_argument_count` passes a primitive if its body contains `args.len()` / `args.is_empty()` anywhere (an `args.len() >= 1` or `args.is_empty() || ..` lower-bound-only check qualifies) or if it contains the text `prim_` (a doc/comment mention counts as "delegates"). The completeness margin `seen >= EFFECT_PRIMITIVES.len()` is slack by the 3 non-table helpers (`prim_player_controls`, `prim_player_animation_event`, `prim_race_spells_arity_ok`: 88 bodies vs 85 rows). Today every primitive does have a real upper bound (verified manually), so this is a latent weakening of the #3496 gate, not a live hole.
- **Evidence**: test body above; manual sweep in "Verified-clean".
- **Impact**: A future primitive could pass the gate while still lowering an over-arity call.
- **Related**: #3496, #5801.
- **Suggested Fix**: Count only bodies that compare `args.len()` with `!=`/`>`/`<=`/range against a literal or named bound (or compare against a per-primitive max table), and exclude helper fns from `seen` by name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
