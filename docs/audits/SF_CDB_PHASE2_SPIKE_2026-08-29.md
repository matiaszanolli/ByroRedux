# Starfield CDB Phase 2 — reverse-engineering spike

**Date**: 2026-08-29 · **Issue**: [#3398](https://github.com/matiaszanolli/ByroRedux/issues/3398)
**Verified against**: vanilla Starfield, `Starfield - Materials.ba2` →
`materials\materialsbeta.cdb` (105,037,616 bytes, BuildVersion `1.16.244.0`),
plus 3,085 distinct material paths read off real NIFs in
`Starfield - Meshes01.ba2`.

This spike exists because Phase 2 was blocked on knowledge that lived nowhere:
not in this repo, and not in the `Gibbed.Starfield` reference clone, whose
`ComponentDatabase/` layer stops at the generic reflection primitives
(`Class.cs`, `Field.cs`, `ObjectInstance.cs`, `Ref.cs`, …) with no material
semantics at all. Nobody had written down which CDB class carries a texture
path, or how a material is keyed to the path a NIF names.

Both questions are now answered. **Phase 2 is unblocked.**

Reproduce with:

```sh
cargo run --release -p byroredux-sfmaterial --example _tmp_cdb_phase2_spike
cargo run --release -p byroredux-nif --example _tmp_sf_matpath_dump -- \
  "$SF/Starfield - Meshes01.ba2" --limit 3000 > /tmp/matpaths.txt
cargo run --release -p byroredux-sfmaterial --example _tmp_cdb_hash_probe -- /tmp/matpaths.txt
```

---

## 1. The lookup key exists, and it is computable

`ComponentDatabaseFile::parse` yields 97 classes / 1,438,780 top-level
instances in 4.74 s. Two of those instances are the whole index:

| Instance | Class | What it holds |
|---|---|---|
| 0 | `BSMaterial::Internal::CompiledDB` | `HashMap: BSResource::ID → u64` — **48,749 entries**, one per material. Plus `BuildVersion`, and empty `Circular` / `Collisions` lists. |
| 1 | `BSComponentDB2::DBFileIndex` | `Objects` (500,403 `ObjectInfo`), `Components` (1,438,778 `ComponentInfo`), `Edges` (451,529 `EdgeInfo`), `ComponentTypes` (56 `ComponentTypeInfo`), `Optimized` |

`ObjectInfo` is the join:

```
<BSComponentDB2::DBFileIndex::ObjectInfo>
  .DBID               = <BSComponentDB2::ID> .Value = U32(2)
  .HasData            = Bool(false)
  .Parent             = <BSComponentDB2::ID> .Value = U32(0)
  .ParentPersistentID = <BSResource::ID> {0, 0, 0}
  .PersistentID       = <BSResource::ID> .Dir=2099118619 .Ext=496326191 .File=7627117
```

`ComponentInfo` carries `{ ObjectID: BSComponentDB2::ID, Type: u16, Index: u16 }`,
and `ComponentTypes` maps that `Type` u16 to a class name
(e.g. `124 → BSMaterial::DecalSettingsComponent`, `151 → BSMaterial::Color`,
`110 → BSMaterial::MaterialID`). `EdgeInfo { SourceID, TargetID, Type }` links
objects into the layer / texture-set / blender graph.

So the resolution chain a Phase-2 consumer needs is:

```
material path  →  BSResource::ID  →  ObjectInfo.PersistentID  →  ObjectInfo.DBID
               →  every ComponentInfo with that ObjectID
               →  ComponentTypes[Type] names the class
               →  Edges expand layers / texture sets / blenders
```

### `BSResource::ID` field labels are rotated — read this before using them

The struct decodes as three `u32`s named `Dir`, `Ext`, `File`. Measured over
all 48,749 keys, those names do **not** describe their contents:

| Our label | Distinct values | What it actually holds |
|---|---|---|
| `.Dir` | 48,423 | **CRC-32 of the file stem** (no directory, no extension) |
| `.Ext` | 2,168 | **CRC-32 of the directory path** (no trailing separator) |
| `.File` | **1** — always `0x0074616d` | **The extension, as packed ASCII** — `0x0074616d` is `"mat\0"` |

The constant column is the giveaway and is what made the mapping decidable
rather than a guess. Whether this is a Bethesda naming quirk or an
off-by-one in our own field-name resolution in
[`reader.rs`](../../crates/sfmaterial/src/reader.rs) is not settled here —
either way, a Phase-2 consumer must not trust the labels. Worth a follow-up
to check the class layout's declared field order against the offsets.

### The hash

Reflected **CRC-32, polynomial `0xEDB88320`, init `0`, no final XOR** — the
"Bethesda" variant, i.e. standard CRC-32 with both the initial and final
complement dropped. Input is the path **lowercased**, with **backslash**
separators, **no `data\` prefix**, hashed as two separate strings (directory,
then stem).

Tested against three normalisations × three CRC parameterisations; exactly one
combination matched, and it matched overwhelmingly:

```
MATCH  dir+stem, backslash, no data prefix | crc32 bethesda (init 0, xor 0)
       stem->Dir col: 3032/3084   dir->Dir col: 0/3084
```

**3,032 of 3,084 real NIF-named material paths (98.3 %) resolve against the
base CDB alone.** The `dir->Dir col: 0/3084` row is the control: the reversed
assignment matches nothing, so the column mapping above is not coincidental.

### Miss breakdown — and a finding for #3230

| Extension | Hit | Miss |
|---|---|---|
| `.mat` | 3,015 | 12 |
| `.bgsm` | 16 | 37 |
| `.bgem` | 1 | 3 |

Two things follow:

1. **`.bgsm` / `.bgem`-named references do key into the CDB.** The extension
   column is always `"mat"`, so the lookup ignores the name's own suffix —
   17 of the 57 `.bgsm`/`.bgem` references in this sample resolve to real CDB
   materials. That is directly relevant to **#3230** (which resolver should
   run for those paths on a Starfield session): they are not uniformly
   CDB-absent, so neither "always CDB" nor "always BGSM" is correct.
2. The 12 `.mat` misses are candidates for the 12 DLC / Creation-namespaced
   CDBs that `discover_starfield_cdbs` already finds under
   `materials\creations\<plugin>\`. Not confirmed — this spike only loaded
   the base CDB. Confirming it is cheap and should happen before the index is
   built, since it decides whether the index must be load-ordered last-wins
   across all 13 CDBs (almost certainly yes).

---

## 2. Per-field values are fully decoded

The earlier framing that the parser yields "only schema and instance counts"
is wrong. Leaves are materialised concretely, with **field names resolved from
the class layout**. Real values, verbatim from the spike:

```
<BSMaterial::MRTextureFile>
  .FileName = String("Data\\Textures\\Landscape\\Xophile\\crystal\\ExoticsCrystalCluster010_emissive.DDS")
<BSMaterial::MaterialParamFloat>
  .Value = Float(0.8)
<BSMaterial::Color>
  .Value = <XMFLOAT4> .x=0.782452 .y=0.633494 .z=0.604837 .w=1.0
```

61 distinct `BSMaterial::*` classes were reached at depth ≤ 4.

### The classes Phase 2 needs, by target field

| `ImportedMaterial` target | CDB class | Field(s) |
|---|---|---|
| texture roles | `BSMaterial::MRTextureFile`, `BSMaterial::TextureFile` | `.FileName` (a `Data\Textures\…\*.DDS` path; note the mixed `\` and `/` separators across instances) |
| texture role slot | `BSMaterial::TextureSetID`, `BSMaterial::LayerID`, `BSMaterial::BlenderID`, `BSMaterial::MaterialID`, `BSMaterial::UVStreamID` | `.ID.Value` — graph edges, not scalars |
| roughness / metalness | `BSMaterial::MaterialParamFloat` | `.Value` — **positional**: which param it is comes from the component's `Index`, not from a name |
| base colour | `BSMaterial::Color` | `.Value` (`XMFLOAT4`) |
| `has_alpha`, `alpha_test`, `alpha_threshold` | `BSMaterial::AlphaSettingsComponent` | `.HasOpacity`, `.AlphaTestThreshold`, `.UseDitheredTransparency`, `.OpacitySourceLayer`, `.Blender` (→ `AlphaBlenderSettings`) |
| blend mode | `BSMaterial::AlphaBlenderSettings` | `.Mode` (`"Linear"`), `.Position`, `.Contrast`, `.UseVertexColor`, `.VertexColorChannel`, `.HeightBlend*` |
| `is_decal` | `BSMaterial::DecalSettingsComponent` | `.IsDecal`, `.IsProjected`, `.IsPlanet`, `.BlendMode`, `.MaterialOverallAlpha`, `.WriteMask`, `.ProjectedDecalSetting` |
| emissive | `BSMaterial::EmissiveSettingsComponent` | `.Enabled` + `.Settings` → `EmittanceSettings { LuminousEmittance, EmissiveTint, EmissiveClipThreshold, EmissiveSourceLayer, ExposureOffset, AdaptiveEmittance, … }` |
| glass / effect optics | `BSMaterial::EffectSettingsComponent` | `.IsGlass`, `.IsAlphaTested`, `.AlphaTestThreshold`, `.BlendingMode` (`"AlphaBlend"`), `.MaterialOverallAlpha`, `.Frosting*`, `.SoftEffect`, `.Backlighting*`, `.UseFallOff` / `.FalloffStart*` / `.FalloffStop*` |
| translucency / SSS | `BSMaterial::TranslucencySettings` | `.Thin`, `.UseSSS`, `.SSSStrength`, `.SSSWidth`, `.TransmissiveScale`, `.SpecLobe0RoughnessScale`, `.SpecLobe1RoughnessScale` |
| layer opacity mixing | `BSMaterial::OpacityComponent` | `.First/Second/ThirdLayerIndex`, `.First/SecondBlenderMode`, `.SpecularOpacityOverride` |
| shader routing | `BSMaterial::ShaderRouteComponent`, `BSMaterial::ShaderModelComponent` | `.Route` (`"Deferred"`), `.FileName` (`"BaseMaterial"`) — **the material-kind discriminator** |
| UV transform | `BSMaterial::Offset`, `BSMaterial::Scale`, `BSMaterial::Channel` | `.Value` (`XMFLOAT2` / `String`) |
| sampler state | `BSMaterial::TextureAddressModeComponent`, `BSMaterial::TextureResolutionSetting`, `BSMaterial::MipBiasSetting` | `.Value` (`"Wrap"`), `.ResolutionHint` (`"UniqueMap"`) |
| flow / animated UV | `BSMaterial::FlowSettingsComponent` | `.FlowMap`, `.FlowSpeed`, `.FlowExtent`, `.FlowUVOffset`, `.FlowUVScale`, `.ApplyFlowOn{ANMR,Emissivity,Opacity}` |
| LOD | `BSMaterial::LevelOfDetailSettings`, `BSMaterial::LODMaterialID` | `.NumLODMaterials`, `.ID.Value` |
| physics | `BSMaterial::CollisionComponent` | `.MaterialTypeOverride.Value` (u32 — a `PhysicsMaterialType` hash, PHYSAL-relevant) |

Enum-valued fields are **strings**, not integers (`"Linear"`, `"Deferred"`,
`"MATERIAL_LAYER_0"`, `"AlphaBlend"`, `"Wrap"`, `"Red"`), which makes the
translation table readable but means every one needs an explicit match arm
with a documented default.

---

## 3. What is still hard

The remaining risk is **not** the vocabulary — it is memory. `parse` peaks at
**9.19 GB RSS** on the 105 MB CDB (87× blow-up, measured in
`AUDIT_STARFIELD_2026-08-16.md` § SF-D3-02, re-confirmed here). `ParseLimits`
does not help: it is a pre-walk reject on object-chunk count, so a low budget
returns `Err(ParseBudgetExceeded)` rather than a partial or streamed result.
Calling `parse` on the cell-load path is not viable, and there are 13 CDBs to
index, not one.

That makes an indexed or streaming reader variant the real Phase-2 work — and
it is now a well-specified problem rather than an open one, because we know
exactly what must survive the walk: the `CompiledDB.HashMap`, the
`DBFileIndex` object/component/edge tables, and the leaf fields of ~20 of the
61 `BSMaterial::*` classes.

## 4. Suggested sequencing (revised)

1. **Confirm the 12 `.mat` misses live in the DLC CDBs**, and settle
   load-order last-wins across all 13. Cheap; decides the index's shape.
2. **Streaming/indexed reader** in `crates/sfmaterial` — yield
   `BSResource::ID → material fields` without materialising 9 GB. The bulk of
   the work. Also re-export `Class` / `Field` / `ObjectInstance` / `Ref` /
   `TypeReference` from `lib.rs`, which today are private and prevent a
   downstream walker from being factored into helpers.
3. **Resolve the `BSResource::ID` label rotation** in the reader (§1) so
   downstream code reads `.Dir` and gets a directory.
4. **Provider plumbing** in `byroredux/src/asset_provider/material.rs` —
   build the load-ordered index at `discover_starfield_cdbs`, store it on
   `MaterialProvider`, cache it beside `sf_cdb_cache` so it survives the
   per-cell provider rebuild.
5. **The merge arm** (`material.rs`, the `.mat` branch) — mirrors the BGSM arm
   directly; ~80 lines. Settle **#3230** here, informed by §1: `.bgsm`/`.bgem`
   names hit the CDB sometimes, so the resolver order must be try-then-fall-
   through, not an unconditional gate.
6. **Invert the pinned invariants** in
   `byroredux/src/asset_provider/tests/starfield_mat.rs:177-188`, which
   deliberately assert today's zero-forwarding state.

## Artifacts

- `crates/sfmaterial/examples/_tmp_cdb_phase2_spike.rs` — HashMap shape,
  column-constancy proof, `ObjectInfo` layout, one expanded example per
  `BSMaterial::*` class.
- `crates/sfmaterial/examples/_tmp_cdb_hash_probe.rs` — the hash brute-force
  and the per-extension hit/miss breakdown.
- `crates/nif/examples/_tmp_sf_matpath_dump.rs` — the NIF-side path corpus the
  probe tests against.
