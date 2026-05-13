use super::steps::{
    collect_concrete_state_payload_domains, collect_explicit_step_variants,
    collect_message_case_process_refs, matching_message_cases, payload_value_bindings,
    resolve_send_target_process_for_discovery, step_discovery_clauses,
};
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiscoveredMessageCase {
    variant: CheckedMessageVariantId,
    payload: Option<CheckedPayloadValue>,
}

impl DiscoveredMessageCase {
    pub(super) fn new(
        variant: CheckedMessageVariantId,
        payload: Option<CheckedPayloadValue>,
    ) -> Self {
        Self { variant, payload }
    }

    pub(super) fn variant(&self) -> CheckedMessageVariantId {
        self.variant
    }

    pub(super) fn payload(&self) -> Option<&CheckedPayloadValue> {
        self.payload.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MessageCaseKey {
    process: CheckedProcessId,
    variant: CheckedMessageVariantId,
}

#[derive(Debug, Clone)]
pub(super) struct MessageCaseTable {
    cases_by_process: Vec<Vec<CheckedMessageCase>>,
    payloads_by_process: Vec<Vec<Vec<CheckedPayloadValue>>>,
    ids_by_key: BTreeMap<MessageCaseKey, CheckedMessageId>,
}

impl MessageCaseTable {
    pub(super) fn build<'a>(
        module: &'a Module,
        entry_process: CheckedProcessId,
        semantic_index: &'a SemanticIndex,
        types: &mut CheckedTypeInterner<'a>,
    ) -> Result<Self> {
        reject_payload_entry_message(module, entry_process, semantic_index)?;
        let mut builders = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                MessageCaseBuilder::new(module, semantic_index, process, process_id)
            })
            .collect::<Result<Vec<_>>>()?;
        let process_ref_targets = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_message_case_process_refs(process, process_id, semantic_index)
            })
            .collect::<Result<Vec<_>>>()?;
        let explicit_step_variants = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_explicit_step_variants(module, process, process_id, semantic_index)
            })
            .collect::<Result<Vec<_>>>()?;
        let concrete_state_payload_domains = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_concrete_state_payload_domains(module, process, process_id, semantic_index)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut iteration_count = 0usize;
        let max_iterations = MAX_MESSAGE_VARIANTS_PER_PROCESS.saturating_mul(builders.len());
        loop {
            let case_snapshots = builders
                .iter()
                .map(MessageCaseBuilder::current_cases)
                .collect::<Result<Vec<_>>>()?;
            let mut changed = false;
            for (process_index, process) in module.processes.iter().enumerate() {
                let process_id = CheckedProcessId::from_index(process_index)?;
                let sender_cases = &case_snapshots[process_index];
                for step in &process.steps {
                    for clause in
                        step_discovery_clauses(module, process, process_id, semantic_index, step)?
                    {
                        for sender_case in matching_message_cases(
                            sender_cases,
                            &clause.pattern,
                            &explicit_step_variants[process_index],
                        ) {
                            let bindings = payload_value_bindings(
                                module,
                                semantic_index,
                                &clause.pattern,
                                sender_case,
                            )?;
                            for statement in &clause.body.statements {
                                let Statement::Send {
                                    target,
                                    message,
                                    payload,
                                } = statement
                                else {
                                    continue;
                                };
                                let target_process_id = resolve_send_target_process_for_discovery(
                                    process,
                                    semantic_index,
                                    &process_ref_targets[process_index],
                                    &clause.pattern,
                                    target,
                                )?;
                                let target_variant = semantic_index.message_id_for_process(
                                    module,
                                    process.name.as_str(),
                                    target_process_id,
                                    message,
                                )?;
                                let builder = builders
                                    .get_mut(target_process_id.index())
                                    .ok_or_else(|| {
                                        Error::new(format!(
                                            "process id {} is not declared",
                                            target_process_id.as_u32()
                                        ))
                                    })?;
                                let mut discovery_context = SendPayloadDiscoveryContext {
                                    sender_cases,
                                    concrete_state_payloads: &concrete_state_payload_domains
                                        [process_index],
                                    module,
                                    semantic_index,
                                    process_refs: &process_ref_targets[process_index],
                                    types,
                                };
                                changed |= add_discovered_send_payload_cases(
                                    builder,
                                    target_variant,
                                    payload.as_ref(),
                                    &bindings,
                                    &clause.state_payload_bindings,
                                    &mut discovery_context,
                                )?;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
            iteration_count = iteration_count
                .checked_add(1)
                .ok_or_else(|| Error::new("message case discovery iteration count overflowed"))?;
            if iteration_count > max_iterations {
                return Err(Error::new(
                    "message case discovery did not converge within the message variant limit",
                ));
            }
        }

        let mut cases_by_process = Vec::with_capacity(builders.len());
        let mut payloads_by_process = Vec::with_capacity(builders.len());
        let mut ids_by_key = BTreeMap::new();
        for builder in builders {
            let process_id = builder.process_id;
            let cases = builder.logical_cases(types)?;
            for (message_index, case) in cases.iter().enumerate() {
                let key = MessageCaseKey {
                    process: process_id,
                    variant: case.variant(),
                };
                if ids_by_key
                    .insert(key, CheckedMessageId::from_index(message_index)?)
                    .is_some()
                {
                    return Err(Error::new(format!(
                        "process id {} declares duplicate message case",
                        process_id.as_u32()
                    )));
                }
            }
            payloads_by_process.push(builder.payload_domains(types)?);
            cases_by_process.push(cases);
        }

        Ok(Self {
            cases_by_process,
            payloads_by_process,
            ids_by_key,
        })
    }

    pub(super) fn cases_for(&self, process: CheckedProcessId) -> Result<&[CheckedMessageCase]> {
        self.cases_by_process
            .get(process.index())
            .map(Vec::as_slice)
            .ok_or_else(|| Error::new(format!("process id {} is not declared", process.as_u32())))
    }

    pub(super) fn message_id(
        &self,
        process: CheckedProcessId,
        variant: CheckedMessageVariantId,
    ) -> Result<CheckedMessageId> {
        let key = MessageCaseKey { process, variant };
        self.ids_by_key.get(&key).copied().ok_or_else(|| {
            Error::new(format!(
                "process id {} has no message case for message id {}",
                process.as_u32(),
                variant.as_u32()
            ))
        })
    }

    pub(super) fn payload_values(
        &self,
        process: CheckedProcessId,
        variant: CheckedMessageVariantId,
    ) -> Result<&[CheckedPayloadValue]> {
        self.payloads_by_process
            .get(process.index())
            .and_then(|process_payloads| process_payloads.get(variant.index()))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                Error::new(format!(
                    "process id {} has no payload domain for message id {}",
                    process.as_u32(),
                    variant.as_u32()
                ))
            })
    }
}

struct MessageCaseBuilder<'a> {
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    process: &'a Process,
    process_id: CheckedProcessId,
    payload_cases:
        BTreeMap<CheckedMessageVariantId, BTreeMap<PayloadDomainKey, CheckedPayloadValue>>,
}

impl<'a> MessageCaseBuilder<'a> {
    pub(super) fn new(
        module: &'a Module,
        semantic_index: &'a SemanticIndex,
        process: &'a Process,
        process_id: CheckedProcessId,
    ) -> Result<Self> {
        let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
        let mut payload_cases = BTreeMap::new();
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if variant.payload_type.is_some() {
                payload_cases.insert(variant_id, BTreeMap::new());
            }
        }
        Ok(Self {
            module,
            semantic_index,
            process,
            process_id,
            payload_cases,
        })
    }

    fn add_payload_case(
        &mut self,
        variant_id: CheckedMessageVariantId,
        payload: Option<&ValueExpr>,
        bindings: &[ValueBinding<'_>],
        process_refs: &BTreeMap<Identifier, CheckedProcessId>,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<bool> {
        let variant =
            self.semantic_index
                .message_variant(self.module, self.process_id, variant_id)?;
        match (&variant.payload_type, payload) {
            (None, None) => Ok(false),
            (None, Some(_)) => Err(Error::new(format!(
                "message {} does not accept a payload",
                variant.name
            ))),
            (Some(_), None) => Err(Error::new(format!(
                "message {} requires a payload",
                variant.name
            ))),
            (Some(payload_type), Some(payload)) => {
                let function_scope = SourceFunctionScope {
                    module: self.module,
                    process_name: Some(&self.process.name),
                    process_functions: &self.process.functions,
                    semantic_index: self.semantic_index,
                };
                let source_bindings = bindings
                    .iter()
                    .map(|binding| SourceValueBinding {
                        name: binding.name,
                        ty: binding.ty,
                    })
                    .collect::<Vec<_>>();
                let payload = resolve_source_value_expr(
                    &function_scope,
                    payload_type,
                    payload,
                    &source_bindings,
                    0,
                )?;
                let checked_type = types.intern(payload_type)?;
                let payload = if let Some(target) =
                    self.semantic_index.process_ref_target_type(payload_type)?
                {
                    let label = canonical_process_ref_payload_label(
                        payload_type,
                        target,
                        &payload,
                        bindings,
                        process_refs,
                    )?;
                    CheckedPayloadValue::process_ref(checked_type, label, target, 0)
                } else {
                    let value = canonical_source_value_with_bindings(
                        self.module,
                        self.semantic_index,
                        payload_type,
                        &payload,
                        bindings,
                    )?;
                    CheckedPayloadValue::new(checked_type, value)
                };
                Ok(self.insert_payload_case(variant_id, payload))
            }
        }
    }

    fn insert_payload_case(
        &mut self,
        variant_id: CheckedMessageVariantId,
        payload: CheckedPayloadValue,
    ) -> bool {
        let payloads = self.payload_cases.entry(variant_id).or_default();
        let key = PayloadDomainKey::from_payload(&payload);
        if payloads.contains_key(&key) {
            return false;
        }
        payloads.insert(key, payload);
        true
    }

    fn current_cases(&self) -> Result<Vec<DiscoveredMessageCase>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        let mut cases = Vec::new();
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if variant.payload_type.is_some() {
                if let Some(payloads) = self.payload_cases.get(&variant_id) {
                    for payload in payloads.values() {
                        cases.push(DiscoveredMessageCase::new(
                            variant_id,
                            Some(payload.clone()),
                        ));
                    }
                }
            } else {
                cases.push(DiscoveredMessageCase::new(variant_id, None));
            }
        }
        Ok(cases)
    }

    fn logical_cases(
        &self,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Vec<CheckedMessageCase>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        msg_enum
            .variants
            .iter()
            .enumerate()
            .map(|(variant_index, variant)| {
                CheckedMessageCase::new(
                    variant.name.to_string(),
                    CheckedMessageVariantId::from_index(variant_index)?,
                    variant
                        .payload_type
                        .as_ref()
                        .map(|payload_type| types.intern(payload_type))
                        .transpose()?,
                )
            })
            .collect()
    }

    fn payload_domains(
        &self,
        _types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Vec<Vec<CheckedPayloadValue>>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        let mut payloads_by_variant = vec![Vec::new(); msg_enum.variants.len()];
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            if variant.payload_type.is_none() {
                continue;
            }
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if let Some(payloads) = self.payload_cases.get(&variant_id) {
                payloads_by_variant[variant_index] = payloads.values().cloned().collect();
            }
        }
        Ok(payloads_by_variant)
    }
}

fn add_discovered_send_payload_cases(
    builder: &mut MessageCaseBuilder<'_>,
    variant_id: CheckedMessageVariantId,
    payload: Option<&ValueExpr>,
    message_bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let mut changed = false;
    for bindings in
        discovery_value_binding_sets(payload, message_bindings, state_payload_bindings, context)?
    {
        let value_bindings = value_bindings_from_discovery(&bindings);
        changed |= builder.add_payload_case(
            variant_id,
            payload,
            &value_bindings,
            context.process_refs,
            context.types,
        )?;
    }
    Ok(changed)
}

fn discovery_value_binding_sets(
    payload: Option<&ValueExpr>,
    message_bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<Vec<Vec<DiscoveryValueBinding>>> {
    let Some(payload) = payload else {
        return Ok(vec![message_bindings.to_vec()]);
    };
    let mut binding_sets = vec![message_bindings.to_vec()];
    for binding in state_payload_bindings {
        if !source_value_uses_binding(payload, &binding.name) {
            continue;
        }
        let payloads = state_payload_discovery_values(
            binding,
            context.sender_cases,
            context.concrete_state_payloads,
            context.semantic_index,
            context.types,
        )?;
        if payloads.is_empty() {
            return Ok(Vec::new());
        }
        let mut expanded = Vec::with_capacity(binding_sets.len().saturating_mul(payloads.len()));
        for base in &binding_sets {
            for payload in &payloads {
                let mut next = base.clone();
                let (label, value) = checked_payload_binding(
                    context.module,
                    context.semantic_index,
                    payload,
                    &PatternPayloadParam {
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                        path: binding.path.clone(),
                    },
                )?
                .ok_or_else(|| {
                    Error::new(format!(
                        "state payload {} does not match binding {}",
                        payload.label(),
                        binding.name
                    ))
                })?;
                let value = value.ok_or_else(|| {
                    Error::new(format!(
                        "state payload {} does not match binding {}",
                        payload.label(),
                        binding.name
                    ))
                })?;
                next.push(DiscoveryValueBinding {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    label,
                    value: Some(value),
                });
                expanded.push(next);
            }
        }
        binding_sets = expanded;
    }
    Ok(binding_sets)
}

fn state_payload_discovery_values(
    binding: &StatePayloadDiscoveryBinding,
    sender_cases: &[DiscoveredMessageCase],
    concrete_state_payloads: &[ConcreteStatePayloadDomain],
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<Vec<CheckedPayloadValue>> {
    let checked_ty = types.intern(&binding.payload_ty)?;
    let mut payloads: BTreeMap<PayloadDomainKey, CheckedPayloadValue> = BTreeMap::new();
    if let Some(domain) = concrete_state_payloads
        .iter()
        .find(|domain| semantic_index.same_type(&domain.ty, &binding.payload_ty))
    {
        for value in &domain.values {
            let payload = CheckedPayloadValue::new(checked_ty.clone(), value.clone());
            payloads.insert(PayloadDomainKey::from_payload(&payload), payload);
        }
    }
    for case in sender_cases {
        let Some(payload) = case.payload() else {
            continue;
        };
        if payload.ty() == &checked_ty && payload.process_ref_payload().is_none() {
            payloads.insert(PayloadDomainKey::from_payload(payload), payload.clone());
        }
    }
    Ok(payloads.into_values().collect())
}

fn value_bindings_from_discovery(bindings: &[DiscoveryValueBinding]) -> Vec<ValueBinding<'_>> {
    bindings
        .iter()
        .map(|binding| ValueBinding {
            name: &binding.name,
            ty: &binding.ty,
            label: binding.label.clone(),
            value: binding.value.clone(),
        })
        .collect()
}

fn canonical_process_ref_payload_label(
    expected_type: &TypeRef,
    expected_target: CheckedProcessId,
    payload: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
) -> Result<String> {
    let ValueExpr::Identifier(name) = payload else {
        return Err(Error::new(format!(
            "process reference payload type {expected_type} must be passed as an immutable process reference value"
        )));
    };
    if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
        if binding.ty == expected_type {
            return Ok(binding.label.to_string());
        }
        return Err(Error::new(format!(
            "value binding {} has type {}, expected {}",
            binding.name, binding.ty, expected_type
        )));
    }
    let Some(actual_target) = process_refs.get(name) else {
        return Err(Error::new(format!(
            "process reference payload {name} is not a bound process reference"
        )));
    };
    if *actual_target != expected_target {
        return Err(Error::new(format!(
            "process reference payload {name} targets process id {}, expected {}",
            actual_target.as_u32(),
            expected_target.as_u32()
        )));
    }
    Ok(expected_type.to_string())
}
