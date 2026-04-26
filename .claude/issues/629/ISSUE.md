# FNV-D2-01: ENCH enchantment records dropped at the catch-all skip — every weapon EITM dangles

## Finding: FNV-D2-01

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: FNV (Pulse Gun, This Machine, Holorifle, etc.), FO3, Oblivion, Skyrim
- **Location**: no `b"ENCH"` arm in [crates/plugin/src/esm/records/mod.rs:265-438](crates/plugin/src/esm/records/mod.rs#L265-L438); FNV.esm ships an ENCH top-level group

## Description

Every `EITM` FormID on `WEAP`/`AMMO`/`ARMO` references an `ENCH` enchantment record (e.g. Pulse Gun's "Pulse" enchantment, This Machine's "Marksman Carbine" charge effect, Holorifle's energy splash). With ENCH undispatched, all of these EITM references dangle — weapon special effects will silently no-op once the consumer side wires up.

Currently invisible because nothing consumes `weapon.eitm` yet, but every WEAP record carries this slot.

## Suggested Fix

Add an `extract_records(..., b"ENCH", ...)` arm. ENCH layout is parallel to SPEL (EFID/EFIT effect chain), so route through `parse_spel`-shaped logic and store in a new `enchantments` map on `EsmIndex`:

```rust
// records/mod.rs
b"ENCH" => extract_records(reader, group, &mut |r| {
    let ench = parse_ench(r)?;  // mirrors parse_spel; ENIT header + EFID/EFIT effects
    index.enchantments.insert(ench.form_id, ench);
    Ok(())
})?,
```

UESP ENCH layout: EDID + FULL + ENIT (4-byte: type/charge/cost/flags) + EFID/EFIT/SCIT/CTDA effect blocks.

## Related

- FNV-D2-02 (companion) — FLST same shape (top-level group dropped at catch-all skip).
- #519 (open) — AVIF dispatch missing; same fix surface.
- #520 (open) — PerkRecord stub; ENCH effects mirror perk-entry-point effects.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: SPEL/MGEF dispatch landed (`records/mod.rs:265+`). Use the same scaffolding pattern.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a spot-check parsing FNV.esm — assert `index.enchantments.len() > 0` and a known FormID (e.g. Pulse Gun's enchantment) resolves with a non-empty effect chain.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
