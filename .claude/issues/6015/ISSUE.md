# SCR-D6-2026-10-17-05: Oblivion-empirical command ids run ungated on FO3/FNV quest scripts; `GetSelf` (0x10ce) has no recorded evidence

Issue: #6015
Labels: low, scripting, bug, legacy-compat, game:fo3, game:fnv

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-05: Oblivion-empirical command ids run ungated on FO3/FNV quest scripts; `GetSelf` (0x10ce) has no recorded evidence

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `byroredux/src/asset_provider/script.rs:962` (no game gate), `obscript_quests.rs:336-338`, `docs/engine/scripting.md` ~2250 ("Command ids are shared across the three games")
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The ids (0x1039, 0x103a, 0x1059, 0x1000, 0x101f, ...) were aligned against Oblivion.esm SCTX only (`obscript_vm.rs:29-34`); the "shared across three games" claim is documented but not backed by an alignment run on FO3/FNV, and no local reference (CS/GECK wiki) lists opcode numbers (grep: none) - so it is unverifiable from the reference set (feedback_no_guessing). `0x10ce => GetSelf` appears only in `obscript_quests.rs` and the original commit message; it is absent from the module-doc table, the skill's list and any test. Only fn-level data (`real_data_tests`, FNV `Patch04Quest00SCRIPT`) covers structure, not command identity. If any id differs on FNV, a Fallout quest script would execute a different command (e.g. a `SetStage` lookalike) with a guessed meaning instead of declining.
- **Suggested Fix**: Run the SCTX-vs-SCDA alignment on FalloutNV.esm/Fallout3.esm once and record the vote counts, or gate the host's stage-mutating commands on `ScriptGame` until verified; add a `GetSelf` row (with evidence) to the module doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
