//! Skyrim `hudmenu.swf` protocol pins (M48.6).
//!
//! Gated on `BYROREDUX_SKYRIM_DATA` (or `BYROREDUX_SKYRIMSE_DATA`)
//! pointing at a real Skyrim SE `Data/` directory, so `cargo test` stays
//! hermetic without the game installed.
//!
//! Pins the menu→engine half of the Scaleform HUD protocol from the
//! installed vanilla movie: every `GameDelegate.call` site in
//! `hudmenu.swf`'s bytecode — the requests the driver
//! (`byroredux/src/scaleform_hud.rs`) must answer through its
//! `ScaleformHostBridge` response handlers.
//!
//! The engine→movie half is pinned live: vanilla hudmenu registers only
//! the standard GameDelegate pair (`call`, `respond`) on
//! ExternalInterface, asserted by the `hud.debug` gate in
//! `docs/smoke-tests/m48-6-skyrim-hud.sh` — `respond` is the driver's
//! push surface (Ruffle has no GFx object-path invoke).

use byroredux_ui::avm1_host::referenced_host_methods;
use byroredux_ui::ScaleformHostCatalog;
use byroredux_ui::ScaleformProfile;

fn data_dir() -> Option<std::path::PathBuf> {
    for var in ["BYROREDUX_SKYRIM_DATA", "BYROREDUX_SKYRIMSE_DATA"] {
        if let Some(dir) = std::env::var(var).ok() {
            return Some(std::path::PathBuf::from(dir));
        }
    }
    let default = std::path::PathBuf::from(
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    );
    default.is_dir().then_some(default)
}

#[test]
fn hudmenu_host_calls_are_pinned_and_cataloged() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: Skyrim data not found");
        return;
    };
    let archive =
        byroredux_bsa::BsaArchive::open(dir.join("Skyrim - Interface.bsa")).expect("open Interface BSA");
    let swf = archive.extract("interface\\hudmenu.swf").expect("extract hudmenu.swf");

    let inventory = referenced_host_methods(&swf).expect("hudmenu.swf scans as AVM1");
    let methods: Vec<&str> = inventory.methods.iter().map(String::as_str).collect();
    assert_eq!(
        methods,
        ["GetButtonFromUserEvent", "PlaySound", "RegisterHUDComponents", "myLog"],
        "vanilla hudmenu's full host-call surface — a new name here means the \
         driver's protocol service needs review"
    );
    // Two dynamic call sites the walk cannot see; documented, not pinned
    // to zero — hudmenu builds some method names at runtime.
    assert_eq!(inventory.unresolved, 2);

    let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::SkyrimAvm1);
    for name in &methods {
        assert!(
            catalog.find(name).is_some(),
            "hudmenu calls '{name}' — the catalog must know it"
        );
    }
}
