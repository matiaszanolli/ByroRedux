# FNV-2026-08-26-D1-01

**Issue**: #3314
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 1 — Cell Loading End-to-End
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/cell/helpers.rs:20`, `crates/plugin/src/esm/cell/walkers.rs:261-373,1075-1090`, `crates/plugin/src/esm/cell/wrld.rs:75-160,419-461`, `crates/plugin/src/esm/cell/support.rs:390-400`

**Premise verified**: `EsmReader::read_record_header` (`crates/plugin/src/esm/reader.rs:557`)
routes every *record* FormID through `remap_form_id`, so every `EsmIndex` map
(`climates`, `waters`, `regions`, `lighting_templates`, `landscape_textures`,
`landscape_texture_sets`, `txst_textures`) is keyed in **global load-order space**.
The REFR walker in the same file follows that convention explicitly —
`walkers.rs:732 base_form_id = reader.remap_form_id(r.u32_or_default())`,
and again at :761, :773, :804-805, :814, :828, :835-836, :344.

The cell/worldspace/landscape sub-record readers do **not**. `read_form_id` is a
bare `u32::from_le_bytes` with no remap:

```rust
// crates/plugin/src/esm/cell/helpers.rs:20
pub(super) fn read_form_id(data: &[u8]) -> Option<u32> {
    (data.len() >= 4).then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}
```

26 call sites consume it raw (`grep -n read_form_id crates/plugin/src/esm/cell/*.rs`):
`XCWT`, `LTMP`, `XCIM`, `XCAS`, `XCMO`, `XCCM`, `XLCN`, `XCLR`, ownership `XOWN`/`XGLB`
in **both** `walkers.rs` (interior) and `wrld.rs` (exterior), plus WRLD `CNAM`
(climate), `NAM2` (water), `NAM3` (LOD water), `WNAM` (parent worldspace),
`NAM1` (default music).

Same defect in the LAND walker and the LTEX walker — both have a live
`&mut EsmReader` in scope and still read raw:

```rust
// crates/plugin/src/esm/cell/walkers.rs:1080  (ATXT; BTXT at :1071 is identical)
b"ATXT" if sub.data.len() >= 8 => {
    let mut r = SubReader::new(&sub.data);
    let ltex_id = r.u32_or_default();          // <-- raw, not reader.remap_form_id(..)
```

```rust
// crates/plugin/src/esm/cell/support.rs:391
b"TNAM" if sub.data.len() >= 4 => {
    let txst_id = u32::from_le_bytes([...]);   // <-- raw
    ltex_to_txst.insert(header.form_id, txst_id);   // key IS remapped, value is not
}
```

No post-walk repass fixes this: `crates/plugin/src/esm/records/mod.rs:493`
(`landscape_textures.insert(*ltex_id, path.clone())`) and
`crates/plugin/src/esm/cell/mod.rs:1230/1254` (the override merge) both pass the
raw values straight through.

**Evidence** — the remap is identity only while every plugin's *local* master index
equals its *global* load index. Verified FNV master lists on disk:

```
DeadMoney.esm:          ['FalloutNV.esm']
HonestHearts.esm:       ['FalloutNV.esm']
OldWorldBlues.esm:      ['FalloutNV.esm']
LonesomeRoad.esm:       ['FalloutNV.esm']
GunRunnersArsenal.esm:  ['FalloutNV.esm']
```

Every DLC has exactly one master, so its own forms carry local top byte `0x01`.
Under `FormIdRemap::remap` (`reader.rs:339-386`) a plugin at global slot 2 composes
`0x01xxxxxx → 0x02xxxxxx`. That is exactly what
`cargo run -- --master FalloutNV.esm --master DeadMoney.esm --esm HonestHearts.esm …`
produces — the repeatable-`--master` invocation CLAUDE.md documents as supported
(its Skyrim example is the same three-plugin shape). Two-plugin orders
(`--master FalloutNV.esm --esm HonestHearts.esm`) stay identity, which is why this
has never surfaced in the single-DLC smoke runs.

Once non-identity:
- `terrain.rs:158 landscape_textures.get(&ltex)` misses → every ATXT splat layer for
  that DLC's LAND is dropped at `log::debug!` level ("LTEX %08X not in
  landscape_textures map; skipping layer"), and `terrain.rs:596` falls the base
  layer back to `DEFAULT_LAND_TEXTURE` (`textures\landscape\dirt02.dds`).
- `water.rs:352 waters.get(&form)` misses → Zion / Big MT / The Divide water loses
  its WATR colours, flags, damage and normal map, falling to `WaterMaterial::default()`.
- `load.rs:697 index.lighting_templates.get(&form)` misses → the LTMP fallback dies.
- `env_translate.rs:1314 record_index.climates.get(&fid)` misses → the whole DLC
  worldspace drops to the procedural Mojave fallback sky.
- `RegionAmbientRes::resolve(&cell.regions, &index.regions)` misses → no REGN ambient.

**Impact**: Loading two or more FNV plugins ahead of a third silently degrades every
DLC worldspace to flat default-dirt terrain, default water, procedural sky and no
region ambient — with no `warn`-level diagnostic. It is a *silent* failure that
looks like missing content authoring rather than a load-order bug, and it scales
with load-order depth (the deeper the plugin, the more of its own forms miss).

**Fix sketch**: Thread the reader (or the already-fetched
`reader.get_form_id_remap()`) into `read_form_id` / `read_form_id_array` /
`parse_land_record`'s BTXT+ATXT arms / `parse_ltex_group`'s TNAM arm, exactly as
`walkers.rs:732` already does for REFR `NAME`. Add a multi-master fixture test that
asserts a DLC LAND's `ltex_form_id` resolves against `EsmIndex.landscape_textures`
when the DLC sits at global slot 2 with one local master.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
