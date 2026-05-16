# Issue #1113 — TD3-203: CommonItemFields::from_subs adoption

**Source**: AUDIT_TECH_DEBT_2026-05-16 (Top 5 Medium #2)
**Severity**: MEDIUM
**Status**: CLOSED in 2ab32ae0

## Resolution

Migrated highest-traffic parsers (actor.rs × 4 + tree.rs + pkin.rs + scol.rs) to `CommonNamedFields::from_subs`. 12 hand-rolled EDID/FULL/MODL/VMAD arms dropped; -12 LOC net.

Helper uptake: 1 file (items.rs) → 5 files (items + actor + tree + pkin + scol).

Lower-traffic candidates (mswp, weather, climate, etc.) deferred — most carry only EDID and don't benefit from the multi-field helper.
