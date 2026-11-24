use super::super::build_sound_archive_provider;
use super::loose_override::{esm_args, write};
use super::write_ba2;
use byroredux_audio::load_sound_from_bytes;

#[path = "../../../../crates/xwma/tests/support/fixture.rs"]
mod xwma_fixture;

#[test]
fn loose_xwma_effect_uses_the_later_root_and_decodes() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("Data");
    let overlay = tmp.path().join("Mod");
    let file = "Sound/FX/Mod/Step.xwm";
    let key = r"sound\fx\mod\step.xwm";
    let base_bytes = xwma_fixture::stream(&xwma_fixture::format(1), 6144);
    let override_bytes = xwma_fixture::stream(&xwma_fixture::format(2), 6144);
    write(&data, file, &base_bytes);
    write(&overlay, file, &override_bytes);

    let mut args = esm_args(&data);
    let provider = build_sound_archive_provider(&args);
    assert_eq!(
        provider.extract(key).expect("resolve from Data"),
        base_bytes
    );

    for root in [&data, &overlay] {
        args.extend([
            crate::mod_roots::LOOSE_ROOT_FLAG.to_owned(),
            root.to_string_lossy().into_owned(),
        ]);
    }
    let provider = build_sound_archive_provider(&args);
    let bytes = provider.extract(key).expect("resolve the mod's effect");
    assert_eq!(bytes, override_bytes);
    let sound = load_sound_from_bytes(bytes).expect("decode the loose override");
    assert_eq!(sound.num_frames(), 6144);
    assert_eq!(sound.sample_rate, 48_000);
}

#[test]
fn archived_xwma_effect_decodes_through_the_same_loader() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("Mod - Sounds.ba2");
    let key = r"sound\fx\mod\step.xwm";
    let bytes = xwma_fixture::stream(&xwma_fixture::format(1), 6144);
    write_ba2(&archive, &[(key, &bytes, false)]);

    let args = vec![
        "--sounds-bsa".to_owned(),
        archive.to_string_lossy().into_owned(),
    ];
    let provider = build_sound_archive_provider(&args);
    let bytes = provider.extract(key).expect("resolve the archived effect");
    let sound = load_sound_from_bytes(bytes).expect("decode the archived effect");
    assert_eq!(sound.num_frames(), 6144);
    assert_eq!(sound.sample_rate, 48_000);
}
