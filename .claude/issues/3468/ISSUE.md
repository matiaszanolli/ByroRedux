# #3468 — NIF-2026-08-27-D1-02: `NiSequence`'s pre-10.1.0.104 `Text Keys` ref is read and discarded, while its sibling `Accum Root Name` from the same prologue is carried forward

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: medium, nif-parser, nif, animation, bug

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 1 (Stream Position Integrity — byte accounting correct; payload dropped). Severity **MEDIUM**.

## Location
`crates/nif/src/blocks/controller/sequence.rs:148-155`; pinned by `crates/nif/src/blocks/controller/sequence_pre_10_1_0_106_tests.rs:130`.

Game affected: any NIF at file version ≤ 10.1.0.103 (NetImmerse / old-Gamebryo era; no vanilla Bethesda title ships there, reachable on legacy mod `.kf` content — the same reachability class as #3174). Introduced by `2695e4fe`, "Fix #2345: implement the pre-10.1.0.106 NiSequence/ControlledBlock layout", landed 2026-08-27.

## Description
nif.xml declares the `NiSequence` prologue pair (nif.xml lines 4204-4205) as
`<field name="Accum Root Name" type="string" until="10.1.0.103">` +
`<field name="Text Keys" type="Ref" template="NiTextKeyExtraData" until="10.1.0.103">`,
the mutually-exclusive complements of `NiControllerSequence`'s own `since="10.1.0.106"` re-declarations. #2345 correctly added both reads, and correctly threads the accum-root-name through to the struct — but drops the text-keys ref on the floor:

```rust
let seq_accum_root_name = if stream.version() <= NifVersion::V10_1_0_103 {
    stream.read_string()?
} else {
    None
};
if stream.version() <= NifVersion::V10_1_0_103 {
    let _seq_text_keys_ref = stream.read_block_ref()?;
}
```

`seq_accum_root_name` is later used as the fallback for `accum_root_name` (`sequence.rs:389-392`); `_seq_text_keys_ref` has no such counterpart, so `NiControllerSequence::text_keys_ref` stays `BlockRef::NULL` on this band. Re-verified at publish time: `sequence.rs:154` still binds `_seq_text_keys_ref`, and `seq_accum_root_name` is still consumed at `:392`.

## Evidence
`text_keys_ref` has exactly one production consumer — `crates/nif/src/anim/sequence.rs:139-144`:

```rust
let mut text_keys = seq
    .text_keys_ref
    .index()
    .and_then(|idx| scene.get_as::<NiTextKeyExtraData>(idx))
    .map(|tkd| tkd.text_keys.clone())
    .unwrap_or_default();
```

which feeds `collect_text_key_events` and thence the ECS text-event channel (footstep / hit / sound triggers). A null ref silently yields zero events. The new test at `sequence_pre_10_1_0_106_tests.rs:130` asserts `seq.text_keys_ref.is_null()`, so the drop is currently pinned as intended behaviour.

## Impact
Byte accounting is correct (no drift), but every animation sequence in sub-10.1.0.104 content loses all of its text keys — the exact asymmetry the same commit avoided for the accum root name. Latent on vanilla; reachable on legacy NetImmerse-era mod animation.

## Related
#2345 (the commit that introduced it); #3174 (same reachability class). The concurrent safety audit's two MEDIUMs on `crates/nif/src/anim/sequence.rs:20,23` (unsanitised `duration`/`weight`, #3432) and `sequence.rs:328-332` (the pre-10.1.0.106 `cycle_type = 0` / `-inf` duration branch, #3437) are adjacent to but distinct from this — they concern the *values* the same band produces, not a dropped ref.

## Suggested Fix
Bind the prologue ref (`let seq_text_keys_ref = …`) and use it as the fallback for `text_keys_ref` exactly the way `seq_accum_root_name` already backs `accum_root_name`; retarget the pinning assertion to the resolved value.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other `until="10.1.0.103"` prologue fields, `NiSequence` vs `NiControllerSequence` re-declaration pairs)
- [ ] **TESTS**: A regression test pins this specific fix (and `sequence_pre_10_1_0_106_tests.rs:130` is retargeted, not deleted)
