# NIF-OBL-D1-02: ControlledBlock has no pre-10.1.0.106 layout — three fields mis-gated

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2345
**Severity**: LOW
**Dimension**: NIF Version Handling (v20.0.0.5 + v10.x NetImmerse Tail)
**Location**: `crates/nif/src/blocks/controller/sequence.rs:124-227` (`NiControllerSequence::parse`)
**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-08-03.md` (finding NIF-OBL-D1-02)
**Labels**: low, nif-parser, legacy-compat, bug

### Description
`NiControllerSequence::parse` implements only the `>= 10.1.0.104`
`ControlledBlock` layout. Three nif.xml gates are missing: `Target Name`
(`until="10.1.0.103"`, never read), `Interpolator` (`since="10.1.0.106"`,
read unconditionally at line 160 — an over-read below that version), and
`Priority` (`since="10.1.0.106" vercond="#BSSTREAM#"`, gated only on
`bsver > 0` at line 177 with the `since` half missing). The inherited
`NiSequence` fields `Accum Root Name`/`Text Keys` (`until="10.1.0.103"`) are
likewise read unconditionally (line 268).

### Impact
Any `NiSequence`/`NiControllerSequence` below v10.1.0.106 mis-advances the
stream in a band with no recovery anchor. Empirically unreached on vanilla
Oblivion content (its sub-10.1.0.106 files with `bsver > 0` all parse clean).
Exposure is mod/non-Bethesda Gamebryo content.

### Suggested Fix
Add the three version gates plus the `NiSequence` `until=10.1.0.103` prologue
pair, with a synthetic byte-exact test at v10.1.0.101/bsver=4.
