# #5893: PAR-D1-2026-10-15-02: `inline_references` bounds the *count* of inlines (64) but not the resulting nesting depth

- **Labels**: medium,bug,animation
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: `inline_references` bounds the *count* of inlines (64) but not the resulting nesting depth. A self-referencing behavior nests about 64 × 33 machines, which every recursive runtime walker then descends.

- **Severity**: MEDIUM (a possible stack overflow from a mod file, not measured)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs:845-898`: `inline_machine` recurses into each newly inlined machine (`:887-889`), and `inline_references_with` recurses through `resolve`.
  - Recursive consumers of the result:
    - `remap_machine_events` / `remap_machine_variables` / `offset_machine_modifier_ids` / `machine_modifier_id_bound` (`:700-807`)
    - derived `Clone` / `Drop` / `PartialEq`
    - the runtime `MachineRun::enter` (`animation_graph.rs:254-269`), `dispatch` (`:861-882`), `schedule_from::walk` (`:779-815`) and `eval_machine`
- **Status**: NEW
- **Description**:
  - One decoded file nests at most 33 machines (`MAX_GENERATOR_DEPTH`).
  - Inlining pushes each referenced graph's root machine under the referencing state, then recurses into it. A graph whose deepest state references its own path is served from `actor_graph.rs`'s `decode_behavior` cache 64 times, giving a chain of about 2,100 nested machines.
  - Nothing caps that depth. The recursive walkers above run on the main thread (8 MB stack) every frame.
  - Separately, each inline interns its events and variables through linear `event_id` / `variable_id` scans (`:812-837`). That makes inlining O(E² · inlines): up to 64 graphs × 4096 events is about 3e10 string comparisons.
- **Evidence**: static. `budget` (`MAX_INLINED_REFERENCES`) is decremented per resolve and is the only bound. `MAX_REFERENCE_HOPS` applies only to `start_clip`. Stack use at about 2,100 levels was not measured.
- **Trigger Input**: a v11 behavior whose nested start-state chain ends in an `hkbBehaviorReferenceGenerator` naming its own file, or a 64-file chain.
- **Suggested Fix**: carry a total nesting-depth budget through `inline_machine`, so an inlined machine counts against the same 32-deep limit as decoded nesting. Intern events and variables through a `HashMap`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
