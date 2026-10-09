# #5456: SAFE-D2-2026-10-08-01: The #4470 Scaleform dialect shim recurses once per nested `DefineSprite` with no depth cap, before Ruffle ever sees the bytes

**Labels**: low,safety,ui,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5456

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-08.md` — `SAFE-D2-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW.
  - A stack overflow aborts the process and cannot be caught. That is why the Dim 2 stack-overflow bullet asks for this class to be reported.
  - The input is game-archive SWF data, not network input.
  - The pinned Ruffle `swf` reader (`0dde981`, `swf/src/read.rs:594` → `read_define_sprite` :1824 → `read_tag_list` :711) also recurses on nested sprites with no limit. So the shim does not open a new crash class; it moves the first overflow site into our code.
- **Dimension**: Memory Corruption / UB (stack-overflow risk)
- **Location**: `crates/ui/src/prepare.rs:111-155` (`patch_place_object3_stream`, recursive call at `:146`). It is reached from `prepare.rs:206` (`prepare_movie`, every root movie) and `crates/ui/src/navigator.rs:530` (every dependency movie).
- **Status**: NEW. It was introduced by `9813af435` (#4470, CLOSED). No issue or audit names it; searched `DefineSprite recursion` and `normalize_scaleform_dialect`.
- **Description**:
  - `normalize_scaleform_dialect` now runs on **every** SWF the engine loads, not only Starfield's.
  - Its tag walker recurses into each `DefineSprite` body (`code == 39 && len >= 4`) at any depth.
  - The SWF spec does not allow `DefineSprite` inside `DefineSprite`, but the walker does not enforce that. A crafted or corrupt movie can nest sprites at about 10 bytes per level (a 6-byte long-form header plus a 4-byte id and frame count).
  - At that rate, a few hundred KB to about 1 MB of input exhausts a 2–8 MiB thread stack.
  - Every slice index in the walker is bounds-checked (`p + 2`, `p + 6` and `body_end <= stream.len()` guards), so the depth is the only unbounded resource. The other is `decompress_zlib_after_header`'s uncapped `read_to_end` (`:85-92`); Ruffle's own `decompress_swf` has the same property.
- **Evidence**:
  ```rust
  } else if code == define_sprite && len >= 4 {
      // DefineSprite body: character id u16 + frame count u16, then a
      // nested (END-terminated) tag stream.
      patched += patch_place_object3_stream(
          &mut stream[body_start + 4..body_end],
          define_sprite,
          place_object_3,
      );
  }
  ```
- **Impact**: A malformed or hostile `.swf` in a mod archive crashes the engine at menu or HUD load with SIGSEGV (stack overflow). Retail content never nests sprites, so there is no effect on vanilla data.
- **Related**: #4470. `/audit-ui` owns the Scaleform host, and `/audit-parsers` owns parser discipline for untrusted input; flag this to both.
- **Suggested Fix**: Since nested sprites are illegal SWF, pass a `depth` argument and stop recursing past depth 1, so no sprite inside a sprite is walked. Alternatively, convert the walk to an explicit work-list. Optionally cap `read_to_end` at the header's declared `uncompressed_len`, which is a u32 at `[4..8]`. Neither change alters retail behaviour.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other SWF tag walkers in `crates/ui` (e.g. `navigator.rs` import scan) and the uncapped `decompress_zlib_after_header` `read_to_end`)
- [ ] **TESTS**: A regression test pins this specific fix
