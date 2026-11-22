# #7235: GAME-2026-11-22-12: `CastState.interval` is written on every cast and never read

**Labels**: low,gameplay,combat,tech-debt,bug,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-12**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 7 (written-never-read)
- **Domain**: combat · **Game**: Oblivion
- **Trigger**: Any Oblivion player cast.
- **Location**: `crates/scripting/src/player_cast.rs:184-185,424`; `byroredux/src/commands/spell.rs:139`
- **Status**: NEW
- **Description**: The field's doc says "The interval the last cast armed, for the report". But `cast.status` prints the constant `CAST_INTERVAL_SECONDS`, and nothing else reads the field.
- **Evidence**: `rg '\.interval'` over `byroredux/src` and `crates/scripting/src` finds only the write.
- **Impact**: A dead field. `cast.status` would report the wrong value once the interval stops being constant.
- **Related**: PSC-DX-001
- **Suggested Fix**: Print `state.interval` in `cast.status`, falling back to the constant before the first cast, or drop the field.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
