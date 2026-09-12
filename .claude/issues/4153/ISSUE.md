# NIF-D2-2026-09-11-03: NiMaterialProperty reuses FLAGS_U32_THRESHOLD for an unrelated field's cut

URL: https://github.com/matiaszanolli/ByroRedux/issues/4153
Labels: bug, nif-parser, medium, nif, game:fo3

---

**Severity**: MEDIUM
**Dimension**: 2 — Version Gating
**Game Affected**: Fallout 3 dev/mod BSVERs 24-33 (brackets the affected value 26)
**Location**: `crates/nif/src/blocks/properties.rs:44`; `crates/nif/src/version.rs:432-436`
**Status**: NEW (re-verified carry-forward of NIF-D2-2026-09-04-03; no matching GitHub issue)

**Description**: `FLAGS_U32_THRESHOLD` is named/documented for the `NiAVObject.flags` u16→u32 widen (`bsver > 26`); `NiMaterialProperty`'s compact-color cut reuses it via `>=` for nif.xml's independent `bsver < 26` condition. Correct only because both constants happen to equal 26 today.

**Evidence** (`properties.rs:40-44`):
```rust
let bethesda_compact = stream.bsver() >= crate::version::bsver::FLAGS_U32_THRESHOLD;
```
`version.rs:432-436` documents `FLAGS_U32_THRESHOLD` strictly for the unrelated `NiAVObject.flags` widen.

**Impact**: Latent — a future correction of the flags cut (e.g. to 27) silently flips `NiMaterialProperty` at bsver == 26 with no test pinning the two together.

**Suggested Fix**: Add a distinct `MATERIAL_COMPACT_COLORS = 26` constant with its own doc and a two-sided test.

## Completeness Checks
- [ ] **TESTS**: A two-sided test pins `NiMaterialProperty`'s compact-color cut independent of `FLAGS_U32_THRESHOLD`

