# SAVE-D2-2026-08-20-01: two required fields were added to registered saved structs after the last FORMAT_MAJOR bump — the guard that enforces the rule is structurally blind to required-field additions

**Issue**: #3164 — https://github.com/matiaszanolli/ByroRedux/issues/3164
**Finding ID**: `SAVE-D2-2026-08-20-01`
**Severity**: MEDIUM
**Dimension**: 2 — Registry & (De)serialization Fidelity
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, ecs, tech-debt, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D2-2026-08-20-01`
**Severity**: MEDIUM
**Dimension**: 2 — Registry & (De)serialization Fidelity
**Data-Loss Class**: corruption-on-load

## Location

Live violations:
- `crates/core/src/ecs/components/material.rs:58` — `Material.water_shader_flags: u32`
- `crates/core/src/ecs/components/material.rs:61` — `Material.is_water_shader: bool`
- `crates/core/src/ecs/components/collision.rs:179` — `RigidBodyData.collidable: bool`

The guard:
- `byroredux/src/save_io/serde_default_guard_tests.rs:114-127` — `serde_attr_declares_unsafe_default`
- `byroredux/src/save_io/serde_default_guard_tests.rs:133-149` — the guard test

The rule: `crates/save/src/snapshot.rs:40-62` (`FORMAT_MAJOR`, currently `4`).

## Description

`FORMAT_MAJOR`'s doc block is explicit that an intra-type change to a saved struct requires a
major bump, because `schema_fingerprint` hashes only column *keys* and cannot see inside a type.
This cycle took that seriously three times — v2 (ActorValues AVHealth keying), v3 (quest
lifecycle fields made required), v4 (`EquippedWeapon.reach`/`.speed`) — the last of which even
carries a model rationale at the field:

```rust
/// Required (no `#[serde(default)]`) per SAVE-D2-01 (#1714): a default
/// here would silently backfill pre-#3096 saves with fabricated `0.0`
/// reach/speed instead of rejecting them. `byroredux_save::FORMAT_MAJOR`
/// was bumped for this field addition instead.
```

Then, on the following day and **after** that bump, two commits added required fields to
registered saved structs with no bump at all:

- `8110f359` (2026-08-19) → `Material.water_shader_flags` + `Material.is_water_shader`.
  `Material` is registered at `save_io.rs:308`.
- `00fc0f3b` (2026-08-19) → `RigidBodyData.collidable`. Registered at `save_io.rs:293`
  **and** a `MUTABLE_DELTA_COLUMNS` entry (`:125`).

Neither touched `crates/save/src/snapshot.rs`. Both fields lack `#[serde(default)]`, so a `v4`
snapshot written between `219e876c` and those commits passes every container gate (magic ✓,
`major == 4` ✓, fingerprint ✓ — no column *key* changed — CRC ✓) and then fails
`serde_json::from_value` with `missing field`.

### The blind spot is structural, not an oversight in the #3020 fix

`serde_attr_declares_unsafe_default` is *by construction* a scanner for `serde(default)`
attributes. A **required** field addition has no attribute to scan for. The engine therefore has
exactly one automated enforcement of `FORMAT_MAJOR`, and it covers exactly the
compatible-default half — which is the *less* likely half to be written by a developer adding a
plain `pub collidable: bool`.

## Method (this is what makes the finding credible)

The guard algorithm was **re-implemented in Python against the live tree and confirmed GREEN**.
It is not broken and it did not miss anything it was written to catch — which is precisely why
its *blind spot* is the finding rather than a guard failure. What the re-implementation shows is
that the scan is keyed on `serde(default)` attribute text, so the required-field half of the same
footgun is invisible to it by construction, not by accident.

Corroborating git evidence:

```
git log --format="%H %ad" -G"FORMAT_MAJOR: u16 = " -- crates/save/src/snapshot.rs
  → last bump 219e876c, 2026-08-18
git log -S"water_shader_flags" -- crates/core/src/ecs/components/material.rs
  → 8110f359, 2026-08-19  (--name-only shows material.rs alone)
git show --stat 00fc0f3b
  → six files, none of them snapshot.rs
```

Diffing every save-participating source file over `219e876c..HEAD` yields **exactly three** field
additions to saved structs — `collidable`, `water_shader_flags`, `is_water_shader` — and **zero**
corresponding bumps. `material.rs:54` and `collision.rs:162-163` confirm both structs derive
`serde::Serialize`/`Deserialize` under the `inspect` feature the save build pulls in.

## Impact

Real-world blast radius **today** is bounded: the exposure window is one day of development
saves, and `Material` is not a delta column, so only `RigidBodyData` reaches the live path. What
is not bounded is the **rule**: the next such field lands with the same green test suite, and its
failure mode is `SAVE-D6-2026-08-20-01`'s half-applied world rather than a clean
`UnsupportedVersion` refusal.

The gap also inverts #1714's own claim that the `serde(default)` half is "the caught half" —
with the matcher now fixed, the required half is the *only* uncaught one, and nothing in the tree
says so.

## Related

- **#3020** — CLOSED; the `cfg_attr` matcher fix, **confirmed in place and verified**
  (`serde_attribute_body` parses both forms, three unit tests pin it).
- **#1714** — the original rule.
- `SAVE-D6-2026-08-20-01` — the failure mode this triggers.
- `SAVE-D2-2026-08-20-02` — residual discovery holes in the same guard.

## Suggested Fix

Two parts.

**(a) Decide the two live violations.** Bump `FORMAT_MAJOR` to 5 with a one-line history entry
naming both commits — the same treatment v2/v3/v4 each got.

**(b) Close the class.** Derive the guard from the *shape* rather than the attribute. Because
`save_type_sources()` already enumerates every save-participating file, a checked-in fingerprint
of each saved struct's field-name list (a small generated `.txt` the test diffs against,
regenerated deliberately alongside a `FORMAT_MAJOR` bump) catches additions, removals **and**
renames uniformly — where the attribute scanner can only ever catch one of the three.

## Completeness Checks
- [ ] **SIBLING**: every registered saved struct is checked for post-`219e876c` field additions, not just the three named here
- [ ] **SIBLING**: the shape-fingerprint guard covers types reached *through* a registered column (nested payloads), not only top-level registered types — see `SAVE-D2-2026-08-20-02`
- [ ] **TESTS**: a regression test fails when a required field is added to a saved struct without a `FORMAT_MAJOR` bump
- [ ] **TESTS**: the existing `rejects_major_version_skew` still passes after the bump to 5
