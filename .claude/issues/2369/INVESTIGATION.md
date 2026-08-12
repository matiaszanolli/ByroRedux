# #2369 (EX-14/15) — buildable slice: FO4 precombine CSG routing

## Scope taken

`#2369` bundles EX-14 (ground cover + trees) and EX-15 (persistent refs,
parent worlds, FO4 spatial data). Per
[`docs/engine/exterior-readiness-plan.md`](../../../docs/engine/exterior-readiness-plan.md)
most of it is blocked:

- EX-14 ground cover Phases 1–5 — §11 open questions need measurement first,
  and the density field lives in GLSL where unit tests cannot reach it.
  (Phase 0, the EXAL types + translate boundary, landed in `e26d35f3`.)
- EX-14 full SpeedTree rendering — out of ground-cover scope by §10.
- EX-15 persistent refs across parent worlds — blocked on EX-09 (#2370).

The **FO4 spatial-data** half of EX-15 is *not* blocked, and its acceptance
clause ("precombine/previs/occlusion data has explicit render, collision,
fallback, and mod-invalidation coverage") is where a real, measurable defect
was sitting. That is what this change closes.

## The defect

`PrecombinedSpawnJob` picked the `<Plugin> - Geometry.csg` blob from the
**cell's owning plugin** — the remapped form-id mod-index byte → load order
(#1590). That is right for the `_oc.nif` *filename* and wrong for the
*geometry*: a plugin that re-bakes a master-owned cell keeps the master's
root `meshes\precombined\<cellfid>_<hash>_oc.nif` name (so the form-id byte
still says `Fallout4.esm`) while storing the new geometry in its own blob.

`docs/engine/fo4-csg-format.md` already named this as the residual
"override-rebake edge", pending a BSCRC32 reproduction that had not happened.

### Measured on the installed FO4

Scanning every plugin in the Data directory for CELL records that override a
`Fallout4.esm` cell carrying `XCRI`:

| | cells |
|---|---|
| override keeps `XCRI` | 3,234 (94 with a *changed* hash list — a re-bake) |
| override drops `XCRI` (invalidates, correctly falls back) | 416 |

The five DLCs alone account for ~460 kept overrides. Decoding one of them
(`meshes\precombined\0000d8ac_1b92095f_oc.nif`, shipped in
`DLCCoast - Main.ba2`) both ways:

```
Fallout4 - Geometry.csg  →  0 meshes     ← what the code picked
DLCCoast - Geometry.csg  →  6 meshes
```

Silent loss, not an error: a `data_offset` is only meaningful inside its own
PSG space, so the wrong blob reads well-formed garbage or nothing at all.
Where a cell mixes vanilla and re-baked hashes the vanilla ones still spawn,
`pc_spawned > 0` holds, and the absorbed-REFR gate then suppresses the very
REFRs whose geometry just failed to decode — a hole with no fallback.

## BSCRC32, solved rather than guessed

`BSPackedGeomObject::filename_hash` names the blob directly. Six independent
ground-truth pairs were read out of the installed archives (the single hash
every `_oc.nif` in each game/DLC archive carries), and the four CRC
parameters were searched against all six at once. Exactly one set reproduces
every pair: reflected CRC-32, poly `0xEDB88320`, **init 0, no final xor**,
over the **lowercased** `"<plugin stem> - geometry"`.

| plugin | hash |
|---|---|
| `Fallout4.esm` | `0xddf19a67` |
| `DLCCoast.esm` | `0x2088054d` |
| `DLCNukaWorld.esm` | `0xe81b308e` |
| `DLCRobot.esm` | `0x3a1b90b8` |
| `DLCworkshop01.esm` | `0x8e566007` |
| `DLCworkshop03.esm` | `0x626dfe98` |

That table is the unit test in `crates/bsa/src/csg.rs`.

## What changed

- `byroredux_bsa::bscrc32` / `csg_name_hash`.
- `precombine_csg_filename_hashes(scene)` in the NIF crate — the cheap half
  of `collect_precombine_geom_refs` (no owning-shape search, no material
  translation), so the loader can pick a blob before paying for the full walk.
- `PrecombinedSpawnJob` hashes the whole load order once, then opens blobs
  lazily by the hash each `_oc.nif` names. `build_precombine_meshes` takes a
  resolver and **skips** objects whose blob isn't open rather than reading
  another plugin's PSG space at those offsets.
- `resolve_precombine_owner` keeps its job — the filename and subdir — and
  its doc now says so.

## Not changed, deliberately

The absorbed-REFR gate (`absorbed_refs_or_empty`) is unchanged. The 12
measured "XCRI kept, XPRI dropped" overrides leave precombines spawning while
the base's absorbed REFRs render — but that is what the winning CELL record
says, and real FO4 reads the same record the same way. Diverging there is a
policy decision about vanilla fidelity, not a bug fix, and belongs to whoever
takes EX-15's mod-invalidation clause as a whole.
