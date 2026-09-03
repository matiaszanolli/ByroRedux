# #3732 — NIFAL-2026-08-30-D8-01: #3458's slot-2 colocation never reached the REFR-overlay sibling

**Severity**: MEDIUM · **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs` (the `pick` closure)
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` (NIFAL-2026-08-30-D8-01)

#3458 established that Skyrim/Starfield tint-family slot 2 is genuinely two
roles at once (`Tint` and `LightingMask`, both riding the one `*_sk.dds`
texture) and introduced `slot_to_colocated_role` to return the second role;
the NIF import loop consults both functions, first-wins. `resolve_mesh_paths`
routes every REFR TXST override through one `pick` closure that consulted
only `slot_to_role`, so `pick(2, o.glow, TextureRole::LightingMask)` could
never match on the exact population #3458 was about — a REFR override on a
Skyrim/Starfield tint-family mesh with `soft_lighting`/`rim_lighting` set
updates `tint` but leaves `lighting_mask` bound to the base mesh's original
texture while the `SLSF2_Soft_Lighting` gate crosses regardless.

The durable defect is structural: `pick`'s signature couldn't express a
colocated role at all, so any future `slot_to_colocated_role` entry would
silently fail to reach the overlay path with no compile error and no test.

## Fix implemented

Exactly the issue's own one-line suggested fix:

```rust
let pick = |slot: u32, raw: Option<FixedString>, role: TextureRole| {
    raw.filter(|_| {
        slot_to_role(slot_context, slot) == Some(role)
            || slot_to_colocated_role(slot_context, slot) == Some(role)
    })
};
```

`slot_to_colocated_role` wasn't previously re-exported outside
`crates/nif`'s internal `slot_role` module (only `slot_to_role` was) — added
it to both re-export chains (`material/mod.rs`'s `pub use slot_role::{...}`
and `import/mod.rs`'s `pub use material::{...}`) so `mesh_instance.rs` can
import it the same way it already imports `slot_to_role`.

**SIBLING** (issue's own checklist item): checked `apply_slot_swap` (#3187,
which turned out to already be CLOSED, not open as the issue's own Related
section stated — that text was stale). Its own doc comment already
establishes it's a "dumb slot→field carrier" with no role-decision logic at
all — it only populates the raw `RefrTextureOverlay.glow` field for slot 2,
which then flows through the exact same `pick(2, o.glow, ...)` call sites
this fix already covers. No separate change needed there; the `pick` fix
covers both the direct-REFR-override path and the XTXR-slot-swap path since
they converge on the same overlay field.

**CANONICAL-BOUNDARY** (issue's own checklist item): the slot→role
vocabulary stays entirely in `slot_role.rs` — the overlay path only
consults it via the two exported functions, never re-derives colocation
logic itself.

**TESTS** (issue's own checklist item):
`xtxr_skyrim_tint_family_slot_two_reaches_both_colocated_roles` builds a
Skyrim `SkinTint` mesh with `soft_lighting: true` and a slot-2 (`glow`)
override, and asserts BOTH `tint` and `lighting_mask` pick up the override
— pre-fix `lighting_mask` would have stayed bound to the base mesh's
original texture.

Full workspace: `cargo test --no-fail-fast` 7059 passing, 0 failing (+1 new
test).
