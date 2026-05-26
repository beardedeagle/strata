use super::templates::{
    validate_bool_condition_template, validate_for_each_collection_type,
    validate_template_loop_elements,
};
use super::*;

impl ArtifactProcess {
    pub(in crate::artifact) fn validate_action_reference(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        spawned_refs: &mut BTreeSet<ProcessRefId>,
        action: &ArtifactAction,
        scope: ActionReferenceScope<'_>,
    ) -> Result<()> {
        match action {
            ArtifactAction::Emit { output } => {
                if output.index() >= artifact.outputs.len() {
                    return Err(Error::new(format!(
                        "process {} emits undefined output id {}",
                        self.debug_name,
                        output.as_u32()
                    )));
                }
            }
            ArtifactAction::Spawn {
                target,
                process_ref,
            } => {
                if scope.is_inside_runtime_if_branch() {
                    return Err(Error::new(format!(
                        "process {} transition {} runtime if branch cannot bind process references in this artifact slice",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                if scope.inside_loop {
                    return Err(Error::new(format!(
                        "process {} transition {} for loop body cannot bind process references",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn process reference id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_refs.insert(*process_ref) {
                    return Err(Error::new(format!(
                        "process {} duplicates process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
            }
            ArtifactAction::Send {
                target,
                message,
                payload,
            } => {
                let target_process_id = self.validate_send_target_reference(
                    artifact,
                    target,
                    transition,
                    spawned_refs,
                )?;
                let target_process = artifact
                    .processes
                    .get(target_process_id.index())
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} sends to undefined process id {}",
                            self.debug_name,
                            target_process_id.as_u32()
                        ))
                    })?;
                if message.index() >= target_process.message_variants.len() {
                    return Err(Error::new(format!(
                        "process {} sends message id {} not accepted by process id {}",
                        self.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32()
                    )));
                }
                let target_message = &target_process.message_variants[message.index()];
                match (&target_message.payload_type, payload) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        return Err(Error::new(format!(
                            "process {} sends payload to process id {} message id {}, which does not accept one",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(_), None) => {
                        return Err(Error::new(format!(
                            "process {} sends process id {} message id {} without required payload",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(payload_type), Some(payload)) => {
                        self.validate_template_process_refs(artifact, payload, spawned_refs)?;
                        validate_template_loop_elements(
                            artifact,
                            payload,
                            scope.active_loop_elements,
                            &format!(
                                "process {} transition {} send payload",
                                self.debug_name,
                                transition.message.as_u32()
                            ),
                        )?;
                        let received_payload_type = self
                            .message_variants
                            .get(transition.message.index())
                            .and_then(|message| message.payload_type);
                        let current_state_payload =
                            self.transition_current_state_payload(transition)?;
                        payload.validate_for_received_payload(
                            artifact,
                            &format!(
                                "process {} transition {} send payload",
                                self.debug_name,
                                transition.message.as_u32()
                            ),
                            ValueTemplatePayloadValidation::new(
                                Some(*payload_type),
                                received_payload_type,
                                current_state_payload.map(|payload| payload.ty),
                                !scope.inside_loop,
                            ),
                            0,
                        )?;
                    }
                }
            }
            ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                scope.validate_runtime_if_allowed(self, transition)?;
                let received_payload_type = self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type);
                let current_state_payload = self.transition_current_state_payload(transition)?;
                validate_bool_condition_template(
                    artifact,
                    &format!(
                        "process {} transition {} if condition",
                        self.debug_name,
                        transition.message.as_u32()
                    ),
                    condition,
                    received_payload_type,
                    current_state_payload,
                )?;
                validate_template_loop_elements(
                    artifact,
                    condition,
                    scope.active_loop_elements,
                    &format!(
                        "process {} transition {} if condition",
                        self.debug_name,
                        transition.message.as_u32()
                    ),
                )?;
                if then_actions.is_empty() && else_actions.is_empty() {
                    return Err(Error::new(format!(
                        "process {} transition {} runtime if action branches cannot both be empty",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                let branch_scope = scope.if_branch();
                for action in then_actions {
                    self.validate_action_reference(
                        artifact,
                        transition,
                        spawned_refs,
                        action,
                        branch_scope,
                    )?;
                }
                for action in else_actions {
                    self.validate_action_reference(
                        artifact,
                        transition,
                        spawned_refs,
                        action,
                        branch_scope,
                    )?;
                }
            }
            ArtifactAction::ForEach {
                element,
                collection,
                max_items,
                body,
            } => {
                if scope.inside_loop {
                    return Err(Error::new(format!(
                        "process {} transition {} nested for loops are not supported in this artifact slice",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                artifact.validate_value_type("for loop element type", element.ty)?;
                validate_count(
                    "for loop element id",
                    element.id.index(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS - 1,
                )?;
                validate_count(
                    "for loop max_items",
                    *max_items,
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                validate_for_each_collection_type(
                    artifact,
                    &format!(
                        "process {} transition {} for collection",
                        self.debug_name,
                        transition.message.as_u32()
                    ),
                    collection,
                    element.ty,
                )?;
                validate_template_loop_elements(
                    artifact,
                    collection,
                    scope.active_loop_elements,
                    &format!(
                        "process {} transition {} for collection",
                        self.debug_name,
                        transition.message.as_u32()
                    ),
                )?;
                let received_payload_type = self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type);
                let current_state_payload = self.transition_current_state_payload(transition)?;
                collection.validate_for_received_payload(
                    artifact,
                    &format!(
                        "process {} transition {} for collection",
                        self.debug_name,
                        transition.message.as_u32()
                    ),
                    ValueTemplatePayloadValidation::new(
                        None,
                        received_payload_type,
                        current_state_payload.map(|payload| payload.ty),
                        false,
                    ),
                    0,
                )?;
                if !collection.depends_on_received_payload() {
                    let evaluated = artifact.evaluate_state_value_with_current_state(
                        collection,
                        None,
                        current_state_payload,
                    )?;
                    let ArtifactValue::List(items) = evaluated.value else {
                        return Err(Error::new(format!(
                            "process {} transition {} for collection must evaluate to a list value",
                            self.debug_name,
                            transition.message.as_u32()
                        )));
                    };
                    if items.len() > *max_items {
                        return Err(Error::new(format!(
                            "process {} transition {} for collection has {} item(s), max_items is {}",
                            self.debug_name,
                            transition.message.as_u32(),
                            items.len(),
                            max_items
                        )));
                    }
                    for (index, item) in items.iter().enumerate() {
                        artifact.validate_value_matches_type(
                            &format!(
                                "process {} transition {} for collection item {index}",
                                self.debug_name,
                                transition.message.as_u32()
                            ),
                            element.ty,
                            item,
                        )?;
                    }
                }
                let active = [ActiveArtifactLoopElement {
                    id: element.id,
                    ty: element.ty,
                }];
                for body_action in body {
                    self.validate_action_reference(
                        artifact,
                        transition,
                        spawned_refs,
                        body_action,
                        ActionReferenceScope::loop_body(&active),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn validate_send_target_reference(
        &self,
        artifact: &MantleArtifact,
        target: &ArtifactSendTarget,
        transition: &ArtifactTransition,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<ProcessId> {
        match target {
            ArtifactSendTarget::ProcessRef(process_ref) => {
                let target_process_id = self.process_ref_target(*process_ref)?;
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends through unbound process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
                Ok(target_process_id)
            }
            ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
                artifact.validate_process_ref_type_id_target(
                    "send target payload type",
                    *ty,
                    *target_process,
                )?;
                let received_payload_type = self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type)
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            self.debug_name,
                            transition.message.as_u32()
                        ))
                    })?;
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type id {}, expected {}",
                        self.debug_name,
                        transition.message.as_u32(),
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }

    fn validate_template_process_refs(
        &self,
        artifact: &MantleArtifact,
        template: &ArtifactValueTemplate,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<()> {
        match template {
            ArtifactValueTemplate::Literal { .. }
            | ArtifactValueTemplate::ReceivedPayload { .. }
            | ArtifactValueTemplate::CurrentStatePayload { .. } => Ok(()),
            ArtifactValueTemplate::EnumPayload { value, .. } => {
                self.validate_template_process_refs(artifact, value, spawned_refs)
            }
            ArtifactValueTemplate::RecordField { record, .. } => {
                self.validate_template_process_refs(artifact, record, spawned_refs)
            }
            ArtifactValueTemplate::ListElement { list, .. }
            | ArtifactValueTemplate::ListPrefixElement { list, .. }
            | ArtifactValueTemplate::ListRest { list, .. } => {
                self.validate_template_process_refs(artifact, list, spawned_refs)
            }
            ArtifactValueTemplate::MapValue { map, .. } => {
                self.validate_template_process_refs(artifact, map, spawned_refs)
            }
            ArtifactValueTemplate::MapRest { map, .. } => {
                self.validate_template_process_refs(artifact, map, spawned_refs)
            }
            ArtifactValueTemplate::IfElse {
                condition,
                then_value,
                else_value,
                ..
            } => {
                self.validate_template_process_refs(artifact, condition, spawned_refs)?;
                self.validate_template_process_refs(artifact, then_value, spawned_refs)?;
                self.validate_template_process_refs(artifact, else_value, spawned_refs)
            }
            ArtifactValueTemplate::Equality { left, right, .. }
            | ArtifactValueTemplate::ScalarArithmetic { left, right, .. }
            | ArtifactValueTemplate::ScalarOrdering { left, right, .. } => {
                self.validate_template_process_refs(artifact, left, spawned_refs)?;
                self.validate_template_process_refs(artifact, right, spawned_refs)
            }
            ArtifactValueTemplate::BooleanNot { operand, .. } => {
                self.validate_template_process_refs(artifact, operand, spawned_refs)
            }
            ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
                self.validate_template_process_refs(artifact, left, spawned_refs)?;
                self.validate_template_process_refs(artifact, right, spawned_refs)
            }
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => {
                artifact.validate_process_ref_type_id_target(
                    "process reference payload type",
                    *ty,
                    *target_process,
                )?;
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target_process {
                    return Err(Error::new(format!(
                        "process {} process reference payload id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        declared_target.as_u32(),
                        target_process.as_u32()
                    )));
                }
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends unbound process reference id {} as payload",
                        self.debug_name,
                        process_ref.as_u32()
                    )));
                }
                Ok(())
            }
            ArtifactValueTemplate::LoopElement { ty, .. } => {
                artifact.validate_value_type("loop element payload type", *ty)
            }
            ArtifactValueTemplate::EnumVariant { payload, .. } => {
                self.validate_template_process_refs(artifact, payload, spawned_refs)
            }
            ArtifactValueTemplate::Record { fields, .. } => {
                for field in fields {
                    self.validate_template_process_refs(artifact, &field.value, spawned_refs)?;
                }
                Ok(())
            }
            ArtifactValueTemplate::List { items, .. } => {
                for item in items {
                    self.validate_template_process_refs(artifact, item, spawned_refs)?;
                }
                Ok(())
            }
            ArtifactValueTemplate::Map { entries, .. } => {
                for entry in entries {
                    self.validate_template_process_refs(artifact, &entry.key, spawned_refs)?;
                    self.validate_template_process_refs(artifact, &entry.value, spawned_refs)?;
                }
                Ok(())
            }
        }
    }

    fn process_ref_target(&self, process_ref: ProcessRefId) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }
}
