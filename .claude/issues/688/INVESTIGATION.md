# Issue #688 Investigation — root NiNode under-consumption clusters at pre-Gamebryo NetImmerse versions, not v=20.0.0.5

**Date**: 2026-04-26
**Investigator**: Claude Opus 4.7

## Audit premise was wrong about the version cluster

The audit framed this as "a subset of v20.0.0.5 content" with a shared
parent (NiAVObject? NiObjectNET?) that has a "field-width discrepancy".
Bucketing the currently-affected files by NIF version refutes that
framing — **none of the 149 root-truncated files are v20.0.0.5**:

| Version    | bsver | Total in archive | Root-truncated |
|------------|-------|------------------|----------------|
| 10.0.1.0   | 0     | 41               | 39             |
| 10.0.1.2   | 1     | 14               | 12             |
| 10.0.1.2   | 3     | 9                | 9              |
| 10.1.0.101 | 4     | 8                | 8              |
| **10.1.0.106** | **5** | **82**       | **77**         |
| 4.0.0.2    | 0     | 4                | 4              |

All affected files are **pre-Gamebryo NetImmerse-vintage content
shipped inside Oblivion's BSA** — the dominant bucket is v=10.1.0.106
(77 files, 52% of the truncations). Critical-path Oblivion content
at v=20.0.0.5 (Anvil Heinrich Oaken Halls etc.) is NOT in this bucket
and renders end-to-end today.

## Empirical block-layout signature

Hex-dumped `meshes\menus\hud_brackets\a_b_c_d_seq.nif` (v=10.1.0.106,
bsver=5) at the block-data start (offset 722):

```
[+0]  00 00 00 00 07 00 00 00 44 75 6d 6d 79 30 31 02
                  ^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^
                  length = 7  "Dummy01"  (NiObjectNET.name)
```

The leading `00 00 00 00` is **inconsistent with both nif.xml and
nifly's NiObjectNET layout**, which both start with `string Name`
directly. With our parser reading at offset 0 (treating those zeros
as `name length = 0`), every downstream field decodes as garbage and
the parse eventually trips a "failed to fill whole buffer" deep
inside the block (e.g. at consumed=16989 for this file).

## Hypotheses explored, both refuted

### A: Per-block leading u32 prefix on v < 10.2.0.0

If we skip 4 bytes before *every* block, the first 6 blocks parse
cleanly with sane consumed counts:

```
[  0] @      4  NiNode                            consumed 105
[  1] @    113  BSXFlags                          consumed 11
[  2] @    128  NiStringExtraData                 consumed 27
[  3] @    159  NiControllerManager               consumed 83
[  4] @    246  NiTransformController             consumed 30
[  5] @    280  NiBlendTransformInterpolator      consumed 60
[  6] @    344  NiControllerSequence              ERR @ consumed 4: …
```

Block 6 (NiControllerSequence) trips an alloc cap with a junk u32 =
`0xFFFF7FFF`, so the per-block-prefix hypothesis is **partially
correct but not uniform** — different blocks expect different
leading layouts.

### B: block_data_offset is short by 4 bytes (one missing header field)

Skip 4 bytes once at the start, then parse normally:

```
[  0] @      4  NiNode                            consumed 105
[  1] @    109  BSXFlags                          consumed 8
[  2] @    117  NiStringExtraData                 ERR @ consumed 4: …
```

Block 1's consumed shifts (11 → 8) and block 2 fails — refuted.

## What's needed to actually fix this

The audit's #1 recommended fix ("debug-mode end-of-block consumed-byte
assertion against `block_size`") **doesn't help here**: `block_sizes`
is gated to `since 20.2.0.5`, and every affected file is < 20.2.0.5.
There's nothing to assert against.

The audit's #3 step ("bisect against the Gamebryo 2.3 NiNode::LoadBinary
source") is the right path — these versions predate the nif.xml
spec coverage we trust today. Without the Gamebryo 2.3 source mounted
(legacy reference at `/media/matias/Respaldo 2TB/...` is not currently
accessible), the exact byte layout cannot be derived.

Candidate sources of truth:

1. **Gamebryo 2.3 / NetImmerse-era** `NiObjectNET::LoadBinary` /
   `NiAVObject::LoadBinary` for v=10.1.0.106 specifically. This is
   the authoritative answer.
2. **Older niflib (C++)** branches that still claim NetImmerse v4–v10
   coverage. The current `nifly` source we have skews Skyrim+/FO4 and
   does not appear to handle pre-Gamebryo-1.0 deviations.
3. **niftools' Python `pyffi`** — has historically had broader
   pre-Gamebryo coverage; might be in the mod-tools ecosystem.

## Recommendation

This issue is **not single-session-fixable from current resources**.
Three options:

1. **Defer until Gamebryo 2.3 source is mounted** — open #688 stays as
   the tracker. Affected files are non-critical-path (UI/menu assets,
   one creature mesh) — interior cells render fine. Lowest risk.
2. **Ship a speculative skip-prefix recovery** for v < 10.2.0.0 NIFs
   (per-block u32 skip) and accept that 6 blocks parse, the rest
   fails. Marginal: recovers a few blocks per file but most files
   still effectively truncate.
3. **Ship the audit's debug assertion infrastructure** — even though
   it doesn't help these specific files (no `block_sizes`), it adds
   the diagnostic surface for future regressions on Skyrim+/FO4 root
   blocks. Useful but doesn't address #688's described symptom.

Pausing per fix-issue Phase 4 to ask the user.
