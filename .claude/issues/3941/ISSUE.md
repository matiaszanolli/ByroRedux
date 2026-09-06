# #3941 — SCR-D7-2026-09-06-01: the legacy `SCRI` accessor has no statics-family arm — 560 scripted DOOR/FURN/LIGH/TACT/FLOR base records on Oblivion/FO3/FNV never reach the new ObScript attach lane or the extender-compatibility census

- **Finding ID**: SCR-D7-2026-09-06-01
- **Labels**: medium,scripting,esm-plugin,legacy-compat,game:oblivion,game:fo3,game:fnv,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3941

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: MEDIUM (the lowerer would decline most vanilla door bodies today; but the compatibility report doesn't decline — it never runs)
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `crates/plugin/src/esm/records/index.rs:734-767` (`base_record_script`: ACTI → CONT → TERM → items → NPC_ → CREA, first hit — **no `cells.statics` arm**; its own doc at `:722-727` lists this as a "coverage gap to close later", `a459f149`, 2026-05-23); `crates/plugin/src/esm/records/dispatch_world_placement.rs:18-28` (DOOR/FURN/LIGH/TACT/FLOR → `cells.statics` only); `crates/plugin/src/esm/cell/support.rs:60-98` (builder captures `VMAD`, no `SCRI` arm); `crates/plugin/src/esm/cell/mod.rs:805-845` (`StaticObject` has no `script_form_id`). Consumers: `byroredux/src/cell_loader/references/attach.rs:240`, `synth_child.rs:193`.
- **Status**: NEW. #2663 (CLOSED) fixed the **VMAD** half of exactly this family; #521 / #1273 closed ACTI/TERM and NPC_/CREA. No issue exists for the `SCRI`-on-statics half (`gh issue list --search` on `SCRI` / `DOOR script` / `base_record_script`, all states: no match).
- **Description**: `base_record_script` is the only way a REFR reaches the legacy SCPT lane. Until this pass the lane's only consumer was one demo spawner; in-range commits `19050cd9`, `9d5829b8`, `7126aa0a` gave `attach_scpt_script` two real consumers — `record_compatibility_report` (`attach.rs:281-293`) and `attach_legacy_obscript_program` (`:294-295`) — both silently skipped for this family on three games (`return false` at `attach.rs:241`, no log at any level).
- **Evidence** (Dim 7's raw sub-record census of the installed masters — uncompressed records, non-zero `SCRI`; 20-byte headers Oblivion, 24 FO3/FNV):

  | Game | DOOR | FURN | LIGH | TACT | FLOR | **unreachable** | ACTI (reachable, for scale) |
  |---|---|---|---|---|---|---|---|
  | FalloutNV.esm | 136/320 | 16/234 | 4/501 | 18/87 | 0 | **174** | 992/1143 |
  | Fallout3.esm | 117/319 | 8/183 | 3/368 | 22/49 | 0 | **150** | 616/774 |
  | Oblivion.esm | 180/501 | 9/186 | 42/1625 | — | 5/155 | **236** | 927/1252 |

  Orchestrator confirmed the six-arm walk and the absence of any `SCRI`/`script_form_id` capture in `cell/mod.rs` / `cell/support.rs`.
- **Impact**: (1) the `CompatibilityRegistry` aggregate (`commands/world_info.rs:173`) under-reports on every Fallout/Oblivion cell — an xNVSE/OBSE probe in a door/talking-activator script is never seen; (2) a pure load-order handler on a door — the shape `pure_load_order_handler_attaches_without_a_hand_written_spawner` proves lowers — never lowers on 560 base records; (3) every future widening of the ObScript lane inherits the hole. No command-line workaround; the field is dropped at parse time.
- **Disproof attempted**: TACT is not dual-dispatched into `activators`; no `SCRI` anywhere in the cell builder; `LegacyObscriptContentCatalog` is plugin metadata, not a script census; census counted non-zero payloads only and found zero compressed records for the relevant types; ACTI column consistent with #521.
- **Related**: #2663, #521, #1273, #3160
- **Suggested Fix**: add `script_form_id: u32` to `StaticObject`, capture `SCRI` (remapped like the `VMAD` arm), append a `cells.statics` arm to `base_record_script` placed last (mirroring #2663's ordering rationale) with a resolves/declines test pair; replace the `index.rs:722-727` comment with the issue number.

---

### LOW

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
