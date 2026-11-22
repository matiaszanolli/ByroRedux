# #6998: UI-D2-2026-11-15-01: nothing in-tree pins per-forwarder identity of the adapter; the refactor's equivalence proof lived in a deleted temporary copy

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6998
- **Labels**: low,ui,test-gap,game:fo4,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: AVM2 Adapter
- **Profile**: Fallout4Avm2
- **Existing**: none (#2724/#2725 introduced the derive-from-push rule; #6952 is the closed refactor issue)
- **Location**: `crates/ui/src/avm2_host.rs` tests, `generated_adapter_is_valid_abc_with_one_helper_per_method` (`:1442`) and `generated_adapter_omits_destroy_helper_when_class_lacks_the_trait`
- **Status**: NEW
- **Description**: the SKILL's own risk statement is that "a wrong constant-pool index yields a movie that loads and misbehaves, and no test fails". 3cfab7f1d moved the whole constant-pool and method wiring across ten functions; its byte-identical check ran against a temporary copy of the old builder that was removed, so after the commit the only in-tree guards are count-based (traits/methods/bodies = catalog + 3/4, installer has `catalog + 1` `SetProperty`s, destroy/installer `CallPropVoid` counts, one identity assertion that the init body pushes `LOADED_CALLBACK`). A future edit that swaps two multiname pushes, offsets one forwarder's transport string, or mis-pairs an installer `(helper, property)` keeps every one of those assertions green. The data-gated pin catches it only for the handful of methods `hudmenu` calls at boot.
- **Evidence**: I wrote the test below, ran it at HEAD (passes), and verified the byte-compare harness catches a one-off `disp_id` start. The test pins, for every installed name i: installer pair i is (helper multiname named `__byro_fallout4_host_<i>`, property named `<method_i>`); script trait i+1 is that helper with `disp_id == i+1`; its body pushes exactly `"BGSCodeObj.<method_i>"`; its method has `NEED_REST`; and the four destroy strings are in the pool iff the flag is set.
- **Impact**: a latent regression class in the highest-risk code, silent at test time and visible only as a menu calling the wrong host method.
- **Suggested Fix**: add the test below (as `#[cfg(test)]` in `avm2_host.rs`, hermetic, no game data). Optionally also pin a golden length/FNV hash of `build_adapter_abc(FO4, &[], true/false)` (33,211 / 33,059 bytes at HEAD) so any intended change to the emitted ABC is a deliberate re-pin.
- **Completeness Checks**: SIBLING the same identity rule holds for the two `Op::GetLex`-by-multiname uses in ready/init (covered by `generated_adapter_is_valid_abc_with_one_helper_per_method`); TESTS this finding is the test.

```rust
    fn ops(code: &[u8]) -> Vec<Op> {
        let mut r = Reader::new(code);
        let mut v = Vec::new();
        while !r.as_slice().is_empty() { v.push(r.read_op().unwrap()); }
        v
    }

    #[test]
    fn every_forwarder_is_wired_to_its_own_name() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let object = catalog.host_object().unwrap();
        for destroy in [false, true] {
            let extra = vec!["ZetaExtra".to_string(), "AlphaExtra".to_string()];
            let abc = Reader::new(&build_adapter_abc(catalog, &extra, destroy).unwrap()).read().unwrap();
            let names = installed_method_names(catalog, &extra);
            let strings = &abc.constant_pool.strings;
            let s = |i: Index<String>| strings[i.as_u30() as usize - 1].clone();
            let script = &abc.scripts[0];
            let installer = &abc.method_bodies[abc.method_bodies.len() - 2];
            let iops = ops(&installer.code);
            let mut pairs = Vec::new();
            let mut k = 0;
            while k + 2 < iops.len() {
                if let (Op::GetLocal { index: 2 }, Op::GetLex { index: h }, Op::SetProperty { index: p }) =
                    (&iops[k], &iops[k + 1], &iops[k + 2]) { pairs.push((*h, *p)); k += 3; } else { k += 1; }
            }
            assert_eq!(pairs.len(), names.len());
            for (i, name) in names.iter().enumerate() {
                let (h, p) = pairs[i];
                assert_eq!(multiname_local_name(&abc, h).unwrap(), helper_name(i).into_bytes());
                assert_eq!(multiname_local_name(&abc, p).unwrap(), name.as_bytes());
                let t = &script.traits[i + 1];
                assert_eq!(t.name, h);
                let TraitKind::Method { disp_id, method } = &t.kind else { panic!() };
                assert_eq!(*disp_id as usize, i + 1);
                let body = abc.method_bodies.iter().find(|b| b.method == *method).unwrap();
                let pushed: Vec<_> = ops(&body.code).into_iter()
                    .filter_map(|o| if let Op::PushString { value } = o { Some(s(value)) } else { None }).collect();
                assert_eq!(pushed, vec![format!("{}.{}", object.property, name).into_bytes()]);
                assert!(abc.methods[method.as_u30() as usize].flags.contains(MethodFlags::NEED_REST));
            }
            let has = |n: &str| strings.iter().any(|x| x.as_slice() == n.as_bytes());
            for n in [DESTROYED_EVENT, DESTROY_CALLBACK, DESTROY_HELPER, object.on_destroy] {
                assert_eq!(has(n), destroy);
            }
        }
    }
```
(`Op`, `MethodFlags`, `TraitKind` are already imported at the top of `avm2_host.rs`; `Op` is `swf::avm2::types::Op`.)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
