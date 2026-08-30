# #3796: LC-2026-08-30-D3-01: slot_role.rs's module header and its canonical_shader_type doc give opposite answers on whether Starfield content reaches the slot table — nine live Starfield match arms are either dead code or a shipped fix

**Labels**: documentation, nif-parser, low, legacy-compat, game:starfield, nifal, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-30.md` — LC-2026-08-30-D3-01 (LOW)
**Dimension**: 3 — NIFAL texture-slot → role routing
**Location**: `crates/nif/src/import/material/slot_role.rs` — module header (`:17-23`) vs the `canonical_shader_type` doc (`:141-153`)

## Description

Two passages ~130 lines apart in the same file give **opposite answers** on whether Starfield content reaches the slot table.

**Module header** (`slot_role.rs:17-23`):
> *"Starfield and FO76 `BSGeometry` materials **deliberately do not enter this table**: their authored texture roles come from the BGSM/BGEM material records (and Starfield's materialsbeta CDB), not a Skyrim-family `BSShaderTextureSet`. **A zero Starfield hit here is therefore an explicit format boundary, not an unmeasured routing gap.**"*

**`canonical_shader_type` doc** (`slot_role.rs:141-153`, added by #3364, `d9d2d16a`):
> *"a Starfield FaceTint (3) **reached the slot table** as Skyrim Parallax and bound the head's detail map as a POM height field — the exact failure #2694 fixed for Skyrim."*

Both cannot be true. Verified at HEAD (`64f64480`) — both passages quoted verbatim.

## Evidence

The upstream issue (#3364) was filed **LOW / PLAUSIBLE**, explicitly *"code-read only — no Starfield install on this machine to census"*, and its consequence paragraph is conditional: *"for a Starfield type-3 property **with a `BSShaderTextureSet`**"* — the very thing the header asserts does not exist. **The fix's doc dropped that conditional and states the misroute as observed fact.**

The file now carries nine live `TextureSlotLayout::Starfield` match arms (`:180, :269, :297, :334, :350, :354, :389, :403` …):

- Read from the **header**, they are unreachable code a cleanup pass could legitimately delete.
- Read from **`canonical_shader_type`**, they are a shipped rendering fix.

Nothing in the tree resolves it: `record_unrouted_texture_slot`'s counters are runtime-only, and there is no checked-in Starfield `BSShaderTextureSet` occupancy figure comparable to the Skyrim ones the rest of the file cites (3158/3158, 1616/1664, …).

**Confidence: CERTAIN** that the contradiction exists; the *resolution* is unmeasured.

## Impact

LOW — doc correctness / audit-reference integrity. No runtime behaviour is wrong either way. The cost is that the file cannot be reasoned about: a maintainer deciding whether nine match arms are dead code gets opposite answers from the same file, and a future audit re-derives the contradiction instead of the answer.

## Suggested Fix

Settle it with the same kind of census the rest of the file uses — count Starfield `BSShaderTextureSet` blocks with a populated slot across `Starfield - MeshesPatch.ba2` / `Meshes01.ba2` — and rewrite whichever passage the number falsifies.

**The census is now cheap**: the game data is mounted at `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (129 BA2 archives), so the "no Starfield install on this machine" constraint that scoped #3364 to a code read no longer applies.

If the count really is zero: keep the arms but label them **forward-compat**, and downgrade #3364's narration back to the conditional it was filed as.

## Related

- #3364 (CLOSED, `d9d2d16a`) — the fix whose doc introduced the contradicting claim
- #2694 — the Skyrim failure the doc analogises to

## Completeness Checks
- [ ] **SIBLING**: The nine `TextureSlotLayout::Starfield` arms are labelled consistently with whichever answer the census gives
- [ ] **CANONICAL-BOUNDARY**: Whatever the census shows, the routing decision stays at the NIFAL parser→`Material` boundary. See `/audit-nifal`.
- [ ] **TESTS**: If the census is non-zero, a fixture pins Starfield FaceTint (3) routing; if zero, the measurement is pinned inline the way the file's Skyrim occupancy figures are
