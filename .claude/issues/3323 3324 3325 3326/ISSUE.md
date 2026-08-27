## #3323 — FNV-RT-2026-08-26-03: interior window-portal transmission is pinned to the hard-coded noon-blue zenith, defeating the stated #925 TOD cross-fade on the exact cells it was written for
State: OPEN
Labels: bug renderer medium legacy-compat game:fnv shaders 

**Severity**: MEDIUM
**Dimension**: 3 — RT Lighting Pipeline
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/renderer/shaders/triangle.frag:1650-1672`,
`byroredux/src/render/sky.rs:56-79`, `crates/renderer/src/vulkan/context/mod.rs:883-892`,
`crates/renderer/src/vulkan/context/draw.rs:2312-2317`

**Premise verified**: the window-portal escape branch transmits `skyTint.rgb` with **no**
interior gate — unlike the two glass *miss* fallbacks, which #1125 correctly gated on
`isExteriorGlass` (`include/raytrace.glsl:46` for the reflection miss and
`triangle.frag:2163` for the refraction miss; both verified live, see Regression guards
below). Meanwhile:
- `sky.rs:70-77` — `build_sky_params` decides interiority once from `CellLightingRes` and,
  when interior, returns `SkyParams { dalc_cube, ..SkyParams::default() }`. Every TOD/weather
  field is dropped deliberately (#1199 / #2226: an interior must never read a stale exterior
  `SkyParamsRes`).
- `context/mod.rs:886` — `SkyParams::default().zenith_color = [0.15, 0.3, 0.6]`.
- `draw.rs:2312-2317` — `sky_tint = [zenith_color.xyz, sun_angular_radius]`.

So on any FNV interior `skyTint.rgb` is *always* `(0.15, 0.3, 0.6)`. The #925 comment sitting
directly above the site claims the opposite — "pull the sky colour from the active TOD/weather
palette … so interior windows cross-fade with night / dawn / dusk / storm … Pre-fix this was
hardcoded `vec3(0.6, 0.75, 1.0)` and Megaton / Vault 21 interiors always looked midday."
The hardcoded literal moved from the shader into `SkyParams::default()`; the behaviour it
described did not change for interiors.

**Impact (FNV-visible)**: architectural glass in FNV interiors that passes the portal test
(`materialKind == GLASS`, render layer Architecture, `texColor.a ∈ (0.02, 0.5)`,
`rtLOD < 2`, `dot(-V,N) > 0.1`, escape ray clears 2000 BU) transmits the same daylight blue
at 03:00 as at noon — Vault 21/34/22 window walls, the Novac motel-room and Dino Dee-lite
panes, and any Prospector/Strip interior with an exterior-facing pane. This is a *narrower*
version of the very symptom #925 says it fixed, still live because that fix only reached
exteriors.

**Fix sketch**: this is a policy question, not a one-line gate — an interior window genuinely
*does* see sky, so dropping to `sceneFlags.yzw` (the #1125 treatment) would be wrong here.
The correct source is the worldspace's current TOD sky colour, which an interior deliberately
does not upload. Either (a) plumb a separate `exteriorSkyTint` lane on `GpuCamera` that
survives interior cells (weather sim already runs — `CloudSimState` is documented as surviving
cell transitions), or (b) accept and re-document the limitation at both sites and delete the
now-false #925 claim. Do **not** simply widen the interior `SkyParams` bypass — that is
exactly what #2226 removed.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


## #3324 — FNV-2026-08-26-D4-01: WEAP `VATS` sub-record is undispatched — 245/261 FNV weapons lose AP cost, required skill, damage multiplier and silence level; the in-code comment blaming an "unconfirmed DNAM offset" has a false premise
State: OPEN
Labels: bug medium legacy-compat game:fnv esm-plugin 

**Severity**: MEDIUM
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/items.rs:245-262` (FNV `DNAM` arm),
field declared at `:119`, defaulted at `:184`.

**Premise verified**: `grep -n 'b"VATS"' crates/plugin/src/esm/records/items.rs`
returns nothing — there is no arm for the sub-record anywhere in the crate.
`ap_cost` is initialised `0.0f32` at `:184` and only ever written on the
`GameKind::Fallout4` path (`:296`). The FNV arm's own comment states:

```
// `ap_cost`'s true DNAM offset is unconfirmed; it stays
// at the zero default until the full layout is mapped.
```

**Evidence** — census of all 261 FalloutNV.esm WEAP records:

```
WEAP sub-record census: EDID:261 OBND:261 ETYP:261 DATA:261 DNAM:261 CRDT:261
  VNAM:261 FULL:251 VATS:245 INAM:237 …
  VATS sizes: {20: 242, 16: 3}
```

A dedicated `VATS` sub-record exists on **245 of 261** weapons. Decoding it as
`u32 effect + f32 + f32 + f32 + u8` (17 B payload padded to 20) produces
self-consistent, clean data — e.g. `WeapKnifeCombatCass`:

```
raw = 00000000 000048 42 3333333f 00006041 01000000
      effect=00000000  f32@4=50.00  f32@8=0.70  f32@12=14.00  u8@16=1
```

and the f32 at offset 12 across all 245 records is an entirely integral
distribution in the AP-cost range:

```
{0.0: 200, 14.0: 4, 15.0: 3, 16.0: 5, 17.0: 3, 22.0: 1, 25.0: 1, 26.0: 1,
 27.0: 1, 29.0: 1, 30.0: 4, 35.0: 4, 36.0: 2, 38.0: 2, 43.0: 1, 44.0: 2,
 45.0: 4, 48.0: 6}
```

45 weapons carry a non-zero value; DNAM was never the right place to look.

**Impact**: `ItemRecord::ap_cost` is pinned to `0.0` for every FNV weapon, and
three further authored fields (VATS required skill, VATS damage multiplier, VATS
silence level) have no destination struct at all. The memory note
*"VATS System — AP formulas already in CHARAL, but the runtime … doesn't exist
yet"* names the runtime as the gap; this finding shows the **data layer is also
missing its per-weapon inputs**, so the VATS runtime has nothing to read when it
lands. Silence level additionally feeds stealth detection. Currently latent —
`grep -rn ap_cost` outside `items.rs` finds only one test literal
(`byroredux/src/npc_spawn/tests.rs:468`) — so no visible defect today.

**Fix sketch**: add `b"VATS" if matches!(game, GameKind::Fallout3NV)` to
`parse_weap`, confirm the field order and semantics against xEdit's
`wbStruct(VATS,…)` / UESP `Mod_File_Format/WEAP` before naming the fields (do
not infer them from the histogram alone), and correct the stale `:252-256`
comment so no future audit re-searches DNAM.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


## #3325 — FNV-2026-08-26-D4-02: `WMI1` (faction → reputation) is dropped everywhere — all 82 FNV reputation bindings absent, leaving `index.reputations` an orphan map
State: OPEN
Labels: bug medium legacy-compat game:fnv esm-plugin 

**Severity**: MEDIUM
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/actor/mod.rs:1511-1551` (`parse_fact`
sub-record match — arms exist for `DATA`, `XNAM`, `MNAM` only);
`FactionRecord` struct at `:584-594` has no reputation field.

**Premise verified**: `grep -rn "WMI1" crates/ byroredux/ docs/` returns **zero
hits** across the entire repository. `RepuRecord`/`index.reputations` is
populated (`dispatch_misc_gameplay_b.rs:135`) and asserted (`>= 10`,
`parse_real_esm.rs:755`) but has no consumer beyond the count assertion.

**Evidence** — census over FalloutNV.esm, resolving every `WMI1` payload against
a whole-file FormID→record-type map:

```
FACT WMI1 count: 46   target record types: {'REPU': 46}
REFR WMI1 count: 36   target record types: {'REPU': 36}
  VRRCKarlFaction        0x17b5b6 -> 000F43DD  REPU (RepNVCaesarsLegion)
  PrivateKowalskiFaction 0x179162 -> 000F43DE  REPU (RepNVNCR)
  BoomerChildFaction     0x1630bf -> 000FFAE8  REPU (RepNVBoomer)
  vTopsPerformerFaction  0x16a265 -> 00118F61  REPU (RepNVTheStrip)
```

**82 of 82 WMI1 payloads resolve to a real REPU record — a 100% hit rate**, which
byte-proves the sub-record is the FACT/REFR→REPU link and not opaque data. The 13
REPU records are FNV's signature faction-reputation set (RepNVNCR,
RepNVCaesarsLegion, RepNVBoomer, RepNVGoodsprings, RepNVFreeside, RepNVNovac,
RepNVPrimm, RepNVFollowers, RepNVBrotherhood, RepNVGreatKhans, RepNVTheStrip,
RepNVWhiteGloveSociety, RepNVPowderGanger).

**Impact**: This is the most FNV-specific authoring in the whole file — reputation
replaces FO3's global karma and gates vendor prices, faction-armor disguise
reactions, quest branches, and hostile/idolized NPC greetings. Without the
FACT→REPU edge nothing can map an NPC's faction to the reputation meter it moves,
so the 13 parsed REPU records are unreachable: the reputation subsystem cannot be
built on top of the current index no matter what runtime lands.

**Fix sketch**: add `b"WMI1" if sub.data.len() >= 4 => record.reputation =
Some(remap_fid(...))` to `parse_fact` plus a `pub reputation: Option<u32>` on
`FactionRecord`; mirror it as `reputation_ref` on `PlacedRef` in
`cell/walkers.rs` for the 36 REFR-scoped overrides. Pin with a floor assertion in
`parse_rate_fnv_esm` (`>= 46` FACT bindings, all resolving into
`index.reputations`).

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


## #3326 — FNV-2026-08-26-D5-01: the per-block baseline keys on the parsed struct's name, not the wire RTTI — 142,649 FNV blocks (21.5%) are counted under a name they do not have
State: OPEN
Labels: bug nif-parser medium legacy-compat game:fnv test-gap 

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


