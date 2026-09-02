# Issue #3437
title:	SAFE-2026-08-27b-02: pre-10.1.0.106 NiControllerSequence defaults cycle_type to 0 (CYCLE_LOOP) where nif.xml specifies CYCLE_CLAMP (=2), with a -inf duration in the same branch
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	animation, bug, game:oblivion, medium, nif, nif-parser, safety
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3437
--
From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 8 — animation / NIF parse mismatch).

- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/controller/sequence.rs:310-332`; mapping at `crates/nif/src/anim/types.rs:35-42`; spec at `/mnt/data/src/reference/nifxml/nif.xml:1024-1026`, `:4218`, `:82-83`
- **Status**: NEW

## Description

For `stream.version() < V10_1_0_106` the `NiControllerSequence` fields are absent and the parser substitutes literals. Its own comment states what those should be:

> Defaults are nif.xml's own (`weight` 1.0, `frequency` 1.0, `cycle_type` **CYCLE_CLAMP = 0**, `start_time` FLT_MAX, `stop_time` FLT_MIN)

`CYCLE_CLAMP` is **not** `0`. nif.xml's `CycleType` enum is `CYCLE_LOOP = 0` / `CYCLE_REVERSE = 1` / `CYCLE_CLAMP = 2` (`nif.xml:1024-1026`), and the block's stated default *is* `CYCLE_CLAMP` (`nif.xml:4218`). The engine's own `CycleType::from_u32` agrees with nif.xml (`0 => Self::Loop`), so the substituted `0` is decoded as **Loop**. Every `NiControllerSequence` in the `10.0.1.0 ≤ v < 10.1.0.106` window therefore plays looping where the format says clamp — the comment asserting they are the same value is what makes it look correct.

The same `else` branch has a second, coupled property. `start_time` defaults to `f32::MAX` and `stop_time` to `f32::MIN`; both match nif.xml (`#FLT_MAX#` = `3.402823466e+38`, `#FLT_MIN#` = **`-3.402823466e+38`**). But `import_sequence` then computes `duration = stop_time - start_time` = `f32::MIN - f32::MAX`, which **overflows to `-inf`** (verified by execution). Today the wrong `cycle_type` masks it: the `Loop` arm gates on `if clip.duration > 0.0`, which is false, so nothing wraps and `local_time` stays finite. Correcting `cycle_type` to `2` **alone** routes these clips into the `Clamp` arm, `(local_time + delta).min(-inf) = -inf` on the first tick, and every such clip freezes at key 0. The two must be fixed together.

## Evidence

```rust
// crates/nif/src/blocks/controller/sequence.rs:328-332
let cycle_type = if has_ctlr_seq_fields {
    stream.read_u32_le()?
} else {
    0                       // ← decoded as CYCLE_LOOP, not CYCLE_CLAMP
};
```
```rust
// crates/nif/src/anim/types.rs:35-42 — agrees with nif.xml, not with the comment
pub fn from_u32(v: u32) -> Self {
    match v {
        0 => Self::Loop,
        1 => Self::Reverse,
        2 => Self::Clamp,
        _ => Self::Clamp,
    }
}
```
```xml
<!-- nif.xml:1024-1026 -->
<option value="0" name="CYCLE_LOOP">Loop</option>
<option value="1" name="CYCLE_REVERSE">Reverse</option>
<option value="2" name="CYCLE_CLAMP">Clamp</option>
<!-- nif.xml:4218 -->
<field name="Cycle Type" type="CycleType" default="CYCLE_CLAMP" since="10.1.0.106" />
<!-- nif.xml:82-83 -->
<default token="#FLT_MAX#" string="3.402823466e+38" />
<default token="#FLT_MIN#" string="-3.402823466e+38" />
```

The version window is live rather than theoretical: `NiControllerSequence` is "Root node in Gamebryo .kf files (version 10.0.1.0 and up)" (`nif.xml:4215`), and `crates/nif/src/version.rs` carries a deliberate *"old Oblivion" (v10.0.x)* layout predicate family (#1337). **Honest limit**: the audit did not census how many `NiControllerSequence` blocks in the supported titles actually land below `10.1.0.106`, so the blast radius is code-provable but not measured.

## Impact

Clips in the pre-`10.1.0.106` window play with the wrong cycle semantics (loop instead of clamp) — a visible animation defect on old-Oblivion content, and one that a naive one-line "fix the constant" turns into frozen poses because of the `-inf` duration in the same branch.

## Related

SAFE-2026-08-27b-01 / #3432 (the `duration` half), #1337 (the v10.0.x layout family), #687 (the last envelope-field misalignment in this parser), #2345 (the gate that introduced these defaults).

## Suggested Fix

Substitute `2` (`CYCLE_CLAMP`) and correct the comment to name nif.xml's actual numbering. In the same change, gate `duration` in `import_sequence` — `let duration = seq.stop_time - seq.start_time; let duration = if duration.is_finite() && duration > 0.0 { duration } else { 0.0 };` — so the corrected `Clamp` arm sees a sane envelope. Add a unit test that parses a `< 10.1.0.106` sequence and asserts `CycleType::Clamp` **and** a finite duration, so the two cannot be separated again.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other `has_ctlr_seq_fields` default substitutions in the same `else` chain)
- [ ] **CANONICAL-BOUNDARY**: version-specific defaulting stays in the parser, never re-derived downstream. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# Issue #3461
title:	NIF-2026-08-27-D3-01: `BSDistantObjectExtraData` has no dispatch arm — 112,716 `NiUnknown` blocks on FO76 distant-LOD content
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, game:fo76, medium, nif, nif-parser
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3461
--
Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 3 (Block Dispatch Coverage). Severity **MEDIUM**. Game: **Fallout 76** (`bsver` 152–167).

## Location
`crates/nif/src/blocks/mod.rs` (`parse_block_inner` — no arm exists; `grep -rn BSDistantObjectExtraData crates/nif/src` returns **zero** hits, re-verified at publish time).

## Description
nif.xml documents this block completely at line 8460 —
`<niobject name="BSDistantObjectExtraData" inherit="NiExtraData" module="BSMain" versions="#F76#">` with a single field
`<field name="Distant Object Flags" type="uint" />`. There is no parser and no dispatch arm, so every instance falls through to the `NiUnknown` placeholder and its flags word is discarded.

## Evidence
Measured live over three FO76 archives (`--release`):
- `SeventySix - GeneratedMeshes01.ba2` — 20,245 files, **41,435** `NiUnknown`, 100% `BSDistantObjectExtraData` (e.g. `meshes\terrain\appalachia\objects\appalachia.4.-42.-53.bto`)
- `SeventySix - GeneratedMeshes02.ba2` + `SeventySix - 10UpdateMain.ba2` — **71,281** more, same single type
- `SeventySix - Meshes.ba2` (the one gated archive) — 58,469 files, **0** `NiUnknown`

The block lives on `.bto` distant-LOD terrain meshes, which ship in the `GeneratedMeshes*` / `*UpdateMain` archives the corpus gate never opens.

## Impact
FO76 has a `block_sizes` table, so this does not cascade — the outer reconciliation absorbs it and `scene.truncated` stays false. The loss is the per-object distant-LOD flags word on 112,716 blocks, the same class of loss #942 fixed for `BSDistantObjectInstancedNode` ("ghost foliage"). Blast radius is confined to FO76 distant terrain.

## Related
Closed #942 (sibling block, same subsystem); the corpus-gate blind spot that hid this is filed as its own issue (NIF-2026-08-27-D3-02).

## Suggested Fix
One arm — `"BSDistantObjectExtraData" => Ok(Box::new(NiExtraData-base + read_u32_le()))`, mirroring the other single-field extra-data parsers, plus the matching `per_block_baselines` regeneration. nif.xml settles the layout; no research needed.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other extra-data block parsers, the `per_block_baselines` / `block_coverage_baselines` regen)
- [ ] **TESTS**: A regression test pins this specific fix


---

# Issue #3464
title:	NIF-2026-08-27-D1-01: `BSFaceGenNiNode` under-reads 2 bytes on 100% of Starfield facegen head nodes
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, game:starfield, medium, nif, nif-parser
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3464
--
Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 1 (Stream Position Integrity). Severity **MEDIUM** (per `_audit-severity.md`: "NIF parse mismatch (stream position off)"). Game: **Starfield**, `bsver = 175` (the `SF_WEAK_REF_GAP` band).

## Location
`crates/nif/src/blocks/mod.rs:314-325` — `BSFaceGenNiNode` is aliased into the plain-`NiNode` arm (re-verified at publish time: the alias sits at `mod.rs:325`, its comment at `:314`).

## Description
Every `BSFaceGenNiNode` block in shipped Starfield content consumes exactly 2 bytes fewer than its declared `block_size`. The block is dispatched to `NiNode::parse` as a "coverage-first stub", and the dispatch comment cites its own corpus count as evidence the alias works:

```rust
// BSFaceGenNiNode (Starfield, 1,282 / 1,282 in `FaceMeshes.ba2`,
// #727) is aliased here as a coverage-first stub: the wire
// layout is unconfirmed and nif.xml has no SF schema for it.
```

Those same 1,282 blocks are 1,282 two-byte under-reads. The comment reads as a coverage claim; the measurement says the alias is 2 bytes short on every single one.

## Evidence
`NifScene::drift_histogram` over `Starfield - FaceMeshes.ba2` + `ShatteredSpace - Main01/02.ba2`:

```
-- BSFaceGenNiNode blocks present, by bsver --
  bsver=175	blocks=1417
-- drift, by bsver --
  bsver=175	drift=+2	count=1417
```

100% (1,417 / 1,417). A representative file (`meshes\actors\character\facegendata\facegeom\starfield.esm\000124aa.nif`, `version=20.2.0.7 bsver=175`) declares `block 0 BSFaceGenNiNode size=122` against 120 consumed.

**The 2 bytes are `BSFaceGenNiNode`-specific, not the `NiNode` base and not the #2105 `SF_WEAK_REF_GAP` field.** Discriminating measurement over every `bsver == 175` file in `MeshesPatch.ba2` + `Meshes01.ba2` + `ShatteredSpace - Main01.ba2`:

```
-- node blocks present in bsver-175 files --
  NiNode              58128
  BSWeakReferenceNode  9440
  BSFaceGenNiNode        135
-- drift in bsver-175 files --
  BSFaceGenNiNode  drift=+2  135
```

58,128 plain `NiNode` blocks in the same band drift by zero; 9,440 `BSWeakReferenceNode` blocks drift by zero (their own gap is already handled at `crates/nif/src/blocks/node.rs:936`). Only `BSFaceGenNiNode` drifts.

## Impact
`block_size` reconciliation realigns the stream, so nothing cascades and Starfield facegen heads still load. What is lost is whatever those 2 bytes carry on every Starfield NPC head node, and — more operationally — the dispatch comment's coverage claim is misleading to the next reader.

## Needs research
nif.xml has **no Starfield `BSFaceGenNiNode` schema** (the type is not present in `/mnt/data/src/reference/nifxml/nif.xml` at all), and the Gamebryo 2.3 source predates it. The *semantics* of the 2 bytes cannot be settled from either authority and must be reverse-engineered from the bytes.

## Related
Closed #727 (the alias itself — the residual drift it leaves has never been filed); #2105 / #2201 (`SF_WEAK_REF_GAP = 175`, `crates/nif/src/version.rs:476` — same width, same bsver band, different block); #1606 (the `read_starfield_tail` opaque-capture precedent).

## Suggested Fix
Capture the 2 bytes opaquely with the #1606 / `BsWeakReferenceNode::parse_with_size` idiom (a dedicated `BsFaceGenNiNode { base: NiNode, starfield_tail: Vec<u8> }` consumed to `block_size`), which removes the drift without fabricating field semantics. Update the dispatch comment so "1,282 / 1,282" no longer reads as a correctness claim.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (`BsWeakReferenceNode::parse_with_size`, the other opaque-tail capture sites, `dispatch_tests/nodes.rs:531-541`)
- [ ] **TESTS**: A regression test pins this specific fix


---

# Issue #3476
title:	NIF-2026-08-27-D2-01: #2345 added 19 raw version comparisons in one parser without routing any through the named-helper surface
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, nif, nif-parser, tech-debt
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3476
--
Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 2 (Version Gating). Severity **LOW**. Game affected: all (maintainability).

## Location
`crates/nif/src/blocks/controller/sequence.rs:148-409` — 19 `stream.version() <op> NifVersion::V…` sites in `NiControllerSequence::parse`.

## Description
`crates/nif/src/version.rs:190-197` states the doctrine — "block parsers query *intent* instead of scattering raw `version < V10_1_0_0` literals" — and the `/audit-nif` Dim-2 checklist makes a new gate that hardcodes a literal *the regression*. The #2345 fix is byte-correct (every gate was re-verified against nif.xml `ControlledBlock` lines 1919-1950 and `NiSequence`/`NiControllerSequence` lines 4201-4231), but it introduced 19 raw comparisons in a single function and no helper.

Two of them — `sequence.rs:148` and `:153`, both `stream.version() <= NifVersion::V10_1_0_103` — are the *exact* predicate `NifVersion::has_keyframe_controller_data()` (`version.rs:262-264`) already implements, and that helper's doc comment explicitly enumerates the sibling fields sharing the boundary (`NiKeyframeController.Data`, `NiVisController.Data`, `NiAlphaController.Data`, …). `NiSequence`'s `Accum Root Name` / `Text Keys` pair sits on the same nif.xml boundary and was not added to that enumeration.

## Evidence
`grep -c "stream.version() [<>=]" crates/nif/src/blocks/controller/sequence.rs` → **19** (re-verified at publish time). `impl NifVersion`'s live helper surface is exactly the 9 methods at `version.rs:204-297`, none of which this parser calls.

## Impact
None at runtime. The cost is that the next person changing the 10.1.0.10x boundaries has to find 19 sites in one function rather than one helper, and that `has_keyframe_controller_data`'s doc now under-describes the fields on its own boundary.

## Related
#1511 / #1840 / #1897 (the "a helper with no call site is dead code" lesson — this is the mirror case: call sites with no helper); #2345.

## Suggested Fix
Add `NifVersion::has_ni_sequence_prologue()` (`self <= V10_1_0_103`) and `has_controller_sequence_fields()` (`self >= V10_1_0_106`) *with* these call sites, and extend `has_keyframe_controller_data`'s doc enumeration to name the `NiSequence` pair.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other block parsers on the 10.1.0.10x boundary that hardcode the same literals)
- [ ] **TESTS**: A regression test pins this specific fix (the new helpers' boundary behaviour, so the refactor is byte-neutral)


---

