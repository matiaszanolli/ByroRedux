# P3-BUILD: Bin crate shipped uncompilable on main (479163836 → eb3784309): the default toolchain cannot build it, so nothing caught it — prevention gap

- **Labels**: low,bug,tech-debt
- **Filed**: 2026-09-19, follow-up to the #4458 fix session (post-/audit-character)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4466

---

Incident record (the break itself is already fixed — this issue tracks why nothing caught it and the standing prevention gap).

**What happened**

- `479163836` (P3 Closure, 2026-09-19) landed with the bin crate not compiling: `pickup_loot` called `mark_picked_up` with `&World` against a `&mut World` signature, and the same never-compiled function carried two more masked errors (a `World::insert` needing `&mut`, and a guard-borrow error). `main` stayed uncompilable until `eb3784309` fixed it later the same day.
- Nothing noticed because **no toolchain available by default in this environment can build the bin crate**: `cranelift-*` 0.134 (via wasmtime / `crates/mod-runtime`) requires rustc ≥ 1.94, and the default `cargo`/`rustc` on PATH is the distro 1.93.1, whose resolver rejects the MSRV before compiling anything.
- The 1.96.0 rustup toolchain IS installed and works, but `cargo +1.96.0 …` silently does nothing useful here: `/usr/bin/cargo` shadows the rustup shims (the `+toolchain` directive is rejected), so the working invocation is:
  ```bash
  TC=$(rustup which --toolchain 1.96.0 cargo)
  PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux
  ```
- Consequence: the bin-crate test suite had not executed at all across the P3 commits, and 9 latent failures accumulated unseen (the loot/save-format follow-ups; one — the save-registry completeness guard — was fixed in `eb3784309`, the other 8 are filed separately).

**Impact**

Any commit touching `byroredux/src/**` can currently break the build without local feedback, exactly as `479163836` did. The audit infrastructure has the same blind spot — the 2026-09-11 and 2026-09-19 `/audit-character` runs both recorded "bin-crate guard test couldn't run" as an environment limit and verified by source read instead.

**Suggested Fix**

1. Document the working 1.96 invocation in `docs/contributing.md` (test tiers) and/or `AGENTS.md` quick reference, next to `cargo test`.
2. Better: get the bin crate building on the default toolchain again — either bump the distro toolchain, or `cargo update` the wasmtime/cranelift tree to versions whose MSRV fits 1.93.1, whichever the project prefers.
3. If CI exists for the bin crate, ensure it runs on a ≥1.94 toolchain so this class cannot recur upstream.

## Completeness Checks
- [ ] **SIBLING**: Whatever lands, verify the audit skills' "bin-crate guard couldn't run" caveat can be retired
- [ ] **TESTS**: A fresh clone + documented invocation builds the bin crate and runs its suite green (once the two sibling issues land)
