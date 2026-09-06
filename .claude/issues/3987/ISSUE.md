# #3987 — REN-2026-09-06-D22-01: the Starfield "no evidenced Flags field" premise that zeroes both light canonicalizers is contradicted by the DAT2 decoder's own verified layout comment

**Labels**: medium, game:starfield, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D22-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Light Animation
- **Location**: `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`, `canonical_light_shadow_flags`, `translate_light`) vs `crates/plugin/src/esm/cell/support.rs` (`build_static_object_from_subs`, `b"DAT2"` arm)
- **Status**: NEW
- **Description**: All three per-game boundary functions in `light_anim.rs` gate
  Starfield to a zero mask on one shared premise, stated verbatim in
  `canonical_light_animation_flags`'s doc: *"SF1Edit's live LIGH definition
  (`wbDefinitionsSF1.pas`) replaced the Skyrim/FO4/FO76 `DATA` subrecord with a
  restructured 76-byte `DAT2` whose only named fields are a handful of floats —
  the bytes a Flags field would occupy are an undifferentiated `wbUnknown`
  block."* `canonical_light_shadow_flags` and `translate_light` each restate it
  and cite the animation sibling as their authority.

  The `DAT2` decoder that actually produces the `flags` word says the opposite,
  citing the *same* reference file. `support.rs`'s arm carries an explicit,
  offset-by-offset layout table introduced as *"Byte layout verified against
  xEdit `wbDefinitionsSF1.pas` (`wbRecord(LIGH … wbStruct(DAT2, 'Data', [...]))`),
  NOT guessed"* — and its third row is `{12} UInt16 Flags (Skyrim DATA stores
  u32)`. The decoder then reads exactly that: `u16::from_le_bytes([sub.data[12],
  sub.data[13]]) as u32`.

  One of the two comments is wrong about what `wbDefinitionsSF1.pas` contains,
  and which one is right decides whether three `match` arms and five decoded
  fields are correct. (The two claims are only reconcilable if the *field* is
  named but its *bit meanings* are not — in which case `light_anim.rs`'s
  "undifferentiated `wbUnknown` block" phrasing is describing the wrong thing
  and should say so, because "no named field at all" is the argument the arms
  currently rest on.)
- **Evidence**:
  - `support.rs` (`b"DAT2" if is_ligh && sub.data.len() >= 11`) reads the flags
    word at offset 12 under a comment naming that offset `Flags`.
  - Downstream consequences of the zero masks, all on real decoded data:
    - `canonical_light_animation_flags` → `0` ⇒ `attach_light_flicker_if_needed`
      hits `if animation_flags == 0 { return; }`, so the three DAT2 flicker
      fields the same arm decodes — `period_secs` (+28), `intensity_amplitude`
      (+32), `movement_amplitude` (+36) — are structurally unreachable on
      Starfield.
    - `translate_light` → `is_spot` is `false` for every Starfield LIGH, so the
      function returns `LightKind::Point` before reading `fov_degrees`; the
      DAT2 FOV at +20 (decoded under `#2439 / NIFAL-D2-01 — same offset as the
      DATA arm above`) is likewise unreachable.
    - `canonical_light_shadow_flags` → `0` ⇒ `LightSource::from_legacy_world_units`
      computes `VisibilityMask::for_legacy_projection(false)` =
      `VisibilityMask::ARCHITECTURE` (`crates/core/src/lighting.rs`), so **every**
      placed Starfield light is invisible to `STATIC_PROP`, `DYNAMIC_ACTOR`,
      `FOLIAGE`, `GLASS` and `EFFECT` shadow rays.
  - That last consequence is the exact failure the shadow canonicalizer's own
    doc argues against: *"Shadow decode is permissive-by-default. Dropping a
    shadow bit that a game does name is the strictly worse error: the light
    silently stops casting RT shadows and the scene just looks flat, with
    nothing to trace it back to."* The Starfield arm applies the strict default
    to an entire game.
- **Impact**: Visual-only, but whole-game on Starfield: no flicker/pulse on any
  LIGH, no spot cones from ESM-placed lights, and props/actors/foliage cast no
  shadows from any placed light. Five decoded DAT2 fields are dead. The
  documentation conflict also means a future reviewer reading either comment
  gets an authoritative-sounding but contradicted answer.
- **Related**: #2251 (the arm's origin), `starfield_has_no_verified_flags_field_for_either_canonicalization`
  (the test that encodes the disputed premise), `crates/core/src/ecs/components/light.rs`
  (`LIGHT_FLAG_SHADOW_MASK`, `VisibilityMask::for_legacy_projection`).
- **Suggested Fix**: Settle the premise against `wbDefinitionsSF1.pas` once and
  make both comments agree. If the field is named but its bits are not, say
  exactly that in `light_anim.rs` and note the shadow-side consequence
  explicitly (all-`ARCHITECTURE` Starfield lights) so it is a recorded decision
  rather than a side effect. If the bit positions can be evidenced, give
  Starfield a real arm in both canonicalizers and let `translate_light` read the
  FOV it already decodes.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
