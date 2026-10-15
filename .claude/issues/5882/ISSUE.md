# #5882: PEX-D3-2026-10-15-05: #5685 maps property `Const`/`Conditional` from user-flag table entries that vanilla files never contain

- **Labels**: low,bug,scripting,game:fo4,game:starfield
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: #5685 maps property `Const`/`Conditional` from user-flag table entries that vanilla files never contain. The real bits sit on the auto property's backing variable, so 22.5 k FO4 / 24.5 k Starfield `Auto Const` and 3 k `Auto Conditional` properties still lower without those flags

- **Severity**: LOW (no consumer keys on `PropertyFlags` yet, per f7afb0e07's own message and a grep of scripting/save/bin; this is the residual of the baseline's LOW PEX-D3-2026-10-12-02)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:436-451` (`lower_property` user-flag loop), `:533-536` (`"const"` arm of `UserFlagMasks::from_pex`); test `property_user_flags_lower_into_property_flags` (`:749`)
- **Status**: NEW (incomplete fix of #5685)
- **Description**: The loop tests only `prop.user_flags`, and it resolves `CONST` through a user-flag name `"const"`. In vanilla data, every user-flag table is exactly `hidden=0, conditional=1` (Skyrim) or adds `default=2, collapsedonref=3, collapsedonbase=4, mandatory=5` (FO4; FO76 also has `transactional=6`). No table names `const`. `Auto Const` is encoded as the backing variable's `const_flag`, and `Auto Conditional` as the backing variable's conditional user bit. Champollion `PscCoder::writeProperty` copies both onto the property (PscCoder.cpp:388-400: "The flags defined on the variable must be set on the property", `writeUserFlag(*var)` + `if (var->getConstFlag()) " Const"`). The test pins a synthetic `UserFlag { name: "const", flag_index: 4 }` layout that the corpus contradicts.
- **Evidence**: Census bins `constprop` and `condprop`:
  - FO4: 22,550 of 25,679 auto properties have a backing variable with `const_flag != 0`. Starfield: 24,531 of 25,213.
  - Conditional bit on the backing variable: Skyrim 2,235, FO4 804, Starfield 94. On the property itself: 0 in every game.
  - Property-level bits that do work: `hidden` (Skyrim 1,461, FO4 381) and `mandatory` (FO4 2,440).
  - Also, struct-member `user_flags` are ignored (`lower.rs:556-563`, `is_hidden`/`is_conditional: false`): 3 FO4 and 12 Starfield members carry a flag.
- **Impact**: The `.psc` and `.pex` frontends still disagree on `CONST`/`CONDITIONAL` for most FO4/Starfield properties, which is what #5685 set out to close. A future consumer (save-skip of const properties, `GetVM*Variable` conditionals) would silently see none.
- **Related**: #5685, #5398, PEX-D3-2026-10-12-02
- **Suggested Fix**: For an auto property, OR the backing variable's `user_flags` into the mask test, and map the backing variable's `const_flag` to `PropertyFlags::CONST`. This reuses the per-object map from -02. Repin the test with the vanilla table layout. Apply the masks to struct members as well.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
