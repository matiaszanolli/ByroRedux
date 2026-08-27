# 3323 / 3324 / 3325 / 3326 — investigation notes

## #3324 — WEAP `VATS` undispatched (FIXED)

The issue said *"confirm the field order and semantics against xEdit / UESP
before naming the fields (do not infer them from the histogram alone)"*. Rather
than reach for a remembered layout, `crates/plugin/examples/probe_weap_vats.rs`
(new) decodes every payload and reports each slot's distribution, plus resolves
the leading `u32` against a whole-file FormID→record-type map:

| offset | type | evidence | name |
|---|---|---|---|
| 0 | `u32` | 13 non-null payloads, **all 13 resolve to `SPEL`**; 232 null | VATS effect |
| 4 | `f32` | exactly two values corpus-wide: `0.0` ×202, `50.0` ×43 | required skill |
| 8 | `f32` | clusters at `0.5 / 0.7 / 1.0 / 1.25 / 2.0` | damage multiplier |
| 12 | `f32` | wholly integral, `14.0..=48.0` on 45 weapons | **AP cost** |
| 16 | `u8` | `{0: 3, 1: 239}` | silence level |

The distribution *shapes* name the fields — a two-valued threshold, a
multiplier around unity, an integral AP domain — so the naming rests on the
archive, not on recall. The silence-level enum's wider domain is deliberately
**not** asserted in the doc, because the corpus only evidences `{0, 1}`.

**Correction to the issue's fix sketch.** It proposed
`b"VATS" if matches!(game, GameKind::Fallout3NV)` and described the finding as
FO3/FNV. `Fallout3.esm` ships **160 `WEAP` records and zero `VATS`** — this is
FNV-only. The gate is still written on the `Fallout3NV` kind (the two games
share one variant) but is documented as FNV-only in practice, so nobody later
"fixes" a phantom FO3 gap.

Three of the 245 payloads are 16 bytes and stop before the silence byte, so
every read is length-guarded; a dedicated test pins that shape.

## #3325 — `WMI1` faction → reputation dropped (FIXED)

Both halves landed: `FactionRecord::reputation` (46 bindings) and
`PlacedRef::reputation_ref` (36 placement-scoped overrides).

`parse_fact` took no `remap` argument, so the signature grew one and the
dispatch site now threads `reader.get_form_id_remap()` — the same idiom `NPC_`
and `CREA` already use. Without that the FormID stays plugin-local and every
`index.reputations` lookup misses on any multi-plugin load, which is exactly
the #1996 failure. A test pins the self-referential remap case, not just the
happy path.

Null payloads map to `None`, not `Some(0)`: a null FormID masquerading as a
binding would make lookups miss confusingly rather than obviously.

The floor assertions in `parse_rate_fnv_esm` do more than count — they assert
every FACT binding **resolves into `index.reputations`**, so a binding pointing
at a non-REPU FormID (a remap bug, or a mis-read sub-record) fails loudly
rather than inflating a count. Verified green against real `FalloutNV.esm`.

## #3326 — per-block baseline keyed on struct name (FIXED)

`NifScene` does not retain its header, and the wire type table lives there, so
the harness re-parses the header off the buffer it already holds (a few hundred
bytes). `record_scene_blocks` now keys on
`header.block_types[header.block_type_indices[i]]`, falling back to the struct
name for pre-`V5_0_0_1` files (no type table) and for truncated scenes holding
fewer blocks than the header describes.

Every number in the issue's evidence table reproduces exactly:

```
NiTriStrips 57796   NiStringExtraData 32161   BSFadeNode 15121   BSXFlags 12578
bhkRigidBodyT 4701  bhkBlendCollisionObject 1165  BSSegmentedTriShape 989
BSMaterialEmittanceMultController 471  bhkConvexTransformShape 109
bhkSPCollisionObject 40
```

and both non-wire-type rows are gone (`NiSingleInterpController`, which nif.xml
declares `abstract="true"`, and the parser-internal `NiPSysBlock`).

**Totals are unchanged by construction, and that was checked rather than
asserted** — all seven games regenerated, every block total byte-identical:

| game | rows | blocks |
|---|---|---|
| fallout_nv | 101 → 150 | 662102 → 662102 |
| fallout_3 | 100 → 145 | 526109 → 526109 |
| fallout_4 | 72 → 114 | 805148 → 805148 |
| fallout_76 | 71 → 117 | 1548202 → 1548202 |
| oblivion | 85 → 120 | 329933 → 329933 |
| skyrim_se | 95 → 145 | 856103 → 856103 |
| starfield | 22 → 28 | 770322 → 770322 |

FNV's 150 rows / 662,102 blocks match the issue's prediction exactly. The
module doc's "How the rows are keyed" section argued the old scheme "still
gates them, in both directions"; that claim is now corrected in place rather
than deleted, so the reasoning error stays visible.

## #3323 — interior window portal pinned to noon blue (FIXED)

Premise verified end to end: `build_sky_params` returns `SkyParams::default()`
on any interior, `SkyParams::default().zenith_color == [0.15, 0.3, 0.6]`, and
`draw.rs` packs that into `sky_tint.rgb`, which the portal branch reads.

Took option (a). Option (b) was tempting for its zero renderer risk, but the
data source turns out to be real: **nothing in production removes
`SkyParamsRes`** (grepped — the only `remove_resource::<SkyParamsRes>` in the
tree is inside `sky_params_cleanup_tests.rs`), and `unload.rs`'s worldspace-
scoped note records that its lifetime matches the World, not the cell, since
#1199. So the live exterior sky *is* available inside an interior; it simply
was not uploaded.

The lane is deliberately separate from `zenith_color` rather than a widening
of it. That is not stylistic: `zenith_color` also feeds
`CompositeParams::sky_zenith` (`draw.rs`:815), so widening it on interiors
would move the composite sky too — which is precisely the interior sky leak
#2226 removed. A distinct field read by exactly one shader branch cannot
reopen it.

**The layout guards did their job.** The first `cargo test` after the shader
edit failed on two tests, and the reflection test named a shader I had missed:
`water.frag` declares `CameraUBO` via the shared include, so its committed
`.spv` was stale. Nine shaders reference `CameraUBO` in total (the five that
re-declare it plus `skin_vertices` / `ssao` / `volumetrics_inject` / `water.frag`
through `include/bindings.glsl`); all nine were recompiled. `GpuCamera` is now
368 B and both assertions were updated to say so.

### Live verification, and what it could not reach

Confirmed on FNV:

* interior-only boot → `SkyParamsRes: <not present — no exterior cell loaded>`,
  i.e. the documented fallback path is the one taken (unit-tested separately);
* after an interior → exterior transition → `SkyParamsRes` present with a real
  TOD zenith `[0.242, 0.320, 0.579]`, and a later sample read
  `[0.262, 0.338, 0.601]` — the weather sim is live and moving it.

The exterior → interior leg could **not** be driven, for a reason unrelated to
this fix and worth its own issue: `LoadedCellIndex` is installed only by
`cell_loader::load::load_cell_with_masters` (`load.rs`:568) and never by the
exterior streaming path, so activating an exterior door on a `--wrld/--grid`
boot logs

```
interaction: entity 693 activated, but its door transition was not queued:
no LoadedCellIndex resource; an ESM-driven cell load is required
```

and no transition occurs. So exterior→interior door travel does not work at
all on that route today. The interior half of #3323 is therefore covered by
unit test plus the code-level certainty that nothing removes the resource,
not by an end-to-end walk.
