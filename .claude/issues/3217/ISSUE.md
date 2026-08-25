# Issue #3217 — SKY-2026-08-20-D3-01

TES5 LVLF bit 0x02 ("calculate for each item in count") is treated as a
multi-pick trigger identical to 0x04 ("Use All"), but it actually means
"repeat the single roll `count` times" — for count==1 (the vanilla case)
that's single-pick. Current code at `crates/plugin/src/equip.rs:411`
treats `flags & (0x02 | 0x04) != 0` as multi-pick, over-expanding 1,491 of
5,118 Skyrim.esm NPCs' outfits (mean 38.7 items vs correct 2.5).

## Fix
Make `0x04` the sole multi-pick trigger; `0x02` alone routes to single-pick.

## Related
- #3069 (CLOSED) — fixed the 0x04 half; this is the 0x02 half it named but
  didn't ship.
- Shared arm affects FO3/FNV/Oblivion/FO4 too (SIBLING check) — but LVLF
  0x04 doesn't exist pre-Skyrim, so need to verify 0x02 semantics are
  identical there.

## Domain
`byroredux-plugin` (ESM record parsing / leveled list resolution).

## Resolution

The code fix, doc comments, and the exact fixture test the issue asked for
(`flags = 0x03` ladder, levels 1/4/7, asserting a single expansion) were
**already landed on `main`** in commit `bfdc3d3f` ("Add tests for body piece
masks and equip state handling", 2026-08-23) — both explicitly reference
`#3217`. Confirmed at HEAD:
- `crates/plugin/src/equip.rs:411` — `multi_pick = lvli.flags & 0x04 != 0`
  (0x02 no longer treated as multi-pick).
- `expand_leveled_calculate_each_item_still_picks_one_tier` test exists and
  passes.

**SIBLING** — the `0x02` arm is shared by all games (LVLI/LVLF parsing is
one code path, `container.rs:190`). `0x04` ("Use All") is a TES5/FO4-only
xEdit flag; FO3/FNV/Oblivion content never sets it, so `multi_pick` is
`false` there regardless — the fix is not merely inert but *correct* for
those games too, since `0x02`'s "repeat the roll `count` times" meaning is
identical across the family and was never "Use All" pre-Skyrim.

**Added in this pass**: the second suggested test was still missing — an
outfit-level regression proving nested `0x03` ladders (a tier ladder whose
entries are themselves `0x03` enchant-variant sublists, the exact
`dunIronbindBeemJa` shape) don't combinatorially explode.
`expand_leveled_nested_tier_ladders_do_not_combinatorially_explode`
(`crates/plugin/src/equip.rs`) asserts the flattened result is a single
item, not 18×5. (A literal `dunIronbindBeemJa` fixture would need real
Skyrim.esm data baked into the test, which the suite avoids elsewhere —
this synthetic fixture reproduces the same two-level-ladder shape.)
