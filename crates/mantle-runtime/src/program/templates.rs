use super::*;

pub(super) fn evaluate_loaded_state_value(
    program: &LoadedProgram,
    template: &ArtifactValueTemplate,
    received_payload: Option<&mantle_artifact::ArtifactPayload>,
    current_state_payload: Option<&mantle_artifact::ArtifactPayload>,
) -> Result<ArtifactStateValue> {
    template.evaluate_state_value(received_payload, current_state_payload, &|ty| {
        program.type_label(ty).map(str::to_owned)
    })
}

#[derive(Clone, Copy)]
pub(super) struct LoadedTemplateAdmission<'a> {
    pub(super) expected_type: Option<TypeId>,
    pub(super) received_payload_type: Option<TypeId>,
    pub(super) current_state_payload_type: Option<TypeId>,
    pub(super) allow_direct_process_ref: bool,
    pub(super) program: &'a LoadedProgram,
    pub(super) process: &'a LoadedProcess,
    pub(super) spawned_refs: &'a [bool],
}

impl LoadedTemplateAdmission<'_> {
    pub(super) fn validate(&self, field: &str, template: &ArtifactValueTemplate) -> Result<()> {
        self.validate_with_depth(field, template, 0)
    }

    fn validate_with_depth(
        &self,
        field: &str,
        template: &ArtifactValueTemplate,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        self.program.type_entry(template.result_type())?;
        if let Some(expected_type) = self.expected_type {
            if template.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type id {}, expected {}",
                    template.result_type().as_u32(),
                    expected_type.as_u32()
                )));
            }
        }

        match template {
            ArtifactValueTemplate::Literal { ty, value } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_payload_value_label(value)
                    .map_err(|err| Error::new(format!("{field}: {err}")))
            }
            ArtifactValueTemplate::ReceivedPayload { ty } => {
                self.validate_received_payload(field, *ty)
            }
            ArtifactValueTemplate::CurrentStatePayload { ty } => {
                self.validate_current_state_payload(field, *ty)
            }
            ArtifactValueTemplate::RecordField {
                ty,
                record,
                field: field_name,
            } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_loaded_ident_field(&format!("{field}.field_name"), field_name)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.record"), record, depth + 1)
            }
            ArtifactValueTemplate::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                if *len == 0 || *len > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "{field}.len must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                if *index >= *len {
                    return Err(Error::new(format!(
                        "{field}.index {index} is outside list length {len}"
                    )));
                }
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.list"), list, depth + 1)
            }
            ArtifactValueTemplate::MapValue { ty, map, key, keys } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_map_projection_keys(field, key, keys)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.map"), map, depth + 1)
            }
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => self.validate_process_ref(field, *ty, *target_process, *process_ref),
            ArtifactValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_loaded_ident_field(&format!("{field}.variant"), variant)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.payload"), payload, depth + 1)
            }
            ArtifactValueTemplate::Record { ty, fields } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_record(field, fields, depth)
            }
            ArtifactValueTemplate::List { ty, items } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_list(field, items, depth)
            }
            ArtifactValueTemplate::Map { ty, entries } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_map(field, entries, depth)
            }
        }
    }

    fn reject_projected_process_ref_type(&self, field: &str, ty: TypeId) -> Result<()> {
        if matches!(
            self.program.type_entry(ty)?.kind,
            ArtifactTypeKind::ProcessRef { .. }
        ) {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        Ok(())
    }

    fn validate_received_payload(&self, field: &str, ty: TypeId) -> Result<()> {
        let Some(received_payload_type) = self.received_payload_type else {
            return Err(Error::new(format!(
                "{field} requires a payload-bearing transition message"
            )));
        };
        if ty != received_payload_type {
            return Err(Error::new(format!(
                "{field} has received payload type id {}, expected {}",
                ty.as_u32(),
                received_payload_type.as_u32()
            )));
        }
        if !self.allow_direct_process_ref
            && matches!(
                self.program.type_entry(ty)?.kind,
                ArtifactTypeKind::ProcessRef { .. }
            )
        {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        Ok(())
    }

    fn validate_current_state_payload(&self, field: &str, ty: TypeId) -> Result<()> {
        let Some(current_state_payload_type) = self.current_state_payload_type else {
            return Err(Error::new(format!(
                "{field} requires a payload-bearing current state"
            )));
        };
        if ty != current_state_payload_type {
            return Err(Error::new(format!(
                "{field} has current state payload type id {}, expected {}",
                ty.as_u32(),
                current_state_payload_type.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_process_ref(
        &self,
        field: &str,
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        if !self.allow_direct_process_ref {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        self.program.validate_process_ref_type_id_target(
            "process reference payload type",
            ty,
            target_process,
        )?;
        let declared_target = self.process.process_ref_target(process_ref)?;
        if declared_target != target_process {
            return Err(Error::new(format!(
                "process {} process reference payload id {} targets process id {}, expected {}",
                self.process.debug_name,
                process_ref.as_u32(),
                declared_target.as_u32(),
                target_process.as_u32()
            )));
        }
        let is_spawned = self
            .spawned_refs
            .get(process_ref.index())
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unloaded process reference id {} as payload",
                    self.process.debug_name,
                    process_ref.as_u32()
                ))
            })?;
        if !is_spawned {
            return Err(Error::new(format!(
                "process {} sends unbound process reference id {} as payload",
                self.process.debug_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_record(
        &self,
        field: &str,
        fields: &[mantle_artifact::ArtifactValueTemplateField],
        depth: usize,
    ) -> Result<()> {
        if fields.is_empty() || fields.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.field_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        let mut names = BTreeSet::new();
        for record_field in fields {
            validate_loaded_ident_field(&format!("{field}.field"), &record_field.name)?;
            if !names.insert(record_field.name.as_str()) {
                return Err(Error::new(format!(
                    "{field} duplicates field {}",
                    record_field.name
                )));
            }
            let nested = Self {
                expected_type: None,
                allow_direct_process_ref: false,
                ..*self
            };
            nested.validate_with_depth(
                &format!("{field}.field.{}", record_field.name),
                &record_field.value,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn validate_list(
        &self,
        field: &str,
        items: &[ArtifactValueTemplate],
        depth: usize,
    ) -> Result<()> {
        if items.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.item_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        let nested = Self {
            expected_type: None,
            allow_direct_process_ref: false,
            ..*self
        };
        for (index, item) in items.iter().enumerate() {
            nested.validate_with_depth(&format!("{field}.item.{index}"), item, depth + 1)?;
        }
        Ok(())
    }

    fn validate_map(
        &self,
        field: &str,
        entries: &[mantle_artifact::ArtifactValueTemplateMapEntry],
        depth: usize,
    ) -> Result<()> {
        if entries.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.entry_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        let nested = Self {
            expected_type: None,
            allow_direct_process_ref: false,
            ..*self
        };
        let mut keys = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            if !loaded_template_is_static_map_key(&entry.key) {
                return Err(Error::new(format!(
                    "{field}.entry.{index}.key must be a static value template"
                )));
            }
            nested.validate_with_depth(
                &format!("{field}.entry.{index}.key"),
                &entry.key,
                depth + 1,
            )?;
            let key = loaded_static_map_key_value(self.program, &entry.key)?;
            if !keys.insert(key.clone()) {
                return Err(Error::new(format!("{field} duplicates key {key}")));
            }
            nested.validate_with_depth(
                &format!("{field}.entry.{index}.value"),
                &entry.value,
                depth + 1,
            )?;
        }
        Ok(())
    }
}

fn validate_map_projection_keys(field: &str, key: &str, keys: &[String]) -> Result<()> {
    if keys.is_empty() || keys.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "{field}.key_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    validate_payload_value_label(key).map_err(|err| Error::new(format!("{field}.key: {err}")))?;
    let mut seen = BTreeSet::new();
    for expected_key in keys {
        validate_payload_value_label(expected_key)
            .map_err(|err| Error::new(format!("{field}.expected_key: {err}")))?;
        if !seen.insert(expected_key.clone()) {
            return Err(Error::new(format!(
                "{field} duplicates expected map key {expected_key}"
            )));
        }
    }
    if !seen.contains(key) {
        return Err(Error::new(format!(
            "{field} projection key {key} is not one of the expected map keys"
        )));
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(format!(
            "{field} expected map keys must be sorted"
        )));
    }
    Ok(())
}

fn loaded_static_map_key_value(
    program: &LoadedProgram,
    template: &ArtifactValueTemplate,
) -> Result<String> {
    evaluate_loaded_state_value(program, template, None, None).map(|value| value.value)
}

fn loaded_template_is_static_map_key(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } => true,
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::RecordField { .. }
        | ArtifactValueTemplate::ListElement { .. }
        | ArtifactValueTemplate::MapValue { .. }
        | ArtifactValueTemplate::ProcessRef { .. } => false,
        ArtifactValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_is_static_map_key(payload)
        }
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .all(|field| loaded_template_is_static_map_key(&field.value)),
        ArtifactValueTemplate::List { items, .. } => {
            items.iter().all(loaded_template_is_static_map_key)
        }
        ArtifactValueTemplate::Map { entries, .. } => entries.iter().all(|entry| {
            loaded_template_is_static_map_key(&entry.key)
                && loaded_template_is_static_map_key(&entry.value)
        }),
    }
}

pub(super) fn loaded_template_depends_on_received_payload(
    template: &ArtifactValueTemplate,
) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } | ArtifactValueTemplate::ProcessRef { .. } => false,
        ArtifactValueTemplate::ReceivedPayload { .. } => true,
        ArtifactValueTemplate::CurrentStatePayload { .. } => false,
        ArtifactValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_received_payload(record)
        }
        ArtifactValueTemplate::ListElement { list, .. } => {
            loaded_template_depends_on_received_payload(list)
        }
        ArtifactValueTemplate::MapValue { map, .. } => {
            loaded_template_depends_on_received_payload(map)
        }
        ArtifactValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_received_payload(payload)
        }
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_received_payload(&field.value)),
        ArtifactValueTemplate::List { items, .. } => items
            .iter()
            .any(loaded_template_depends_on_received_payload),
        ArtifactValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_received_payload(&entry.key)
                || loaded_template_depends_on_received_payload(&entry.value)
        }),
    }
}
