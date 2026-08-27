

================ #2339 [OPEN] bug, nif-parser, low, legacy-compat, game:fnv ================
# FNV-D7-04: extract_ragdoll has silent drop sites alongside loud #1539/#1850 warnings for the same edge-loss class

**Severity**: LOW
**Dimension**: Dimension 7 — PHYSAL Ragdoll (FNV reference slice; PHYSAL-wide, not FNV-specific)
**Location**: `crates/nif/src/import/collision/ragdoll.rs` (`extract_ragdoll`, bodies loop ~lines 46-76 and constraints loop ~lines 122-125)
**Status**: NEW

## Description

Four drop sites in `extract_ragdoll` stay silent (`continue` with no log),
while two adjacent guards in the same function — the #1850
`bhkBreakableConstraint` drop and the #1539 `BhkConstraintData::Other` drop —
log loudly via `log::warn!` for the same class of lost articulation edge:

Silent sites (all bare `continue`, no logging):
- unhosted body: `body_to_bone.get(&idx)` returns `None`
- failed shape resolve: `resolve_shape(...)` returns `None`
- non-finite guards: non-finite `mass` / `translation` / `rotation` on the
  body CInfo (#1534)
- unresolved constraint endpoint: `block_to_body.get(&i)` fails for either
  entity ref in the constraints loop

Loud sites (already correct, for comparison):
```rust
log::warn!(
    "extract_ragdoll: dropping bhkBreakableConstraint ... (#1850)", ...
);
...
log::warn!(
    "extract_ragdoll: dropping unsupported constraint linking bones ... (#1539)", ...
);
```

## Impact

Telemetry-only — the downstream forest/emptiness gates in `build_ragdoll`
still correctly fire regardless — but it undercuts the diagnostic story the
#1539/#1850 guards exist for: an auditor or developer investigating a
malformed ragdoll sees warnings for some dropped edges but not others, making
the silent classes harder to diagnose from logs alone.

## Suggested Fix

Route all four silent sites through the same "dropping … linking bones 'a'
<-> 'b'" phrasing used by the #1539/#1850 guards. Add a test driving a
`BhkConstraintData::Other` edge (or one of the other three drop conditions)
through `extract_ragdoll` and asserting the warning is emitted.

## Completeness Checks
- [ ] **SIBLING**: Same function, same file — no cross-game or cross-file duplication to check; all four sites are local to `extract_ragdoll`.
- [ ] **TESTS**: A regression test drives each of the four previously-silent drop conditions through `extract_ragdoll` and asserts a `log::warn!` (or equivalent) is emitted.



================ #2345 [OPEN] bug, nif-parser, low, legacy-compat ================
# NIF-OBL-D1-02: ControlledBlock has no pre-10.1.0.106 layout — three fields mis-gated

**Severity**: LOW
**Dimension**: NIF Version Handling (v20.0.0.5 + v10.x NetImmerse Tail)
**Location**: `crates/nif/src/blocks/controller/sequence.rs:124-227` (`NiControllerSequence::parse`, `ControlledBlock` array loop)
**Status**: NEW

### Description
`NiControllerSequence::parse` implements only the `>= 10.1.0.104`
`ControlledBlock` layout. Three nif.xml gates are missing:

- `Target Name` (`until="10.1.0.103"`) — never read at all (confirmed by
  `grep -n "Target Name\|target_name"` over the file: zero hits).
- `Interpolator` — `interpolator_ref` is read unconditionally at line 160
  (`stream.read_block_ref()?`), but nif.xml gates the field `since="10.1.0.106"`
  — an over-read on any file below that version.
- `Priority` — line 177 gates the byte read only on `bsver > 0`
  (`let priority = if bsver > 0 { stream.read_u8()? } else { 0 };`); nif.xml's
  actual gate is `since="10.1.0.106" vercond="#BSSTREAM#"` — the `since` half
  is missing, so a Bethesda file (`bsver > 0`) below 10.1.0.106 gets a
  phantom priority-byte read that shouldn't happen.

The inherited `NiSequence` fields `Accum Root Name` (line 268,
`stream.read_string()?`, unconditional) and `Text Keys` are likewise read
without their nif.xml `until="10.1.0.103"` gate.

### Evidence
Read directly at `crates/nif/src/blocks/controller/sequence.rs:160-161`
(unconditional `interpolator_ref`/`controller_ref` reads), `:170-175` (the
existing, correct `V10_1_0_104..=V10_1_0_110` gate for the *separate*
`Blend Interpolator`/`Blend Index` pair — proving the file already has the
version-gate idiom available, just not applied to these three fields),
`:177` (bsver-only `Priority` gate), and `:268` (unconditional
`accum_root_name` read). `grep -n "Target Name\|until.*10.1.0.103"` over the
file returns no matches, confirming the pre-10.1.0.106 `ControlledBlock`
layout and the `NiSequence` `until=10.1.0.103` prologue pair are both absent.

### Impact
Any `NiSequence`/`NiControllerSequence` below v10.1.0.106 mis-advances the
stream in a band with no recovery anchor. Empirically unreached on vanilla
content: Oblivion's 23 v10.0.1.2 + 8 v10.1.0.101 files (the only sub-10.1.0.106
content with `bsver > 0`) all parse clean in the fresh corpus run, so vanilla
Oblivion does not put controller sequences in those bands. Exposure is
mod/non-Bethesda Gamebryo content.

### Suggested Fix
Add the three version gates (`Target Name` until=10.1.0.103,
`Interpolator` since=10.1.0.106, `Priority` since=10.1.0.106 && bsver>0) plus
the `NiSequence` `until=10.1.0.103` prologue pair, with a synthetic
byte-exact test at v10.1.0.101/bsver=4.

## Completeness Checks
- [ ] **SIBLING**: Check the same pre-10.1.0.106 gating gap in other `ControlledBlock`-consuming controllers if any exist
- [ ] **TESTS**: A synthetic byte-exact test at v10.1.0.101/bsver=4 pins the corrected pre-10.1.0.106 layout



================ #2360 [OPEN] bug, import-pipeline, low, legacy-compat, game:starfield ================
# SF-BA2-02: v3 header-boundary diagnostic log reads the stream position 4 bytes early

**Severity**: LOW
**Dimension**: 1 — BA2 v2/v3 LZ4 Block Decompression (Starfield audit, 2026-08-03)
**Location**: `crates/bsa/src/ba2.rs:233-236`, `:447-472` (`log_v2_v3_extra_bytes`)
**Status**: NEW, CONFIRMED against current code

## Description

For v3, the header-boundary sanity log captures `stream_position()` before the 4-byte `compression_method` field is read (32 bytes in, not the true 36-byte post-header offset). The v2 branch captures it correctly (nothing left to read at that point).

## Evidence

Confirmed by reading `ba2.rs:233-236` directly: the `BA2_V_STARFIELD_V3` arm calls `log_v2_v3_extra_bytes("v3", &extra, name_table_offset, reader.stream_position()?)` **before** `method_buf` (the 4-byte compression method) is read a few lines later.

## Impact

Log-only — a `log::trace!`/`log::debug!` diagnostic, never affects control flow or parsing correctness.

## Suggested Fix

Move the log call to after `method_buf` is read, or pass `stream_pos + 4` with a comment explaining the offset.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the corrected stream-position value in the v3 diagnostic log (or asserts logic equivalence if untestable via log capture)


================ #2361 [OPEN] bug, nif-parser, low, legacy-compat, game:starfield, doc-rot ================
# SF2D2-04: .mesh suffix/geometries\ head composed unconditionally, contradicting the field's documented path-or-stem semantics

**Severity**: LOW
**Dimension**: 2 — BSGeometry Mesh Extraction (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:70`
**Status**: NEW, CONFIRMED against current code

## Description

The importer always composes `geometries\{mesh_name}.mesh` with no inspection of `mesh_name` (`let canonical = format!("geometries\\{mesh_name}.mesh");`), but nifly (the cited wire-format authority) and this codebase's own block-level doc both document the field as holding *either* a bare stem *or* a full path. A `mesh_name` already carrying the prefix/suffix double-composes into a guaranteed miss.

## Evidence

Confirmed by reading `bs_geometry.rs:70` directly — the `format!` unconditionally prepends `geometries\` and appends `.mesh` with no case/separator-insensitive head/tail check.

## Impact

Zero on vanilla (every real `.mesh` name sampled is a bare 20-hex stem); affects authoring-tool output / mods using readable paths, where the mesh silently vanishes (compounded by SF2D2-03, #2357's silence on resolve misses).

## Suggested Fix

Skip the prepend/append when the name already carries them, reusing the case/separator-insensitive head test already written in `normalize_mesh_path`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `format!`-composed archive path in the importer has the same double-composition risk
- [ ] **TESTS**: A regression test pins a `mesh_name` that already carries `geometries\`/`.mesh` resolving correctly instead of double-composing
