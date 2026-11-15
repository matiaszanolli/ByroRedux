//! Fallout 4 `hudmenu.swf` protocol pins (M48.7).
//!
//! Needs a real Fallout 4 `Data/` directory (`BYROREDUX_FO4_DATA`, else the
//! canonical install path), so the pin is `#[ignore]`d: plain `cargo test`
//! stays hermetic, and the real-data lane's `--ignored` run fails on a
//! missing archive instead of skipping green (#6999).
//!
//! Pins the runtime lifecycle contract the Scaleform HUD driver
//! (`byroredux/src/scaleform_hud.rs`) relies on for the AVM2 game: the
//! vanilla `hudmenu.swf` out of `Fallout4 - Interface.ba2` accepts the
//! injected BGSCodeObj forwarding adapter (`AdapterInjected`), registers
//! the adapter's lifecycle callbacks, acknowledges destruction on drop
//! (it declares the `onCodeObjDestruction` hook), and everything it calls
//! on the host is cataloged — `unknown_methods` stays empty.

use std::sync::Arc;

use byroredux_bsa::Ba2Archive;
use byroredux_ui::{ScaleformHostObjectState, ScaleformProfile, ScaleformValue, SwfPlayer};

/// The game's `Data/` directory; an explicitly set override must exist.
fn data_dir() -> std::path::PathBuf {
    use byroredux_plugin::esm::test_paths::{data_dir, FO4_DEFAULT, FO4_ENV};
    data_dir(FO4_ENV, FO4_DEFAULT)
}

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn fallout4_hudmenu_lifecycle_and_calls_are_pinned() {
    let dir = data_dir();
    let archive =
        Arc::new(Ba2Archive::open(dir.join("Fallout4 - Interface.ba2")).expect("open Interface BA2"));

    let mut player = SwfPlayer::from_resource_provider(
        archive,
        "interface\\hudmenu.swf",
        1280,
        720,
        Some(ScaleformProfile::Fallout4Avm2),
    )
    .expect("load FO4 hudmenu and its archive imports");

    assert_eq!(
        player.host_object_state(),
        ScaleformHostObjectState::AdapterInjected,
        "vanilla FO4 hudmenu declares the full BGSCodeObj contract"
    );

    // Tick the movie to life: the adapter registers its lifecycle hooks
    // and answers readiness; everything hudmenu calls stays cataloged.
    let lifecycle = player.host_bridge();
    for _ in 0..5 {
        player.tick(1.0 / 60.0);
    }
    assert!(
        lifecycle.has_callback("__byroBGSAdapterLoaded"),
        "adapter load hook registered"
    );
    assert!(
        lifecycle.has_callback("__byroBGSCodeObjDestroy"),
        "hudmenu declares the destroy hook, so the destroy callback is registered"
    );
    assert_eq!(
        player
            .invoke_callback("__byroBGSCodeObjReady", std::iter::empty())
            .and_then(|v| match v {
                ScaleformValue::Bool(b) => Some(b),
                _ => None,
            }),
        Some(true),
        "the injected adapter reports the BGSCodeObj ready"
    );
    assert!(
        lifecycle.unknown_methods().is_empty(),
        "every host call hudmenu made at boot is cataloged, got {:?}",
        lifecycle.unknown_methods()
    );

    // Dropping the player acknowledges destruction exactly once (the
    // hook exists; `AdapterInjectedWithoutDestroyHook` menus would stay
    // at 0 by design).
    let player = player;
    drop(player);
    for _ in 0..200 {
        if lifecycle.code_object_destruction_count() == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        lifecycle.code_object_destruction_count(),
        1,
        "destruction acknowledged through the movie's own hook"
    );
}
