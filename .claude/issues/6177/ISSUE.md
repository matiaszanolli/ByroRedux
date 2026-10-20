# TD4-2026-10-19-05: `.claude/agents/*.md` residuals after #5839 (still outside the validator)

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW | **Dimension**: 4 | **Effort**: small | **Age**: `88ea1a2f6` fixed the renderer facts; these were missed
- **Evidence**: `ecs-specialist.md:20` lists `resources.rs` (it is `resources/mod.rs`); `:32-33` says `transform_propagation_system` lives in `animation.rs` (it is `make_transform_propagation_system` at `crates/core/src/ecs/systems.rs:65`; only `animation_system` is in `byroredux/src/systems/animation.rs:1057`); `:50` "current workspace target is 1979+" (a `cargo test --workspace -- --list` listing shows ~11.2k test lines). `legacy-specialist.md:38` maps `m_kWorld` to `WorldTransform` (the ECS component is `GlobalTransform`; `WorldTransform` is an SDK projection type, `crates/sdk/src/projection.rs:21`), `:43` `CollisionObject` (no such component; `components/collision.rs` has `CollisionShape`/`RigidBodyData`), `:49` "34.1.1.3 (Skyrim SE)" (Skyrim SE is 20.2.0.7 / BSVER 100; 34.x is Starfield). Its header counts (NiMain 245, SDK 1,592) cannot be re-measured: see "Unverifiable".
- **Proposed fix**: point the layout bullets at `CLAUDE.md` (as `renderer-specialist.md` now does), delete the test-count invariant, fix the three legacy-table names; add `.claude/agents/*.md` to `command_files` in `_audit-validate.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
