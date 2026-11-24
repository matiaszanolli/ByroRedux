# #7258 Portable Save References — tracking (PRD portable-save-references)

State: OPEN
Labels: enhancement, save-load

Tracking issue for **Portable Save References**, the promise in [`docs/prd/portable-save-references.md`](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/portable-save-references.md) (APPROVED 2026-11-15; revision 4 in #7242).

A player can add, remove or reorder plugins between saving and loading. Today only actor values are saved portably (OAI-SAVE, through `register_component_codec`). Other saved record keys are raw session FormIDs, so a changed slot can silently attach a saved reference to a different plugin's record. Examples are `Inventory` item stacks, scripted faction changes, actor relocations, the pre-reload reference ledgers, and the active-effect and timed-restoration sources that AUDIT_SAVE deferred. The PRD classifies every saved identity by meaning, makes each plugin-record reference portable on the existing codec path, and reloads against the current plugin set. A missing reference is dropped only where the catalog proves it safe; otherwise the load is refused before teardown.

**Decisions (§10, both decided 2026-11-15, each taking the recommendation):**
- **Q1:** every persisted plugin-record reference becomes portable before live reload uses the current plugin set.
- **Q2:** a field-specific safe drop applies only where the catalog proves the invariants hold; otherwise the load is refused before teardown.

**Scope:** Oblivion, FO3, FNV, Skyrim (LE/SE/AE, including light slots), FO4, Starfield. FO76 is out.

## Phases (PRD §9)
- [ ] #7255 Phase 0: catalog, guard design and codec-path section (no engine code)
- [ ] #7256 Phase 1: portable references on `register_component_codec`, plus the catalog guard
- [ ] #7257 Phase 2: live reload against the current plugin set, preflight before teardown, acceptance

The PRD has no open **Phase 0: verify** items: revision 4 cites its original-engine claims (UESP *Skyrim Mod:Save File Format*; CK wiki `sSaveGameContentIsMissing`).


## Comments
