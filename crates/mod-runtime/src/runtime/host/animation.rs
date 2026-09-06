//! `animation` WIT host interface.

use crate::runtime::*;

impl animation::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<animation::AnimationSnapshot>> {
        self.require_animation_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::animation)
            .map(wit_animation_snapshot))
    }

    fn queue_play_idle(
        &mut self,
        entity: state::EntityRef,
        idle: state::FormRef,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("animation playback is only accepted during a callback");
        }
        if !self.grants.contains(ANIMATION_PLAY_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {ANIMATION_PLAY_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        let entity = sdk_entity_ref(entity)?;
        if !self.entity_projections.contains_key(&entity) {
            wasmtime::bail!("animation target is not visible to this callback");
        }
        let idle = sdk_form_ref(idle);
        if idle.local() == 0 {
            wasmtime::bail!("animation IDLE identity reserves local zero");
        }
        self.pending_commands
            .push(HostCommand::PlayIdle(PlayIdleCommand::new(entity, idle)));
        Ok(())
    }
}
