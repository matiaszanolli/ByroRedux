# #5511: SKY-D3-2026-10-09-02: `PackRecord::force_greet` reads only the package's own procedure tree, so 334 of the 339 Skyrim.esm force-greet packages (every concrete template instance) classify `NotAForceGreet`; the "exactly 5" census and #5376's…

**Labels**: ai, bug, dialogue, esm-plugin, game:skyrim, legacy-compat, medium

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-09.md` — finding `SKY-D3-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: 3 — Skyrim AI-package data on the spawn path (runtime owners: `/audit-gameplay`, `/audit-scripting`)
- **Location**:
  - `crates/plugin/src/esm/records/misc/pack.rs:424-468`: `force_greet` looks for the leaf in `self.procedures` only.
  - `:723` and `:953`: `parse_skyrim_procedures` fills the pack's own tree only, and no post-pass inherits a tree from the template.
  - `byroredux/src/commands/quest.rs:897-904`: `dialogue.forcegreet` rejects `NotAForceGreet`.
  - `byroredux/src/npc_spawn/ai_package.rs:276-278`: the false premise. The comment was added in window commit `4e58f241d`.
  - `crates/plugin/tests/parse_real_esm.rs:5335-5390`: `force_greet_skyrim_tree_dialect_floor`, census 5.
  - `docs/engine/dialogue-trees.md:161-176`: "Census: exactly 5 `ForceGreet` packs".
- **Status**: NEW.
  - `force_greet` landed in `287214103` (#5367, before the baseline) and the false comment in `4e58f241d` (window).
  - Open force-greet issues (#5429, #5430, #5452, #5471) cover FO3 verification, the kill switch, KCC movers and doc rot — none of them this.
  - `gh` "ForceGreet PKCU template" across all states returns nothing.
- **Description**:
  - **How Skyrim stores force-greets.** A Skyrim concrete package names its behaviour through `PKCU`'s template FormID and carries only data-input values (`PDTO` topic, location and so on). The procedure tree lives on the template.
  - **The codebase already knows this:** `ai_package.rs:1104-1106` builds the Sandbox fixture as "a template *instance* — what an NPC's PKID list actually names. Carries no procedures of its own", and `AmbientBehavior::from_package` chases `package_template_form_id` for Sandbox and Patrol (`:806-810`, `:974-978`).
  - **Where it breaks.** `force_greet` does not chase the template. It finds a leaf only on the 5 template packs themselves, and returns `NotAForceGreet` for every package an NPC or alias actually uses.
  - **Why the topic fallback never runs.** The method's "template inheritance" fallback (`:457-463`) is unreachable for an instance, because the leaf lookup fails first.
- **Evidence**: `fg_census.py`, a byte walk of SE `Skyrim.esm`.
  - **The population.** 5,961 PACKs; 5,758 carry no procedure tree of their own. 339 PACKs have a `ForceGreet` leaf in their own or their `PKCU` template's tree.
  - **Own-tree packs (5):** the templates `ForceGreet`, `ForceGreetFromSitting`, `ForceGreetWaitSitting`, `OrcGuardOutsideForcegreetPackage` and `dunWhiteRiverWatch_WatchmanForcegreetTemplate`. The dt2 gate uses only these (`0x000BBAA4`, `0x0003C1C4`).
  - **Template instances (334):** their templates are `ForceGreet` (302), `ForceGreetFromSitting` (24), `ForceGreetWaitSitting` (4), `OrcGuardOutside…` (3) and the White River Watch template (1). **320 of the 334 author their own type-0 `PDTO` topic.**
  - **On NPC_ default packages:** 41 of the instances sit on NPC_ `PKID` lists, for 87 NPC_ edges. Examples: `VigilantofStendarrForcegreetDaedric` ×39, `CartDriverOnCartForcegreet` ×6, `TG04EECGuardForcegreetPackage` ×2, `GuardRiftenKeepPrisonJailerFG` ×2, `HousecarlWhiterunGreetPlayer`, `MQPaarthurnaxForceGreetNormal`, `MarkarthMuseumForcegreetPackage`, `DA01FINAraneaForcegreetPackage` and `MS01ForswornThreatenForcegreetPackage`.
  - **This falsifies the ai_package.rs comment** that no vanilla NPC_ default lists a ForceGreet-tree package.
- **Impact**:
  - **The feature works only on templates.** The Skyrim half of #5367 Phase 4 ("Landed 2026-10-08") is live for the 5 templates only. `dialogue.forcegreet` refuses all 334 authored concrete Skyrim force-greets, Paarthurnax's and the carriage drivers' included.
  - **Future installers inherit the blind spot.** The quest/alias installers the design says "adopt the same bridge" would get `NotAForceGreet` for every quest-alias force-greet.
  - **The scope decision rests on the false count.** The decision not to wire the Skyrim tree dialect into ambient selection was justified by the 0-reference census. The real reach is 41 packages and 87 NPC_ edges.
  - **The gates cannot catch it.** The real-data test pins a floor of 5, and dt2 exercises only own-tree packs.
- **Related**:
  - #5367 (closed) and #5376 (closed): the FO3/FNV census correction that made the same "0 references" mistake for procedure 15.
  - ESM-2026-10-09-D2-02: PKDD Dialogue Type misread. That is the FO3/FNV arm of the same gate set.
- **Suggested Fix**:
  - Give `force_greet` the template that `from_package` already resolves (`package_template_form_id` chased through the index). Find the leaf on the template, and read the topic from the instance's own data inputs at the leaf's `PKC2` indexes.
  - Correct the ai_package comment, `dialogue-trees.md` §4 and the real-data floor to the 339 / 334 / 320 census.
  - Add an instance pack to dt2, for example `MQPaarthurnaxForceGreetNormal` or a `CartDriverOnCartForcegreet` carrier.
  - Whether ambient selection should then install Skyrim force-greets is a separate gameplay decision. It needs the instance's conditions modeled first, the same gate #5376 applies to FO3/FNV.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
