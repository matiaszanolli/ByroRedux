# #3715 — ESM-2026-08-30-D3-02: the #3400/#3401 remap source guard is a hardcoded 8-parser allowlist, and 11 more embedded-FormID reads sit outside it

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: FormID & Load Order
**Record / Sub-record**: `WEAP`/`AMMO`+`ETYP`, `AMMO`/`DATA`+`DAT2`+`DNAM`, `NOTE`/`SNAM`, `COBJ`/`CNAM`+`BNAM`, `MGEF`, `PERK`, `ECZN`/`DATA`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D3-02)

**Location**: guard at `crates/plugin/src/esm/records/tests.rs` (`record_parsers_with_embedded_form_ids_take_a_remap`, ~:2159). Un-swept sites:
- `items.rs:371, 407` (`ammo_form`), `:411` (`skill_form`), `:654, 668, 677` (`projectile_form`), `:670` (`casing_form`), `:893` (`topic_form`)
- `misc/equipment.rs:216, 219` (`COBJ`)
- `misc/magic.rs:120` (`MGEF` light), `:348, 356` (`PERK` quest/spell)
- `misc/world.rs:1121` (`ECZN` owner)

**Status note**: successor to ESM-2026-08-27-D3-02 / #3401, which named a different, now-fixed set; #3401's title claim of "~12 more" was not exhaustive.

## Description

The guard enumerates exactly the **8** parsers the sweep touched (`parse_scol`, `parse_pkin`, `parse_movs`, `parse_flst`, `parse_tree`, `parse_acti`, `parse_navm`, `parse_regn`) and asserts each *source* contains a `remap: &Option<FormIdRemap>` parameter. A parser simply absent from the list is invisible to it — so the guard cannot detect the drift it was written to prevent.

Verified at HEAD: `parse_arma`, `parse_cobj` (`misc/equipment.rs:202`), `parse_eczn` (`misc/world.rs:1106`), `parse_mgef` (`misc/magic.rs:651`) and `parse_race` do not even take the parameter.

## Impact

Latent for the sites listed here (none has a live cross-record consumer today; `topic_form` is closest — `crates/scripting/src/dialogue.rs:349` keys a topic lookup by FormID). Their cost is that each is one consumer away from becoming the HIGH sibling finding filed alongside this one, which is exactly how that one happened.

## Suggested Fix

Invert the guard — assert that no `records/**.rs` file contains a FormID-shaped read outside a `remap_fid(` / `read_form_id(` call. Failing that, extend the allowlist to every `parse_*` that reads an embedded FormID and let the compiler force the parameter through (40 of 73 parsers currently take none; most legitimately read no FormIDs, ~10 do).

## Completeness Checks
- [ ] **SIBLING**: The inverted guard covers `cell/` as well as `records/`
- [ ] **TESTS**: A regression test proves the new guard *fails* when a remap parameter is removed from an arbitrary parser
