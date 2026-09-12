# NIF-D5-2026-09-04-01: FO4+ bhkRigidBody reads the Skyrim CInfo body against a CInfo2014 wire layout, by the parser's own admission

URL: https://github.com/matiaszanolli/ByroRedux/issues/4156
Labels: bug, nif-parser, medium, nif, game:fo4, game:fo76, game:starfield, physics

---

**Severity**: MEDIUM
**Dimension**: 5 — Collision/Shader Parsing
**Game Affected**: Fallout 4, Fallout 76, Starfield
**Location**: `crates/nif/src/blocks/collision/rigid_body.rs:107-113,173-178`
**Status**: NEW (previously identified in the unpublished `AUDIT_NIF_2026-09-04.md`, re-verified unchanged this session; no matching GitHub issue)

**Description**: `BhkRigidBody::parse` has no real FO4+ CInfo prefix arm — it skips 4 trailing bytes and reads the remaining fields at the Skyrim field order, with in-code comments explicitly calling the FO4+ (`bhkRigidBodyCInfo2014`) layout "very different" and "knowingly incomplete," preserved post-`#546` specifically to avoid a new regression rather than because the layout was verified.

**Evidence** (`rigid_body.rs:107-113`):
```rust
// bsver >= crate::version::bsver::FALLOUT4 (FO4+): bhkRigidBodyCInfo2014 has a very different
// layout — motion system / deactivator / quality / penetration
// depth / time factor are interleaved with callback delay. That
// path is knowingly incomplete and is tracked separately; we
// preserve the pre-#546 behaviour of reading straight into
// Translation here so FO4 doesn't newly regress.
```
and the FO4+ arm at `:173-178` preserves the pre-#546 4-byte skip rather than decoding CInfo2014's real layout.

**Impact**: Any FO4/FO76/Starfield NIF using the classic (non-NP) `bhkRigidBody` chain yields garbage `mass`/`friction`/`motion_type`/`havok_filter` feeding straight into the PHYSAL solver's Static/Dynamic/Keyframed classification. Invisible to `cargo test`.

**Related**: `#546` (deliberately left this arm alone); `#3809` (the separate NP/`BhkSystemBinary` chain — not the same code path).

**Suggested Fix**: Decode `bhkRigidBodyCInfo2014` per nif.xml, or at minimum make the assumption measurable via `summarize_collision_authoring`.

## Completeness Checks
- [ ] **TESTS**: A fixture pinning the correct CInfo2014 field layout once decoded (or, short-term, a measurement hook via `summarize_collision_authoring`)

