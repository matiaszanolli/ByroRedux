# FNV-2026-08-26-D5-01

**Issue**: #3326
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 5 — NIF Parser Regression Guard
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/tests/common/mod.rs:670-684` (`PerBlockHistogram::record_scene_blocks`)
· baseline `crates/nif/tests/data/per_block_baselines/fallout_nv.tsv`

**Premise verified**: `record_scene_blocks` buckets on `block.block_type_name()` — the *parsed
struct's* static name. Many dispatch arms deliberately parse several wire types into one struct
and preserve the discriminator in a **field** rather than the name (`BhkRigidBody.is_t`,
`BhkCollisionObject.is_blend`, `BsRangeNode.kind`, `NiPSysBlock.original_type`,
`NiTriShape::parse_segmented`). Those wire names therefore never reach the histogram.

**Evidence** — census of the FNV header block-type tables across all 11 archives vs. the
checked-in baseline rows:

| wire type (on disk) | FNV blocks | counted in the baseline as |
|---|---:|---|
| `NiTriStrips` | 57,796 | `NiTriShape` |
| `NiStringExtraData` | 32,161 | `NiExtraData` |
| `BSFadeNode` | 15,121 | `NiNode` |
| `BSXFlags` | 12,578 | `NiExtraData` |
| `bhkRigidBodyT` | 4,701 | `bhkRigidBody` |
| `bhkBlendCollisionObject` | 1,165 | `bhkCollisionObject` |
| `BSSegmentedTriShape` | 989 | `NiTriShape` |
| `BSMaterialEmittanceMultController` | 471 | `NiSingleInterpController` |
| `bhkConvexTransformShape` | 109 | `bhkTransformShape` |
| `BSBlastNode` / `BSDamageStage` / `BSDebrisNode` | 81 / 69 / 6 | `BSRangeNode` |
| `bhkSPCollisionObject` | 40 | `bhkPCollisionObject` |
| 30 × `NiPSys*` modifier/controller types | ~15,400 | `NiPSysBlock` |
| … 55 wire types total | **142,649 (21.5 %)** | |

Two baseline rows are provably not wire types at all:
* `NiSingleInterpController 614` — nif.xml:3646 declares it `abstract="true"`. No such block can
  exist on disk; the 614 are `BSMaterialEmittanceMultController` (471) +
  `BSRefractionStrengthController` (87) + `BSFrustumFOVController` (56) = **exactly 614**.
* `NiPSysBlock 15,446` — a parser-internal catch-all name that appears nowhere in nif.xml.

**Impact**: this is the gate whose declared purpose is catching *silent* parse loss. The blind
spot is precise: any change that re-routes wire type X from struct A to struct B where
`A::block_type_name() == B::block_type_name()` moves nothing in the TSV. On FNV that covers, at
minimum:
* `BSSegmentedTriShape` (989) reverting to plain `NiTriShape::parse` — the exact #146 bug, which
  leaves the `Num Segments` + `BSGeometrySegmentData[]` tail (nif.xml:6957-6961) unread and relies
  on block_size realignment. Histogram: unchanged.
* `bhkRigidBodyT` (4,701) losing `is_t = true` (`blocks/mod.rs:1189`) — the #2316 bug, which
  silently identity-collapses the CInfo translation/rotation on 28% of FNV rigid bodies, moving
  their colliders. Histogram: unchanged.
* `bhkBlendCollisionObject` (1,165) losing `is_blend` (`blocks/mod.rs:1183`). Histogram: unchanged.
* `BSDamageStage`/`BSBlastNode`/`BSDebrisNode` losing their `BsRangeKind` stamp (#364).
  Histogram: unchanged.

Each of the above does have a synthetic-fixture unit test, so the risk is mitigated — but the
corpus gate contributes nothing, which is not what its docstring claims, and it is why #3175's
regen needed a hand-verified arithmetic argument in the commit body instead of just going green.

**Fix sketch**: key the histogram on the header's wire name —
`header.block_types[header.block_type_indices[i]]` — instead of `block_type_name()`; keep
`NiUnknown` rows keyed the same way. The FNV histogram then reconciles 1:1 with the census above
(150 rows / 662,102), and no struct-name collapse can hide a re-route. A one-shot regen is needed
for all seven games; the totals are unchanged by construction.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
