# TD2-003: `movs.rs`/`scol.rs` re-implement EDID/MODL/VMAD presence logic that `CommonNamedFields::from_subs_with_remap` already generalizes

Labels: low,tech-debt,esm-plugin,bug

**Description**: `common.rs` documents `from_subs_with_remap` as the intended single funnel, already followed at several tracked sites (`container.rs` #1045, `misc/world.rs` #2068, `misc/scene.rs` #2414). `movs.rs` hand-rolls its own EDID/MODL/VMAD loop instead. `scol.rs` does call the shared helper but its second SCOL-specific loop still re-adds a redundant `VMAD => has_script = true` arm, discarding the shared result in favor of a local shadow. No divergent bug-fix history between the copies.

**Evidence**:
`crates/plugin/src/esm/records/movs.rs:87-125`, `crates/plugin/src/esm/records/scol.rs:132-150`, `crates/plugin/src/esm/records/common.rs`.

**Impact**: No runtime impact — duplication risk; a future EDID/MODL/VMAD parsing fix would need to be applied in multiple places.

**Related**: #1045, #2068, #2414 (sibling sites already using the shared funnel).

**Suggested Fix**: In `movs.rs`, replace the manual loop with `CommonNamedFields::from_subs_with_remap`; in `scol.rs`, drop the redundant arm and use the already-computed `common.has_script`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
