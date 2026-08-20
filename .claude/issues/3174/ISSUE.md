# NIF-D1-2026-08-20-01: the NiPSys*Ctlr family has the same until=10.1.0.103 / since=10.1.0.104 split #2562/#2563 just fixed on nine siblings — 22 dispatch names, compound desync

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3174
Finding: NIF-D1-2026-08-20-01
Labels: medium,nif-parser,nif,bug
Source: docs/audits/AUDIT_NIF_2026-08-20.md

Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 1 — Stream Position Integrity).

**Severity**: MEDIUM — escalates to HIGH on any sizeless file that actually carries one, since `read_string` then reads a garbage length prefix.
**Game Affected**: any NIF below file-version **10.1.0.104** — on shipping content, the Oblivion-era NetImmerse band (`bsver` <= 11, file versions v3.3–v10.1.0.103). FO3+ (`v20.2.0.7`) and later are above the boundary and unaffected.

**Location**:
- `crates/nif/src/blocks/particle.rs:938-947` (`parse_modifier_ctlr`)
- `crates/nif/src/blocks/particle.rs:950-963` (`parse_emitter_ctlr`)
- `crates/nif/src/blocks/particle.rs:965-978` (`parse_multi_target_emitter_ctlr`)
- dispatched from `crates/nif/src/blocks/mod.rs:1135-1169`
- existing gate helper that should be reused: `NifVersion::has_keyframe_controller_data` (`crates/nif/src/version.rs:262`)

## This is an INCOMPLETE FIX, not a new class

#2562 / #2563 (landed as `e32e2b1f`) fixed the missing `until="10.1.0.103"` `Data` ref on **nine** `NiSingleInterpController` subclasses. nif.xml declares the same field on **four more**, all in the `NiPSys*Ctlr` family, and none of them were touched — the fix only reached `blocks/controller/mod.rs` and `blocks/controller/shader.rs`, and `version.rs:247-262`'s enumerating doc comment lists exactly the nine that were fixed.

| type | `Data` template | reaches |
|---|---|---|
| `NiPSysEmitterCtlr` | `NiPSysEmitterCtlrData` | `parse_emitter_ctlr` |
| `NiPSysModifierActiveCtlr` | `NiVisData` | `parse_modifier_ctlr` |
| `NiPSysModifierFloatCtlr` (abstract base of 19 dispatch names) | `NiFloatData` | `parse_modifier_ctlr` |
| `NiFloatsExtraDataController` | `NiFloatData` | *no dispatch arm* |

## Description — a compound desync, worse than a plain missing field

These three functions do **not** delegate to `NiSingleInterpController::parse` (which gates its interpolator ref correctly at `controller/mod.rs:257`). They open-code the inheritance chain and read the interpolator ref *unconditionally*:

```rust
// particle.rs:938-943 — parse_modifier_ctlr (verified at HEAD)
let _base = parse_interp_controller_base(stream)?;
let _interpolator_ref = stream.read_block_ref()?; // NiSingleInterpController
let _modifier_name = stream.read_string()?;       // NiPSysModifierCtlr
//  ^ no version gate                             ^ and no trailing Data ref
```

Below v10.1.0.104 that produces a three-stage desync:
1. 4 bytes of a non-existent interpolator ref are consumed;
2. `modifier_name` is then read from a 4-byte-shifted offset — and since the string table only exists at `STRING_TABLE_THRESHOLD` and above, `read_string` here reads a `u32` length prefix, so a shifted read yields an arbitrary length;
3. the real trailing `Data` ref is never read.

## Evidence

nif.xml field declarations (extracted mechanically from `/mnt/data/src/reference/nifxml/nif.xml`):
```
NiPSysEmitterCtlr:           <field name="Data" type="Ref" template="NiPSysEmitterCtlrData" until="10.1.0.103" />
                             <field name="Visibility Interpolator" type="Ref" template="NiInterpolator" since="10.1.0.104" />
NiPSysModifierActiveCtlr:    <field name="Data" type="Ref" template="NiVisData"   until="10.1.0.103" />
NiPSysModifierFloatCtlr:     <field name="Data" type="Ref" template="NiFloatData" until="10.1.0.103" />
NiFloatsExtraDataController: <field name="Data" type="Ref" template="NiFloatData" until="10.1.0.103" />
```

**The inconsistency is visible inside one function.** `parse_emitter_ctlr` already gates the *visibility* interpolator on `>= V10_1_0_104` (`particle.rs:958`, the #1544 fix) and its own comment even names the missing field — *"the pre-10.1.0.104 `Data` ref is the mutually-exclusive legacy slot"* — while the *primary* interpolator two lines above it (`:952`) is read with no gate at all, and the legacy slot is never read.

Neither `niobject` carries a `since=` attribute in nif.xml, so the schema itself considers the pre-split form reachable; that is why the `Data` ref is declared at all.

**Dispatch blast radius: 22 type names** — `NiPSysEmitterCtlr`, `BSPSysMultiTargetEmitterCtlr`, `NiPSysModifierActiveCtlr`, and the 19 `NiPSysModifierFloatCtlr` descendants listed at `blocks/mod.rs:1139-1168` (`NiPSysEmitterSpeedCtlr`, `NiPSysGravityStrengthCtlr`, `NiPSysAirFieldSpreadCtlr`, `NiPSysRotDampeningCtlr`, …). Notably `blocks/mod.rs:1156-1164`'s own comment asserts the trailing ref "is gated `until="10.1.0.103"` so FO76 (v20.2.0.7) skips it via the same NiTimeController base" — but **no code path implements that gate**; the field is simply never read at any version.

## Impact

Latent on vanilla content (0 truncations on Oblivion today proves no vanilla sub-10.1.0.104 file carries a `NiPSys*Ctlr`), exactly as #2563 characterised its own eight latent types. Reachable on mod / legacy NetImmerse particle content. On a sizeless file there is no `block_size` anchor, so the desync cascades through every subsequent block — the same failure mode that cost `meshes\marker_map.nif` 8 of its 13 blocks before #2562.

## Suggested Fix

In `particle.rs`, gate the interpolator ref on `stream.version() >= NifVersion::V10_1_0_104` (or better: delegate to `NiSingleInterpController::parse` so the gate cannot drift), and add the complementary `has_keyframe_controller_data()`-gated `Data` ref after `modifier_name` in all three functions — the field sits at the same offset for `NiPSysModifierActiveCtlr` (via `NiPSysModifierBoolCtlr`, no own fields) and `NiPSysModifierFloatCtlr`, so one read covers both. Extend `version.rs:247-262`'s enumerating doc comment with the four new types. Pin with synthetic v10.1.0.103 fixtures following the convention `crates/nif/src/blocks/dispatch_tests/controllers.rs` already established for the nine sibling types.

## Related

- Direct continuation of #2562 / #2563 (`e32e2b1f`) — same defect class, four types they did not reach.
- `NiFloatExtraDataController` (**singular**) *was* fixed by them; `NiFloatsExtraDataController` (**plural**) has no dispatch arm and so is out of scope until one is added — the two names are one character apart and will be easy to conflate.

## Completeness Checks
- [ ] **SIBLING**: all three `particle.rs` controller parsers fixed, not just the one reproduced; `version.rs`'s enumerating doc comment extended to 13 types
- [ ] **GATE-REUSE**: the fix uses `NifVersion::has_keyframe_controller_data` / `V10_1_0_104` rather than a fourth open-coded version literal
- [ ] **TESTS**: synthetic v10.1.0.103 + v10.1.0.104 fixtures pin both halves (interpolator gate AND trailing `Data` ref) per `dispatch_tests/controllers.rs`
- [ ] **DISPATCH-PARITY**: the stale comment at `blocks/mod.rs:1156-1164` (claims a gate that does not exist) is corrected in the same pass
