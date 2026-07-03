# #1718: FNV-D7-01: Ragdoll body + dependent constraints dropped silently on bone-name miss (no telemetry)

**Severity**: MEDIUM
**Location**: `byroredux/src/ragdoll.rs` — `template_from_imported`

`template_from_imported` silently dropped any `ImportedRagdoll` body whose
`bone_name` wasn't in the skeleton's name→EntityId map, and silently dropped
any constraint referencing such a dropped body. The only signal was the
`< 2 bodies` / empty-constraint `None` return — no diagnostic about *why* a
ragdoll degraded or vanished. Distinct from #1539 (constraint-*kind* drop at
NIF import time); this is the body-*name*-resolution drop at spawn time.

## Fix
Added `log::warn!` at both drop sites in `template_from_imported`:
- Dropped bodies: one warn listing the count + bone names not found in the
  skeleton.
- Dropped constraints: one warn per constraint, in the same "dropping ...
  linking bones 'a' <-> 'b'" phrasing as the sibling `#1539` diagnostic in
  `crates/nif/src/import/collision.rs::extract_ragdoll`, so both
  ragdoll-fragmentation drop sites read as one unified telemetry stream.

No rate-limiting/`Once`-gating was needed: `template_from_imported` runs once
per NIF load (not per-frame), so the natural call cadence already bounds the
log volume.

## Completeness Checks
- [x] **SIBLING**: Confirmed `extract_ragdoll` (#1539) already emits a
      `log::warn!` for its constraint-kind drop; mirrored its exact phrasing
      for the new body/constraint-name-resolution warns.
- [x] **TESTS**: No log-capture harness exists in this codebase (the #1539
      sibling warn has no test either), so added functional regression tests
      pinning the *drop/remap logic* the warns are attached to:
      `all_bones_resolve_yields_full_template`,
      `dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest`,
      `single_surviving_body_returns_none`,
      `surviving_bodies_with_no_surviving_constraints_returns_none`.

---

# #1728: SCR-D1-02: No Skyrim-BE / Starfield-guards round-trip test on an untrusted parser

**Severity**: MEDIUM
**Location**: `crates/pex/src/lib.rs` (`build_sample` / test module)

The only `.pex` round-trip writer test exercised the FO4 little-endian
dialect. The big-endian Skyrim path (no const-flag/struct-info/guards fields,
BE int/float decode) and the Starfield-guards path (per-object const_flag,
struct_infos, per-variable const_flag, Starfield-only guards list) had no
round-trip regression — a field-order/endian regression in either arm would
pass CI silently.

## Fix
Extended `PexWriter` with a `big_endian` mode (magic is always written
little-endian per `reader.rs::read_header`'s `u32_opt(true)`; every other
multi-byte field flips BE/LE per the writer's mode) and added:
- `build_sample_skyrim_be` / `parses_a_handbuilt_skyrim_be_pex` — BE magic,
  BE-encoded fields, and the Skyrim object layout (no const_flag byte, no
  struct_infos count, no guards, no per-variable const_flag).
- `build_sample_starfield_with_guards` / `parses_a_handbuilt_starfield_pex_with_guards`
  — LE magic + `game_id == 4`, exercising per-object `const_flag`,
  `struct_infos` (count-only), per-variable `const_flag`, and a populated
  Starfield-only `guards` list.

## Completeness Checks
- [x] **SIBLING**: Both the BE-Skyrim arm and the Starfield-guards arm got a
      round-trip test, not just one.
- [x] **TESTS**: New round-trip regressions pin both decode paths; verified
      both are discovered and pass (`cargo test -p byroredux-pex`, 40 tests).
