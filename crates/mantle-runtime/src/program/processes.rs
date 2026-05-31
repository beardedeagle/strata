use std::collections::BTreeSet;

use super::*;
use authority_usage::LoadedAuthorityUsage;

mod authority_usage;
mod supervision;

impl LoadedProcess {
    pub(in crate::program) fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        let transitions = load_transitions(process)?;
        #[cfg(test)]
        let transition_lookup = TransitionLookup::from_transitions(&transitions);

        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_type: process.state_type,
            state_values: process
                .state_values
                .iter()
                .map(LoadedStateValue::from_artifact)
                .collect::<Result<Vec<_>>>()?,
            message_type: process.message_type,
            message_variants: process
                .message_variants
                .iter()
                .map(LoadedMessageVariant::from_artifact)
                .collect(),
            authorities: process
                .authorities
                .iter()
                .map(LoadedAuthority::from_artifact)
                .collect(),
            spawn_sites: process
                .spawn_sites
                .iter()
                .map(LoadedSpawnSite::from_artifact)
                .collect(),
            supervisor_plans: process
                .supervisor_plans
                .iter()
                .map(LoadedSupervisorPlan::from_artifact)
                .collect(),
            process_refs: process
                .process_refs
                .iter()
                .map(LoadedProcessRef::from_artifact)
                .collect(),
            mailbox_bound: process.mailbox_bound,
            init_state: process.init_state,
            transitions,
            #[cfg(test)]
            transition_lookup,
        })
    }

    #[cfg(test)]
    pub(crate) fn transition_for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Result<&LoadedTransition> {
        let lookup_state = self
            .transition_lookup
            .is_state_specific_message(message)
            .then_some(current_state);
        let payload_specific = self
            .transition_lookup
            .is_payload_specific_base(message, lookup_state);
        let transition_index = self
            .transition_lookup
            .for_dispatch(message, current_state, payload, &self.transitions)
            .ok_or_else(|| {
                self.transition_lookup_error(message, lookup_state, payload_specific, payload)
            })?;
        self.transition_by_lookup_index(transition_index)
    }

    #[cfg(test)]
    fn transition_by_lookup_index(&self, index: usize) -> Result<&LoadedTransition> {
        self.transitions.get(index).ok_or_else(|| {
            Error::new(format!(
                "process {} transition index {} is not loaded",
                self.debug_name, index
            ))
        })
    }

    #[cfg(test)]
    fn transition_lookup_error(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
        payload_specific: bool,
        payload: Option<&RuntimePayload>,
    ) -> Error {
        let state = current_state
            .map(|state| format!(" current_state id {}", state.as_u32()))
            .unwrap_or_default();
        if payload_specific {
            return match payload {
                Some(payload) => Error::new(format!(
                    "process {} has no transition for message id {}{} payload {}",
                    self.debug_name,
                    message.as_u32(),
                    state,
                    payload.label()
                )),
                None => Error::new(format!(
                    "process {} has payload-specific transition(s) for message id {}{}, but the queued message has no payload",
                    self.debug_name,
                    message.as_u32(),
                    state
                )),
            };
        }
        Error::new(format!(
            "process {} has no transition for message id {}{}",
            self.debug_name,
            message.as_u32(),
            state
        ))
    }

    pub(in crate::program) fn validate_admission(
        &self,
        program: &LoadedProgram,
        process_id: ProcessId,
    ) -> Result<()> {
        self.validate_state_table(program)?;
        self.validate_message_table(program)?;
        self.validate_authorities(program, process_id)?;
        self.validate_process_refs(program, process_id)?;
        if self.mailbox_bound == 0 || self.mailbox_bound > MAX_MAILBOX_BOUND {
            return Err(Error::new(format!(
                "process {} loaded mailbox_bound must be between 1 and {MAX_MAILBOX_BOUND}",
                self.debug_name
            )));
        }
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a loaded state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.transitions.is_empty() || self.transitions.len() > MAX_TRANSITIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded transition_count must be between 1 and {MAX_TRANSITIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let action_count = self
            .transitions
            .iter()
            .try_fold(0usize, |count, transition| {
                count
                    .checked_add(actions::action_count(&transition.actions)?)
                    .ok_or_else(|| Error::new("loaded action_count overflowed"))
            })?;
        if action_count > MAX_ACTIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        validate_loaded_transition_coverage(self)?;
        let mut authority_usage = LoadedAuthorityUsage::new();
        for transition in &self.transitions {
            let message = transition.message;
            transition.validate_admission(program, self, process_id, message)?;
            self.collect_authority_usage(&transition.actions, &mut authority_usage)?;
            transition.effect_authority.validate_actions(
                &self.debug_name,
                message,
                &transition.actions,
            )?;
        }
        self.validate_authority_usage(&authority_usage)?;
        self.validate_message_type_shape(program)?;
        Ok(())
    }

    fn validate_authorities(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        if self.authorities.len() > MAX_AUTHORITIES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded authority_count must be no greater than {MAX_AUTHORITIES_PER_PROCESS}",
                self.debug_name
            )));
        }
        if self.spawn_sites.len() > MAX_SPAWN_SITES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded spawn_site_count must be no greater than {MAX_SPAWN_SITES_PER_PROCESS}",
                self.debug_name
            )));
        }
        if self.supervisor_plans.len() > MAX_SUPERVISORS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded supervisor_count must be no greater than {MAX_SUPERVISORS_PER_PROCESS}",
                self.debug_name
            )));
        }
        let mut names = BTreeSet::new();
        let mut descriptors = BTreeSet::new();
        for authority in &self.authorities {
            validate_loaded_ident_field("authority debug_name", &authority.debug_name)?;
            if !names.insert(authority.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "process {} duplicates loaded authority {}",
                    self.debug_name, authority.debug_name
                )));
            }
            if !descriptors.insert(authority.descriptor) {
                return Err(Error::new(format!(
                    "process {} duplicates loaded authority descriptor",
                    self.debug_name
                )));
            }
            match authority.descriptor {
                LoadedCapabilityDescriptor::Spawn { target } => {
                    program.process(target)?;
                    if target == program.entry_process {
                        return Err(Error::new(format!(
                            "process {} loaded authority {} targets entry process id {}",
                            self.debug_name,
                            authority.debug_name,
                            target.as_u32()
                        )));
                    }
                    if target == process_id {
                        return Err(Error::new(format!(
                            "process {} loaded authority {} targets itself, which is not supported",
                            self.debug_name, authority.debug_name
                        )));
                    }
                }
                LoadedCapabilityDescriptor::PortConnect { port } => {
                    program.ports.get(port.index()).ok_or_else(|| {
                        Error::new(format!(
                            "process {} loaded authority {} targets unloaded port id {}",
                            self.debug_name,
                            authority.debug_name,
                            port.as_u32()
                        ))
                    })?;
                }
                LoadedCapabilityDescriptor::ProtocolBoundary { .. }
                | LoadedCapabilityDescriptor::ComponentExport { .. } => {
                    return Err(Error::new(format!(
                        "process {} loaded authority {} uses a boundary-table-only capability",
                        self.debug_name, authority.debug_name
                    )));
                }
            }
        }
        for (index, spawn_site) in self.spawn_sites.iter().enumerate() {
            program.process(spawn_site.target)?;
            match spawn_site.kind {
                LoadedSpawnKind::DynamicLocal => {
                    if spawn_site.supervisor.is_some() || spawn_site.child.is_some() {
                        return Err(Error::new(format!(
                            "process {} dynamic loaded spawn site {index} carries supervisor ids",
                            self.debug_name
                        )));
                    }
                    let authority_id = spawn_site.authority.ok_or_else(|| {
                        Error::new(format!(
                            "process {} dynamic loaded spawn site {index} has no authority id",
                            self.debug_name
                        ))
                    })?;
                    let authority_index = authority_id.index();
                    let authority = self.authorities.get(authority_index).ok_or_else(|| {
                        Error::new(format!(
                            "process {} loaded spawn site {index} references undefined authority id {}",
                            self.debug_name,
                            authority_id.as_u32()
                        ))
                    })?;
                    match authority.descriptor {
                        LoadedCapabilityDescriptor::Spawn { target }
                            if target == spawn_site.target => {}
                        LoadedCapabilityDescriptor::Spawn { target } => {
                            return Err(Error::new(format!(
                                "process {} loaded spawn site {index} targets process id {}, but authority id {} targets {}",
                                self.debug_name,
                                spawn_site.target.as_u32(),
                                authority_id.as_u32(),
                                target.as_u32()
                            )));
                        }
                        _ => {
                            return Err(Error::new(format!(
                                "process {} loaded spawn site {index} authority id {} is not a spawn capability",
                                self.debug_name,
                                authority_id.as_u32()
                            )));
                        }
                    }
                }
                LoadedSpawnKind::LexicalSupervisorChild => {
                    if spawn_site.authority.is_some() {
                        return Err(Error::new(format!(
                            "process {} lexical supervisor child spawn site {index} carries dynamic authority",
                            self.debug_name
                        )));
                    }
                    if spawn_site.supervisor.is_none() || spawn_site.child.is_none() {
                        return Err(Error::new(format!(
                            "process {} lexical supervisor child spawn site {index} must carry supervisor and child ids",
                            self.debug_name
                        )));
                    }
                }
            }
        }
        self.validate_supervisors(program, process_id)?;
        Ok(())
    }

    fn validate_authority_usage(&self, usage: &LoadedAuthorityUsage) -> Result<()> {
        for (spawn_site_index, spawn_site) in self.spawn_sites.iter().enumerate() {
            match spawn_site.kind {
                LoadedSpawnKind::DynamicLocal if !usage.spawn_site_referenced(spawn_site_index) => {
                    return Err(Error::new(format!(
                        "process {} declares unused loaded dynamic spawn site {spawn_site_index}",
                        self.debug_name
                    )));
                }
                LoadedSpawnKind::LexicalSupervisorChild
                    if !self.supervisor_spawn_site_referenced(spawn_site_index) =>
                {
                    return Err(Error::new(format!(
                        "process {} declares unused loaded lexical supervisor child spawn site {spawn_site_index}",
                        self.debug_name
                    )));
                }
                _ => {}
            }
        }
        for (authority_index, authority) in self.authorities.iter().enumerate() {
            if !usage.authority_referenced(authority_index) {
                return Err(Error::new(format!(
                    "process {} declares unused loaded authority {}",
                    self.debug_name, authority.debug_name
                )));
            }
        }
        Ok(())
    }

    fn supervisor_spawn_site_referenced(&self, spawn_site_index: usize) -> bool {
        self.supervisor_plans.iter().any(|plan| {
            plan.children
                .iter()
                .any(|child| child.spawn_site.index() == spawn_site_index)
        })
    }

    fn collect_authority_usage(
        &self,
        actions: &[LoadedAction],
        usage: &mut LoadedAuthorityUsage,
    ) -> Result<()> {
        for action in actions {
            match action {
                LoadedAction::Spawn {
                    target, spawn_site, ..
                }
                | LoadedAction::SpawnOutcome {
                    target, spawn_site, ..
                } => {
                    self.record_spawn_site_usage(usage, *spawn_site, *target)?;
                }
                LoadedAction::Send {
                    port: Some(port), ..
                }
                | LoadedAction::SendOutcome {
                    port: Some(port), ..
                } => {
                    self.record_port_authority_usage(usage, *port)?;
                }
                LoadedAction::IfElse {
                    then_actions,
                    else_actions,
                    ..
                } => {
                    self.collect_authority_usage(then_actions, usage)?;
                    self.collect_authority_usage(else_actions, usage)?;
                }
                LoadedAction::ForEach { body, .. } => {
                    self.collect_authority_usage(body, usage)?;
                }
                LoadedAction::Emit { .. }
                | LoadedAction::Send { port: None, .. }
                | LoadedAction::SendOutcome { port: None, .. } => {}
            }
        }
        Ok(())
    }

    fn record_spawn_site_usage(
        &self,
        usage: &mut LoadedAuthorityUsage,
        spawn_site: SpawnSiteId,
        target: ProcessId,
    ) -> Result<()> {
        let site = self.validate_spawn_site(spawn_site, target)?;
        usage.mark_spawn_site(&self.debug_name, spawn_site.index())?;
        let authority = site.authority.ok_or_else(|| {
            Error::new(format!(
                "process {} dynamic spawn site id {} does not reference an authority",
                self.debug_name,
                spawn_site.as_u32()
            ))
        })?;
        usage.mark_authority(&self.debug_name, authority.index())
    }

    fn record_port_authority_usage(
        &self,
        usage: &mut LoadedAuthorityUsage,
        port: PortId,
    ) -> Result<()> {
        let authority_index = self
            .authorities
            .iter()
            .position(|authority| {
                matches!(
                    authority.descriptor,
                    LoadedCapabilityDescriptor::PortConnect { port: authority_port }
                        if authority_port == port
                )
            })
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} loaded send through port id {} requires authority port_connect for the same port id",
                    self.debug_name,
                    port.as_u32()
                ))
            })?;
        usage.mark_authority(&self.debug_name, authority_index)
    }

    fn validate_state_table(&self, program: &LoadedProgram) -> Result<()> {
        validate_loaded_ident_field("process debug_name", &self.debug_name)?;
        program.validate_value_type("state_type", self.state_type)?;
        if self.state_values.is_empty() || self.state_values.len() > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded state_value_count must be between 1 and {MAX_STATE_VALUES_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut states = BTreeSet::new();
        for state in &self.state_values {
            program
                .validate_value_type("state value type", state.ty)
                .map_err(|err| {
                    Error::new(format!(
                        "process {} state value type: {err}",
                        self.debug_name
                    ))
                })?;
            program
                .validate_value_matches_type("state value", state.ty, &state.value)
                .map_err(|err| {
                    Error::new(format!("process {} state value: {err}", self.debug_name))
                })?;
            validate_state_value_identity_label(&state.value, &state.label)
                .map_err(|err| Error::new(format!("process {} {err}", self.debug_name)))?;
            if state.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} loaded state value {} has type id {}, expected {}",
                    self.debug_name,
                    state.label,
                    state.ty.as_u32(),
                    self.state_type.as_u32()
                )));
            }
            if let Some(payload) = &state.payload {
                program
                    .validate_value_type("state value payload type", payload.ty)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload type: {err}",
                            self.debug_name
                        ))
                    })?;
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state.label
                    )));
                }
                program
                    .validate_value_matches_type("state value payload", payload.ty, &payload.value)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload: {err}",
                            self.debug_name
                        ))
                    })?;
            }
            if !states.insert((state.ty, state.value.clone())) {
                return Err(Error::new(format!(
                    "process {} loads duplicate state value {} with type id {}",
                    self.debug_name,
                    state.value.label(),
                    state.ty.as_u32()
                )));
            }
        }
        Ok(())
    }

    fn validate_message_table(&self, program: &LoadedProgram) -> Result<()> {
        program.validate_value_type("message_type", self.message_type)?;
        if self.message_variants.is_empty()
            || self.message_variants.len() > MAX_MESSAGE_VARIANTS_PER_PROCESS
        {
            return Err(Error::new(format!(
                "process {} loaded message_count must be between 1 and {MAX_MESSAGE_VARIANTS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut messages = BTreeSet::new();
        for message in &self.message_variants {
            validate_message_label(&message.label).map_err(|err| {
                Error::new(format!("process {} message label: {err}", self.debug_name))
            })?;
            if let Some(payload_type) = message.payload_type {
                program.type_entry(payload_type).map_err(|err| {
                    Error::new(format!(
                        "process {} message payload_type: {err}",
                        self.debug_name
                    ))
                })?;
            }
            if !messages.insert(message.label.as_str()) {
                return Err(Error::new(format!(
                    "process {} loads duplicate message label {}",
                    self.debug_name, message.label
                )));
            }
        }
        Ok(())
    }

    fn validate_message_type_shape(&self, program: &LoadedProgram) -> Result<()> {
        let message_type = program.type_entry(self.message_type)?;
        let ArtifactValueShape::Enum { variants } = message_type.value_shape()? else {
            return Err(Error::new(format!(
                "process {} loaded message_type id {} must be an enum aligned with message variants",
                self.debug_name,
                self.message_type.as_u32()
            )));
        };
        if variants.len() != self.message_variants.len() {
            return Err(Error::new(format!(
                "process {} loaded message_type id {} declares {} variant(s), expected {} message variant(s)",
                self.debug_name,
                self.message_type.as_u32(),
                variants.len(),
                self.message_variants.len()
            )));
        }
        for (index, (message, variant)) in self
            .message_variants
            .iter()
            .zip(variants.iter())
            .enumerate()
        {
            if variant.label != message.label {
                return Err(Error::new(format!(
                    "process {} loaded message_type id {} variant {index} label {} does not match message label {}",
                    self.debug_name,
                    self.message_type.as_u32(),
                    variant.label,
                    message.label
                )));
            }
            if variant.payload_type != message.payload_type {
                return Err(Error::new(format!(
                    "process {} loaded message_type id {} variant {index} payload type {:?}, expected {:?}",
                    self.debug_name,
                    self.message_type.as_u32(),
                    variant.payload_type.map(TypeId::as_u32),
                    message.payload_type.map(TypeId::as_u32)
                )));
            }
        }
        Ok(())
    }

    fn validate_process_refs(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        if self.process_refs.len() > MAX_PROCESS_REFS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded process_ref_count must be no greater than {MAX_PROCESS_REFS_PER_PROCESS}",
                self.debug_name
            )));
        }

        for (process_ref_index, process_ref) in self.process_refs.iter().enumerate() {
            program.process(process_ref.target)?;
            if process_ref.target == program.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets entry process id {}",
                    self.debug_name,
                    process_ref_index,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets itself",
                    self.debug_name, process_ref_index
                )));
            }
        }
        Ok(())
    }

    pub(in crate::program) fn process_ref_target(
        &self,
        process_ref: ProcessRefId,
    ) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references unloaded process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }

    pub(crate) fn validate_spawn_site(
        &self,
        spawn_site: SpawnSiteId,
        target: ProcessId,
    ) -> Result<&LoadedSpawnSite> {
        let site = self.spawn_sites.get(spawn_site.index()).ok_or_else(|| {
            Error::new(format!(
                "process {} references unloaded spawn site id {}",
                self.debug_name,
                spawn_site.as_u32()
            ))
        })?;
        if site.target != target {
            return Err(Error::new(format!(
                "process {} spawn site id {} targets process id {}, expected {}",
                self.debug_name,
                spawn_site.as_u32(),
                site.target.as_u32(),
                target.as_u32()
            )));
        }
        if site.kind != LoadedSpawnKind::DynamicLocal {
            return Err(Error::new(format!(
                "process {} spawn site id {} is not a dynamic local spawn site",
                self.debug_name,
                spawn_site.as_u32()
            )));
        }
        let authority_id = site.authority.ok_or_else(|| {
            Error::new(format!(
                "process {} dynamic spawn site id {} does not reference an authority",
                self.debug_name,
                spawn_site.as_u32()
            ))
        })?;
        let authority = self.authorities.get(authority_id.index()).ok_or_else(|| {
            Error::new(format!(
                "process {} spawn site id {} references unloaded authority id {}",
                self.debug_name,
                spawn_site.as_u32(),
                authority_id.as_u32()
            ))
        })?;
        match authority.descriptor {
            LoadedCapabilityDescriptor::Spawn {
                target: authority_target,
            } if authority_target == target => {}
            LoadedCapabilityDescriptor::Spawn {
                target: authority_target,
            } => {
                return Err(Error::new(format!(
                    "process {} spawn site id {} authority id {} targets process id {}, expected {}",
                    self.debug_name,
                    spawn_site.as_u32(),
                    authority_id.as_u32(),
                    authority_target.as_u32(),
                    target.as_u32()
                )));
            }
            _ => {
                return Err(Error::new(format!(
                    "process {} spawn site id {} authority id {} is not a spawn capability",
                    self.debug_name,
                    spawn_site.as_u32(),
                    authority_id.as_u32()
                )));
            }
        }
        Ok(site)
    }
}
