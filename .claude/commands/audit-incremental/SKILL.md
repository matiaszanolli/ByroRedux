---
description: "Delta audit — check only recently changed code for regressions and new bugs"
argument-hint: "[--working] [--commits <N>] [--range <A>..<B>] [--since <date>]"
---

# Incremental / Delta Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Audit **only what changed**. This is a meta-audit: it defines no dimensions of its own, it *dispatches* the diff to the owning audits (`.claude/commands/audit-<name>/SKILL.md`) and applies their checks to the hunks alone. Do not re-audit untouched code.

## Step 1 — Scope

Default is the working tree.

| Argument | Diff command |
|---|---|
| *(none)* / `--working` | `git diff HEAD --name-only` (staged only: `git diff --staged --name-only`) |
| `--commits <N>` | `git diff HEAD~<N>..HEAD --name-only` |
| `--range <A>..<B>` | `git diff <A>..<B> --name-only` |
| `--since <date>` | base=`$(git log --since="<date>" --format=%H \| tail -1)`; `git diff ${base}^..HEAD --name-only` |

Also pull the hunks for the same scope (`git diff <scope>`; `-U6` for context) and `git log --oneline <scope>` for themes and milestone tags.

## Step 2 — Route (mechanical)

```bash
git diff <scope> --name-only | .claude/commands/_audit-route.sh | sort -t$'\t' -k2,2r
```

Output is `path  RISK  owners` from `.claude/commands/_audit-owners.md` (first matching row wins; every listed owner applies; `Dim N` narrows an owner to one dimension). Then:

1. Group changed files by owner. For each owner, open its SKILL.md, read the `Paths:` line of each dimension, and run **only the dimensions whose Paths intersect the diff** — apply their checklists to the diff.
2. Risk is the *floor* severity for an un-disproven finding there.
3. `(unrouted)` paths mean the ownership map has a gap: report it as a `tech-debt` finding against `_audit-owners.md` (add a row) and route the file by judgement meanwhile.
4. A file can hit several owners (a shader + its `#[repr(C)]` host struct → renderer **and** the GPU-struct rule). Check both.

## Step 3 — Delta checks on every changed hunk

- [ ] **New bug** — logic error, off-by-one, wrong byte width, missing version/era gate (B-splines reach FNV/FO3, not just Skyrim+).
- [ ] **Contract break** — public signature changed without every call site (`git grep` the symbol workspace-wide).
- [ ] **Silent divergence** — a value built at two sites and only one edited (the classic NIFAL leak: the two `Material` load paths, `byroredux/src/cell_loader/spawn.rs` and `byroredux/src/scene/nif_loader.rs`; defer to `/audit-nifal`).
- [ ] **Unsafe delta** — new `unsafe`, or a SAFETY comment that no longer matches the body (MEDIUM floor for unsafe-without-comment).
- [ ] **Lock / query delta** — changed `RwLock` scope or new multi-component query: TypeId-sorted acquisition (deadlock → HIGH). A new system acquisition must be declared at its registration under `byroredux/src/boot/schedule/` (guard: `byroredux/src/boot/schedule/mod.rs` `system_access_declaration_tests`).
- [ ] **Vulkan delta** — new pipeline/barrier/sync, AS build/refit, descriptor write: missing barrier or wrong AS geometry → severity special rules.
- [ ] **GPU-struct lockstep** — a touched `#[repr(C)]` struct (`GpuInstance`/`GpuCamera`/`GpuMaterial`/`GpuLight`) **and** its mirror in every shader reading it; size/offset drift → HIGH. Guards live in `crates/renderer/src/vulkan/scene_buffer/`.
- [ ] **Save shape** — a touched `Serialize` type: does `byroredux/src/save_io/serde_default_guard_tests.rs` need a baseline refresh or a `FORMAT_MAJOR` bump (`/audit-save`)?
- [ ] **Missing test** — changed path with no test update; list under "Missing Tests" even if the code is right.
- [ ] **Rust** — Vulkan destroy order still reverse of build; new `unwrap()`/`expect()` on a recoverable path; borrow-scope changes; a new impl consistent with its family (Component storage decl, `Send + Sync`).

Multi-file translation chains — a diff to one tier is incomplete without the others:
- **Particle emitter**: typed blocks (`crates/nif/src/blocks/particle.rs`) → extraction (`crates/nif/src/import/walk/emitter.rs`) → system (`byroredux/src/systems/particle.rs`).
- **Collision shape**: a new `Bhk*Shape` parser (`crates/nif/src/blocks/collision/`) must also be mapped in `crates/nif/src/import/collision/mod.rs`, or it is silently dropped (MEDIUM; HIGH if visible content vanishes). PHYSAL consumes ragdoll constraints, so it can ripple into `byroredux/src/ragdoll.rs` + `crates/physics/`.

## Step 4 — Dedup and report

Dedup per `_audit-common.md` § Deduplication (a regression of a closed issue is "Regression of #NNN"). Extra finding field: **Changed in**: `<file>` (commit `<hash>` / working tree).

Write `docs/audits/AUDIT_INCREMENTAL_<TODAY>.md`: (1) change summary — scope, files, themes; (2) routing map — file → owner dimensions audited; (3) findings; (4) missing tests. Then suggest `/audit-publish docs/audits/AUDIT_INCREMENTAL_<TODAY>.md`.
