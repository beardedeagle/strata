use super::values::{
    validate_list_prefix_projection, validate_list_rest_prefix_len, validate_map_projection_keys,
    validate_map_rest_keys, validate_non_process_ref_value,
};
use super::*;
use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField};
use std::collections::BTreeSet;

pub(super) fn evaluate_loaded_state_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<LoadedStateValue> {
    let payload =
        evaluate_loaded_payload_value(program, template, received_payload, current_state_payload)?;
    Ok(LoadedStateValue::from_payload(payload))
}

pub(super) fn validate_loaded_bool_condition(
    program: &LoadedProgram,
    process: &LoadedProcess,
    field: &str,
    condition: &LoadedValueTemplate,
    received_payload_type: Option<TypeId>,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<()> {
    let bool_type = condition.result_type();
    let ty = program.type_entry(bool_type)?;
    let is_bool_contract = matches!(ty.kind, ArtifactTypeKind::Value)
        && ty.enum_variants.len() == 2
        && ty.enum_variants[0] == "False"
        && ty.enum_variants[1] == "True";
    if !is_bool_contract {
        return Err(Error::new(format!(
            "{field} must have type enum Bool {{ False, True }}"
        )));
    }
    LoadedTemplateAdmission {
        expected_type: Some(bool_type),
        received_payload_type,
        current_state_payload_type: current_state_payload.map(|payload| payload.ty),
        allow_direct_process_ref: false,
        program,
        process,
        spawned_refs: &[],
    }
    .validate(field, condition)?;
    validate_loaded_static_bool_condition_value(program, field, condition, current_state_payload)
}

fn validate_loaded_static_bool_condition_value(
    program: &LoadedProgram,
    field: &str,
    condition: &LoadedValueTemplate,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<()> {
    if loaded_template_depends_on_received_payload(condition) {
        return Ok(());
    }

    let value = evaluate_loaded_payload_value(program, condition, None, current_state_payload)?;
    validate_loaded_bool_atom_value(field, &value.value)
}

fn validate_loaded_bool_atom_value(field: &str, value: &RuntimeValue) -> Result<()> {
    match value {
        RuntimeValue::Atom(label) if label == "False" || label == "True" => Ok(()),
        _ => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

fn evaluate_loaded_payload_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<RuntimePayload> {
    match template {
        LoadedValueTemplate::Literal { ty, value } => RuntimePayload::value(*ty, value.clone()),
        LoadedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "received payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::CurrentStatePayload { ty } => {
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "current state payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_loaded_payload_value(
                program,
                value,
                received_payload,
                current_state_payload,
            )?;
            let variant = program.enum_variant_label(value.ty, *variant)?;
            RuntimePayload::value(*ty, value.value.project_enum_payload(variant)?)
        }
        LoadedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_loaded_payload_value(
                program,
                record,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(*ty, record.value.project_record_field(field)?)
        }
        LoadedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(*ty, list.value.project_list_element(*index, *len)?)
        }
        LoadedValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(
                *ty,
                list.value
                    .project_list_prefix_element(*index, *prefix_len)?,
            )
        }
        LoadedValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(*ty, list.value.project_list_rest(*prefix_len)?)
        }
        LoadedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map = evaluate_loaded_payload_value(
                program,
                map,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(*ty, map.value.project_map_value(key, keys, *projection)?)
        }
        LoadedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map = evaluate_loaded_payload_value(
                program,
                map,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(*ty, map.value.project_map_rest(excluded_keys)?)
        }
        LoadedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires runtime process reference bindings",
        )),
        LoadedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_loaded_payload_value(
                program,
                payload,
                received_payload,
                current_state_payload,
            )?;
            RuntimePayload::value(
                *ty,
                RuntimeValue::EnumVariant {
                    variant: program.enum_variant_label(*ty, *variant)?.to_string(),
                    payload: Box::new(payload.value),
                },
            )
        }
        LoadedValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            let mut seen = BTreeSet::new();
            for field in fields {
                let value = evaluate_loaded_payload_value(
                    program,
                    &field.value,
                    received_payload,
                    current_state_payload,
                )?;
                if !seen.insert(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record template duplicates field {}",
                        field.name
                    )));
                }
                values.push(ArtifactRecordField {
                    name: field.name.clone(),
                    value: value.value,
                });
            }
            RuntimePayload::value(
                *ty,
                RuntimeValue::Record {
                    constructor: program.type_label(*ty)?.to_string(),
                    fields: values,
                },
            )
        }
        LoadedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(
                    evaluate_loaded_payload_value(
                        program,
                        item,
                        received_payload,
                        current_state_payload,
                    )?
                    .value,
                );
            }
            RuntimePayload::value(*ty, RuntimeValue::List(values))
        }
        LoadedValueTemplate::Map { ty, entries } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = evaluate_loaded_payload_value(
                    program,
                    &entry.key,
                    received_payload,
                    current_state_payload,
                )?;
                let value = evaluate_loaded_payload_value(
                    program,
                    &entry.value,
                    received_payload,
                    current_state_payload,
                )?;
                if !seen.insert(key.value.clone()) {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.value.label()
                    )));
                }
                values.push(ArtifactMapEntry {
                    key: key.value,
                    value: value.value,
                });
            }
            RuntimePayload::value(*ty, RuntimeValue::Map(values))
        }
    }
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
    pub(super) fn validate(&self, field: &str, template: &LoadedValueTemplate) -> Result<()> {
        self.validate_with_depth(field, template, 0)
    }

    fn validate_with_depth(
        &self,
        field: &str,
        template: &LoadedValueTemplate,
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
            LoadedValueTemplate::Literal { ty, value } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_non_process_ref_value(field, value)
            }
            LoadedValueTemplate::ReceivedPayload { ty } => {
                self.validate_received_payload(field, *ty)
            }
            LoadedValueTemplate::CurrentStatePayload { ty } => {
                self.validate_current_state_payload(field, *ty)
            }
            LoadedValueTemplate::EnumPayload { ty, value, variant } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_enum_variant_projection(field, value.result_type(), *variant)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.value"), value, depth + 1)
            }
            LoadedValueTemplate::RecordField {
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
            LoadedValueTemplate::ListElement {
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
            LoadedValueTemplate::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_list_prefix_projection(field, *index, *prefix_len)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.list"), list, depth + 1)
            }
            LoadedValueTemplate::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_list_rest_prefix_len(field, *prefix_len)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.list"), list, depth + 1)
            }
            LoadedValueTemplate::MapValue {
                ty,
                map,
                key,
                keys,
                projection: _,
            } => {
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
            LoadedValueTemplate::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                self.reject_projected_process_ref_type(field, *ty)?;
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_map_rest_keys(field, excluded_keys)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.map"), map, depth + 1)
            }
            LoadedValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => self.validate_process_ref(field, *ty, *target_process, *process_ref),
            LoadedValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_enum_variant_projection(field, *ty, *variant)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.payload"), payload, depth + 1)
            }
            LoadedValueTemplate::Record { ty, fields } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_record(field, fields, depth)
            }
            LoadedValueTemplate::List { ty, items } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_list(field, items, depth)
            }
            LoadedValueTemplate::Map { ty, entries } => {
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

    fn validate_enum_variant_projection(
        &self,
        field: &str,
        enum_ty: TypeId,
        variant: EnumVariantId,
    ) -> Result<()> {
        self.program
            .validate_value_type(&format!("{field}.enum_type"), enum_ty)?;
        self.program
            .enum_variant_label(enum_ty, variant)
            .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
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
        fields: &[LoadedValueTemplateField],
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
        items: &[LoadedValueTemplate],
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
        entries: &[LoadedValueTemplateMapEntry],
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
                return Err(Error::new(format!(
                    "{field} duplicates key {}",
                    key.label()
                )));
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

fn loaded_static_map_key_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
) -> Result<RuntimeValue> {
    evaluate_loaded_state_value(program, template, None, None).map(|value| value.value)
}

fn loaded_template_is_static_map_key(template: &LoadedValueTemplate) -> bool {
    match template {
        LoadedValueTemplate::Literal { .. } => true,
        LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::EnumPayload { .. }
        | LoadedValueTemplate::RecordField { .. }
        | LoadedValueTemplate::ListElement { .. }
        | LoadedValueTemplate::ListPrefixElement { .. }
        | LoadedValueTemplate::ListRest { .. }
        | LoadedValueTemplate::MapValue { .. }
        | LoadedValueTemplate::MapRest { .. }
        | LoadedValueTemplate::ProcessRef { .. } => false,
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_is_static_map_key(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .all(|field| loaded_template_is_static_map_key(&field.value)),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().all(loaded_template_is_static_map_key)
        }
        LoadedValueTemplate::Map { entries, .. } => entries.iter().all(|entry| {
            loaded_template_is_static_map_key(&entry.key)
                && loaded_template_is_static_map_key(&entry.value)
        }),
    }
}

pub(super) fn loaded_template_depends_on_received_payload(template: &LoadedValueTemplate) -> bool {
    match template {
        LoadedValueTemplate::Literal { .. } | LoadedValueTemplate::ProcessRef { .. } => false,
        LoadedValueTemplate::ReceivedPayload { .. } => true,
        LoadedValueTemplate::CurrentStatePayload { .. } => false,
        LoadedValueTemplate::EnumPayload { value, .. } => {
            loaded_template_depends_on_received_payload(value)
        }
        LoadedValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_received_payload(record)
        }
        LoadedValueTemplate::ListElement { list, .. }
        | LoadedValueTemplate::ListPrefixElement { list, .. }
        | LoadedValueTemplate::ListRest { list, .. } => {
            loaded_template_depends_on_received_payload(list)
        }
        LoadedValueTemplate::MapValue { map, .. } => {
            loaded_template_depends_on_received_payload(map)
        }
        LoadedValueTemplate::MapRest { map, .. } => {
            loaded_template_depends_on_received_payload(map)
        }
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_received_payload(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_received_payload(&field.value)),
        LoadedValueTemplate::List { items, .. } => items
            .iter()
            .any(loaded_template_depends_on_received_payload),
        LoadedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_received_payload(&entry.key)
                || loaded_template_depends_on_received_payload(&entry.value)
        }),
    }
}
