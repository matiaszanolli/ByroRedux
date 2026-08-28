# Issue #3503: REG-2026-08-27-01: regression of #3237 — the GRUP depth cap reaches 6 of 14 recursion sites; 8 self-recursive ESM walkers are still unbounded

- **Severity**: HIGH
- **Dimension**: Regression / ESM parser safety
- **Labels**: high, esm-plugin, safety, bug
- **Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-27.md`
- **Filed**: 2026-08-28

---

## Description

**Regression of #3237** (CLOSED 2026-08-26, `high`/`bug`/`esm-plugin`) — the fix was only partially applied.

#3237's premise was *"Every GRUP-tree walker in the ESM/ESP parser recurses into nested groups unconditionally — there is no depth counter."* The fix added `MAX_GRUP_NESTING_DEPTH = 64` (`crates/plugin/src/esm/reader.rs:32`) and a centralising helper `bounded_group_content_end` (`crates/plugin/src/esm/reader.rs:768`) whose own doc comment states the intent — *"Centralising the guard keeps every GRUP walker on the same boundary and makes future recursive walkers harder to add without noticing the safety contract."*

It was wired into **6** call sites: `records/grup_walker.rs:40,96,164,315`, `cell/walkers.rs:132`, `cell/wrld.rs:260` — exactly the four walkers the issue body enumerated by name (`extract_records` / `extract_records_with_modl` / `extract_dial_with_info` / `extract_quest_dialogue_scene_tree_inner`) plus `parse_cell_group` and `parse_wrld_children`, which were correctly refactored into `_inner(…, depth: u32)` forms.

**Eight further self-recursive walkers were not touched.** Each still calls the unguarded `group_content_end` and recurses into itself with no depth parameter threaded anywhere in its signature:

| Walker | Definition | Unguarded recursion | Reached from |
|---|---|---|---|
| `parse_refr_group` | `cell/walkers.rs:653` | `:663-669` | `parse_cell_group_inner` (types 6/8/9), `parse_wrld_children_inner` |
| `parse_modl_group` | `cell/support.rs:342` | `:349-354` | `cell/dispatch_world_placement.rs` |
| `parse_ltex_group` | `cell/support.rs:375` | `:382-387` | `records/mod.rs:292` (`b"LTEX"`) |
| `parse_txst_group` | `cell/support.rs:432` | `:440-445` | `records/mod.rs:294` (`b"TXST"`) |
| `parse_scol_group` | `cell/support.rs:555` | `:562-567` | `records/mod.rs:303` (`b"SCOL"`) |
| `parse_pkin_group` | `cell/support.rs:624` | `:631-636` | `records/mod.rs:319` (`b"PKIN"`) |
| `parse_movs_group` | `cell/support.rs:689` | `:696-701` | `records/mod.rs:335` (`b"MOVS"`) |
| `parse_mswp_group` | `cell/support.rs:751` | `:757-762` | `records/mod.rs:351` (`b"MSWP"`) |

All eight sit on the ordinary top-level GRUP dispatch path that runs on every `.esm`/`.esp` the engine loads, including third-party mod content — the same reachability argument #3237 itself made.

> **Note for future triage — a withdrawn counter-claim.** A sibling tech-debt report from the same 2026-08-27 sweep asserted the opposite (that the guard reaches every walker). That claim is **WITHDRAWN and wrong**; it was re-verified symbol-by-symbol against the live tree before this issue was filed. The verification command is one line and is reproduced under Evidence — please re-run it rather than re-litigating from the prior report.

## Location

`crates/plugin/src/esm/cell/support.rs:351,384,442,564,633,698,759` · `crates/plugin/src/esm/cell/walkers.rs:665` · guard at `crates/plugin/src/esm/reader.rs:768-786`

## Evidence

Reproduce the split in one line — 6 guarded sites vs. 8 unguarded production recursions:

```
$ grep -rn "bounded_group_content_end" crates/plugin/src/ | grep -v "reader.rs\|MAX_GRUP"
crates/plugin/src/esm/records/grup_walker.rs:40  …"extract_records_with_modl"
crates/plugin/src/esm/records/grup_walker.rs:96  …"extract_records"
crates/plugin/src/esm/records/grup_walker.rs:164 …"extract_dial_with_info"
crates/plugin/src/esm/records/grup_walker.rs:315 …
crates/plugin/src/esm/cell/walkers.rs:132        …"parse_cell_group"
crates/plugin/src/esm/cell/wrld.rs:260           …"parse_wrld_children"

$ grep -rn "reader.group_content_end" crates/plugin/src/esm/cell/{support,walkers}.rs
crates/plugin/src/esm/cell/walkers.rs:665
crates/plugin/src/esm/cell/support.rs:351
crates/plugin/src/esm/cell/support.rs:384
crates/plugin/src/esm/cell/support.rs:442
crates/plugin/src/esm/cell/support.rs:564
crates/plugin/src/esm/cell/support.rs:633
crates/plugin/src/esm/cell/support.rs:698
crates/plugin/src/esm/cell/support.rs:759
```

```rust
// crates/plugin/src/esm/cell/walkers.rs:663-669 — verbatim
while reader.position() < end && reader.remaining() > 0 {
    if reader.is_group() {
        // Nested groups within cell children — recurse.
        let sub = reader.read_group_header()?;
        let sub_end = reader.group_content_end(&sub);
        parse_refr_group(reader, sub_end, refs, landscape, navmeshes, deleted)?;
        continue;
    }
```

```rust
// crates/plugin/src/esm/cell/support.rs:349-354 — the shape repeated 7x
if reader.is_group() {
    let sub = reader.read_group_header()?;
    let sub_end = reader.group_content_end(&sub);
    parse_modl_group(reader, sub_end, statics)?;
    continue;
}
```

Contrast the guarded form the same fix installed one file over:

```rust
// crates/plugin/src/esm/cell/walkers.rs:130-135
let sub_group = reader.read_group_header()?;
let Some(sub_end) =
    reader.bounded_group_content_end(&sub_group, depth, "parse_cell_group")
else {
    continue;
};
```

The single guard test, `deeply_nested_grup_is_skipped_at_shared_limit` (`crates/plugin/src/esm/records/grup_walker.rs:394-419`), builds `MAX_GRUP_NESTING_DEPTH + 128` nested `GRUP`s and feeds them to `extract_records` **only**. Substituting any of the eight walkers above into that same fixture reproduces the pre-#3237 unbounded descent — the fixture is already written, it simply never points at them.

## Impact

The stack-overflow-on-crafted-plugin vector #3237 was closed for is still live through eight independent entry points. A `GRUP` header is 20–24 bytes, so a few hundred KB of nested minimal groups drives the recursion tens of thousands of levels deep and aborts the process — an uncatchable crash, not a `Result`-typed parse failure.

`parse_refr_group` is the worst of the eight: it carries six `&mut` parameters and a large local set, so its frame is among the biggest in the parser, and it is reachable from *both* the interior-cell and the worldspace descents.

## Related

- #3237 — the partially-applied fix (CLOSED)
- #3279 (OPEN) — same defect class in `Effect::Conditional`'s `lower_statements`
- #1385 — `MAX_COLLISION_SHAPE_DEPTH`, the reference model
- REG-2026-08-27-02 — the traceability gap that let this close silently: #3237's fix landed inside mega-commit `06f86742` under the line *"refactor(plugin): implement bounded group content parsing for ESM readers"*, with no closing keyword and no per-site accounting

## Suggested Fix

Convert all eight to the `_inner(…, depth: u32)` + `bounded_group_content_end` form the fix already established, then extend `deeply_nested_grup_is_skipped_at_shared_limit` into a table-driven test that drives the *same* nested fixture through every recursive walker, so the next walker added without a depth parameter fails CI. Consider making `group_content_end` `pub(super)`-restricted or `#[deprecated]`-annotated so the unguarded helper is hard to reach from a new walker at all.

## Source

`docs/audits/AUDIT_REGRESSION_2026-08-27.md` — REG-2026-08-27-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all recursive GRUP walkers across `records/`, `cell/`, `cell/wrld.rs`, not just the eight listed)
- [ ] **TESTS**: A regression test pins this specific fix
