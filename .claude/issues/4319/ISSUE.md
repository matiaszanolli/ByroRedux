# #4319 SCR-D3-2026-09-14-01: `decompile_script` inverts named auto states — it hoists the auto state to script scope and demotes the real default state to a non-auto `State ''`, the reverse of Champollion (983 vanilla scripts)

**Labels**: medium,scripting,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM (a wrong AST on vanilla content across all three Papyrus games, silently, with no decline; not HIGH today because no production consumer changes outcome, see Impact; escalates to HIGH as soon as any state-aware consumer reads a `.pex` AST)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower (script assembly)
- **Untrusted-Input**: No
- **Location**: `crates/pex/src/decompile/lower.rs` `decompile_script` (the `for state in &object.states` loop: `if is_auto_state(object, state)` hoists; the else arm pushes `ScriptItem::State { is_auto: false }` hardcoded); `crates/pex/examples/pex_corpus_smoke.rs::expected_top_level_item_count` (encodes the same rule); `.claude/commands/audit-scripting/SKILL.md` Dim 3 assembly bullet (encodes the wrong premise)
- **Status**: NEW. #3786 and #3943 fixed this comparison's case-sensitivity, not what it compares.
- **Description**: In a `.pex` object the script-scope state is always the one named `""`. `auto_state_name` names the state the object *boots into*: `""` without an `Auto State`, `"X"` with `Auto State X`. `decompile_script` treats "the state whose name equals `auto_state_name`" as script scope, which is only right when that name is `""`. For `Auto State waiting`, three things go wrong:
  - `waiting`'s callables become top-level items;
  - the real `""` state becomes `State '' { is_auto: false }`;
  - no item ever carries `is_auto: true`.
- **Evidence**: The orchestrator read the Champollion reference, `Decompiler/PscCoder.cpp::writeStates` (lines 468–497): `if (name.empty()) writeFunctions(0, ...)`, else emit `State name`, prefixed `Auto ` when `caselessCompare(name, autoStateName) == 0`. The `.psc` frontend agrees (`defaultRumbleOnActivate.psc` → `State { is_auto: true }`). Dim 3 corpus census over 26,641 objects:

  | Game | named `Auto State` | …`State ''` holds user callables | …`on`-events | …auto overrides a same-named default |
  |---|---|---|---|---|
  | Skyrim SE | 570 | 299 | 206 | 36 |
  | FO4 | 276 | 133 | 92 | 25 |
  | Starfield | 137 | 74 | 38 | 24 |
  | **Total** | **983** | **506** | **336** | **85** |

  Concrete case: `defaultsetstagealiasscript.pex` (`auto_state_name='waitingForPlayer'`) decompiles to `State ''` {onActivate, testForTrigger, GetState, GotoState}, `State 'hasBeenTriggered'`, and 11 `waitingForPlayer` handlers at top level.
- **Impact**: No present-day divergence. `quest_stage_gate::find_advance_event`, `fragment/populate.rs::function_body`, `papyrus_provider/lower_program.rs` and `compatibility.rs` all flatten top-level and `State` items in body order, and none reads `is_auto` or a state name. The hazard is structural: the two frontends disagree on state topology for 983 vanilla scripts, the smoke harness cannot see it, and in the 85 override cases a future state-aware consumer would pick the wrong handler.
- **Related**: #3786, #3943; the stale `boolean.rs` departure-1 note ("the smoke harness discards the resulting `Script`", stale since #3017)
- **Suggested Fix**:
  - Route the `name.is_empty()` state to script scope, and emit each non-empty state as `State { is_auto: name.eq_ignore_ascii_case(auto_state_name) }`.
  - Update `expected_top_level_item_count` to the same rule.
  - Rewrite `mismatched_casing_auto_state_still_hoists_its_callables_to_script_scope`. It models a layout the corpus never contains (all 983 also have a `""` state) and pins the wrong behavior.
  - Add a `lower.rs` unit test (`""` {OnInit} + `Waiting` {OnActivate}) and a `.pex` ↔ `.psc` topology test on `defaultRumbleOnActivate`.
  - Correct the SKILL.md bullet.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
