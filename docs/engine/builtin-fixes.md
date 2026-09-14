# Built-in Fixes — Community Patch Parity in Core

**Status**: PROPOSED (2026-09-14). This is a requirement and design; none of it is built yet.

Every Gamebryo/Creation game shipped with bugs that its community spent years
fixing. Some fixes are native plugins for a script extender. Others ship as an
unofficial patch ESP. A player who wants the game to run properly still has to
install that whole stack, in the right order, on the right executable version.

ByroRedux does not repeat this. **Every fix from the script-extender and
unofficial-patch ecosystem of every supported game is part of the engine.**
Vanilla game data on ByroRedux behaves as if the full community fix stack were
installed, and the player installs nothing.

Normative terms **MUST**, **SHOULD**, and **MAY** have their RFC 2119 meanings,
as in [Sandboxed Linked Mods](sandboxed-linked-mods.md).

## 1. Requirements

- **FIX-001 — Parity by default.** For each supported game, the engine MUST
  include every correction in that game's fix catalog (§3), with no user
  action, plugin, or setting required.
- **FIX-002 — Re-derived, never redistributed.** Each fix MUST be implemented
  from its documented description (changelog, fix list, bug tracker entry,
  or verified analysis of vanilla data). Patch ESPs, meshes, textures, scripts,
  and DLL code MUST NOT be copied into the engine. Doing so would break their
  licences and [NG-007](sandboxed-linked-mods.md#5-non-goals).
- **FIX-003 — Cited.** Every fix MUST record where it came from (§4). A fix
  whose vanilla behaviour or corrected value cannot be sourced is not
  implemented. This is the project's no-guessing rule applied to errata.
- **FIX-004 — One translation boundary.** Data, script, and asset fixes MUST be
  applied where that content is parsed or translated. Systems and the renderer
  MUST NOT carry per-game bug branches, and consumers see only corrected
  content.
- **FIX-005 — Load order still wins.** A user-loaded plugin, including an
  unofficial patch the player installs anyway, MUST override built-in fixes
  through normal load order. Built-in fixes MUST NOT double-apply or fight it.
- **FIX-006 — Guarded.** A data, script, or asset fix MUST apply only when its
  target still matches the known vanilla (buggy) content. A mismatch, such as
  a different game build or content already corrected upstream, skips the fix
  with a diagnostic. It MUST NOT force the value.
- **FIX-007 — Idempotent.** Corrections MUST set absolute values. They MUST NOT
  apply deltas, so applying a fix to already-fixed content changes nothing.
- **FIX-008 — Tested.** Every fix MUST have a regression test. A data or script
  fix tests against real vanilla content, opt-in via `#[ignore]` like the
  other game-data corpus tests. An engine-behaviour fix tests the corrected
  behaviour directly.
- **FIX-009 — Explainable.** The engine MUST be able to list which fixes
  applied, which were skipped, and why, per loaded profile. This extends
  [G-006](sandboxed-linked-mods.md#4-goals).

## 2. Kinds of fix

The ecosystem mixes four different kinds of correction. Each has a different
home in Redux.

| Kind | Typical source | What it corrects | Where it lives in Redux |
|---|---|---|---|
| **Engine behaviour** | Extender fix plugins | Bugs in the original executable: timing, physics, AI, inventory, save logic, crashes | The owning subsystem, as ordinary correct code plus a catalog entry and test |
| **Record erratum** | Unofficial patch ESP | Wrong authored record data: FormIDs, conditions, leveled lists, placements, flags, navmesh | ESM load: the merged record index (§5.1) |
| **Script erratum** | Unofficial patch scripts | Bugs in shipped scripts | Skyrim+: `.pex` translation (§5.2). Oblivion/FO3/FNV: SCPT records, i.e. a record erratum |
| **Asset erratum** | Unofficial patch meshes/textures | Broken meshes, collision, UVs, texture paths | NIF import, before NIFAL translation (§5.3) |

### 2.1 Engine behaviour: absent by construction is a claim, not an assumption

Most executable-level fixes target code that Redux does not contain: heap
replacers, crash guards around the original allocator, and hook-address
repairs. They have nothing to port. That still has to be established per fix:
the catalog entry is marked `not-applicable` with a one-line reason, such as
"Redux has no fixed-size stack used here".

Behavioural fixes do carry over, because the bug is in the rules and not the
binary. Examples are frame-rate-dependent physics or timers, and a wrong formula
in a derived stat. Redux implements the corrected rule directly. If a rule's
vanilla behaviour matters for content (GAME-005 authored quirks), the catalog
records that decision explicitly.

## 3. Fix catalog

Each game has one catalog listing every fix in scope and its state
(`implemented`, `not-applicable`, `blocked`, `pending`). The catalog is what
makes "every fix" a checkable claim rather than an aspiration.

**Starting inventory.** These are the sources to survey. The list is not
exhaustive, and each entry MUST be verified during Phase 0 before it counts:

| Game | Unofficial patch | Extender and fix plugins |
|---|---|---|
| Oblivion | Unofficial Oblivion Patch (UOP) | OBSE and its engine-fix plugins |
| Fallout 3 | Unofficial Fallout 3 Patch (UFO3P) | FOSE and its engine-fix plugins |
| Fallout: New Vegas | Yukichigai Unofficial Patch (YUP) | xNVSE, JIP LN NVSE, New Vegas Tick Fix, NVAC, lStewieAl's Tweaks |
| Skyrim (LE / SE) | USLEEP / USSEP | SKSE, SSE Engine Fixes, Bug Fixes SSE, Scrambled Bugs, powerofthree's Tweaks |
| Fallout 4 | Unofficial Fallout 4 Patch (UFO4P) | F4SE, Buffout 4, High FPS Physics Fix |
| Starfield | Starfield Community Patch | SFSE and its engine-fix plugins |
| Fallout 76 | None identified yet | None identified yet |

Unofficial patches mix defect fixes with restorations of apparent design
intent. The catalog tags each entry `defect` or `intent`, so a contested entry
can be singled out (§8, Q2).

## 4. Fix record

This is a proposed shape, not an existing type. Field names are illustrative.

```text
FixRecord
  id           stable namespaced ID, e.g. "fnv.record.<n>"; never reused
  games        GameKind(s) plus the rules profile where FO3/FNV must split
  kind         engine | record | script | asset
  class        defect | intent
  source       project, project version, changelog/tracker reference
  target       record:  FormID + owning master filename
               script:  vanilla .pex fingerprint (Skyrim+) or SCPT FormID
               asset:   archive-relative path + vanilla content hash
               engine:  owning subsystem / module path
  precondition the vanilla value(s) the target must still hold (FIX-006)
  correction   the absolute replacement value(s) (FIX-007)
  test         path of the regression test (FIX-008)
```

Record targets use the owning master's filename plus its local FormID. That is
the same identity the load order remaps from, not the global slot, so an
entry stays valid under any load order.

## 5. Application points

### 5.1 Record errata: after official content, before user plugins

[`parse_record_indexes_in_load_order`](../../byroredux/src/cell_loader/load_order.rs)
parses each plugin with `parse_esm_with_load_order` and folds it into one
`EsmIndex` with `merged.merge_from(plugin_records)`, in load order.

Record errata MUST apply to the **merged index once the game's official
content is merged and before the first user plugin merges.** Official content
is the masters plus official DLC, listed per game by
[`GameProfileEntry`](../../crates/core/src/ecs/game_profiles.rs). The loop
does not currently distinguish official plugins from user plugins, so this is
new work at that seam.

This point is chosen because it reproduces how unofficial patches work while
keeping FIX-005:

- **DLC overrides are covered.** Official DLC often overrides a master record
  and copies its bug. Unofficial patches load after the DLC for this reason.
  Applying after official content checks the precondition against the
  record that actually won.
- **User plugins still win.** A user-loaded USSEP, YUP, or any mod merges later
  and replaces the corrected record like any other override.
- **No double-apply.** A user-loaded patch that sets the same value is a no-op
  by FIX-007. A user mod that sets a different value simply wins.

Errata act on decoded, typed records, never on raw sub-record bytes. A fix
whose target field the parser does not decode yet is `blocked` on that decoder.
It is not implemented by byte patching.

### 5.2 Script errata (Skyrim+)

`translate_pex_detailed_with_providers` in
[`crates/scripting/src/translate/mod.rs`](../../crates/scripting/src/translate/mod.rs)
already computes `pex_fingerprint(pex_bytes)` for every compiled script. It
then parses, decompiles to the Papyrus AST, and runs the recognizer chain.

Script errata key on that fingerprint and correct the **decompiled AST**
before provider lowering and recognition. A fingerprint mismatch means
non-vanilla bytes: a different build or a user's patched script. The fix is
skipped (FIX-006), so a user-installed patched `.pex` passes through untouched.

Oblivion, FO3, and FNV scripts are SCPT records in the ESM, decoded into
`EsmIndex::scripts` and compiled by `compile_legacy_obscript_program` (records
with source text) or `compile_legacy_obscript_bytecode_program` (records with
only compiled `SCDA` bytecode), both in
[`obscript_runtime.rs`](../../crates/scripting/src/obscript_runtime.rs).
Their fixes are record errata (§5.1) on the SCPT record, not a separate path.

### 5.3 Asset errata

Mesh fixes apply to the `ImportedScene` returned by
[`import_nif_scene`](../../crates/nif/src/import/mod.rs), before NIFAL
translation. They are keyed by archive-relative path plus a hash of the
vanilla file bytes. No such content hash is computed on the NIF load path
today; it is part of this work. A loose file or a later archive that replaces
the mesh fails the hash guard and is left alone.

Texture fixes that exist only as edited image data are not re-derivable
without copying the asset (FIX-002). They are catalogued as `blocked`
unless the defect is in a reference, such as a wrong path, which is a record
or NIF erratum.

## 6. Diagnostics

- A per-profile report lists every fix as applied, skipped with its reason
  (precondition mismatch, blocked decoder, not applicable to this build), or
  overridden by a named later plugin.
- After the full merge, a record erratum whose target no longer holds the
  corrected value is reported with the plugin that reverted it. This is a
  diagnostic, not a re-application (FIX-005).
- A debug-CLI command exposes the report (name to be chosen; see
  [Debug CLI](debug-cli.md)).

## 7. Delivery sequence

**Phase 0 — catalog.** Build the per-game catalogs from the sources in §3 and
verify each entry. Start with FNV as the reference title. Classify every entry
by kind and state. There is no engine code in this phase.

**Phase 1 — record errata path.** Add the official/user split to the load-order
seam (§5.1), the fix registry, the precondition and correction application, the
report, and the first FNV record errata with corpus tests.

**Phase 2 — engine behaviour.** Work through the `pending` engine entries
subsystem by subsystem, each landing with its catalog entry and test.

**Phase 3 — script and asset errata.** Add the `.pex` AST correction hook
(§5.2) and the NIF content hash plus `ImportedScene` hook (§5.3).

**Phase 4 — remaining games.** Apply Phases 0–3 per game in the
[compatibility order](game-compatibility.md).

## 8. Open questions

1. **Saves.** Should a save record the fix-catalog version it was made under?
   If so, what happens when a save made before a fix loads after it? An example
   is a quest-stage fix on a quest already past that stage.
2. **Intent-class entries.** Are `intent` fixes on by default like `defect`
   fixes? FIX-001 says every fix is in, but some unofficial-patch changes are
   disputed by players.
3. **Vanilla reproduction.** Should a developer-only switch disable built-in
   fixes, so vanilla behaviour can be compared in regression triage? If yes,
   it must not become a player-facing setting that recreates the problem.
4. **Mods that revert fixes.** A pre-patch mod that copied a buggy vanilla
   record wins by load order (FIX-005). Is the §6 diagnostic enough, or should
   profiles be able to pin specific fixes above user plugins?
