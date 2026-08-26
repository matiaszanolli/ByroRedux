# #2369 (EX-14/15) — buildable slice 2 (2026-08-26): item C2's reconcile half

**This section.** A second, unrelated buildable slice of the same epic —
EX-14/15 item C2's "reconcile instead of re-spawning" half, the sequencing
step `docs/engine/stream-boundary-state-continuity.md` §7 names as step 2
of 3 (step 1, `CellRootRefIndex`, landed 2026-08-23 in `27ae9e09`; step 3,
EX-16 item 4's full snapshot/restore, is #3299 and stays blocked on this
step landing and holding up in practice).

## Scope taken

Only the identity-comparison-and-skip mechanism the design doc's §1 table
and §7 describe: **compare resolved persistent-CELL identity across a
worldspace crossing before draining; skip the persistent-CELL drain+rebuild
specifically when it's unchanged, while the ordinary grid tiles still always
drain.** Not attempted: EX-16 item 4's snapshot/restore (blocked on this
landing first, per the doc's own sequencing — filed separately as #3299).

## What changed

- `cell_loader::exterior::persistent_cell_identity_unchanged` — pure core
  (no `World`): does `new_index`/`new_worldspace_key` resolve, via the
  already-landed `resolve_persistent_cell`, to the same FormID as
  `current_form_id`? Unit-tested the same way `resolve_persistent_cell`
  itself is (child→parent, sibling→sibling via shared ancestor, distinct
  persistent CELL, no resolvable destination).
- `cell_loader::exterior::persistent_root_survives_crossing` — thin `World`
  wrapper: reads the currently-active root's `CellFormId` component and
  calls the pure fn above.
- `scene::assemble_exterior_streaming` gained a `preserved_persistent_root:
  Option<EntityId>` parameter, installed on the fresh `WorldStreamingState`
  *before* `stream_initial_radius` runs, so that fn's existing
  `persistent_root.is_none()` guard skips the fresh spawn instead of
  building one and leaking the old one underneath it.
- `App::step_cell_transition`'s Exterior arm (`app_step.rs`) — the one real
  wiring site. Restructured to build the destination's `ExteriorWorldContext`
  *before* draining (previously built only after, inside
  `begin_exterior_streaming`, called after the drain already ran).
  Necessary ordering, not just cleanliness: the identity comparison needs
  the destination's resolved cell index, which doesn't exist until this
  parse runs — deciding "should we preserve" is only possible before the
  destructive drain if the parse also happens before it. Bypasses
  `begin_exterior_streaming` in favor of calling
  `assemble_exterior_streaming` directly with the already-built context
  (mirroring the pattern `save_io::reload_exterior_session` already used,
  which is exactly what `assemble_exterior_streaming`'s own doc comment
  anticipated as a future caller shape).

## A latent bug this restructuring incidentally fixes

Pre-fix, `step_cell_transition`'s Exterior arm drained the old streaming
state *unconditionally*, then tried to build the new one — so a failure at
that second step (missing/corrupt destination ESM, e.g.) left the player in
an empty, undrivable world. Building the context first and bailing before
any drain if it fails (this change's necessary ordering) closes that gap
too, the same way `save_io`'s SAVE-D6-02 fix closed the equivalent gap on
the save-load path. Not the point of this change, but free.

## Deliberately NOT wired into `save_io::reload_exterior_session`

The design doc's §6 names this reload path as one of three candidate wiring
sites. Investigated and declined: a save load's correctness model is
"always rebuild fresh from ESM, then restore" — the full per-component
registry round-trip for most components, plus `MUTABLE_DELTA_COLUMNS`'s
targeted FormID-keyed overlay (`Transform`, `WanderState`, `TravelState`,
`Traveled`, etc.) for the rest. A preserved LIVE persistent root would never
pass through either restore step, so its entities would silently keep
whatever state the *current* session left them in instead of the state
actually recorded in the save file being loaded — a real save-fidelity
regression disguised as an optimization, and not a rare edge case: it would
fire on the common case of reloading the same worldspace you were already
in. `reload_exterior_session` passes `None` explicitly, with this same
reasoning inlined as a comment at the call site.

## Verification

`persistent_cell_identity_unchanged`'s 4 unit tests (child→parent,
sibling→sibling, own-persistent-CELL is-changed, no-resolvable-destination
is-changed) are the correctness-critical, decidable-without-I/O piece. The
wiring itself (detach-before-drain, reattach-after-assemble) is mechanical
glue around already-tested primitives (`drain_streaming_state`,
`assemble_exterior_streaming`, `WorldStreamingState::new`); an end-to-end
"the entity really survives a live crossing" test needs a `VulkanContext` +
on-disk game data, out of `cargo test` scope, matching this file's own
established convention for untestable wiring paths (see
`logical_stub_source_pin_tests` above). Full workspace suite: 6000 passing
(was 5996), 0 failing, 163 ignored, zero new warnings.

---

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
