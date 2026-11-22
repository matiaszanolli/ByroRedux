# #7094 Doc rot: FED_SKILLS comment says Block and fall Acrobatics are unfed; door-runtime.md says door-runtime.sh is not written

Labels: low, doc-rot · State: OPEN

## Problem
Two comments describe work as unbuilt after it shipped.

1. **`crates/scripting/src/oblivion_skill_use.rs:468-470`.** The `FED_SKILLS` doc comment says: "Block waits on a target-side guard (no producer models one: `HitEvent::blocked` is the aggressor's own stance, #4324), and Acrobatics' fall on fall damage."
   - The list right below it contains 15 (Block), and `byroredux/src/skill_use_emit.rs` emits Block uses (#7054, `182a33e2d`).
   - Acrobatics is fed by a damaging fall (#7056, `787cdf19c`).
   - The comment calls itself "the one list behind `levelup`'s unfed report", so a reader trusting the prose undercounts the fed skills.
2. **`docs/engine/door-runtime.md:85`.** The §5 table's last row reads "`docs/smoke-tests/door-runtime.sh` | not written". The script exists: it was added in `180080de7`, gained its keyed-door and FNV legs in `0be04d94d`, and the same doc's DRT-DX-001 row records it passing.

## Suggested fix
- Rewrite the `FED_SKILLS` sentence to say that Block is fed by a blocked hit on the player (#7054) and Acrobatics by both a jump and a damaging fall (#7056).
- Update the door-runtime.md row to point at the script and its legs (`oblivion`, `fnv`).

Found by `/audit-sync` (2026-11-20).

