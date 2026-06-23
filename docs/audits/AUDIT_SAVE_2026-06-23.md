# Save / Load Subsystem Audit (M45 + M45.1) — 2026-06-23

Scope: `crates/save/src/{lib,snapshot,registry,driver,disk,validate}.rs`, the
crate integration tests (`crates/save/tests/round_trip.rs`), and the sole
engine-side consumer `byroredux/src/save_io.rs` (M45.1 live load-apply), plus
the cross-cut ground truth in `byroredux/src/main.rs`,
`byroredux/src/cell_loader/transition.rs`, `crates/core/src/ecs/world.rs`,
`crates/core/src/ecs/{packed,sparse_set}.rs`, `crates/core/src/string/mod.rs`,
and `crates/physics/src/sync.rs`.

Methodology per `.claude/commands/_audit-common.md`; severity per
`.claude/commands/_audit-severity.md` (Data loss is CRITICAL on that scale).
Every finding names its **Data-Loss Class**. Run inline (no sub-agents).

Dedup: no OPEN issue in the cached/`gh`-fetched list touches save/load/snapshot
(`grep -iE 'save|load|snapshot|corrupt|formid|durab|ring|validate'` on
`issues.json` returns only unrelated TD/condition/NIF rows). No prior
`AUDIT_SAVE_*` exists. **All findings below are NEW.**

---

## Executive Summary

The M45 crate and M45.1 live load-apply are **shipped and largely sound**. The
disk durability dance, the container gate ordering, the FNV schema fingerprint,
the form-id remap, the read-only frame-boundary capture, the off-frame `&mut`
drain, and the "validation gate blocks the only production write path" thesis are
all **code-confirmed**. The crate has real round-trip + remap regression
coverage.

The one materially serious finding is **SAVE-D6-01 (HIGH)**: `apply_deltas`
remaps a row's *key* (saved id → live id) but never remaps `EntityId` /
session-local-handle fields *inside* the component value. On a live load this
overwrites a freshly-spawned, correct `AnimationPlayer`/`AnimationStack` with one
carrying a stale `root_entity` and stale `clip_handle` — breaking animation on
reloaded actors. Everything else is MEDIUM/LOW (defense-in-depth, durability
residue, doc-rot).

### Docstring design claims — verified against code

| `lib.rs` / `snapshot.rs` claim | Verdict |
|---|---|
| Full ECS snapshot (not delta-log), size scales with loaded-cell entity count | **CODE-CONFIRMED** — `save_world` walks every registered column; empty columns omitted |
| Atomic write: tmp → fsync → read-back-verify → rename | **CODE-CONFIRMED** (`disk::write_slot`); residual dir-fsync gap → SAVE-D3-01 |
| Slot ring so a quicksave never clobbers the last good save | **CODE-CONFIRMED in-session**; in-memory cursor resets on restart → SAVE-D3-02 |
| Pre-save validation gate refuses a poisoned save | **CODE-CONFIRMED** — `SaveCommand::execute` aborts before `save_world`; no bypass path exists |
| CRC32 over payload detects power-cut/partial write | **CODE-CONFIRMED** (`encode`/`decode` over `[HEADER_LEN..payload_end]`) |
| Schema fingerprint catches type-set drift; intra-type change caught at load by serde | **CODE-CONFIRMED** (coarse-by-design) → see SAVE-D2-01 nuance |
| StringPool dumped/restored in symbol order so `FixedString` round-trips | **CODE-CONFIRMED** for the clear/restore path; live `apply_deltas` path never restores the pool, but no delta column carries a `FixedString` → SAVE-D1-02 (benign-today, latent) |
| Load runs off-frame (`&mut World`), drained between ticks | **CODE-CONFIRMED** (`step_save_loads` at main.rs:1341/2305) |
| Reproducible CRC across runs at equal state | **DRIFTED** — true only at column-KEY level + packed columns; sparse columns are insertion-ordered → SAVE-D1-01 |

### Doc-rot (KNOWN, confirmed)

`docs/feature-matrix.md:176` still reads `Save / load (M45) | … | M45
(unstarted)`. M45 + M45.1 shipped (crate, `save`/`save.info`/`load` console
commands, live load, player-pose restore). LOW → SAVE-D0-01.

### Findings by severity

- CRITICAL: 0
- HIGH: 1 (SAVE-D6-01)
- MEDIUM: 6 (SAVE-D1-01, SAVE-D2-01, SAVE-D3-01, SAVE-D3-02, SAVE-D4-01, SAVE-D6-02)
- LOW: 3 (SAVE-D0-01, SAVE-D1-02, SAVE-D2-02)

### Findings by Data-Loss Class

| Class | Findings |
|---|---|
| reference-break | SAVE-D6-01 (animation), SAVE-D6-02 (cell-resolve teardown) |
| corruption-on-load | SAVE-D4-01 (unchecked `ItemInstanceId`), SAVE-D1-02 (latent FixedString) |
| irrecoverable-write | SAVE-D3-01 (dir fsync), SAVE-D3-02 (ring session-reset) |
| silent-drop | none confirmed (the two-list parity holds — see Ledger) |
| none | SAVE-D1-01 (CRC-determinism doc), SAVE-D2-01/02 (robustness), SAVE-D0-01 (doc-rot) |

---

## Data-Loss Class Matrix

| Finding | Sev | Dimension | Data-Loss Class |
|---|---|---|---|
| SAVE-D6-01 | HIGH | M45.1 Live Load-Apply | reference-break |
| SAVE-D1-01 | MED | Snapshot Determinism | none (doc/contract) |
| SAVE-D2-01 | MED | Registry & (De)serialization | none (hardening) |
| SAVE-D3-01 | MED | Disk Format & Durability | irrecoverable-write |
| SAVE-D3-02 | MED | Disk Format & Durability | irrecoverable-write |
| SAVE-D4-01 | MED | Validation Gates | corruption-on-load |
| SAVE-D6-02 | MED | M45.1 Live Load-Apply | reference-break |
| SAVE-D0-01 | LOW | Doc | none |
| SAVE-D1-02 | LOW | Snapshot Completeness | corruption-on-load (latent) |
| SAVE-D2-02 | LOW | Registry & (De)serialization | none (robustness) |

---

## Completeness Ledger — `build_save_registry` × `MUTABLE_DELTA_COLUMNS`

`build_save_registry` (`byroredux/src/save_io.rs:119`) registrations cross-checked
against `MUTABLE_DELTA_COLUMNS` (`byroredux/src/save_io.rs:44`). The live load
only overlays a column present in BOTH lists. **No save-but-never-replay drift:
every mutable column is in both lists.**

| Registered column | Kind | In `MUTABLE_DELTA_COLUMNS`? | Classification |
|---|---|---|---|
| `Transform` | component (Packed) | yes | SAVED+OVERLAID |
| `Inventory` | component (Sparse) | yes | SAVED+OVERLAID |
| `EquipmentSlots` | component (Sparse) | yes | SAVED+OVERLAID |
| `LightSource` | component (Sparse) | yes | SAVED+OVERLAID |
| `LightFlicker` | component (Sparse) | yes | SAVED+OVERLAID |
| `AnimationPlayer` | component (Sparse) | yes | SAVED+OVERLAID (carries un-remapped refs — SAVE-D6-01) |
| `AnimationStack` | component (Sparse) | yes | SAVED+OVERLAID (carries un-remapped refs — SAVE-D6-01) |
| `ScriptTimer` | component (Sparse) | yes | SAVED+OVERLAID |
| `Name` | component (Sparse) | no | structural-identity (reloaded cell owns it) |
| `Parent` | component (Sparse) | no | structural-identity |
| `Children` | component (Packed) | no | structural-identity |
| `FormIdComponent` | form-id key (`apply: None`) | no | identity / remap key |
| `ItemInstancePool` | resource | n/a (restored wholesale via `restore_resources`) | SAVED resource |
| `CurrentCellContext` | resource | n/a | SAVED resource (drives reload target) |
| `PlayerPose` | resource | n/a | SAVED resource (drives pose restore) |

**Mutable-component completeness check** — every player-mutable component type in
the codebase is either registered above or documented reconstruct-on-load:
- Reconstructed (correctly absent): `GlobalTransform`/`WorldBound` (derived),
  `MeshHandle`/`TextureHandle`/`SkinnedMesh`/`RapierHandles` (GPU/physics
  handles), transient event markers (`ActivateEvent`/`HitEvent`/`TimerExpired`).
- No unregistered *mutable* state component was found → **no silent-drop**.

---

## Findings

### SAVE-D6-01: `apply_deltas` remaps the row key but not `EntityId`/handle fields inside the component value
- **Severity**: HIGH
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break
- **Location**: `crates/save/src/registry.rs:104-121` (the component `ApplyFn`); component fields at `crates/core/src/animation/player.rs:23` (`root_entity`), `:15` (`clip_handle`), `crates/core/src/animation/stack.rs:86` (`root_entity`), `:17` (`clip_handle`); overwrite confirmed against `byroredux/src/cell_loader/spawn.rs:1196`
- **Status**: NEW
- **Description**: The live load overlays each `MUTABLE_DELTA_COLUMNS` column via the `ApplyFn`, which does `filter_map(|(old, comp)| remap.get(&old).map(|&live| (live, comp)))` — it remaps the row **key** (`old` saved id → `live` id) but moves `comp` **verbatim**. Components whose serialized value embeds an `EntityId` or a session-local registry handle are therefore applied with stale references:
  - `AnimationPlayer.root_entity` / `AnimationStack.root_entity` (`Option<EntityId>`) hold a SAVED-session entity id, meaningless in the freshly reloaded cell (fresh ids).
  - `AnimationPlayer.clip_handle` / `AnimationLayer.clip_handle` (`u32`) index the `AnimationClipRegistry`, which is session-local and not guaranteed stable across a reload.
  The cell loader sets the *correct* fresh `root_entity` at `spawn.rs:1196` when it respawns the entity; the saved delta then **overwrites** that correct value with the stale one (`insert_bulk` is last-writer-wins on the remapped id — `crates/core/src/ecs/packed.rs:199-219` / default `insert_bulk` loop for sparse).
- **Evidence**: `ApplyFn` body moves `comp` unchanged; `AnimationPlayer`/`AnimationStack` both carry `root_entity: Option<EntityId>` + `clip_handle: u32`; both are in `MUTABLE_DELTA_COLUMNS`; the animation system scopes name lookups to `root_entity`'s descendants (player.rs:21-23 doc).
- **Impact**: Any reloaded animated actor/object that had a non-`None` `root_entity` or a meaningful `clip_handle` at save time gets its animation broken on a live `load`: name-scoped channel lookups target a wrong/absent subtree, or the clip resolves to nothing/another clip. Blast radius = every animated entity overlaid by a live load. The crate's `delta_apply_reroutes_by_form_id_after_cell_reload` test only covers `Transform`/`Inventory` (no embedded refs), so this is unguarded.
- **Related**: SAVE-D4-01 (validate runs on SAVE only — these stale ids are valid at save time, so the gate can't catch them); the same un-remapped-inner-field hazard applies to any future delta column carrying an `EntityId`.
- **Suggested Fix**: Either (a) exclude `AnimationPlayer`/`AnimationStack` from `MUTABLE_DELTA_COLUMNS` and let the reloaded cell own them (their post-spawn animation state is largely transient), or (b) teach the `ApplyFn`/`register_component` path to also remap declared inner `EntityId` fields (a per-type "remap hook"), and re-resolve `clip_handle` by clip identity rather than index. (a) is the low-risk fix; (b) is the general one.

---

### SAVE-D6-02: destructive teardown before a live load that can fail strands the engine in an empty cell
- **Severity**: MEDIUM
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break (player stranded in the void; no live-state loss on disk)
- **Location**: `byroredux/src/save_io.rs:518-554` (`execute_pending_save_loads`)
- **Status**: NEW
- **Description**: The drain tears down the current cell (`drain_streaming_state` + `unload_current_interior`) **before** calling `load_cell_with_masters`. If the reload errors (corrupt/missing ESM, renamed cell editor id), the function logs + `return`s — leaving the engine with the old cell already destroyed and no new cell loaded. The on-disk save is untouched (recoverable by relaunch), but the running session is left in an undefined empty-cell state with no in-engine recovery.
- **Evidence**: teardown at lines 518-521; `Err(e) => { log::error!(...); return; }` at lines 546-553, after teardown, before any restore.
- **Impact**: A `load` of a slot whose cell can't be reloaded drops the player into the void mid-session. Note `CurrentCellContext` *is* re-validated at drain (`snapshot_cell_context` at line 504 errors if it vanished between queue and drain — the defensive double-check the skill asked for is present and confirmed).
- **Related**: SAVE-D6-01.
- **Suggested Fix**: Attempt the reload into a staging area (or validate the cell editor id resolves) before tearing down the live cell; on reload failure, keep the current cell rather than leaving an empty world. At minimum surface a user-visible error rather than only a log line.

---

### SAVE-D4-01: validation gate does not check `ItemStack.instance` resolvability against `ItemInstancePool`
- **Severity**: MEDIUM
- **Dimension**: Validation Gates
- **Data-Loss Class**: corruption-on-load
- **Location**: `crates/save/src/validate.rs:49-58` (`validate_world` covers only Hierarchy / Equipment / Animation); ref class at `crates/core/src/ecs/components/inventory.rs:71` (`ItemStack.instance: Option<ItemInstanceId>`) → `crates/core/src/ecs/resources.rs:1364` (`ItemInstancePool`)
- **Status**: NEW
- **Description**: `validate_world` enumerates exactly three reference classes. `Inventory` rows can carry `ItemStack.instance` = an `ItemInstanceId` indexing the per-world `ItemInstancePool` (saved as a resource). The gate never checks that those instance ids resolve in the pool. A dangling `ItemInstanceId` (pool entry dropped while the stack referencing it survived) passes validation, is written, and on load indexes a non-existent / wrong instance — the exact "persist an inconsistent reference" the format's thesis claims to prevent.
- **Evidence**: `validate.rs` has no `ItemInstancePool`/`ItemInstanceId` reference. The crate's docstring (`validate.rs:11-13`) explicitly defers *cross-plugin FormId* checks to the binary; but the binary side (`SaveCommand::execute`) calls only the core `validate_world` and layers **no** additional check — so the deferred FormId-resolvability check is also **MISSING, not deferred** (a second instance of the same gap).
- **Impact**: A corrupted instance-pool reference, or an unresolvable `FormIdComponent`, seeds a corruption tail on load — defeating the format's whole defense-in-depth premise for those reference classes. Realistic only once inventory-instance churn / cross-plugin removal exists, but the gate claims to cover "the references that matter."
- **Related**: SAVE-D6-01; the docstring's "cross-plugin checks live in the binary" promise.
- **Suggested Fix**: Add an `ItemInstancePool`-resolvability sub-check to `validate_world` (it needs only core types) and a `ValidationKind::ItemInstance`; add the deferred binary-side FormId-resolve check (it needs the `DataStore`/`FormIdPool`) into `SaveCommand::execute` before the write, as the docstring promises.

---

### SAVE-D3-01: rename is not durable — parent directory is never fsynced after `rename`
- **Severity**: MEDIUM
- **Dimension**: Disk Format & Durability
- **Data-Loss Class**: irrecoverable-write
- **Location**: `crates/save/src/disk.rs:34-59` (`write_slot`)
- **Status**: NEW
- **Description**: `write_slot` fsyncs the **file** (`sync_all` at line 43) and does a byte-exact read-back before `rename` (lines 47-57) — so half-written *content* can never replace a good slot. But on most filesystems the directory entry created by `rename` is not durable until the **directory** is fsynced. A power cut after `rename` returns but before the dir metadata flushes can lose the rename (revert to the old slot, or lose both). The content-safety guarantee holds; the rename-durability tail does not.
- **Evidence**: No `File::open(dir)?.sync_all()` after the `fs::rename` at line 57. Header doc-comment (`disk.rs:1-12`) claims the dance is fully crash-safe.
- **Impact**: Residual power-cut window where a just-completed save's directory entry is lost. Lower probability than torn content (which is fully handled), hence MEDIUM not HIGH.
- **Related**: SAVE-D3-02.
- **Suggested Fix**: After `fs::rename`, open the parent dir and `sync_all()` it (best-effort; ignore `ENOTSUP` on filesystems that don't support dir fsync).

---

### SAVE-D3-02: `SaveRing` cursor is in-memory only — first quicksave after restart clobbers slot 0
- **Severity**: MEDIUM
- **Dimension**: Disk Format & Durability
- **Data-Loss Class**: irrecoverable-write
- **Location**: `crates/save/src/disk.rs:96-126` (`SaveRing`); install at `byroredux/src/main.rs:1015-1018` (ring size 10, cursor starts at 0)
- **Status**: NEW
- **Description**: `SaveRing` is "stateless on disk beyond the slot files" (its own doc, line 94). The cursor lives only in the `SaveState` resource for the session. On every relaunch the ring resets to cursor 0, so the **first** arg-less `save` of a new session writes slot 0 regardless of which slot is the newest on disk — silently overwriting slot 0's prior (possibly most-recent) save. This partially undoes the "ring so a quicksave never clobbers the last good save" goal across sessions.
- **Evidence**: `SaveRing::new` sets `cursor: 0` (line 105); no persistence of the cursor; no "resume from highest existing slot" logic; `list_slots` exists but is not consulted to seed the cursor.
- **Impact**: Cross-session quicksave can overwrite the previous session's slot-0 save. Within a session the ring works (test `ring_wraps`). MEDIUM: data-loss of slot 0's prior contents on the first post-restart quicksave.
- **Related**: SAVE-D3-01.
- **Suggested Fix**: Seed the ring cursor at startup from `list_slots` (e.g. start at `max(existing)+1 mod size`, or persist the cursor in a small sidecar / the newest save's mtime). Document the cross-session behavior either way.

---

### SAVE-D1-01: "reproducible CRC across runs at equal state" holds only at column-key level — sparse columns are insertion-ordered
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: none (doc/contract mismatch — the save still round-trips correctly)
- **Location**: docstring `crates/save/src/snapshot.rs:40-42`; row source `crates/save/src/registry.rs:85` (`q.iter().collect()`); iteration order `crates/core/src/ecs/sparse_set.rs:4` ("Iteration is dense but not sorted by EntityId") vs `crates/core/src/ecs/packed.rs` (entity-sorted)
- **Status**: NEW
- **Description**: `Snapshot.components`/`.resources` are `BTreeMap`, so column **keys** are deterministic. But the **rows** within a column come from `World::query::<T>().iter()`. For `PackedStorage` (Transform, Children, GlobalTransform) that is entity-id-sorted (stable). For `SparseSetStorage` (Name, Inventory, EquipmentSlots, LightSource, LightFlicker, AnimationPlayer, AnimationStack, ScriptTimer, FormIdComponent) it is **insertion order**, and swap-remove on despawn reorders the dense array. Two playthroughs reaching the same logical state via different spawn/despawn histories produce different row orders in sparse columns → different JSON byte order → different CRC. The "reproducible CRC across runs at equal state" claim is therefore false for sparse columns; it is a deterministic-per-run-at-equal-history hash, not a content hash.
- **Evidence**: `sparse_set.rs` iter zips `dense`/`data` (insertion order); `packed.rs insert_bulk` sorts by entity id. The CRC is computed over the serialized payload (`encode`), which serializes rows in iteration order.
- **Impact**: No data loss — restore is order-independent (`insert_batch` re-sorts/keys by id). The only consequence is that the docstring's "reproducible CRCs across runs at equal state" / "stable diffs" promise overstates the guarantee. MEDIUM doc/contract mismatch.
- **Related**: SAVE-D2-01.
- **Suggested Fix**: Either sort each saved column by entity id in `save_world` before serialize (cheap; makes the CRC a true content hash and stabilizes diffs), or soften the docstring to "deterministic per run / column-key order is stable; row order follows storage iteration."

---

### SAVE-D2-01: schema fingerprint is coarse by design — confirm no save-participating type masks an intra-type change
- **Severity**: MEDIUM
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none today (latent across versions)
- **Location**: `crates/save/src/registry.rs:234-249` (`schema_fingerprint`); decode minor-advisory note `crates/save/src/snapshot.rs:113-114`
- **Status**: NEW
- **Description**: `schema_fingerprint` is FNV-1a over kind-tagged, ordered column **keys** only — it catches add/remove/rename of a *type*, not a *field* change within a type (correctly documented at registry.rs:228-233). The intended backstop for intra-type change is `serde_json::from_value` **failing** at load. That backstop only fires if the new field is required. Combined with `decode`'s advisory `minor` (a newer minor still loads, serde default-fills), a future field added with `#[serde(default)]` or as `Option` would load an OLD save silently default-filled — masking the change rather than rejecting it.
- **Evidence**: Current save-participating structs were grepped: none carries `#[serde(default)]` today, so the trap is not yet sprung. `decode` does not reject on minor skew (snapshot.rs:113). FNV constants verified canonical (offset `0xcbf29ce484222325`, prime `0x100000001b3`); the hash depends only on names+order, no `TypeId`/address — confirmed stable across runs/builds.
- **Impact**: No current data loss; a forward-compat hazard. The moment a `#[serde(default)]`/`Option` is added to a saved struct without a major bump, old saves load silently downgraded.
- **Related**: SAVE-D1-01.
- **Suggested Fix**: Add a guard test (or doc rule) forbidding `#[serde(default)]`/new `Option` on save-participating structs without a `FORMAT_MAJOR` bump, until a versioned migrator chain exists. Optionally extend the fingerprint to hash a per-type schema version.

---

### SAVE-D2-02: `FormIdComponent` load closure panics (not `SaveError`) if `FormIdPool` is absent
- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none (panic, not silent loss; guarded in practice)
- **Location**: `crates/save/src/registry.rs:202-217` (load closure: `world.resource_mut::<FormIdPool>()`); panic semantics `crates/core/src/ecs/world.rs:597-611`
- **Status**: NEW
- **Description**: The save side resolves the pool defensively via `try_resource` and skips with a WARN on an unresolvable handle (registry.rs:186-196 — confirmed: save never panics on an unresolvable handle, and the form-id remap on the live path uses `try_resource` too, so it can't panic). But the **load** closure uses `resource_mut::<FormIdPool>()`, which **panics** ("Resource not found") if no pool is installed. A save containing a `FormIdComponent` column restored into a world without a `FormIdPool` aborts the whole load via panic rather than a `SaveError::Serde`. The live path always has a pool (boot + reloaded cell install one) and `restore_world` callers install one, so this is latent.
- **Evidence**: `resource_mut` `unwrap_or_else(panic!)` at world.rs:599-606; load closure calls it directly (registry.rs:210).
- **Impact**: Asymmetry with the defensive save side; an unexpected panic instead of a typed error in a degenerate restore. LOW.
- **Suggested Fix**: Use `try_resource_mut` and return `SaveError` (or insert a default `FormIdPool`) when absent, mirroring the save side's defensiveness.

---

### SAVE-D1-02: live `apply_deltas` path never restores the saved `StringPool` — latent dangling-symbol trap if a `FixedString`-bearing column is ever added to the delta set
- **Severity**: LOW
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: corruption-on-load (latent only)
- **Location**: `byroredux/src/save_io.rs:556-563` (live path = `restore_resources` + `apply_deltas`, no StringPool restore); contrast `crates/save/src/driver.rs:83` (`restore_world` does restore it)
- **Status**: NEW
- **Description**: The clear/restore path (`restore_world`) re-installs `StringPool::from_dump` so `FixedString` symbols resolve. The live load path deliberately does NOT (it overlays onto a reloaded cell that owns its own pool). This is **safe today** because every `MUTABLE_DELTA_COLUMNS` entry was verified free of `FixedString` fields (`AnimationStack`/`AnimationPlayer` use `FixedString` only in transient scratch helpers, not in serialized fields; `Inventory`/`ScriptTimer`/`Light*`/`Transform` carry none). But there is no guard: adding any `FixedString`-bearing component (e.g. `Name`) to `MUTABLE_DELTA_COLUMNS` would overlay symbol indices that mean nothing in the reloaded pool — silent string corruption.
- **Evidence**: live path calls only `restore_resources` (resources) + `apply_deltas` (components); no `StringPool::from_dump`. Delta columns grepped for `FixedString` in serialized fields — none.
- **Impact**: None today; a footgun for a future maintainer extending the delta set.
- **Related**: SAVE-D6-01 (same "delta column carries a cross-session reference" family).
- **Suggested Fix**: Document the invariant ("delta columns must not carry `FixedString`/`EntityId`/session-handle fields") at `MUTABLE_DELTA_COLUMNS`, ideally with a compile-time or test guard.

---

### SAVE-D0-01: feature-matrix still lists Save/load as "M45 (unstarted)"
- **Severity**: LOW
- **Dimension**: Documentation
- **Data-Loss Class**: none
- **Location**: `docs/feature-matrix.md:176`
- **Status**: NEW
- **Description**: The row reads `Save / load (M45) | Game sessions persist | M45 (unstarted)`. M45 (crate: snapshot/registry/disk/validate) and M45.1 (live load-apply + player-pose restore) have shipped, with console commands and round-trip + remap tests.
- **Evidence**: `crates/save/` exists with full impl + tests; `save`/`save.info`/`load` wired in `byroredux/src/save_io.rs` and installed in `main.rs:1014-1025`.
- **Suggested Fix**: Update the row to reflect M45 + M45.1 shipped (note the M45.1 live-load caveats from this audit).

---

## Confirmed-correct behaviours (verified, no finding)

These were checked against the skill checklist and **hold** — recorded so a future
change knows the invariant exists:

- **Validation gate is on the only write path** — `SaveCommand::execute` runs
  `validate_world` and aborts before `save_world`/`encode`/`write_slot`
  (save_io.rs:309-322). A repo-wide grep confirms no other production
  `save_world`/`write_slot`/`encode` caller. (Dim 4)
- **Live path never calls `restore_world`** — `execute_pending_save_loads` calls
  only `restore_resources` + `build_form_id_remap` + `apply_deltas`
  (save_io.rs:558-563). No id-collision risk from resurrecting saved ids onto
  reloaded ids. (Dim 5)
- **`next_entity` round-trips** in the clear/restore path (`save_world` records
  `world.next_entity_id()`; `restore_world` replays via `set_next_entity` before
  inserts — driver.rs:56/84). The release-mode silence of `insert_batch`'s
  `debug_assert` (world.rs:204) is real but unreachable here: restore always sets
  `next_entity` first, and the live path uses only remapped *live* ids. (Dim 1)
- **`apply_deltas` overlay is last-writer-wins** — `insert_bulk` dedups
  consecutive duplicate ids keeping the last (packed.rs:199-219), default
  `insert_bulk` loops single-insert overwrite for sparse — so a delta correctly
  *overwrites* the reloaded cell's row for the same remapped entity rather than
  duplicating it. (Dim 1/6)
- **Remap drops un-matched rows safely** — `ApplyFn` `filter_map`s out rows whose
  saved id isn't in the remap; no panic, no wrong-entity write
  (registry.rs:113-117). Entities without a form id are absent from the map and
  their deltas silently skipped (correct — the loader respawns them). (Dim 6)
- **`decode` gate ordering** — length → magic → major → schema_fpr →
  payload_len `checked_add` overflow → length bound → CRC → `from_slice`, all
  before any JSON parse (snapshot.rs:100-144). CRC scope is payload-only, so a
  header version edit is caught by the version gate, not CRC (test
  `rejects_major_version_skew`). (Dim 3)
- **`parse_slot_filename` strictness** — rejects `save_42.ess.tmp`, `save_x.ess`,
  `notes.txt` (test `parse_slot_names`), so a stray tmp never registers as a
  loadable slot. (Dim 3)
- **Capture is read-only + frame-boundary ordered** — `capture_player_pose`
  (`&World`, interior-mutable resource write) runs at main.rs:2300 AFTER the
  scheduler's camera systems publish this frame's Transform and BEFORE
  `step_save_loads` (2305). Pose source is post-propagation. (Dim 5/6)
- **Off-frame drain `take()`s the slot** — `execute_pending_save_loads` takes the
  `PendingSaveLoadSlot`, runs once, no-ops on empty (save_io.rs:495-503). Mirrors
  `PendingDebugLoadSlot`. Idempotent: teardown
  (`drain_streaming_state`+`unload_current_interior`) is unconditional before
  reload, so a second load resets to a clean cell rather than stacking deltas.
  (Dim 6)
- **Player-pose restore** — yaw/pitch always written to `InputState` (the
  rotation source of truth in both modes); Character+live-body sets Transform +
  GlobalTransform + clears `CharacterController` momentum
  (`vertical_velocity`/`is_grounded`/`wants_jump`) + `set_kinematic_translation`;
  the no-live-body fallback drops the camera via `reposition_camera`.
  `set_kinematic_translation` returns `false` (no panic) without a Rapier handle
  (sync.rs:64-71). Mode mismatch (saved Character / loaded FlyCam) falls through
  to the camera branch and never writes a body Transform. (Dim 6) — all guarded
  by `player_pose_round_trips_flycam` / `player_pose_character_tracks_body`.
- **Exterior/loose saves rejected at queue time** — `LoadCommand` refuses a
  snapshot with no `CurrentCellContext` before queuing (save_io.rs:463-467).
  (Dim 6)
- **Feature chain** — `crates/save` depends on `byroredux-core` with
  `features=["save"]`; `save=["inspect"]`, `inspect=["serde","serde_json",
  "glam/serde"]` (Cargo.tomls). A non-default build can't compile away the serde
  impls and ship a save crate that round-trips `null`. (Dim 2)

---

## Regression Guards Discovered

| Test | File | Invariant pinned |
|---|---|---|
| `full_world_round_trips_through_container` | `crates/save/tests/round_trip.rs:93` | sparse-id layout, hierarchy, inventory/equip, stable FormIdPair, `next_entity` survive save→encode→decode→restore |
| `empty_columns_are_omitted_from_the_snapshot` | `round_trip.rs:175` | empty columns produce no JSON column |
| `validation_catches_equipment_out_of_bounds` | `round_trip.rs:186` | `EquipmentSlots` occupant past `Inventory.len()` is caught |
| `validation_catches_dangling_parent` | `round_trip.rs:203` | `Parent` past `next_entity` flagged `DanglingEntity` |
| `validation_passes_on_a_consistent_world` | `round_trip.rs:215` | clean world → empty error vec |
| `delta_apply_reroutes_by_form_id_after_cell_reload` | `round_trip.rs:229` | FormId remap reroutes `Transform`/`Inventory` deltas to live ids (does NOT cover embedded-ref columns — SAVE-D6-01 gap) |
| `round_trips_through_container` + magic/truncated/payload/crc/schema/version tests | `crates/save/src/snapshot.rs:162-231` | every `decode` gate + payload-only CRC scope |
| `ring_wraps`, `ring_size_floored_to_one`, `parse_slot_names`, `write_read_round_trip_and_atomic_rename` | `crates/save/src/disk.rs:132-179` | ring round-robin, size floor, slot-name strictness, atomic write + no leftover tmp |
| `binary_registry_round_trips_including_scripttimer` | `byroredux/src/save_io.rs:604` | cross-crate `ScriptTimer` + the full curated registry round-trips |
| `fresh_world_validates_clean` | `save_io.rs:637` | save precondition |
| `save_then_load_command_queues_with_cell_context` | `save_io.rs:649` | command plumbing end-to-end (minus GPU drain); cell context survives disk round-trip |
| `player_pose_round_trips_flycam` / `player_pose_character_tracks_body` / `player_pose_survives_snapshot_round_trip` | `save_io.rs:698/746/782` | pose capture/restore in both modes + as a snapshot resource |

**Coverage gaps worth a follow-up test**: no test exercises a live `apply_deltas`
of `AnimationPlayer`/`AnimationStack` (would surface SAVE-D6-01); no test covers
a dangling `ItemInstanceId` (SAVE-D4-01); no test covers cross-session ring
cursor reset (SAVE-D3-02).

---

*Report generated by `/audit-save` (first run). Suggested next step:*
`/audit-publish docs/audits/AUDIT_SAVE_2026-06-23.md`
