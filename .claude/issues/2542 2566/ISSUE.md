# #2542: SCR-D3-NEW10-01: feature-matrix.md's M47.2 row states an incorrect decompiler pass order

**Severity**: LOW (doc-only; the correct ordering lives in `lower.rs`'s and `control_flow.rs`'s own module docs, so an engineer reading source would not be misled — only a reader of the feature matrix alone would be)
**Dimension**: Decompiler Control-Flow/Boolean/Lower
**Untrusted-Input**: No — documentation only
**Location**: `docs/feature-matrix.md:157`
**Status**: NEW (not previously filed; confirmed absent from the 94 open issues checked)

## Description
The parenthetical lists the decompiler pipeline as `CFG→lift→control-flow→lower→short-circuit`. The real order, verified against `decompile_body` in `lower.rs`, is `cfg → lift → rebuild_boolean_operators (short-circuit) → reconstruct (control-flow) → lower_body`. Two swaps: short-circuit collapse is third, not last; control-flow reconstruction is fourth, not third.

## Evidence
Confirmed directly at `docs/feature-matrix.md:157`: "CFG→lift→control-flow→lower→short-circuit". `crates/pex/src/decompile/lower.rs:230-236`:
```rust
let mut cfg = build_cfg(func)?;
let mut scopes = lift_function(object, func, &cfg)?;
// Collapse `&&`/`||` short-circuits before control-flow reconstruction
// so compound conditions surface as one expression, not nested ifs.
rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
let nodes = reconstruct(cfg, scopes, &func.name)?;
Ok(lower_body(&nodes))
```

## Impact
Cosmetic only. A reader relying solely on the feature matrix could form an incorrect mental model of pipeline structure.

## Suggested Fix
Update `docs/feature-matrix.md:157` to read `CFG→lift→short-circuit→control-flow→lower` (matching module names).

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)

---

# #2566: OBL-D1-05: audit-oblivion SKILL.md mis-states the Oblivion retail version and the pre-Gamebryo fallback behaviour

**Severity**: LOW
**Dimension**: NIF Version Handling
**Location**: `.claude/commands/audit-oblivion/SKILL.md:22-24,68-70`
**Status**: NEW

## Description
The skill's own brief names v20.0.0.5 as the dominant retail body; the live census says it's actually v20.0.0.4 (7,282 files vs. 1,680) — `version.rs` already documents this correctly, the skill contradicts it. The skill also claims pre-v3.3.0.13 files return an empty `NifScene`; the parser actually parses inline and only fails (with a `warn`) on a mid-file inline-name read error.

## Evidence
Confirmed directly: `SKILL.md:22-24` names v20.0.0.5 as "what most clutter / architecture / creature meshes are."

## Impact
Documentation-only, but it is the brief every Dimension-1 agent reads first — feeds incorrect framing into every future Oblivion audit cycle.

## Related
#2348/#2347 (same doc-drift class from earlier Oblivion audits).

## Suggested Fix
Correct the skill to name v20.0.0.4 as dominant (matching `version.rs`'s live census), and correct the pre-v3.3.0.13 fallback description to match the parser's actual behavior (inline parse + warn-on-error, not an empty-scene return).

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
