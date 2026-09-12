# #4242 — FO4-D6-02: TXST override comments cite XPRD as a texture-override REFR sub-record; it is not one

**Severity**: LOW
**Dimension**: 6 — ESM Architecture Records (SCOL/MOVS/PKIN/TXST)
**Location**: `crates/plugin/src/esm/cell/support.rs:474`, `crates/plugin/src/esm/cell/mod.rs:868,1182`
**Status**: NEW

**Description**: Three comments (inherited from #357) say "REFR XTNM/XPRD overrides". `XPRD` is "Patrol Data" (unrelated to textures) and has no parse arm anywhere in the crate. The real FO4 override mechanism is `XATO`/`XTXR` (both implemented and tested), plus `XTNM` (also real).

**Evidence**: Confirmed in current code — all three cited sites (`support.rs:474`, `mod.rs:868`, `mod.rs:1182`) still read "XTNM/XPRD overrides"; no `XPRD` parse arm exists anywhere under `crates/plugin/src/esm/`.

**Impact**: None on behavior — purely a misleading citation that could send a contributor looking for a nonexistent parsing gap.

**Suggested Fix**: Replace "XTNM/XPRD" with "XTNM/XATO/XTXR" in the three comments.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)

---

# #4243 — FO3-2026-09-11-D2-02: BSShaderPPLightingProperty.refraction_fire_period decoded, zero consumers workspace-wide

**Severity**: LOW
**Dimension**: NIF v20.2.0.7 Parser (FO3 Block Subset) — `/audit-fo3` Dimension 2
**Location**: `crates/nif/src/blocks/shader.rs:55` (field `refraction_fire_period`), `crates/nif/src/import/material/legacy_properties.rs:559-561` (forwards `refraction_strength` but not `refraction_fire_period`)
**Status**: NEW

**Description**: `BSShaderPPLightingProperty.refraction_fire_period` is decoded but has zero consumers workspace-wide.

**Evidence**: `refraction_fire_period` is defined and populated in `shader.rs` (field at `:55`, populated at `:119`); `legacy_properties.rs:559-561` forwards the sibling `refraction_strength` field into `info.refraction_strength` but has no equivalent line for `refraction_fire_period`.

**Impact**: FO3 heat-haze proxies (fire/flamer/plasma FX, 27 `BSRefractionStrengthController` blocks present) distort statically rather than scrolling. Cosmetic only.

**Related**: `docs/audits/AUDIT_FO3_2026-09-11.md` (FO3-2026-09-11-D2-02).

**Suggested Fix**: Wire as a scroll-phase offset in the fire-refraction warp, or document the time-invariant model as deliberate.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If wired, forward at the NIF import → `Material`/effect boundary, not re-derived at render time.
- [ ] **TESTS**: N/A until a consumer is added.

**Disposition**: Documented as deliberate, not wired. Wiring a scroll-phase warp would mean adding a new time-varying uniform to `triangle.frag`'s refraction pass — a Vulkan/shader change with no `cargo test` signal for whether the visual result is correct (per `feedback_speculative_vulkan_fixes`). Added an explanatory comment at the `refraction_strength` forwarding site in `legacy_properties.rs` recording why the sibling field is intentionally left unforwarded; the field itself stays decoded and pinned by `blocks/shader_tests` for when a scroll-phase warp lands.

---

# #4244 — FO3-2026-09-11-D2-03: _audit-common.md points extract_emitter_params/extract_emitter_rate at walk/mod.rs; both live in walk/emitter.rs

**Severity**: LOW
**Dimension**: NIF v20.2.0.7 Parser (FO3 Block Subset) — `/audit-fo3` Dimension 2
**Location**: `.claude/commands/_audit-common.md:16`
**Status**: NEW

**Description**: `_audit-common.md`'s `NIF Import:` row attributes `extract_emitter_params`/`extract_emitter_rate` to `walk/mod.rs`; both functions actually live in `walk/emitter.rs`.

**Evidence**: `_audit-common.md:16` reads "walk/{mod, tests} (mod.rs carries extract_emitter_params/extract_emitter_rate)". Confirmed in current source: `extract_emitter_params` is defined at `crates/nif/src/import/walk/emitter.rs:245`, `extract_emitter_rate` at `crates/nif/src/import/walk/emitter.rs:375`. Neither symbol appears in `walk/mod.rs`.

**Impact**: Sent this session's auditor to the wrong file; contributed to FO3-2026-09-11-D2-01 (particle azimuth not forwarded) going unaudited on the forwarding half for months, since the shared audit reference pointed at the wrong module.

**Related**: `docs/audits/AUDIT_FO3_2026-09-11.md` (FO3-2026-09-11-D2-03).

**Suggested Fix**: Update the `NIF Import:` row to list `walk/{mod, emitter, lights, node_attrs, texture_effect, tests}` and attribute `extract_emitter_params`/`extract_emitter_rate` to `emitter.rs`.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.

---

# #4247 — FO3-D5-2026-09-11-01: collision module docstring cites a non-existent example path (_tmp_fo3_d5_collision.rs)

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape) — `/audit-fo3` Dimension 5
**Location**: `crates/nif/src/import/collision/mod.rs:56-60`
**Status**: NEW

**Description**: The collision module docstring for `examine_collision_kind` cites a non-existent example path (`examples/_tmp_fo3_d5_collision.rs`), which was never committed and doesn't resolve.

**Evidence**: `crates/nif/src/import/collision/mod.rs:56-60` states `examine_collision_kind` is "a diagnostic entry point for corpus scans (`examples/_tmp_fo3_d5_collision.rs`)". No file of that name exists anywhere in the repository; `crates/nif/examples/` exists and holds many other real, committed probe examples, but not this one.

**Impact**: Self-inflicted doc rot invisible to the path-validate gate (it only scans `.claude/commands/*.md`, not `.rs` docstrings). Points the next FO3 dimension-5 auditor at a file that doesn't exist.

**Related**: `docs/audits/AUDIT_FO3_2026-09-11.md` (FO3-D5-2026-09-11-01).

**Suggested Fix**: Italicize as a deleted throwaway, or commit the probe under `crates/nif/examples/` and backtick the real path.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.

**Disposition**: Took the italicize-as-deleted-throwaway option rather than committing a placeholder example file with no real content.
