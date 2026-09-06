# #3896: SF-2026-09-05-D7-01: launch profiles still order archives first-wins after #3637 inverted lookup to last-wins — every Starfield and FNV patch archive is shadowed

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3896 --json state`).*

---

**Audit**: `docs/audits/AUDIT_STARFIELD_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: HIGH · **Dimension**: 7 (archive/profile wiring)

## Description

Commit `3562401b` ("Fix #3637: mesh/texture/material archive lookup is last-wins, not first-wins", 2026-09-05) inverted archive precedence. `assets/debug_profiles.toml` was never updated and still orders its archive lists on the **old first-wins premise** — its own comment says so verbatim:

> `# archives because archive lookup is first-listed-wins.`

Under the new last-wins resolution, every archive listed *first* now loses to the base archives listed after it. For Starfield that shadows the entire patch set.

## Evidence

`byroredux/src/asset_provider/texture.rs:37,67,88,134` and `material.rs:653,694` all iterate `.iter().rev()` — last-listed answers. Confirmed live.

`assets/debug_profiles.toml` Starfield `default_bsas` order:

```
"Starfield - MeshesPatch.ba2",     <-- patch, listed FIRST -> now LOSES
"Starfield - Meshes01.ba2",
"Starfield - Meshes02.ba2",        <-- base, listed after -> now WINS
"Starfield - LODMeshesPatch.ba2",  <-- patch -> LOSES
"Starfield - LODMeshes.ba2",
```

`TexturesPatch01.ba2` / `TexturesPatch02.ba2` are shadowed the same way.

**The FNV profile has the identical inversion**, and `crates/game-detect/src/lib.rs:174-204` is a **green test actively pinning the wrong order**, with the now-falsified premise stated in its own assertion message:

```rust
assert!(update_pos < base_pos,
    "Update.bsa must precede Fallout - Meshes.bsa in fnv.default_bsas \
     {default_bsas:?} — archive resolution is first-listed-wins, so this \
     order is what makes the patch archive actually win");
```

FO4 is **accidentally** correct — its patch archive happens to be listed last. That the three profiles disagree is itself the proof the ordering was never one deliberate convention.

## Impact

On the default `--game starfield` launch, every patched Starfield mesh and texture silently reverts to its unpatched base-game version. Silent: no warning, no counter. The same applies to `--game fnv` and `Update.bsa`, which is the archive vanilla FNV uses to ship its own bug fixes. Because the test encodes the old premise and passes, the next person to touch this is told the wrong order is correct.

## Suggested Fix

Reverse the archive lists in the `starfield` and `fnv` profiles in `assets/debug_profiles.toml` (patch archives **last**), correct the file's `first-listed-wins` comment, and invert `fnv_profile_lists_update_bsa_before_the_base_meshes_archive` in `crates/game-detect/src/lib.rs` to assert `base_pos < update_pos` with a corrected message. Consider asserting the same invariant for the starfield and fo4 profiles so all three are pinned to one convention rather than one being accidentally right.

## Completeness Checks
- [ ] **SIBLING**: All three profiles (fnv, fo4, starfield) checked — and any other profile with a patch/base pair
- [ ] **TESTS**: The `game-detect` precedence test asserts last-wins, and its message states the real reason
- [ ] **TESTS**: A regression test pins this specific fix

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
