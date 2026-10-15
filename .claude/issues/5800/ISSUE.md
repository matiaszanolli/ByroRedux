# TD1-2026-10-15-01: `crates/scripting/src/fragment/vm.rs` is 6213 production LOC in one file, created three days ago, with a 1259-line `object_read` and a 773-line `check_call`

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-01`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/vm.rs` (6215 lines, tests already external in `fragment/vm/tests.rs`, 5631 lines)
- **Status**: NEW
- **Age**: file added e766d3147 (2026-10-13, "the Papyrus VM runs quest fragments the lowering declines"); 77 commits touch it in three days (`852c61078`, `20acec55d`, `141dbbb90`, `a0045b0cf`, `8b3b75d4d`, `5320d4ac9`, …)
- **Effort**: large (decompose first)
- **Description**: three responsibilities sit in contiguous runs, none of them cut by a module boundary:
  1. values + marshalling (L93–350, L694–760, L4320–4403): `Value`, `Stop`, `Frame`, `store`/`restore`, `cast`, `arithmetic`, `binary`;
  2. script-model lookup (L497–950): `Callable`, `callable_in`, `script_chain`, `resolve_in`, `auto_state`, member typing;
  3. the interpreter `impl Vm<'_>` (L1028–4168, **3140 lines, ~95 methods**): registrations (`quest_update`, `event_registration`, `distance_registration`, `hit_registration`, L1038–1275), frame/exec/eval/assign (L1323–2260), collection/array/layer natives (L1717–2350), **`object_read` (L2483–3741, 1259 lines, 190 match arms)** and `call_statement` (L3938, 227 lines);
  4. the static pre-pass `Checker` (L4404–6095, ~1700 lines) whose **`check_call` is 773 lines / 109 arms** and `check_native` another 81.
  `object_read` is two blocks: a `static_class` match for `Game.*` / `Utility.*` / `Weather.*` (L2513–2660) and an instance-method `match (&owner, name)` (L2761–3740, ~100 arms) that interleaves unrelated Papyrus API families (animation events, crime gold, bribe/favor, destruction, linked-ref chains, map markers, owner changes, form lists). Each arm is 5–40 lines and calls into a subsystem module that already exists (`crime_gold.rs`, `bribe.rs`, `destruction.rs`, `linked_refs.rs`, `animation_graph.rs`, `map_markers.rs`, `ownership.rs`).
- **Evidence**: `prod_loc` 6213; `fnscan.py` rows `object_read 1259 / depth 6 / 190 arms`, `check_call 773 / 109 arms`, `call_statement 227`. `awk` over L2761–3750 lists the arm heads (`(Value::Form(actor), "getrace")`, `"setbribed"`, `"damageobject"`, `"getlinkedrefchain"`, …).
- **Impact**: the highest-churn file in the repo and every new native edits it in two places that must agree — the runtime read arm in `object_read` and the "can the VM run this" arm in `check_call`/`check_native`. A native with a read arm but no check arm leaves a fragment installed as `Effect::Interpret` that stops mid-run; the reverse leaves a fragment declined that could run. Nothing ties the two halves together today.
- **Related**: #5468 / #5467 (the split-by-family precedent landed 10-14: `fragment/effects/{inventory,lock,…}.rs`); finding 02 and 03 (same wave).
- **Suggested Fix**: split along the one axis both halves share — the Papyrus API family — not along runtime-vs-checker. `fragment/vm/{value.rs, script_model.rs, interp.rs (run_fragment/exec/eval/assign/frames), registrations.rs, checker.rs (Checker core: kind/static_type/check_block/check_stmt)}` plus `fragment/vm/natives/{game_utility.rs, alias.rs, form_list.rs, actor.rs, reference.rs, animation.rs, ownership_crime.rs, destruction.rs, linked_refs.rs, map_stats.rs}` where each family file holds both `read_<family>(&mut Vm, ..)` and `check_<family>(&mut Checker, ..)`, chained from `object_read` / `check_call` as `or_else`-style dispatch (order matters only for the first-match arms; pin it with the existing `vm/tests.rs`). Hazard: `fragment.rs` `SOURCES` (`concat!(include_str!(…))`, L109–137) and its `sources_completeness` test must list every new file (#3854 pattern), and `byroredux/src/boot/schedule/mod.rs:626` reads `fragment/effects/*` the same way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
