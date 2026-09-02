# #3071 — SKY-2026-08-16-D2-02: slot-7 back-lighting map has no canonical role

**Severity**: MEDIUM · **Location**: `crates/nif/src/import/material/slot_role.rs`:148-155 (arm), role enum at :36-54
**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-16.md` (Dimension 2)

1,564 vanilla properties author a slot-7 back-lighting map on Skyrim shader type 11 (non-MSN, non-multi-layer-parallax) with no canonical `TextureRole` to land in — the arm returns `None` silently, no counter/log.

**Suggested Fix**: Add a `TextureRole::BackLighting` variant routed to a `MaterialTextureSet` slot, or count the drop so the deferral is visible. Precedent: #2997/#2999 (canonical destination already existed, only the enum variant was missing).

**Related**: #2998 (FO4 sibling, different gate), #3068 (same file's slot-2 mis-roling), #2742 (MSN specular rule).

---

# #3072 — NIFAL-D4-2026-08-16-01: finish_partial_import hardcodes furniture: None

**Severity**: MEDIUM · **Location**: `byroredux/src/cell_loader/partial.rs`:170
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 4)

`finish_partial_import` hardcodes `furniture: None`; the process-lifetime NIF cache propagates the loss into interiors — furniture markers feed the M42 sandbox seat system, so actors can't find seats in any cell whose meshes were first imported by the exterior streamer.

**Suggested Fix**: Populate `furniture` in `finish_partial_import` from the same source the synchronous path uses. If genuinely unavailable on the partial path, the cache entry must record incompleteness so a later full import can replace it.

**Related**: #3074 (sibling hardcoded `None` in the same function, whose comment cites this one as precedent).

---

# #3074 — NIFAL-D4-2026-08-16-02: false blocker comment for dropping flame_attach_offset

**Severity**: LOW · **Location**: `byroredux/src/cell_loader/partial.rs`:131-139 (comment), :162 (the drop)
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 4)

The stated blocker for dropping `flame_attach_offset` on the streaming path is false — the extraction helper takes `&NifScene`, not `&ImportedScene`, and the partial path already holds a `&NifScene`.

**Suggested Fix**: Call the helper with the `&NifScene` already in hand; delete the false blocker comment. Re-examine #3072's `furniture: None` — its comment at :164 explicitly cites this one as precedent, so it may rest on the same false premise.

**Related**: #3072 (sibling `None`, whose justification may share this false premise).

---

# #3078 — SPT-D1-2026-08-16-01: fatal parse_spt error discards a recoverable placeholder

**Status**: Already CLOSED (2026-08-18), fix verified present at `byroredux/src/cell_loader/references/import.rs` (`parse_and_import_spt`'s `Err` arm now falls through to `SptScene::default()` with a warning) — landed in commit `aee8783f`. Citation comment added retroactively this session. No further action needed.
