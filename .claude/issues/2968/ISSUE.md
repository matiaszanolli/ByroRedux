# UI-D1-01: the archive load path decompresses the same SWF four times and fully tag-parses it twice before frame 1

**Issue**: #2968
**Severity**: LOW
**Dimension**: Profile & VM Selection
**Labels**: `low,performance,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 1 — Profile & VM Selection). Profile: both (worse on `Fallout4Avm2`).

**Location**: `crates/ui/src/profile.rs`:20-23 · `crates/ui/src/avm2_host.rs`:53-55 · `crates/ui/src/navigator.rs`:461-464 · `crates/ui/src/player.rs`:199-213

## Description

`SwfPlayer::from_resource_provider` performs, in order:

1. `ScaleformProfile::detect` (→ `SwfMovie::from_data` → `swf::decompress_swf`)
2. `inject_host_object_adapter` (`decompress_swf` + `parse_swf` + `write_swf`)
3. `ScaleformNavigatorRuntime::create` → `import_asset_paths` (`decompress_swf` + `parse_swf`)
4. `SwfMovie::from_data` again (`decompress_swf`)

Confirmed against the pinned Ruffle checkout that `SwfMovie::from_data` is a full `swf::read::decompress_swf` (whole-stream inflate), not a header peek. The loose `--swf` path does three decompresses and one full parse.

Nothing is wrong with any individual call; the decode is simply repeated because each stage takes bytes rather than a parsed movie.

## Impact

Load-time only, but Fallout 4's `hudmenu.swf` and `pipboymenu.swf` are multi-megabyte compressed movies, so this is **four zlib inflates and two full tag walks per menu open — on the winit main-loop thread, synchronously**.

It is the largest single cost in a menu open and it buys nothing.

## Suggested Fix

Thread the already-decompressed buffer (and, where possible, the already-parsed `movie.tags`) from `inject_host_object_adapter` into `import_asset_paths` and the final `SwfMovie::from_data`. Detection can read `decompressed.header.is_action_script_3()` instead of re-parsing.

**Caution**: caching patched bytes is the obvious implementation and is exactly what makes UI-D3-02 (no idempotency guard on injection) reachable. Land the idempotency guard first or in the same change.

## Related

- UI-D3-02 — the natural fix for this finding is what makes that latent hazard live
- The injection path's full `parse_swf`→`write_swf` round-trip (formerly *SAFEUI-03*, mitigated by #2717's sweep)

## Completeness Checks
- [ ] **SIBLING**: All three `SwfPlayer` constructors get the same threading, not just `from_resource_provider`
- [ ] **IDEMPOTENCY**: If patched bytes become cached, UI-D3-02's guard is in place first
- [ ] **TESTS**: A regression test asserts the decompress count per menu open (the property, not just "it still loads")

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2968 --json state` when live state is needed.*
