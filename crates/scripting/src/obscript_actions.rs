//! Reference-bound ObScript action commands (M47.3 phase 2).
//!
//! `ref.Enable`, `ref.Disable`, `ref.GetDisabled`, `ref.MoveTo <target> [x y z]`
//! and `ref.EVP` act on the *calling reference*, so they take the explicit
//! caller the VM resolved (`SEHaskillRef.MoveTo player`) and lower onto the sink
//! the Papyrus effects already use — never a second copy of the behaviour:
//!
//! | id | command | sink shared with Papyrus |
//! |----|---------|--------------------------|
//! | `0x1021` / `0x1022` | Enable / Disable | [`ReferenceEnableState::set_enabled`] (`Effect::Enable` / `Disable`) |
//! | `0x1023` | GetDisabled | [`reference_enabled`] (scripted state, else the authored flag) |
//! | `0x109e` | MoveTo | [`apply_move_to_effect`] (`Effect::MoveTo`); the player goes through [`PlayerMoveRequest`] (`Effect::MovePlayerTo`) |
//! | `0x105e` | EvaluatePackage / EVP | [`request_package_evaluation`] (`Effect::EvaluatePackage`) |
//!
//! The ids are `names`-verified (`examples/obscript_tools`) to name the same
//! command in Oblivion, Fallout 3 and New Vegas. The semantics are the GECK /
//! CS wiki pages':
//!
//! - *Enable* / *Disable*: the optional fade argument is cosmetic and ignored. The
//!   documented "no effect on a reference with an Enable Parent" rule is not
//!   applied here — the Papyrus path does not apply it either, and the runtime
//!   has no enable-parent ledger to consult.
//! - *MoveTo*: "Moves the object to the specified reference's location with
//!   optional axis offset parameters"; neither wiki says the rotation follows,
//!   so the rotation is left alone. Moving the player "will queue up a movement
//!   request" (Fallout), but on Oblivion it "also act[s] as a Return function",
//!   which this host cannot do, so the player's `MoveTo` is declined there. The
//!   player's request carries no offset, so an offset on it is declined too
//!   rather than dropped.
//!
//! Every action whose reference or target cannot be resolved (a cell that is not
//! loaded, no `ReferenceEnableState`) is [`Action::Declined`] — counted by the
//! host, never faked as done.

use byroredux_core::ecs::World;

use crate::actor_value_names::ScriptGame;
use crate::condition::resolve_entity_by_global_form_id;
use crate::fragment::{apply_move_to_effect, reference_enabled, PlayerMoveRequest, ReferenceEnableState};
use crate::obscript_vm::ObScriptValue;
use crate::package::request_package_evaluation;
use crate::papyrus_demo::PapyrusPlayerEntity;

const CMD_ENABLE: u16 = 0x1021;
const CMD_DISABLE: u16 = 0x1022;
const CMD_GET_DISABLED: u16 = 0x1023;
const CMD_EVALUATE_PACKAGE: u16 = 0x105e;
const CMD_MOVE_TO: u16 = 0x109e;

/// What the action layer made of a command.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Not an action command: the host keeps looking.
    Unhandled,
    /// Ran; the value it answers with (getters) and a line for the effect log.
    Done { value: Option<ObScriptValue>, effect: String },
    /// An action command whose target could not be acted on.
    Declined,
}

/// Run `cmd` if it is a reference-bound action. `caller` is the explicit
/// calling reference's FormID; every action here needs one.
pub(crate) fn run(world: &World, cmd: u16, args: &[ObScriptValue], caller: Option<u32>) -> Action {
    if !matches!(cmd, CMD_ENABLE | CMD_DISABLE | CMD_GET_DISABLED | CMD_EVALUATE_PACKAGE | CMD_MOVE_TO) {
        return Action::Unhandled;
    }
    let Some(caller) = caller.filter(|&form| form != 0) else {
        return Action::Declined;
    };
    let done = |value: Option<ObScriptValue>, effect: String| Action::Done { value, effect };
    match cmd {
        CMD_ENABLE | CMD_DISABLE => {
            let enabled = cmd == CMD_ENABLE;
            let Some(mut state) = world.try_resource_mut::<ReferenceEnableState>() else {
                return Action::Declined;
            };
            state.set_enabled(caller, enabled);
            done(None, format!("{} {caller:08X}", if enabled { "Enable" } else { "Disable" }))
        }
        CMD_GET_DISABLED => {
            let disabled = !reference_enabled(world, caller);
            done(Some(ObScriptValue::Num(disabled as i32 as f32)), String::new())
        }
        CMD_EVALUATE_PACKAGE => {
            match resolve_entity_by_global_form_id(world, caller) {
                Some(actor) if request_package_evaluation(world, actor) => {
                    done(None, format!("EvaluatePackage {caller:08X}"))
                }
                _ => Action::Declined,
            }
        }
        _ => move_to(world, args, caller),
    }
}

/// `caller.MoveTo <target> [x y z]`.
fn move_to(world: &World, args: &[ObScriptValue], caller: u32) -> Action {
    // A null target (`Ref(0)`, the unresolved-slot or `Z` null tag) names no place.
    let Some(ObScriptValue::Ref(target)) = args.first().copied().filter(|v| *v != ObScriptValue::Ref(0)) else {
        return Action::Declined;
    };
    let offset = [1, 2, 3].map(|i| args.get(i).map_or(0.0, |v| v.as_num()));
    let Some(moved) = resolve_entity_by_global_form_id(world, caller) else {
        return Action::Declined;
    };
    let effect = format!("MoveTo {caller:08X} -> {target:08X}");
    if PapyrusPlayerEntity::get(world) == Some(moved) {
        // Fallout queues the player's move; Oblivion's also ends the script.
        if !ScriptGame::is_fallout3_nv(world) || offset != [0.0; 3] {
            return Action::Declined;
        }
        return match world.try_resource_mut::<PlayerMoveRequest>() {
            Some(mut request) => {
                request.0 = Some(target);
                Action::Done { value: None, effect }
            }
            None => Action::Declined,
        };
    }
    let Some(destination) = resolve_entity_by_global_form_id(world, target) else {
        return Action::Declined;
    };
    if apply_move_to_effect(world, moved, destination, offset, false, caller) {
        Action::Done { value: None, effect }
    } else {
        Action::Declined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::ecs::storage::EntityId;
    use byroredux_core::ecs::{GlobalTransform, Transform};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_plugin::esm::reader::GameKind;
    use crate::EvaluatePackageRequest;

    const MOVED: u32 = 0x0001_0001;
    const MARKER: u32 = 0x0001_0002;
    const PLAYER_REF: u32 = 0x14;

    fn done(effect: &str) -> Action {
        Action::Done { value: None, effect: effect.to_owned() }
    }

    /// A world whose pool knows `MOVED` and `MARKER`, both placed with a
    /// transform (marker at (10, 0, 0) turned about Y), plus the ledgers.
    fn world(game: GameKind) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let mut place = |local: u32, at: Vec3, rotation: Quat| {
            let id = pool.intern(FormIdPair {
                plugin: PluginId::from_filename("FalloutNV.esm"),
                local: LocalFormId(local),
            });
            (id, at, rotation)
        };
        let moved = place(MOVED, Vec3::ZERO, Quat::IDENTITY);
        let marker = place(MARKER, Vec3::new(10.0, 0.0, 0.0), Quat::from_rotation_y(1.0));
        world.insert_resource(pool);
        world.insert_resource(ReferenceEnableState::default());
        world.insert_resource(PlayerMoveRequest::default());
        world.insert_resource(ScriptGame(game));
        world.register::<EvaluatePackageRequest>();
        let mut spawn = |(id, at, rotation): (_, Vec3, Quat)| {
            let entity = world.spawn();
            world.insert(entity, FormIdComponent(id));
            world.insert(entity, Transform::new(at, rotation, 1.0));
            world.insert(entity, GlobalTransform::new(at, rotation, 1.0));
            entity
        };
        let moved = spawn(moved);
        let marker = spawn(marker);
        (world, moved, marker)
    }

    fn enabled(world: &World, form: u32) -> bool {
        reference_enabled(world, form)
    }

    /// `Enable` / `Disable` write the ledger the Papyrus effects write, and
    /// `GetDisabled` reads it back (scripted state wins).
    #[test]
    fn enable_disable_and_get_disabled_share_the_papyrus_ledger() {
        let (world, _, _) = world(GameKind::Fallout3NV);
        let get = |world: &World| run(world, CMD_GET_DISABLED, &[], Some(MOVED));
        assert_eq!(get(&world), Action::Done { value: Some(ObScriptValue::Num(0.0)), effect: String::new() });
        assert_eq!(run(&world, CMD_DISABLE, &[], Some(MOVED)), done("Disable 00010001"));
        assert!(!enabled(&world, MOVED));
        assert_eq!(get(&world), Action::Done { value: Some(ObScriptValue::Num(1.0)), effect: String::new() });
        // The optional fade argument is cosmetic.
        assert_eq!(run(&world, CMD_ENABLE, &[ObScriptValue::Num(0.0)], Some(MOVED)), done("Enable 00010001"));
        assert!(enabled(&world, MOVED));
    }

    /// An authored "Initially Disabled" reference reads disabled until a script
    /// enables it.
    #[test]
    fn get_disabled_honours_the_authored_flag() {
        let (mut world, _, _) = world(GameKind::Fallout3NV);
        world.insert_resource(crate::AuthoredDisabledReferences([MOVED].into()));
        assert_eq!(
            run(&world, CMD_GET_DISABLED, &[], Some(MOVED)),
            Action::Done { value: Some(ObScriptValue::Num(1.0)), effect: String::new() }
        );
        run(&world, CMD_ENABLE, &[], Some(MOVED));
        assert!(enabled(&world, MOVED));
    }

    /// Every action needs a calling reference; none acts on form 0 or on "no one".
    #[test]
    fn an_action_without_a_calling_reference_is_declined() {
        let (world, _, _) = world(GameKind::Fallout3NV);
        for cmd in [CMD_ENABLE, CMD_DISABLE, CMD_GET_DISABLED, CMD_EVALUATE_PACKAGE, CMD_MOVE_TO] {
            let args = [ObScriptValue::Ref(MARKER)];
            assert_eq!(run(&world, cmd, &args, None), Action::Declined, "{cmd:#06x}");
            assert_eq!(run(&world, cmd, &args, Some(0)), Action::Declined, "{cmd:#06x}");
        }
        assert!(enabled(&world, 0), "form 0 was not written");
        assert_eq!(run(&world, 0x1039, &[], Some(MOVED)), Action::Unhandled);
    }

    /// Without the enable ledger there is nowhere to record the change: declined.
    #[test]
    fn enable_without_the_ledger_is_declined() {
        let (mut world, _, _) = world(GameKind::Fallout3NV);
        world.remove_resource::<ReferenceEnableState>();
        assert_eq!(run(&world, CMD_DISABLE, &[], Some(MOVED)), Action::Declined);
    }

    /// `MoveTo` puts the caller at the target plus the offset, in Gamebryo's
    /// Z-up axes, and leaves the rotation alone.
    #[test]
    fn move_to_moves_the_caller_to_the_target_plus_the_offset() {
        let (world, moved, _) = world(GameKind::Fallout3NV);
        let args = [ObScriptValue::Ref(MARKER), ObScriptValue::Num(1.0), ObScriptValue::Num(2.0), ObScriptValue::Num(3.0)];
        assert_eq!(run(&world, CMD_MOVE_TO, &args, Some(MOVED)), done("MoveTo 00010001 -> 00010002"));
        let transform = *world.get::<Transform>(moved).unwrap();
        // Z-up (x, y, z) offset (1, 2, 3) is engine (1, 3, -2).
        assert_eq!(transform.translation, Vec3::new(11.0, 3.0, -2.0));
        assert_eq!(transform.rotation, Quat::IDENTITY, "MoveTo documents a position, not a facing");
    }

    #[test]
    fn move_to_with_no_offset_lands_on_the_target() {
        let (world, moved, _) = world(GameKind::Fallout3NV);
        run(&world, CMD_MOVE_TO, &[ObScriptValue::Ref(MARKER)], Some(MOVED));
        assert_eq!(world.get::<Transform>(moved).unwrap().translation, Vec3::new(10.0, 0.0, 0.0));
    }

    /// A target that is null or not loaded is a decline; the caller stays put.
    #[test]
    fn move_to_an_unresolved_target_is_declined_and_nothing_moves() {
        let (world, moved, _) = world(GameKind::Fallout3NV);
        for args in [vec![], vec![ObScriptValue::Ref(0)], vec![ObScriptValue::Ref(0x00FF_FFFF)]] {
            assert_eq!(run(&world, CMD_MOVE_TO, &args, Some(MOVED)), Action::Declined);
        }
        assert_eq!(run(&world, CMD_MOVE_TO, &[ObScriptValue::Ref(MARKER)], Some(0x00FF_FFFF)), Action::Declined);
        assert_eq!(world.get::<Transform>(moved).unwrap().translation, Vec3::ZERO);
    }

    fn with_player(mut world: World, moved: EntityId) -> World {
        world.insert_resource(PapyrusPlayerEntity(moved));
        world
    }

    /// Fallout queues the player's move for the engine; the request carries the
    /// target only, so an offset is declined instead of dropped.
    #[test]
    fn the_player_move_is_queued_on_fallout_and_never_loses_an_offset() {
        let (world, moved, _) = world(GameKind::Fallout3NV);
        let world = with_player(world, moved);
        let offset = [ObScriptValue::Ref(MARKER), ObScriptValue::Num(512.0)];
        assert_eq!(run(&world, CMD_MOVE_TO, &offset, Some(PLAYER_REF)), Action::Declined);
        assert_eq!(world.resource::<PlayerMoveRequest>().0, None);
        assert_eq!(run(&world, CMD_MOVE_TO, &[ObScriptValue::Ref(MARKER)], Some(PLAYER_REF)), done("MoveTo 00000014 -> 00010002"));
        assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(MARKER));
        assert_eq!(world.get::<Transform>(moved).unwrap().translation, Vec3::ZERO, "the engine moves the player, not the script");
    }

    /// Oblivion's player `MoveTo` also ends the script, which this host cannot
    /// do, so it is declined rather than half-run.
    #[test]
    fn the_players_move_is_declined_on_oblivion() {
        let (world, moved, _) = world(GameKind::Oblivion);
        let world = with_player(world, moved);
        assert_eq!(run(&world, CMD_MOVE_TO, &[ObScriptValue::Ref(MARKER)], Some(PLAYER_REF)), Action::Declined);
        assert_eq!(world.resource::<PlayerMoveRequest>().0, None);
    }

    /// `EVP` queues the Papyrus `EvaluatePackage` request for the actor.
    #[test]
    fn evaluate_package_queues_the_request_for_the_caller() {
        let (world, moved, marker) = world(GameKind::Fallout3NV);
        assert_eq!(run(&world, CMD_EVALUATE_PACKAGE, &[], Some(MOVED)), done("EvaluatePackage 00010001"));
        assert!(world.has::<EvaluatePackageRequest>(moved));
        assert!(!world.has::<EvaluatePackageRequest>(marker));
        assert_eq!(run(&world, CMD_EVALUATE_PACKAGE, &[], Some(0x00FF_FFFF)), Action::Declined);
    }
}
