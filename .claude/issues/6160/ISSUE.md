# TD2-2026-10-19-15: Small same-file twins on new code (each trivial; grouped to keep the report short)

Labels: low,scripting,esm-plugin,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location / fix** (each names its consolidation):
  - `crates/plugin/src/esm/records/misc/challenge.rs:97-98`, `music.rs:41`, `cell/wrld.rs:127-152`: hand-indexed `u32/f32::from_le_bytes([data[0], ..])` reads where `SubReader` (`esm/sub_reader.rs`) or `read_u32_sub` / `read_f32_sub` (`records/common.rs:287,295`) exist; `challenge.rs` SNAM/XNAM want a `read_form_sub(subs, code, remap)` helper next to them (14 raw reads remain in `records/`, 3 are new).
  - `crates/scripting/src/obscript_quests.rs:490,863`: two `let num = |i| args.get(i).map(..).unwrap_or(0.0)` closures that are `obscript_effects::num` (`pub(crate)`, `obscript_effects.rs:291`); and four `obscript_actions::Context { caller, known, action_ref, owner, elapsed }` literals (`:593,737,759,812`) for a `fn context(&self, caller) -> Context` on `ObScriptCommandHost`.
  - `byroredux/src/cell_loader/references/synth_child.rs:445-470` vs `:527-552`: the stamp / attach-script / count / `spawn_logical_quest_reference` tail, kept in step by a "same split as the LIGH-only branch above" comment (#3016); extract `fn finish_synth_child(...)`.
  - `crates/scripting/src/dialogue.rs:592-602` vs `package.rs:670-680`: the `ScenePlayer -> scene_form_ids` map; extract `scene::scene_form_ids(world)`.
  - `crates/audio/src/lib.rs:698-715` vs `:728-745`: `play_oneshot` / `play_oneshot_following` differ only in `SoundSource`; `fn queue_oneshot(&mut self, source, ...)`.
  - `crates/nif/src/blocks/collision/constraints.rs:266-313`: `LimitedHingeCInfo::parse_fo3` and `parse_hinge_fo3` read the same eight Vec4s; share `read_hinge_axes(stream) -> HingeAxes`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
