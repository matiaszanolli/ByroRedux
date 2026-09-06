//! `events` WIT host interface.

use crate::runtime::*;

impl events::Host for HostState {
    fn publish(&mut self, event: String, payload: Vec<u8>) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("custom events are only accepted during an event callback");
        }
        if !self.grants.contains(EVENTS_PUBLISH_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {EVENTS_PUBLISH_CAPABILITY}",
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
        let event = EventId::new(event)
            .map_err(|error| wasmtime::Error::msg(format!("invalid custom event id: {error}")))?;
        if !custom_event_publishable_by(&event, self.principal.id()) {
            wasmtime::bail!(
                "principal {} may not publish custom event {}",
                self.principal.id(),
                event
            );
        }
        let command = PublishEventCommand::new(event, payload).ok_or_else(|| {
            wasmtime::Error::msg("custom event id or payload exceeds the SDK contract")
        })?;
        self.pending_commands
            .push(HostCommand::PublishEvent(command));
        Ok(())
    }

    fn legacy_builder_create(&mut self, event_name: String) -> wasmtime::Result<u32> {
        self.require_legacy_builder_access()?;
        Ok(self.legacy_mod_event_builders.create(&event_name))
    }

    fn legacy_builder_push_bool(&mut self, handle: u32, value: bool) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders
            .push(handle, LegacySkseModEventValue::Bool(value));
        Ok(())
    }

    fn legacy_builder_push_int(&mut self, handle: u32, value: i32) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders
            .push(handle, LegacySkseModEventValue::Int(value));
        Ok(())
    }

    fn legacy_builder_push_float(&mut self, handle: u32, value: f32) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders
            .push(handle, LegacySkseModEventValue::float(value));
        Ok(())
    }

    fn legacy_builder_push_string(&mut self, handle: u32, value: String) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders
            .push(handle, LegacySkseModEventValue::String(value));
        Ok(())
    }

    fn legacy_builder_push_form(
        &mut self,
        handle: u32,
        value: Option<state::FormRef>,
    ) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders.push(
            handle,
            LegacySkseModEventValue::Form(value.map(sdk_form_ref)),
        );
        Ok(())
    }

    fn legacy_builder_send(&mut self, handle: u32) -> wasmtime::Result<bool> {
        self.require_legacy_builder_access()?;
        if !self.legacy_mod_event_builders.contains(handle) {
            return Ok(false);
        }
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        let command = self
            .legacy_mod_event_builders
            .send(handle)
            .expect("validated legacy builder must remain encodable");
        self.pending_commands
            .push(HostCommand::PublishEvent(command));
        Ok(true)
    }

    fn legacy_builder_release(&mut self, handle: u32) -> wasmtime::Result<()> {
        self.require_legacy_builder_access()?;
        self.legacy_mod_event_builders.release(handle);
        Ok(())
    }

    fn queue_legacy_subscribe(
        &mut self,
        event_name: String,
        callback: String,
    ) -> wasmtime::Result<()> {
        self.require_legacy_subscription_command()?;
        let command = LegacyModEventSubscriptionCommand::subscribe(&event_name, callback)
            .ok_or_else(|| wasmtime::Error::msg("invalid legacy mod-event name or callback"))?;
        self.pending_commands
            .push(HostCommand::LegacyModEventSubscription(command));
        Ok(())
    }

    fn queue_legacy_unsubscribe(&mut self, event_name: String) -> wasmtime::Result<()> {
        self.require_legacy_subscription_command()?;
        let command = LegacyModEventSubscriptionCommand::unsubscribe(&event_name)
            .ok_or_else(|| wasmtime::Error::msg("invalid legacy mod-event name"))?;
        self.pending_commands
            .push(HostCommand::LegacyModEventSubscription(command));
        Ok(())
    }

    fn queue_legacy_unsubscribe_all(&mut self) -> wasmtime::Result<()> {
        self.require_legacy_subscription_command()?;
        self.pending_commands
            .push(HostCommand::LegacyModEventSubscription(
                LegacyModEventSubscriptionCommand::UnsubscribeAll,
            ));
        Ok(())
    }

    fn current_legacy_callback(&mut self) -> wasmtime::Result<Option<String>> {
        if self.current_custom_event.is_none() {
            wasmtime::bail!("legacy callback is only visible during on-custom-event");
        }
        Ok(self.current_legacy_callback.clone())
    }

    fn current_payload_len(&mut self) -> wasmtime::Result<u32> {
        let event = self.current_custom_event.as_ref().ok_or_else(|| {
            wasmtime::Error::msg("custom event payload is only visible during on-custom-event")
        })?;
        Ok(u32::try_from(event.payload.len())
            .expect("custom event payload is bounded below u32::MAX"))
    }

    fn current_payload_byte(&mut self, index: u32) -> wasmtime::Result<Option<u8>> {
        let event = self.current_custom_event.as_ref().ok_or_else(|| {
            wasmtime::Error::msg("custom event payload is only visible during on-custom-event")
        })?;
        Ok(event.payload.get(index as usize).copied())
    }
}
