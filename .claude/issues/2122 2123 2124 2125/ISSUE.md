# Batch fix: #2122, #2123, #2124, #2125

All four found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md` and filed via `/audit-publish`.

## #2122 — SCR-D2-NEW3-01 (HIGH): build_cfg attaches JmpF/JmpT condition/edges to a stale block key on a backward interior jump target
**Location**: `crates/pex/src/decompile/cfg.rs:213-243` (the `OpCode::JmpF | OpCode::JmpT` arm of `build_cfg`)

`block_key` is computed once before either of the instruction's two `split_block` calls run.
If the jump `target` is backward and lands strictly between the block's `begin` and `ip`, the
target-split subdivides the same block a second time, and `block_key` ends up stale — pointing
at the leftover head piece instead of the tail piece containing `ip`. `condition`/`next`/
`on_false` get written onto the wrong block; the real conditional block is left `condition: None`,
silently dropping its `on_false` (loop-back) edge. The sibling `Jmp` arm avoids this by setting
`next` before the second split. Not reachable by real Bethesda-compiled `.pex` (compiler always
emits forward jmpf/jmpt), but a hardening gap on the untrusted VMAD-attach path.

**Fix**: re-resolve the block containing `ip` via `find_block_for_instruction` after both splits
settle, and write the condition/edges onto that resolved key instead of the stale one.

## #2123 — SCR-D6-NEW3-01 (MEDIUM): RunOn::Reference conditions always evaluate false
**Location**: `crates/scripting/src/condition.rs:258-268` (`ConditionContext::resolve`, `RunOn::Reference` arm)

The `RunOn::Reference` arm unconditionally returns `None` with a stale comment claiming the
resolver isn't wired — but `resolve_entity_by_global_form_id` already exists in the same file
and is already used by the `GetDistance` arm for the identical FormID remap space. Every CTDA
condition authored "Run on: Reference" silently and permanently evaluates false.

**Fix**: `RunOn::Reference => resolve_entity_by_global_form_id(_world, condition.reference_form_id)`.

## #2124 — SCR-D6-NEW3-02 (MEDIUM): Quest-fragment cascade genuine-transition guard compares against the wrong variable
**Location**: `crates/scripting/src/fragment.rs:488-495` (`quest_fragment_dispatch_system`, cascade loop)

`if adv.new_stage != stage` compares against the currently-dispatching fragment's own stage,
not `adv.previous_stage` (the actual pre-image on the event) and not scoped to `adv.quest ==
quest`. Can silently drop a different quest's genuine cascade (stage-number coincidence) or
duplicate-apply a stage's effects (e.g. duplicate AddItem) when one fragment converges two
effects on the same value.

**Fix**: replace `adv.new_stage != stage` with `adv.previous_stage != adv.new_stage`.

## #2125 — SCR-D4-NEW3-01 (MEDIUM): A parser-level error in one function inside a State/Group/Struct discards the entire container
**Location**: `crates/papyrus/src/parser/script.rs:509-551` (`parse_state`), `:556-576` (`parse_struct`), `:579-619` (`parse_group`)

Each parses its children with a bare `?` and no per-item catch; recovery only happens at
`parse_script`'s top-level loop. A syntax error in one function inside a `State` block discards
the entire `ScriptItem::State`, including sibling functions with zero errors.

**Fix**: give `parse_state`/`parse_group`/`parse_struct` their own per-child recovery loop
mirroring the top-level one in `parse_script` (same fix shape as #1734/SCR-D4-02, one level
deeper).
