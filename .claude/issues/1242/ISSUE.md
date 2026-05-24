# NIF-D2-NEW-01: bsver::FO4_ENV_SCALE docstring contradicts wire format + constant's actual usage

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1242

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 2)
**Severity**: LOW (doc-hygiene)
**Dimension**: Version Handling

## Description

The constant `bsver::FO4_ENV_SCALE = 140` at `crates/nif/src/version.rs:234-237` carries a docstring claiming `env_map_scale "moves inside an extended wetness block"` at `bsver >= FO4_ENV_SCALE`. The comment at `crates/nif/src/blocks/shader.rs:943-957` (added by #1223 on 2026-05-19) explicitly refutes this claim: gating the wetness env_map_scale on `>= 140` dropped Starfield Meshes01 parse rate from 97.21% → 95.77%, so the gate was forced to `false` for both `>= 130` (FO4) and `>= 155` (Starfield/FO76).

The constant is now repurposed as the upper bound of the FO4-DLC SSR/skin-tint range — at `shader.rs:1205` and `shader.rs:1218` it gates `bsver >= FALLOUT4 && bsver < FO4_ENV_SCALE` (i.e. BSVER 130..=139 only — the SSR/skin-tint bools absent at FO76+ BSVER 155).

## Evidence

```rust
// version.rs:234-237 (claim)
/// FO4 patch — `env_map_scale` moves inside an extended wetness
/// block. Content with `bsver >= FO4_ENV_SCALE` uses the new
/// layout; `bsver < FO4_ENV_SCALE` keeps the old position.
pub const FO4_ENV_SCALE: u32 = 140;

// shader.rs:943-957 (refutation)
// The `FO4_ENV_SCALE = 140` constant's docstring claimed
// env_map_scale "moves inside wetness" at BSVER >= 140 —
// that claim doesn't hold against the Starfield (BSVER 168+)
// corpus: gating wetness env_map_scale on `>= FO4_ENV_SCALE`
// dropped Starfield Meshes01 parse rate from 97.21% to 95.77%.

// shader.rs:1205, :1218 (actual use)
if bsver >= crate::version::bsver::FALLOUT4 && bsver < crate::version::bsver::FO4_ENV_SCALE {
    // SSR bools / skin-tint alpha — BSVER 130..=139 only
}
```

## Impact

Future contributor reading `bsver.rs` sees a docstring that points at an empirically wrong gate, and may reintroduce the duplicate-read bug #1223 just fixed. Pure doc drift; runtime behaviour is correct.

## Suggested Fix

Rewrite the docstring to describe the **actual** semantic — "Upper bound of the FO4-DLC range that authors `BSEffectShader` SSR/skin-tint trailing bools (BSVER 130..=139). FO76+ (BSVER ≥ 155) does NOT carry these fields. The previous interpretation (env_map_scale moves inside wetness at this BSVER) was empirically wrong — see `shader.rs:943-957` for the Starfield Meshes01 parse-rate evidence and #1223." Optionally rename to `FO4_DLC_UPPER` to make the new semantic load-bearing in the name.

## Related

- #1223 (CLOSED 2026-05-19): the fix that obsoleted the docstring claim
- #982 D2-NEW-09: prior doc-hygiene pattern (closed 2026-05-13)

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: spot-check the other `FO3_*` / `FO4_*` BSVER constants in `version.rs` for similar empirically-stale docstrings
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A (docstring rewrite + optional rename — no behaviour change)