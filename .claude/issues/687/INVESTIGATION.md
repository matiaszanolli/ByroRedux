# Issue #687 Investigation — alloc-cap rejects in particle.rs

**Date**: 2026-04-26
**Investigator**: Claude Opus 4.7

## Summary

The audit framed the bug as "particle parsers tripping `check_alloc`",
but per the audit's own H-3 finding ("victims, not perpetrators") the
alloc rejects on `NiPSysBoxEmitter` / `NiPSysGrowFadeModifier` /
`NiPSysSpawnModifier` are downstream of upstream stream-drift on
**non-particle** parsers. Tracing the two named example files
(`obgatemini01.nif`, `artrapchannelspikes01.nif`) via
`crates/nif/examples/trace_block.rs` identified two distinct
perpetrators in [crates/nif/src/blocks/controller.rs](crates/nif/src/blocks/controller.rs).

## Perpetrator 1 — `NiGeomMorpherController` missing trailing fields

**File**: `meshes/oblivion/gate/obgatemini01.nif` (v=20.0.0.4, bsver=11).

`NiGeomMorpherController` was reading the standard
`NiTimeControllerBase + flags + data + always_update +
num_interpolators + interpolator_weights[]` payload but missing the
two trailing Bethesda fields per nif.xml:

```xml
<field name="Num Unknown Ints" type="uint" since="10.2.0.0" until="20.1.0.3"
       vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
<field name="Unknown Ints" type="uint" length="Num Unknown Ints" since="10.2.0.0"
       until="20.1.0.3" vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
```

The condition `bsver in 1..=11` makes this Oblivion-only — FNV/FO3
(bsver 24+) and Skyrim+ skip the field entirely.

**Drift cascade**: the omitted u32 (typically `0` on vanilla
content, so no array follows) caused a 4-byte under-consumption.
The next block (`NiMorphData`) read `num_morphs` from the
under-consumed slot, parsed as a 9-byte stub instead of the
~14 KB block, and downstream `NiBlendFloatInterpolator` /
`NiFloatData` blocks read morph names as keyframe data, eventually
tripping `check_alloc` on `NiPSysBoxEmitter`-class instances with
billion-byte ghost allocations.

## Perpetrator 2 — `NiControllerSequence` missing `Phase` field

**File**: `meshes/dungeons/ayleidruins/interior/traps/artrapchannelspikes01.nif`
(v=10.2.0.0, bsver=9 — pre-Oblivion Gamebryo content shipped in
Oblivion).

Per nif.xml `<niobject name="NiControllerSequence">`:

```xml
<field name="Phase" type="float" since="10.1.0.106" until="10.4.0.1" />
```

The existing parser at the trailing-fields block went directly
from `frequency` to `start_time`, dropping the `Phase` f32 for
content in v ∈ [10.1.0.106, 10.4.0.1]. Oblivion (v=20.0.0.4/5) is
past that range and unaffected; pre-Oblivion v=10.2.0.0 content
inside `Oblivion - Meshes.bsa` was misaligned by 4 bytes.

**Drift cascade**: `start_time` was read as the original `Phase`,
`stop_time` as the original `start_time`, etc., until
`accum_root_name`'s u32 length was read as the original
`stop_time`'s float (which decoded as a 3-char `"ART"` length —
the first three chars of `"ARTrapChannelSpikes01"`). The remaining
18 chars of the string then bled into the next block, which
truncated 233 blocks downstream.

Also added handling for the `Play Backwards` bool (exact v=10.1.0.106
only) for completeness — no observed content hits this version.

## Verification

`cargo test -p byroredux-nif --release --test parse_real_nifs
parse_rate_oblivion -- --ignored`:

- Pre-fix: 95.21% clean (7647/8032), 384 truncated, 1 hard-fail.
- Post-fix: **96.24% clean (7730/8032), 301 truncated**, 1 hard-fail.

83 Oblivion files recovered from one bug-fix bundle. The remaining
hard-fail is `meshes/marker_radius.nif` (#698, separate issue —
intentional debug stub with corrupt-by-design 318 MB allocation
request).

The 301 remaining truncations include further drift sources for
follow-up issues; this PR closes the two surfaced by tracing the
audit's named example files.

## Cross-game

- FNV / FO3 (bsver 24+): both `Num Unknown Ints` (gate `bsver <= 11`)
  and `Phase` (version > 10.4.0.1) are skipped. No layout change.
- Skyrim+ / FO4 / FO76 / Starfield: same — both gates exclude.
- Pre-Oblivion v < 10.1.0.106: `Phase` gate excludes.

R3 per-block baselines for FO3 / FNV / Skyrim SE / FO4 / FO76 /
Starfield must remain unchanged. Oblivion baseline regenerates.

## Audit completeness checks

- **SIBLING**: same pattern checked in `controller.rs` —
  no other version-gated trailing fields obviously missing on the
  two parsers' immediate ancestors. Other suspect parsers
  (`NiPSysData`, `NiKeyframeController` quaternion variants) are
  candidates for follow-up tracing once the easy 83-file recovery
  lands.
- **TESTS**: regression test pinning the byte-correct consumed
  count for `NiGeomMorpherController` on Oblivion bsver=11 added
  in `controller.rs` test module; existing
  `parse_controller_sequence_oblivion_string_palette_format`
  exercises the v=20.0.0.4 NiControllerSequence path — Phase
  field is not present at that version, so existing test stays
  valid. Add a v=10.2.0.0 case for the Phase-bearing path.
- **CROSS-GAME**: confirmed via per_block_baselines no shrink
  on FNV/FO3/Skyrim/FO4/FO76/Starfield.
- **DOC**: ROADMAP / CLAUDE.md don't directly cite the affected
  block-level layouts; HISTORY-class context covered in commit body.
