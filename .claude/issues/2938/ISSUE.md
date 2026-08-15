# CHAR-D2-04: DerivedInput::actor_value's "never 0" caller guarantee is unenforced; Some(0) collapses into UNUSED

- **Issue**: [#2938](https://github.com/matiaszanolli/ByroRedux/issues/2938)
- **Finding ID**: `CHAR-D2-04`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2938 --json state`.

---

- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: all
- **Location**: `crates/core/src/character/derived.rs` (`DerivedInput::actor_value`, `DerivedInput::UNUSED`) · `byroredux/src/npc_spawn.rs` (`build_character_ruleset`) · `crates/plugin/src/esm/records/index.rs` (`actor_value_form_id`) · `crates/core/src/character/tes.rs` (test `oblivion_ruleset_skips_unresolved_pools`)
- **Status**: NEW
- **Description**: `DerivedInput` packs "unused" into the value `0`, documented as a **caller** guarantee: *"(Caller guarantees the id is neither `0` nor `u32::MAX` — real Bethesda FormIDs never are.)"* The skill asks to check the callers, not the constructor. The single production construction site is `build_character_ruleset`, whose resolver is `|editor_id| index.actor_value_form_id(editor_id)`; `actor_value_form_id` returns `.map(|avif| avif.form_id)` with **no non-zero filter**. A `Some(0)` therefore flows into `DerivedInput::actor_value(0)`, which compares equal to `UNUSED`, and `read()` returns `0.0` — the coefficient is silently dropped and the formula still registers, producing a wrong value rather than the resolve-or-**skip** degradation the builders promise everywhere else. The `u32::MAX` half is genuinely unreachable (index `0xFF` is reserved), so only the `0` half is exposed. That the invariant is undefended is already demonstrated in-repo: `tes.rs`'s `oblivion_ruleset_skips_unresolved_pools` resolver maps `"Strength" => 0x00`, while its sibling test `oblivion_ruleset_assembles_and_evaluates_end_to_end` carries the comment *"Non-zero ids throughout: FormID 0 is the null form and also `DerivedInput::UNUSED`, so a real AV never resolves to it."* Two tests in one file, opposite assumptions.
- **Evidence**: `derived.rs` `read()`: `match self.0 { 0 => 0.0, u32::MAX => f32::from(level), … }` — a `0` input is indistinguishable from `UNUSED` at evaluation time. `index.rs` `actor_value_form_id` has no guard. No `debug_assert` exists in `actor_value`.
- **Impact**: Requires an AVIF whose (remapped) FormID is `0` — not observed in vanilla data, so this is defence-in-depth rather than a live defect. If it ever happens the failure is silent and per-stat: e.g. Carry Weight would evaluate to its bare bias (150/200) for every actor, with no warning.
- **Related**: Dim 1 owns whether these ids are remapped to global space; Dim 5 owns the population path.
- **Suggested Fix**: Make the guarantee enforceable rather than documented — have `build_character_ruleset`'s resolver filter `Some(0) → None` (turning the collision into the existing resolve-or-skip path), and add a `debug_assert!(avif_form_id != 0 && avif_form_id != u32::MAX)` in `DerivedInput::actor_value`. Fix the `tes.rs` test resolver's `0x00` while there.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
