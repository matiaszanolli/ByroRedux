# #7218 CHAR-2026-11-21-L-21: About a dozen code comments still name the closed #3768 "pre-AVIF resolver" or say Oblivion has no AVIFs, no ruleset or no shot/guard producer

**Labels**: low,character,documentation,doc-rot,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine (code comments describing an intended insertion site)
- **Game**: oblivion
- **Location**:
  - **Citing #3768 or a pre-AVIF resolver**:
    - `byroredux/src/boot/world.rs:56-59`
    - `byroredux/src/boot/schedule/update/player_animation/player_stats.rs:37-44` (which also says every non-Fallout game never inserts the regen config)
    - `crates/core/src/character/regen.rs` `PoolRegenConfig::fatigue_avif` doc ("all of today's loads")
    - `crates/core/src/character/profile.rs:271` (`vital_pools`, "Oblivion pre-#3768")
    - `byroredux/src/inventory/catalog.rs:263` (`build_player_vitals`)
    - `crates/core/src/character/tes.rs:40-44` ("revisit with #3768") and `:109-112` ("Pre-AVIF Oblivion supplies a resolver … at the parser boundary")
  - **"Oblivion has no AVIFs" or no ruleset**:
    - `crates/plugin/src/esm/records/actor_value_derive.rs:165` ("Empty for every other game (Oblivion)")
    - `crates/scripting/src/actor_value_names.rs:270-272`
    - `crates/scripting/src/condition/eval.rs:107`
    - `crates/scripting/src/condition/catalog.rs:34`
    - `crates/plugin/src/esm/records/tests.rs:166-179` (Oblivion "carries `RulesetBuilder::None`, confirmed unreachable")
  - **Behaviour descriptions**:
    - `crates/core/src/character/regen.rs:81-82` (`magicka_regen_per_sec`: "always `false` from the tick system today")
    - `byroredux/src/skill_use_emit.rs:20-21` (module header: "a bow has no shot path here … a blocked hit feeds nothing"; the same file feeds Marksman and Block)
    - `crates/scripting/src/oblivion_skill_use.rs:497-498` (`skill_use_report`: pending levels and unfed skills "arrive with the phases that build them"; both are reported now)
- **Status**: NEW (merges CHAR-D5-04, items a-c of CHAR-D4-08, items 2, 4 and the code part of 5 of CHAR-D3-05, and the code prose of CHAR-D1-02)
- **Description**: Each comment states a precondition that no longer holds:
  - `build_tes_pool_regen` installs the Oblivion lanes.
  - The engine-supplied 72-record roster resolves Oblivion's AVIF ids (`EsmIndex::actor_value_form_id`, `condition_actor_value_form` via `vanilla_name` + `ActorValueNames`).
  - `derive_resolved_actor_values_with_player_level` routes Oblivion to `oblivion_population::derive`.
  - The emitters feed Marksman (#7055) and Block (#7054).

  #3768 is CLOSED and was a doc-claim issue rather than a resolver tracker, so it points at nothing actionable.
- **Impact**: a reader of the scheduler registration or the condition evaluator is told Oblivion's regen is inert and its actor values unreadable.
- **Related**: L-19, L-20, #4460 (closed, an earlier wiring event's twin).
- **Suggested Fix**: reword each comment to the live mechanism. Add a source-text pin like `mod_docstring_makes_no_unversioned_unreachable_claims` that forbids `pre-AVIF resolver` and `#3768` outside `docs/audits/`.

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
