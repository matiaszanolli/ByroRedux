# NIF-D2-2026-09-11-05: NifVariant::detect's FO76 lower bound contradicts its own constant's doc comment

URL: https://github.com/matiaszanolli/ByroRedux/issues/4161
Labels: documentation, nif-parser, low, nif, doc-rot

---

**Severity**: LOW
**Dimension**: 2 — Version Gating
**Location**: `crates/nif/src/version.rs:676-698,487-490`
**Status**: NEW

**Description**: `NifVariant::detect`'s FO76 lower bound (`bsver::FO76 = 155`) contradicts its own constant's doc comment ("Fallout 76 (lower bound; FO76 spans 152..=167 in shipping content)"), and the FO76/Starfield boundary in `detect` is a bare `170` literal rather than a named constant (`bsver::STARFIELD = 172`).

**Evidence** (`version.rs:487-490`): `FO76` constant doc says the range starts at 152, but `detect`'s match arms (`:676-698`) branch on the literal FO76 constant value (155) and a bare `170`, not `152` or `STARFIELD` (172).

**Impact**: Latent inconsistency between documentation and code; "cosmetic distinction today" per the code's own comment (every shader gate that matters uses `bsver >= 132`, which both bands satisfy identically), but a future reader trusting the doc comment over the code (or vice versa) draws a wrong conclusion about the boundary.

**Suggested Fix**: Either correct the `FO76` doc comment to match the code's actual lower bound, or use a named `bsver::FO76_STARFIELD_BOUNDARY` constant (170) instead of the bare literal in `detect`, matching the file's own naming convention.

## Completeness Checks
- [ ] none — documentation/naming-consistency fix only, no behavioral test applies

