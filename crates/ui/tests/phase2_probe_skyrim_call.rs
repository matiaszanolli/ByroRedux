//! TEMPORARY M48 Phase 2 probe (#6661) — does vanilla hudmenu's own `call` callback dispatch
//! `SetHealthMeterPercent` to the meter? Removed once answered.
use byroredux_bsa::BsaArchive;
use byroredux_ui::{ScaleformProfile, ScaleformValue, SwfPlayer};
use std::sync::Arc;

#[test]
#[ignore = "probe"]
fn probe_skyrim_call_dispatch() {
    use byroredux_plugin::esm::test_paths::{data_dir, SKYRIM_SE_DEFAULT, SKYRIM_SE_ENV};
    let dir = data_dir(SKYRIM_SE_ENV, SKYRIM_SE_DEFAULT);
    let archive = Arc::new(BsaArchive::open(dir.join("Skyrim - Interface.bsa")).unwrap());
    let mut player = SwfPlayer::from_resource_provider(archive, "interface\\hudmenu.swf", 1280, 720, Some(ScaleformProfile::SkyrimAvm1)).unwrap();
    player.set_stage_transparent();
    for _ in 0..30 { player.tick(1.0 / 30.0); }
    let bridge = player.host_bridge();
    eprintln!("callbacks: {:?}", bridge.available_callbacks());
    let frame = |player: &mut SwfPlayer| { player.tick(1.0 / 30.0); player.render().map(<[u8]>::to_vec) };
    let base = frame(&mut player).unwrap_or_default();
    let send = |player: &mut SwfPlayer, method: &str, args: Vec<ScaleformValue>| {
        let r = player.path_op(&byroredux_ui::PathOp::Call(format!("_root.HUDMovieBaseInstance.{method}"), args));
        eprintln!("path call {method} -> {r:?}");
    };
    send(&mut player, "RegisterComponents", vec![]);
    send(&mut player, "ShowElements", vec![ScaleformValue::String("All".into()), ScaleformValue::Bool(true)]);
    for _ in 0..10 { frame(&mut player); }
    for pct in [100.0, 35.0, 80.0] {
        send(&mut player, "SetHealthMeterPercent", vec![ScaleformValue::Number(pct), ScaleformValue::Bool(false)]);
        send(&mut player, "SetMagickaMeterPercent", vec![ScaleformValue::Number(pct), ScaleformValue::Bool(false)]);
        send(&mut player, "SetStaminaMeterPercent", vec![ScaleformValue::Number(pct), ScaleformValue::Bool(false)]);
        send(&mut player, "SetCompassAngle", vec![ScaleformValue::Number(90.0), ScaleformValue::Number(90.0), ScaleformValue::Bool(true)]);
        let mut last = None;
        for _ in 0..40 { if let Some(f) = frame(&mut player) { last = Some(f); } }
        let f = last.unwrap_or_else(|| base.clone());
        let changed = |y0: usize, y1: usize, x0: usize, x1: usize| (y0..y1).flat_map(|y| (x0..x1).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i..i + 4] != base[i..i + 4]).count();
        let ink = |y0: usize, y1: usize| (y0..y1).flat_map(|y| (0..1280).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i + 3] > 0).count();
        let rows: Vec<usize> = (0..72).map(|band| changed(band * 10, band * 10 + 10, 0, 1280)).collect();
        eprintln!("pct {pct}: changed per 10-row band {rows:?}");
        eprintln!("pct {pct}: changed bottom band {} top band {} | ink bottom {} top {}", changed(620, 720, 0, 1280), changed(0, 100, 0, 1280), ink(620, 720), ink(0, 100));
    }
    use byroredux_ui::PathOp;
    for path in ["_root.HUDMovieBaseInstance", "_root.HUDMovieBaseInstance.Health", "_root.HUDMovieBaseInstance.Health._visible", "_root.HUDMovieBaseInstance.Health._alpha", "_root.HUDMovieBaseInstance.Health._currentframe", "_root.HUDMovieBaseInstance.Health._totalframes", "_root.HUDMovieBaseInstance.Health._x", "_root.HUDMovieBaseInstance.Health._y", "_root.HUDMovieBaseInstance.HealthMeterLeft.CurrentPercent", "_root.HUDMovieBaseInstance.HealthMeterLeft.TargetPercent", "_root.HUDMovieBaseInstance.lastHealthMeterPercent", "_root.HUDMovieBaseInstance._visible", "_root.HUDMovieBaseInstance.Health.HealthMeter_mc._visible", "_root.HUDMovieBaseInstance.Health.All", "_root.HUDMovieBaseInstance.Magicka._currentframe", "_root.HUDMovieBaseInstance.lastMagickaMeterPercent", "_root._width", "_root._height", "_root.HUDMovieBaseInstance.Health.PlayForward"] {
        eprintln!("get {path}: {:?}", player.path_op(&PathOp::Get(path.into())));
    }
    eprintln!("unknown {:?} unanswered {:?}", bridge.unknown_methods(), bridge.unanswered_methods());
}

#[test]
#[ignore = "probe"]
fn probe_fo4_meter_paths() {
    use byroredux_bsa::Ba2Archive;
    use byroredux_plugin::esm::test_paths::{data_dir, FO4_DEFAULT, FO4_ENV};
    use byroredux_ui::PathOp;
    let dir = data_dir(FO4_ENV, FO4_DEFAULT);
    let archive = Arc::new(Ba2Archive::open(dir.join("Fallout4 - Interface.ba2")).unwrap());
    let mut player = SwfPlayer::from_resource_provider(archive, "interface\\hudmenu.swf", 1280, 720, Some(ScaleformProfile::Fallout4Avm2)).unwrap();
    player.set_stage_transparent();
    for _ in 0..30 { player.tick(1.0 / 30.0); }
    eprintln!("callbacks {:?}", player.host_bridge().available_callbacks());
    let frame = |player: &mut SwfPlayer| { player.tick(1.0 / 30.0); player.render().map(<[u8]>::to_vec) };
    let mut base = Vec::new();
    for _ in 0..10 { if let Some(f) = frame(&mut player) { base = f; } }
    for path in ["LeftMeters_mc", "LeftMeters_mc.HPMeter_mc", "LeftMeters_mc.HPMeter_mc.MeterBar_mc", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent", "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.Percent", "LeftMeters_mc.visible", "LeftMeters_mc.HPMeter_mc.visible", "LeftMeters_mc.HPMeter_mc.alpha", "LeftMeters_mc.alpha", "RightMeters_mc.visible"] {
        eprintln!("get {path}: {:?}", player.path_op(&PathOp::Get(path.into())));
    }
    for path in ["LeftMeters_mc.HPMeter_mc.Optional_mc.Percent", "LeftMeters_mc.HPMeter_mc.RadsBar_mc.Percent"] {
        eprintln!("before {path}: {:?}", player.path_op(&PathOp::Get(path.into())));
        eprintln!("zero {path}: {:?}", player.path_op(&PathOp::Set(path.into(), ScaleformValue::Number(0.0))));
    }
    for _ in 0..10 { if let Some(f) = frame(&mut player) { base = f; } }
    for pct in [1.0, 0.35] {
        for path in ["LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent", "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.Percent"] {
            eprintln!("set {path} {pct}: {:?}", player.path_op(&PathOp::Set(path.into(), ScaleformValue::Number(pct))));
        }
        let mut last = None;
        for _ in 0..30 { if let Some(f) = frame(&mut player) { last = Some(f); } }
        let f = last.unwrap_or_else(|| base.clone());
        let rows: Vec<usize> = (0..72).map(|band| (band * 10..band * 10 + 10).flat_map(|y| (0..1280).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i..i + 4] != base[i..i + 4]).count()).collect();
        eprintln!("pct {pct}: changed per 10-row band {rows:?}");
        let half = |x0: usize, x1: usize| (600..720).flat_map(|y| (x0..x1).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i..i + 4] != base[i..i + 4]).count();
        let xs: Vec<usize> = (0..16).map(|c| (600..720).flat_map(|y| (c * 80..c * 80 + 80).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i..i + 4] != base[i..i + 4]).count()).collect();
        let ink = |x0: usize, x1: usize| (600..720).flat_map(|y| (x0..x1).map(move |x| (y * 1280 + x) * 4)).filter(|&i| f[i + 3] > 8).count();
        let colour = |x0: usize, x1: usize| { let mut c = [0u64; 4]; let mut n = 0u64; for y in 600..720 { for x in x0..x1 { let i = (y * 1280 + x) * 4; if f[i + 3] > 8 { n += 1; for k in 0..4 { c[k] += f[i + k] as u64; } } } } c.map(|v| if n > 0 { v / n } else { 0 }) };
        eprintln!("pct {pct}: ink left {} right {} | mean rgba left {:?} right {:?}", ink(0, 640), ink(640, 1280), colour(0, 640), colour(640, 1280));
        eprintln!("pct {pct}: left {} right {} per-80px-column {xs:?}", half(0, 640), half(640, 1280));
        base = f;
    }
    for path in ["LeftMeters_mc.x", "LeftMeters_mc.y", "LeftMeters_mc.HPMeter_mc.x", "LeftMeters_mc.HPMeter_mc.y", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.scaleX", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.visible", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.alpha", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.width", "RightMeters_mc.x", "RightMeters_mc.y", "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.scaleX", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent", "LeftMeters_mc.HPMeter_mc.currentFrame", "LeftMeters_mc.HPMeter_mc.totalFrames", "LeftMeters_mc.currentFrame", "stage.stageWidth", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.BarAlpha", "LeftMeters_mc.numChildren", "LeftMeters_mc.HPMeter_mc.numChildren", "LeftMeters_mc.HPMeter_mc.transform.colorTransform.alphaMultiplier", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.transform.colorTransform.alphaMultiplier", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.height", "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.width", "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.height", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.numChildren", "LeftMeters_mc.HPMeter_mc.MeterBar_mc.mask", "LeftMeters_mc.HPMeter_mc.mask", "LeftMeters_mc.mask"] {
        eprintln!("get {path}: {:?}", player.path_op(&PathOp::Get(path.into())));
    }
}
