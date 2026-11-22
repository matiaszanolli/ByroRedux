# Portable Save References — Product Requirements

**Status**: ACTIVE (2026-11-22). Revision 5 opens the tracking issues; revision 4 folds in review round 1; the history is §12.

**Tracking**: #7258 (parent). Phases: #7255 (0), #7256 (1), #7257 (2).

**Scope**: Persisted references to plugin records across save-registered components, resources and nested state, including the live reload path after plugins are added, removed or reordered. Runtime consumers continue to use session keys. This does not promise migration from old save formats, nor does it change original-game save files.

**Related**: this document states the promise; Phase 0 extends the designs to cover every mechanism below.

| Mechanism | Owning design |
|---|---|
| Save container, registry, reload and version rules | [Save and load](../engine/save-load-roundtrip.md) |
| The portable conversion path (`register_component_codec`, `SaveError::UnownedFormId`) that Phase 1 extends | [Save and load](../engine/save-load-roundtrip.md); it has no section yet, and Phase 0 writes one |
| The field-by-field catalog of registry-reachable identities (PSR-ID-001) | [Portable save reference inventory](../engine/portable-save-reference-inventory.md) (in progress since 2026-11-15) |
| Actor-value identity and the motivating changed-order scenario | [Oblivion Actor-Value Identity](oblivion-actor-value-identity.md) |
| Plugin identities and loading | [Plugin Loading](../engine/plugin-loading.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings. Requirement IDs use the `PSR-` prefix.

## 1. Product thesis

A player can add, remove or reorder plugins between saving and loading. A saved session must not silently attach inventory, script state or other records to a different plugin because a session slot changed. The actor-value pair codec alone does not provide that guarantee for the rest of the save.

The corollary is that the save boundary must classify each persisted identity by meaning (§3): a plugin record, runtime-created object, entity handle, pool index, sentinel or ordinary integer. Treating every `u32` alike would corrupt state.

## 2. Terminology

| Term | Meaning |
|---|---|
| **Durable record identity** | A plugin identity and local record id, independent of its current load-order slot. |
| **Runtime key** | A session-specific global FormID used by runtime consumers. |
| **Registry field** | A field reachable from any save-registered component or resource, including nested and parked state. |
| **Missing reference** | A durable identity that cannot resolve against the current plugin set. |
| **Pre-reload state** | Saved resources read while the destination cell is being constructed. |

## 3. Core model

Every registry field holding an identity belongs to exactly one class, and the class decides its disposition:

| Class | Meaning | Disposition on save and load | Reported by |
|---|---|---|---|
| **Plugin record** | A record authored in a plugin | Saved as a durable pair; resolved against the destination plugin set; a missing one is dropped where the catalog proves it safe, otherwise the load is refused before teardown (Q2) | PSR-LOAD-004's diagnostic |
| **Runtime-created id** | A form the session created (not owned by any plugin) | Kept with its own meaning, never read as a plugin record | the catalog row |
| **Entity handle** | An ECS entity id | Remapped through `build_form_id_remap` | the catalog row |
| **Pool index** | An index into a saved pool | Saved and restored with its pool | the catalog row |
| **Sentinel or null** | A reserved value such as 0 | Kept as-is | the catalog row |
| **Ordinary integer** | A count, flag or value that is not an identity | Kept as-is | the catalog row |

At HEAD on 2026-11-22, `byroredux/src/save_io/live_load.rs` reloads the saved plugin paths (the context comes from the snapshot), and `crates/save/src/driver.rs::build_form_id_remap` maps saved entities to live entities rather than mapping arbitrary fields. `Inventory::ItemStack.base_form_id` (registered raw), scripting faction changes and actor relocations are examples of saved raw record keys; `docs/audits/AUDIT_SAVE_2026-11-17.md` defers `SavedTimedRestoration.source_form_id` and `SavedActiveEffect.{source, mgef, caster, script}` to this PRD. The portable path already exists for actor values (OAI-SAVE): `SaveRegistry::register_component_codec` converts a column with the current world on save, restore and overlay and drops a row it cannot restore, and `SaveError::UnownedFormId` refuses a save whose key has no load-order identity. Phase 1 extends that codec to the catalogued fields and does not add a second path. The Phase 0 inventory replaces this illustrative list with a complete field-by-field catalog and its verification guard.

The current session's complete plugin set is the destination; the saved location and gameplay state are restored against it. Pre-reload state must resolve against the destination identity before it is consumed. A runtime-created identity and a sentinel each retain their distinct meaning; they are not assumed to be plugin records.

## 4. Goals and non-goals

### 4.1 Goals

- **PSR-G-001 — Identity preservation.** Persisted plugin-record references resolve to the same durable identities after a load-order change.
- **PSR-G-002 — No aliasing.** A missing plugin cannot cause a saved reference to name a different plugin's record.
- **PSR-G-003 — Complete coverage.** Every registry field is classified and every persisted plugin-record reference has a portable disposition.

### 4.2 Non-goals

- **PSR-NG-001.** Importing or exporting original-game saves; the Redux save container owns this contract.
- **PSR-NG-002.** A migrator chain for pre-change Redux saves; the save format version rule applies.
- **PSR-NG-003.** Changing runtime consumer APIs to expose durable pairs.

## 5. Requirements by area

### 5.1 Inventory and identity (`PSR-ID`)

- **PSR-ID-001.** Phase 0 MUST catalog every save-registered field that contains a record reference, including nested and parked state, with its owning type, its §3 class and its disposition.
- **PSR-ID-002.** A guard MUST detect a newly registered or changed saved type that has not been classified by the catalog. Observable: a test beside `byroredux/src/save_io/registry_completeness_tests.rs` that fails on an unclassified column, run by `cargo test -p byroredux --bin byroredux`.
- **PSR-ID-003.** Every persisted plugin-record reference MUST retain its durable identity across load-order changes, through the existing `register_component_codec` path (§3).
- **PSR-ID-004.** Runtime-created ids, entity handles, pool indices, nulls and ordinary integers MUST retain their own semantics rather than being interpreted as plugin identities.
- **PSR-ID-005.** Saved actor-value references MUST continue satisfying the `OAI-SAVE` requirements.

### 5.2 Restore (`PSR-LOAD`)

- **PSR-LOAD-001.** Live load MUST reconstruct the saved location using the current complete plugin set.
- **PSR-LOAD-002.** Each pre-reload reference MUST be resolved against the destination identity before its first consumer runs.
- **PSR-LOAD-003.** A missing reference MUST NOT resolve to a different record occupying its former slot.
- **PSR-LOAD-004.** A missing reference MUST produce a diagnostic identifying the durable identity and its owning field or entity. Observable: one `log::error!` line per missing reference, and, when the load is refused, the same text through `notify_player` (`live_load.rs`).
- **PSR-LOAD-005.** When safely dropping a missing reference cannot preserve the enclosing state's invariants, the load MUST fail before destructive teardown.
- **PSR-LOAD-006.** A failed preflight MUST leave the current live session and its ledgers unchanged.
- **PSR-LOAD-007.** Post-reload and parked state MUST use the same destination identity as pre-reload state.

### 5.3 Save compatibility (`PSR-SAVE`)

- **PSR-SAVE-001.** A persisted shape change MUST bump `FORMAT_MAJOR` and record the change in its version history.
- **PSR-SAVE-002.** The registry fingerprint and saved-type shape guard MUST be updated with the affected codecs.
- **PSR-SAVE-003.** A save MUST fail with a named diagnostic when a required runtime record key cannot be converted to a durable identity, as `SaveError::UnownedFormId` already does for actor-value keys.

## 6. Deliberate deviations and risks

This is a Redux save contract, not a claim about an original game's save format. Skyrim's own saves, for example, record the plugin list beside 3-byte RefIDs that index a saved form-id array, with a "Created" type for forms the session made (UESP *Skyrim Mod:Save File Format*, "RefID", "Plugin Info"), and the loader warns about missing content and lets the player continue (row 1).

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | A save that names a missing plugin loads after a prompt: "This save relies on content that is no longer present. Some objects may no longer be available. Continue Loading?" (CK wiki *Category:Settings*, `sSaveGameContentIsMissing`) | A missing reference is dropped per field where the catalog proves it safe; otherwise the load is refused before teardown (PSR-LOAD-003, PSR-LOAD-005) | none | A load the original would allow is refused → the PSR-LOAD-004 diagnostic names the identity and its field |

Risks with no original-engine counterpart:

| # | Risk | Handled by |
|---|---|---|
| 2 | Silent cross-plugin aliasing when slots are reused | PSR-LOAD-003 |
| 3 | Damaging dependent state when a referenced plugin is removed | PSR-LOAD-005 (refusal is the fallback when a safe disposition is not established) |

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | First end-to-end changed-order scenario, required by the actor-value PRD. |
| Fallout 3 | yes | Shared Redux save boundary and authored record identities. |
| Fallout: New Vegas | yes | Shared Redux save boundary and authored record identities. |
| Skyrim (LE / SE / AE) | yes | Shared Redux save boundary, including light-slot identities (SE saves carry a separate light-plugin list, "only for SE save games … info about ESL plugins", UESP *Skyrim Mod:Save File Format*, "Light Plugin Info"). |
| Fallout 4 | yes | Shared save boundary where these systems are supported. |
| Fallout 76 | no | No cell loader and no save session; online, out of the engine's single-player scope. |
| Starfield | yes | Shared save boundary where these systems are supported. |

### 7.2 Acceptance

1. Headless tests with synthetic plugins save and restore reordered regular and light slots, including reused slots, inventory/equipment, pre-reload ledgers, active effects, quests, parked state and runtime-created identities; compare each reference by durable identity.
2. Missing-plugin tests verify the field-specific disposition, named diagnostics, unaffected unrelated state and pre-teardown refusal where needed.
3. A registry-wide catalog guard (PSR-ID-002) passes and reports zero unclassified fields; save-format fingerprint and version tests pass.
4. The owner-run Oblivion smoke scenario saves, adds a plugin, reloads with the current plugin set and observes, through the attachable `byro-dbg` CLI, the same `GetAV Sneak` and the same base record for an inventory stack whose plugin's slot moved.
5. The touched crates' tests and `python3 scripts/check-prd-conformance.py` pass; owner-run save and incremental audits report no unresolved blocking findings.

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Registry field coverage | classified fields divided by all audited fields | per phase |
| Portable reference coverage | portable or justified non-record fields divided by all identified reference fields | per phase |
| Changed-order scenarios | passing scenarios divided by attempted scenarios | per game and phase |

Publish the changed-order result as a "Changed-order load" entry in each in-scope game's row of the ROADMAP compatibility matrix, beside the Oblivion row's existing "a save loads live … across a changed plugin set" sentence (OAI), and link the catalog from the save design.

## 9. Delivery sequence

**Phase 0 — Catalog and design (no engine code).** Inventory all registry-reachable fields into the [portable save reference inventory](../engine/portable-save-reference-inventory.md), classify each by §3, classify missing-reference outcomes and pre-reload dependencies, and extend the save design with the conversion path's section, a guard and an implementation sequence. Exit: every registered field has a class and a disposition, and each unresolved policy is decided or explicitly tracked.

**Phase 1 — Portable references.** Implement and test the cataloged codecs on `register_component_codec` and the identity handling, including pre-reload and parked state. Exit: the registry guard and synthetic changed-order and missing-plugin scenarios pass.

**Phase 2 — Live reload and acceptance.** Select the current plugin set, preflight conversion before teardown, and run the full reload scenarios. Exit: acceptance scenarios 1–5 pass, including owner-run live verification.

## 10. Decisions required

1. **Q1 — Changed-order scope.** Recommended: make all persisted plugin-record references portable before using the current plugin set for live reload. Alternative: restore only actor values portably while leaving other fields vulnerable to slot aliasing. **Decided** (2026-11-15): the owner approved the recommended broader portability direction.
2. **Q2 — Missing references.** Recommended: use a field-specific safe drop only where the catalog proves invariants hold; otherwise refuse before teardown. Alternative: drop every unresolved value, which can invalidate dependent state. **Decided** (2026-11-15): the owner approved the recommended policy; detailed dispositions are established in Phase 0.

## 11. Document map

| Need | Go to |
|---|---|
| Save container and reload design | [Save and load](../engine/save-load-roundtrip.md) |
| The Phase 0 catalog | [Portable save reference inventory](../engine/portable-save-reference-inventory.md) |
| Raw keys already deferred to this PRD | [Save audit, 2026-11-17](../audits/AUDIT_SAVE_2026-11-17.md) |
| Actor-value identity and its acceptance | [Oblivion Actor-Value Identity](oblivion-actor-value-identity.md) |
| Plugin identity | [Plugin Loading](../engine/plugin-loading.md) |
| Published compatibility metrics | [ROADMAP](../../ROADMAP.md) |

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-15 | Proposed full persisted-reference coverage and recorded the owner's direction | — |
| 2 | 2026-11-15 | Recorded the owner's approval and missing-reference policy | owner: approved |
| 3 | 2026-11-15 | PROPOSED → APPROVED at the owner's direction | — |
| 4 | 2026-11-22 | Review round 1 folded in: the existing portable codec path (`register_component_codec`, `SaveError::UnownedFormId`) named as what Phase 1 extends; the Phase 0 inventory and AUDIT_SAVE's deferred fields linked; an identity-class table; observables for PSR-ID-002, PSR-LOAD-004 and scenario 4; the warn-and-continue deviation (CK wiki `sSaveGameContentIsMissing`); Fallout 76 out of scope; the light-plugin source; the metric's ROADMAP column. The tracking issues and the ACTIVE transition are not part of this revision | claude: revise |
| 5 | 2026-11-22 | APPROVED → ACTIVE: tracking issues #7258 (parent) and #7255–#7257 (Phases 0–2) filed; no **Phase 0: verify** items are open | — |
