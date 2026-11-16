# Portable Save References — Product Requirements

**Status**: APPROVED (2026-11-15). Revision 3 records the owner-directed transition after approval and the missing-reference decision.

**Scope**: Persisted references to plugin records across save-registered components, resources and nested state, including the live reload path after plugins are added, removed or reordered. Runtime consumers continue to use session keys. This does not promise migration from old save formats, nor does it change original-game save files.

**Related**: this document states the promise; Phase 0 extends the designs to cover every mechanism below.

| Mechanism | Owning design |
|---|---|
| Save container, registry, reload and version rules | [Save and load](../engine/save-load-roundtrip.md) |
| Actor-value identity and the motivating changed-order scenario | [Oblivion Actor-Value Identity](oblivion-actor-value-identity.md) |
| Plugin identities and loading | [Plugin Loading](../engine/plugin-loading.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings. Requirement IDs use the `PSR-` prefix.

## 1. Product thesis

A player can add, remove or reorder plugins between saving and loading. A saved session must not silently attach inventory, script state or other records to a different plugin because a session slot changed. The actor-value pair codec alone does not provide that guarantee for the rest of the save.

The corollary is that the save boundary must classify each persisted identity by meaning: a plugin record, runtime-created object, entity handle, pool index, sentinel or ordinary integer. Treating every `u32` alike would corrupt state.

## 2. Terminology

| Term | Meaning |
|---|---|
| **Durable record identity** | A plugin identity and local record id, independent of its current load-order slot. |
| **Runtime key** | A session-specific global FormID used by runtime consumers. |
| **Registry field** | A field reachable from any save-registered component or resource, including nested and parked state. |
| **Missing reference** | A durable identity that cannot resolve against the current plugin set. |
| **Pre-reload state** | Saved resources read while the destination cell is being constructed. |

## 3. Core model

At HEAD on 2026-11-15, `byroredux/src/save_io/live_load.rs` reloads the saved plugin paths, and `crates/save/src/driver.rs::build_form_id_remap` maps saved entities to live entities rather than mapping arbitrary fields. `Inventory::ItemStack.base_form_id`, scripting faction changes and actor relocations are examples of saved raw record keys. `actor_value_save.rs` already stores actor-value keys as durable pairs. The Phase 0 inventory replaces this illustrative list with a complete field-by-field catalog and its verification guard.

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

- **PSR-ID-001.** Phase 0 MUST catalog every save-registered field that contains a record reference, including nested and parked state, with its owning type, semantics and disposition.
- **PSR-ID-002.** A guard MUST detect a newly registered or changed saved type that has not been classified by the catalog.
- **PSR-ID-003.** Every persisted plugin-record reference MUST retain its durable identity across load-order changes.
- **PSR-ID-004.** Runtime-created ids, entity handles, pool indices, nulls and ordinary integers MUST retain their own semantics rather than being interpreted as plugin identities.
- **PSR-ID-005.** Saved actor-value references MUST continue satisfying the `OAI-SAVE` requirements.

### 5.2 Restore (`PSR-LOAD`)

- **PSR-LOAD-001.** Live load MUST reconstruct the saved location using the current complete plugin set.
- **PSR-LOAD-002.** Each pre-reload reference MUST be resolved against the destination identity before its first consumer runs.
- **PSR-LOAD-003.** A missing reference MUST NOT resolve to a different record occupying its former slot.
- **PSR-LOAD-004.** A missing reference MUST produce a diagnostic identifying the durable identity and its owning field or entity.
- **PSR-LOAD-005.** When safely dropping a missing reference cannot preserve the enclosing state's invariants, the load MUST fail before destructive teardown.
- **PSR-LOAD-006.** A failed preflight MUST leave the current live session and its ledgers unchanged.
- **PSR-LOAD-007.** Post-reload and parked state MUST use the same destination identity as pre-reload state.

### 5.3 Save compatibility (`PSR-SAVE`)

- **PSR-SAVE-001.** A persisted shape change MUST bump `FORMAT_MAJOR` and record the change in its version history.
- **PSR-SAVE-002.** The registry fingerprint and saved-type shape guard MUST be updated with the affected codecs.
- **PSR-SAVE-003.** A save MUST fail with a named diagnostic when a required runtime record key cannot be converted to a durable identity.

## 6. Deliberate deviations and risks

This is a Redux save contract, not a claim about an original game's save format. The risk is silent cross-plugin aliasing when slots are reused; `PSR-LOAD-003` forbids it. A second risk is damaging dependent state when a referenced plugin is removed; `PSR-LOAD-005` makes refusal the fallback when a safe disposition is not established.

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | First end-to-end changed-order scenario, required by the actor-value PRD. |
| Fallout 3 | yes | Shared Redux save boundary and authored record identities. |
| Fallout: New Vegas | yes | Shared Redux save boundary and authored record identities. |
| Skyrim (LE / SE / AE) | yes | Shared Redux save boundary, including light-slot identities. |
| Fallout 4 | yes | Shared save boundary where these systems are supported. |
| Fallout 76 | yes | Shared save boundary where these systems are supported. |
| Starfield | yes | Shared save boundary where these systems are supported. |

### 7.2 Acceptance

1. Headless tests with synthetic plugins save and restore reordered regular and light slots, including reused slots, inventory/equipment, pre-reload ledgers, active effects, quests, parked state and runtime-created identities; compare each reference by durable identity.
2. Missing-plugin tests verify the field-specific disposition, named diagnostics, unaffected unrelated state and pre-teardown refusal where needed.
3. A registry-wide catalog guard passes and reports zero unclassified fields; save-format fingerprint and version tests pass.
4. The owner-run Oblivion smoke scenario saves, adds a plugin, reloads with the current plugin set and observes the same `GetAV Sneak` and representative non-actor-value state using the attachable `byro-dbg` CLI.
5. The touched crates' tests and `python3 scripts/check-prd-conformance.py` pass; owner-run save and incremental audits report no unresolved blocking findings.

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Registry field coverage | classified fields divided by all audited fields | per phase |
| Portable reference coverage | portable or justified non-record fields divided by all identified reference fields | per phase |
| Changed-order scenarios | passing scenarios divided by attempted scenarios | per game and phase |

Publish results in ROADMAP's per-game compatibility matrix and link the catalog from the save design.

## 9. Delivery sequence

**Phase 0 — Catalog and design (no engine code).** Inventory all registry-reachable fields, classify missing-reference outcomes and pre-reload dependencies, and extend the save design with a guard and implementation sequence. Exit: every registered field has a disposition and each unresolved policy is decided or explicitly tracked.

**Phase 1 — Portable references.** Implement and test the cataloged codecs and identity handling, including pre-reload and parked state. Exit: the registry guard and synthetic changed-order and missing-plugin scenarios pass.

**Phase 2 — Live reload and acceptance.** Select the current plugin set, preflight conversion before teardown, and run the full reload scenarios. Exit: acceptance scenarios 1–5 pass, including owner-run live verification.

## 10. Decisions required

1. **Q1 — Changed-order scope.** Recommended: make all persisted plugin-record references portable before using the current plugin set for live reload. Alternative: restore only actor values portably while leaving other fields vulnerable to slot aliasing. **Decided** (2026-11-15): the owner approved the recommended broader portability direction.
2. **Q2 — Missing references.** Recommended: use a field-specific safe drop only where the catalog proves invariants hold; otherwise refuse before teardown. Alternative: drop every unresolved value, which can invalidate dependent state. **Decided** (2026-11-15): the owner approved the recommended policy; detailed dispositions are established in Phase 0.

## 11. Document map

| Need | Go to |
|---|---|
| Save container and reload design | [Save and load](../engine/save-load-roundtrip.md) |
| Actor-value identity and its acceptance | [Oblivion Actor-Value Identity](oblivion-actor-value-identity.md) |
| Plugin identity | [Plugin Loading](../engine/plugin-loading.md) |
| Published compatibility metrics | [ROADMAP](../../ROADMAP.md) |

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-15 | Proposed full persisted-reference coverage and recorded the owner's direction | — |
| 2 | 2026-11-15 | Recorded the owner's approval and missing-reference policy | owner: approved |
| 3 | 2026-11-15 | PROPOSED → APPROVED at the owner's direction | — |
