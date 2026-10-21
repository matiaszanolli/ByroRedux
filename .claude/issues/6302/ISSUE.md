# #6302: NIF-D5-2026-10-21-01: FO76/Starfield `bhkRagdollSystem` payloads are Havok tagfiles (`TAG0`), not packfiles: a misleading "packfile: magic mismatch" WARN fires on 820 vanilla NIFs, and `physal.md` claims the container is decoded for them

**Labels**: low,bug,nif-parser,nif,physics,game:fo76,game:starfield

**Source**: docs/audits/AUDIT_NIF_2026-10-21.md · **Severity**: LOW · **Dimension**: Collision/Shader Parsing · **Location**: `crates/nif/src/import/collision/ragdoll.rs` (`extract_np_ragdoll`); `crates/nif/src/blocks/collision/havok_packfile.rs`; `docs/engine/physal.md`

- **Game Affected**: FO76 (`bsver` 155), Starfield (`bsver` ≥ 172)
- **Location**: `crates/nif/src/import/collision/ragdoll.rs:361-374` (`extract_np_ragdoll`), `crates/nif/src/import/mod.rs:163` (called from every `import_nif_scene*`), `crates/nif/src/blocks/collision/havok_packfile.rs:377-384`, `docs/engine/physal.md:189` and `:318-325`
- **Status**: NEW (searched open and closed issues for `TAG0`, `tagfile`, `bhkRagdollSystem Starfield` and `extract_np_ragdoll`: no match; #2355 counted the 316 Starfield `bhkRagdollSystem` blocks but never identified their format)
- **Description**: `extract_ragdoll` runs on every scene import. When a scene has no discrete `bhkConstraint`, it falls through to `extract_np_ragdoll`, which hands the first `bhkRagdollSystem` blob to `decode_np_ragdoll` and then `parse_havok_packfile`. Fallout 4 blobs are classic packfiles. Every FO76 and Starfield blob instead starts with a big-endian size word followed by the `TAG0` tag (`00 00 4a 9c 54 41 47 30 …`), which is the Havok tagfile container. `parse_havok_packfile` rejects them on the magic, and `extract_np_ragdoll` logs `log::warn!("extract_np_ragdoll: bhkRagdollSystem block {i} not decoded: havok packfile: magic mismatch")`.
- **Evidence**: The `ragdecode` probe ran over `SeventySix - Meshes.ba2` + `MeshesExtra.ba2` and Starfield `Meshes01/02/Patch`:
  ```
  [fo76] {"err havok packfile: magic mismatch": 249} files 249 imported 0
    meshes\actors\robot\parts\handrightcannon.nif: … (len 11660 magic [00, 00, 2d, 8c, 54, 41, 47, 30])
  [sf] {"err havok packfile: magic mismatch": 571} files 571 imported 0
    meshes\actors\hoppera\characterassets\skeleton_ref.nif: … (len 74720 magic [00, 01, 23, e0, 54, 41, 47, 30])
  ```
  `physal.md:189` puts FO4, FO76 and Starfield in one row whose status is "container + object table decoded (#3809, `parse_havok_packfile`)". `:318-325` describes "the classic packfile header" as *the* FO4+ container. The `np_ragdoll.rs` module doc also opens with "FO4+ `bhkRagdollSystem` payload".
- **Impact**: Nothing breaks. FO76 and Starfield ragdolls are out of scope, and `None` is the right result. However, each distinct FO76 or Starfield actor model that gets imported logs a WARN, including robot hand parts. The WARN names a "magic mismatch", which reads like corrupt data rather than a known unsupported format. The doc row also tells the next reader that the container is decoded for those two games, when the decoder in fact rejects 100% of their blobs. Whoever picks up FO76 or Starfield ragdolls will start from the wrong premise.
- **Related**: #5015 (FO4 decode), #3809 (container), #2355.
- **Suggested Fix**: Recognise the `TAG0` tag (offset 4) in `parse_havok_packfile` and give it a distinct error kind or variant, or check it in `extract_np_ragdoll`. Decline that case at `debug` level with "Havok tagfile (FO76/Starfield) — not decoded". Split the `physal.md` row so FO4 says "packfile" and FO76/Starfield say "tagfile, opaque". Change the module doc to "FO4".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
