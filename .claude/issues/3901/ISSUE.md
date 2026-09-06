# #3901: NIFAL-2026-09-05-D8-01: TextureFlipEntry.texture_slot leaks a raw TexType onto a canonical ECS component, and the recorded remediation names the wrong slot table

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3901 --json state`).*

---

**Audit**: `docs/audits/AUDIT_NIFAL_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: MEDIUM · **Dimension**: 8 (texture roles) · **Tier violated**: `no-leak`

## Description

Dimension 8's cardinal rule is that a per-format texture slot index must not survive past the NIF import boundary — `MaterialTextureSet`'s named roles exist precisely so it does not. The 2026-07-27 role unification converted every *material* producer. It did not convert the *animation* producer.

`TextureFlipEntry.texture_slot: u32` sits on `AnimatedTextureFlip`, a canonical ECS component in `crates/core`, and its own doc comment names what it is: *"Raw `TexType` slot from the source NIF (0=BASE_MAP, …)"*. The consumer cannot use it without re-resolving it — and today does not resolve it at all, hard-coding the raw number:

```rust
// byroredux/src/render/static_meshes.rs:260-263
.and_then(|f| f.handle_for_slot(0))
```

Every `NiFlipController` targeting a slot other than 0 is silently dropped: no warning, no `unrouted_*` counter of the kind `slot_to_role` grew for exactly this class of gap.

## The durable half: the recorded plan is wrong

The deferral comment at `byroredux/src/render/static_meshes.rs:133-139` says a flip on another slot *"needs the same shader-type-aware `slot_to_role` dispatch that `byroredux/src/cell_loader/spawn/mesh_instance.rs` uses for XTXR overrides"*.

`slot_to_role` is the **`BSShaderTextureSet`** table — a **different, incompatible numbering**. An implementer who follows the comment maps:

| `TexType` | correct role | what the comment's table yields |
|---|---|---|
| 1 DARK_MAP | `dark` | **`Normal`** |
| 3 GLOSS_MAP | `smooth_spec` | **`Height`/`Detail`/`GreyscaleLut`** |
| 4 GLOW_MAP | `emissive` | **`Environment`** |

That is a wrong-role binding written down as the plan, in the file an implementer reads first.

The translation is not blocked on missing information: `TexType` is a closed 12-value enum (`nif.xml:383-397`) mapping one-to-one onto `MaterialTextureSet`, which is what `crates/nif/src/import/material/legacy_properties.rs` already does for the static `NiTexturingProperty` set of the very same meshes.

## Evidence — measured, not assumed

Census run during the audit (`BsaArchive::open` + `parse_nif`, downcasting every block to `NiFlipController`, histogramming `texture_slot`):

| Archive | NIFs | `NiFlipController` | slots |
|---|---|---|---|
| `Oblivion - Meshes.bsa` | 9,875 | 54 | 53 × BASE_MAP(0), **1 × GLOW_MAP(4)** — `meshes\creatures\endgame\battle.nif` |
| `Fallout - Meshes.bsa` (FNV) | 19,197 | 0 | — |
| `Fallout - Meshes.bsa` (FO3) | 13,729 | 0 | — |
| `Skyrim - Meshes0/1.bsa` | 22,047 | 0 | — |

So **exactly one** vanilla mesh loses its authored flipbook today. Stating that plainly so the rating is not read as bigger than it is.

## Impact

One vanilla Oblivion creature mesh's glow flipbook does not animate, and any mod-authored non-base-slot flip is dropped the same silent way. The larger cost is structural: a raw source-format vocabulary is live on a canonical ECS component — the exact condition Dimension 8 exists to prevent — and the written plan for removing it points at a table with incompatible numbering, so the most likely future "fix" binds wrong textures instead of no texture.

## Suggested Fix

Resolve `TexType` to a `MaterialTextureSet` role at `byroredux/src/anim_convert.rs`'s import-side hop — where the handles are already resolved — and store the canonical role on `TextureFlipEntry` instead of the raw `u32`.

Then correct the `static_meshes.rs:133-139` comment: the resolver for this vocabulary is the `NiTexturingProperty` mapping in `crates/nif/src/import/material/legacy_properties.rs`, **not** `slot_to_role`. **Correcting the comment is worth doing even if the rest is deferred.**

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The slot→role resolution happens at the import boundary, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Other `crates/core` components checked for surviving raw source-format slot/enum fields
- [ ] **TESTS**: A regression test pins a non-base-slot `NiFlipController` resolving to the right role

## Related
- #2221 (created these sinks; CLOSED), #3251 (`handle_for_slot` out-of-range aliasing; CLOSED), #2695 (one shared slot table precedent), #3814 (`supplemental_texture_indices` role pinning; CLOSED)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
