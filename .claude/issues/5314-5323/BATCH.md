# Batch 5314–5323 — batch summary (2026-10-05)

10/10 fixed. 11 LOCAL commits (23fd95055..fbadf5c7b), unpushed.

| # | Commit | What |
|---|---|---|
| 5314 (HIGH) | `23fd95055` | menuxml: three document-wide budgets in IncludeState — MAX_INCLUDE_BYTES 256 KiB (census: vanilla max payload 38,378 B), MAX_INCLUDE_FRAGMENT_BYTES 64 KiB (census max 12,582 B; oversized cached as miss, closing #5007's retained-bytes vector), MAX_DOCUMENT_TILES 16,384 (census max 205). Closes the ~7.2 MiB/KiB linear OOM at `--hud`. 3 regression tests; corpus lanes green |
| 5322 (MED) | `a5c3f2428` + `fbadf5c7b` | papyrus: strict expect_eol (records recovered error + skip_to_line_end that leaves End*/Else* unconsumed), require_input_end for the parse_expr lib entry, Token::KwIs + Expr::Is type-test at cast precedence (matches .pex OpCode::Is lift). Downstream exhaustive matches got transparent Is arms (scripting compat/effects, mq101 example, pex example) |
| 5316 (MED) | `d8295d77c` | hkx: MAX_TRANSFORM_SAMPLES 16 M → 1 M (8× census max 124,821, re-verified live: 6,126 clips); census test now asserts max_samples; probe's 279 KB → 15.67 M-sample shape pinned as unit test |
| 5323 (LOW) | `258ba2b90` | sfmaterial: skip_user_class_body walks read_order (offset-sorted) like the read path; byte-exact skip≡read test on a reordered class with String + List |
| 5320 (LOW) | `6489ae225` | sfmaterial: build() fails loudly — MissingDbFileIndex, RowInstanceMismatch (rows vs post-index instances), ObjectTrailingBytes in stream_db_file_index, DIFF/USRD index rejected (UnsupportedDbFileIndexChunk), DuplicateDbFileIndex replaces WrongChunkType{Objt,Objt}. Synthetic builder refactored into mutable chunk list + assembler (byte-identical); 4 new tests; real-data lane green |
| 5319 (LOW) | `8be8c3d3a` | bin: cdb_material_index open/extract `.ok()?` → matched + warned (source/path/error) before memoizing None; restored two lost `\` continuations (22-space runs); memoized-failure test |
| 5315 (LOW) | `ed4ab315f` | renderer: deleted the 2 truncated doc lines above draw_frame_size_budget_tests; tree-wide sibling scan clean |
| 5317 (LOW) | `f3b2be21e` | docs: feature-matrix gameplay table synced to the 2026-10-01 slice closure (P1/P2 closed, corpse loot ✓, P3/P4/P5 rows + gate scripts, animation+sound row → #4747); slice doc :98 reworded |
| 5318 (LOW) | `96567ea22` | docs: game-loop.md schedule table gained loading_model_turntable_system (Update, exclusive, e60911864); footstep row reworded for #5146 body emitter |
| 5321 (LOW) | `332be492e` | skill: audit-tech-debt SKILL.md gpu_material_size_claims bullet matches pinned_sizes() (#5203's five structs), hand-check kept for Vertex::SIZE + non-pinned structs |

Census (new, 2026-10-05, three legacy corpora): Oblivion 89 docs / FNV 121 / FO3 99;
max include elements per doc 50 (FNV), max fragment 12,582 B, max doc include payload 38,378 B,
max doc tiles 205. All well under every #5314 cap.

Gotchas hit:
- `cargo test --workspace` catches exhaustive matches in EXAMPLES that scoped `-p` lib checks miss
  (pex_corpus_shapes) — after adding an enum variant, gate with `--workspace --all-targets`, not per-crate check.
- Bin-crate (byroredux) work needs the rustup 1.96.0 cargo via `rustup which` + PATH prefix (per AGENTS.md).
