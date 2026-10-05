# #5304 — SCR-D6-2026-10-05-01: #5223 recognises FO3/FNV kill-on-load ObScript by editor ID, not by bytecode, so a same-EDID override or name-alike mod script is still treated as an unconditional kill

- **Labels**: low,scripting,game:fo3,game:fnv,bug
- **Filed from**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5304

- **Severity**: LOW
- **Dimension**: Legacy ObScript (owner of the file: `/audit-gameplay`. Filed here for the decline invariant.)
- **Untrusted-Input**: Yes (plugin `SCPT` records)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:360-441`, functions `base_script_editor_id` and
  `script_killed_corpse_forms`.
- **Status**: NEW
- **Description**: Pass 1 marks every XLKR target of any placement whose base script EDID **contains**
  `"DismembermentSCRIPT"`. Pass 2 marks an actor whose base script EDID contains that substring or **equals**
  `GenericKillSCRIPT` / `OnLoadKillSelf`. The semantics ("unconditional `Begin OnLoad` under `doOnce`:
  `linkedRef.killactor`") are inferred from the vanilla record name. The engine already decodes SCDA structurally
  (`obscript_vm`, `decode_extender_calls`), but the body is never checked.
- **Impact**: the vanilla outcome is right, and the census is pinned by the `#[ignore]`d
  `fo3_fnv_script_killed_corpses_match_the_measured_census`. A patch or mod that overrides one of these SCPTs with a
  conditional body, or ships a script whose EDID merely contains the substring, gets live actors spawned as corpses: no
  AI, and lootable. Nothing crashes.
- **Suggested Fix**: gate on the decoded body. Require an `OnLoad` block whose only effect is a `KillActor` on
  `GetLinkedRef`/self, and decline otherwise. Failing that, match the exact vanilla EDID family list instead of a
  substring, and only when the SCPT's defining plugin is a vanilla master.

_Source: `AUDIT_SCRIPTING_2026-10-05.md` (SCR-D6-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
