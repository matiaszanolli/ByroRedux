# SKY-2026-08-27-D3-04: #3217's `multi_pick` narrowing has no Skyrim real-data pin, even though its entire justification is Skyrim-sourced

Labels: low,esm-plugin,test-gap,bug,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: CONFIRMED (measured on real `Skyrim.esm`; current behaviour is sane)
- **Location**: `crates/plugin/src/equip.rs:411`; tests at `:765`, `:793`, `:827`; the only real-data pin at `crates/plugin/tests/parse_real_esm.rs:2954`
- **Description**:
  Checklist item 3 asks for verification of #3217 on real Skyrim data. The narrowing
  from `flags & (0x02 | 0x04)` to `flags & 0x04` is correct at HEAD
  (`let multi_pick = lvli.flags & 0x04 != 0;`) and behaves sanely on real Skyrim data,
  but all three of its own tests are synthetic fixtures. The only real-data pin that
  exists is `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`
  — FNV, added by #3285 as a *side-effect* characterisation of a Skyrim-motivated
  change. The population #3217 actually names ("1,491 vanilla Skyrim NPCs") is pinned
  by nothing.
- **Evidence**: measured on `Skyrim.esm` through the production
  `expand_leveled_form_id`, at each NPC's `effective_actor_level`:
  ```
  Skyrim LVLI total=3075
    flags histogram {0:553, 1:62, 2:239, 3:1855, 4:280, 8:5, 9:1, 10:39, 11:41}
    #3217-affected (0x02 set, 0x04 clear, multi-level) = 935;  Use-All(0x04) = 280
  OTFT expansion size histogram (NPCs): {0:28, 1:3221, 2:3, 3:96, 4:229, 9:1, 24:55}
    worst: 24 items on NPC 00038451
  ```
  No combinatorial blow-up: the worst case across all 5,118 NPCs is 24 items, from an
  authored `0x04` Use-All list. 935 Skyrim records sit in the affected set — 4.7×
  FNV's 200-record floor — with zero coverage.
  (Note the `1: 3221` bucket is inflated by SKY-…-D3-01, which truncates most outfits
  to one entry before expansion ever runs; re-measure this histogram after that fix.)
- **Impact**: a future change to `expand_leveled_inner` can regress the exact
  population #3217 was written for and only the FNV pin will notice — and FNV's LVLI
  shape differs (2,700+ lists, different flag mix), so it is not a proxy.
- **Suggested Fix**: mirror `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`
  for Skyrim: assert the 935-record affected set has not collapsed, and pin
  `dunIronbindBeemJa`'s outfit (the record named in #3217's own fixture doc at
  `equip.rs:785`) to exactly one item at a representative level.
- **Related**: #3217 (CLOSED — fix verified present and correct at HEAD), #3285,
  #3340/#3341 (OPEN, FNV-side LVLI issues; distinct).

---

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
