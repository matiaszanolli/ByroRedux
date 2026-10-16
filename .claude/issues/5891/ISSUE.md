# #5891: PAR-D1-2026-10-15-01: The behavior-graph reader follows shared object pointers with no visited set, so the generator, state-machine and modifier walks are exponential in depth on a DAG or a cycle

- **Labels**: high,bug,animation
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: The behavior-graph reader follows shared object pointers with no visited set, so the generator, state-machine and modifier walks are exponential in depth on a DAG or a cycle. A small behavior file can hang or exhaust memory on the main thread.

- **Severity**: HIGH (a hang or OOM reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs:1089-1217` (`read_generator`)
    - the blender children loop at `:1188-1203` re-enters `read_generator` per child pointer
    - the selector loop is at `:1164-1170`
    - the state-machine arm `:1119-1122` calls `read_machine_at` → `read_state` per state pointer (`:1363-1368`)
  - `:1226-1333` (`read_modifier`): the `hkbModifierList` loop at `:1321-1327` and the `child(...)` arms
- **Status**: NEW. There is no open or closed issue about a visited set or DAG blowup in behavior decoding.
- **Description**:
  - The caps bound each array (`MAX_STATES` 1024 entries) and the recursion depth (`MAX_GENERATOR_DEPTH` 32, `MAX_MODIFIER_DEPTH` 16). Nothing bounds the total number of nodes visited.
  - Every pointer is followed again each time it is reached, and each visit builds a fresh owned subtree (`Generator`, `BlendChild`, `Arc<ModifierNode>`).
  - Example: a blender whose two child entries both point back at itself expands into roughly 2^32 nodes before the depth cap returns `None`.
  - Example: a modifier list with K entries pointing at itself costs K^17 calls. At K = 3 that is about 1.3e8 allocations; at K = 1024 it never completes.
  - Such a file is a few kilobytes. All three decode call sites are on the main thread with no time or size budget: `object_graph.rs:63`, `actor_graph.rs:99` and `archetype_idle.rs:173`, reached from `app_step.rs:93/1032/1044`, `cell_loader/load.rs:731-733` and `scene/world_setup.rs:1135-1137`.
  - `Packfile::class_at` is a linear scan over all objects (`packfile.rs:282-287`) on every visit, which multiplies the cost further.
- **Evidence**:
  - Static: there is no `HashSet` or memo in `read_generator`, `read_machine_at` or `read_modifier`. `depth` is the only bound.
  - Vanilla is unaffected: all 751 vanilla graphs decode, the slowest in 35 ms (probe).
- **Trigger Input**: an `hkbBlenderGenerator` / `hkbManualSelectorGenerator` / `hkbStateMachine` / `hkbModifierList` whose child pointers (global fixups) target an ancestor or a shared descendant, with ≥ 2 such pointers per level.
- **Impact**: one mod behavior file named by a placed object's NIF or by an FO4 RACE subgraph freezes the engine at cell load, or aborts it on OOM.
- **Suggested Fix**:
  - Memoise on object offset per decode. A `HashMap<usize, Generator>` / `HashMap<usize, Arc<ModifierNode>>` turns the walk into O(objects); `Arc` already lets modifier nodes be shared.
  - Alternatively, carry an on-stack set that rejects a cycle and a global visit budget (e.g. 64 K nodes per file) that returns `InvalidData`.
  - Add a fixture: a self-referencing blender and a self-referencing modifier list must decode or fail in bounded time.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
