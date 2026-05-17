use super::values::{
    validate_list_prefix_projection, validate_list_rest_prefix_len, validate_map_projection_keys,
    validate_map_rest_keys,
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
    validate_loaded_bool_condition_with_loop_elements(
        program,
        process,
        field,
        condition,
        received_payload_type,
        current_state_payload,
        &[],
    )
}

pub(super) fn validate_loaded_bool_condition_with_loop_elements(
    program: &LoadedProgram,
    process: &LoadedProcess,
    field: &str,
    condition: &LoadedValueTemplate,
    received_payload_type: Option<TypeId>,
    current_state_payload: Option<&RuntimePayload>,
    loop_elements: &[LoadedLoopElement],
) -> Result<()> {
    let bool_type = condition.result_type();
    let ty = program.type_entry(bool_type)?;
    let is_bool_contract = matches!(
        ty.value_shape(),
        Ok(ArtifactValueShape::Enum { variants })
            if variants.len() == 2
                && variants[0].label == "False"
                && variants[0].payload_type.is_none()
                && variants[1].label == "True"
                && variants[1].payload_type.is_none()
    );
    if !is_bool_contract {
        return Err(Error::new(format!(
            "{field} must have type enum Bool {{ False, True }}"
        )));
    }
    validate_loaded_bool_condition_shape(field, condition)?;
    LoadedTemplateAdmission {
        expected_type: Some(bool_type),
        received_payload_type,
        current_state_payload_type: current_state_payload.map(|payload| payload.ty),
        allow_direct_process_ref: false,
        loop_elements,
        program,
        process,
        spawned_refs: &[],
    }
    .validate(field, condition)?;
    validate_loaded_static_bool_condition_value(program, field, condition, current_state_payload)
}

fn validate_loaded_bool_condition_shape(
    field: &str,
    condition: &LoadedValueTemplate,
) -> Result<()> {
    match condition {
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::EnumPayload { .. }
        | LoadedValueTemplate::RecordField { .. }
        | LoadedValueTemplate::ListElement { .. }
        | LoadedValueTemplate::ListPrefixElement { .. }
        | LoadedValueTemplate::MapValue { .. }
        | LoadedValueTemplate::LoopElement { .. } => Ok(()),
        LoadedValueTemplate::ListRest { .. }
        | LoadedValueTemplate::MapRest { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::EnumVariant { .. }
        | LoadedValueTemplate::Record { .. }
        | LoadedValueTemplate::List { .. }
        | LoadedValueTemplate::Map { .. } => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

fn validate_loaded_static_bool_condition_value(
    program: &LoadedProgram,
    field: &str,
    condition: &LoadedValueTemplate,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<()> {
    if loaded_template_depends_on_received_payload(condition)
        || loaded_template_depends_on_loop_element(condition)
    {
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
        LoadedValueTemplate::Literal { ty, value } => {
            program.runtime_payload_value("literal value template", *ty, value.clone())
        }
        LoadedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            program.validate_runtime_payload_matches_type("received payload", *ty, payload)?;
            Ok(payload.clone())
        }
        LoadedValueTemplate::CurrentStatePayload { ty } => {
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            program.validate_runtime_payload_matches_type("current state payload", *ty, payload)?;
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
            program.runtime_payload_value(
                "enum payload projection value",
                *ty,
                value.value.project_enum_payload(variant)?,
            )
        }
        LoadedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_loaded_payload_value(
                program,
                record,
                received_payload,
                current_state_payload,
            )?;
            program.runtime_payload_value(
                "record field projection value",
                *ty,
                record.value.project_record_field(field)?,
            )
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
            program.runtime_payload_value(
                "list element projection value",
                *ty,
                list.value.project_list_element(*index, *len)?,
            )
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
            program.runtime_payload_value(
                "list prefix projection value",
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
            program.runtime_payload_value(
                "list rest projection value",
                *ty,
                list.value.project_list_rest(*prefix_len)?,
            )
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
            program.runtime_payload_value(
                "map value projection value",
                *ty,
                map.value.project_map_value(key, keys, *projection)?,
            )
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
            program.runtime_payload_value(
                "map rest projection value",
                *ty,
                map.value.project_map_rest(excluded_keys)?,
            )
        }
        LoadedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires runtime process reference bindings",
        )),
        LoadedValueTemplate::LoopElement { .. } => Err(Error::new(
            "loop element template requires runtime loop element bindings",
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
            program.runtime_payload_value(
                "enum variant template value",
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
            program.runtime_payload_value(
                "record template value",
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
            program.runtime_payload_value("list template value", *ty, RuntimeValue::List(values))
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
            program.runtime_payload_value("map template value", *ty, RuntimeValue::Map(values))
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LoadedTemplateAdmission<'a> {
    pub(super) expected_type: Option<TypeId>,
    pub(super) received_payload_type: Option<TypeId>,
    pub(super) current_state_payload_type: Option<TypeId>,
    pub(super) allow_direct_process_ref: bool,
    pub(super) loop_elements: &'a [LoadedLoopElement],
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
                self.program.validate_value_matches_type(field, *ty, value)
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
                self.validate_enum_payload_projection(field, value.result_type(), *variant, *ty)?;
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
                self.validate_record_field_projection(
                    field,
                    record.result_type(),
                    field_name,
                    *ty,
                )?;
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
                self.validate_list_element_projection(field, list.result_type(), *ty)?;
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
                self.validate_list_element_projection(field, list.result_type(), *ty)?;
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
                self.validate_list_rest_projection(field, list.result_type(), *ty)?;
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
                self.validate_map_value_projection(field, map.result_type(), key, keys, *ty)?;
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
                self.validate_map_rest_projection(field, map.result_type(), excluded_keys, *ty)?;
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
            LoadedValueTemplate::LoopElement { ty, element } => {
                self.validate_loop_element(field, *ty, *element)
            }
            LoadedValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_enum_variant_payload(field, *ty, *variant, payload.result_type())?;
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
                self.validate_record(field, *ty, fields, depth)
            }
            LoadedValueTemplate::List { ty, items } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_list(field, *ty, items, depth)
            }
            LoadedValueTemplate::Map { ty, entries } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_map(field, *ty, entries, depth)
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

    fn validate_enum_payload_projection(
        &self,
        field: &str,
        enum_ty: TypeId,
        variant: EnumVariantId,
        projected_ty: TypeId,
    ) -> Result<()> {
        self.program
            .validate_value_type(&format!("{field}.enum_type"), enum_ty)?;
        let payload_type = self
            .program
            .enum_variant_payload_type(enum_ty, variant)
            .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
        match payload_type {
            Some(expected) if expected == projected_ty => Ok(()),
            Some(expected) => Err(Error::new(format!(
                "{field}.type has type id {}, expected enum payload type id {}",
                projected_ty.as_u32(),
                expected.as_u32()
            ))),
            None => Err(Error::new(format!(
                "{field}.variant_id {} does not carry a payload",
                variant.as_u32()
            ))),
        }
    }

    fn validate_enum_variant_payload(
        &self,
        field: &str,
        enum_ty: TypeId,
        variant: EnumVariantId,
        payload_ty: TypeId,
    ) -> Result<()> {
        self.program
            .validate_value_type(&format!("{field}.enum_type"), enum_ty)?;
        let expected = self
            .program
            .enum_variant_payload_type(enum_ty, variant)
            .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
        match expected {
            Some(expected) if expected == payload_ty => Ok(()),
            Some(expected) => Err(Error::new(format!(
                "{field}.payload has type id {}, expected {}",
                payload_ty.as_u32(),
                expected.as_u32()
            ))),
            None => Err(Error::new(format!(
                "{field}.variant_id {} does not carry a payload",
                variant.as_u32()
            ))),
        }
    }

    fn validate_record_field_projection(
        &self,
        field: &str,
        record_ty: TypeId,
        field_name: &str,
        projected_ty: TypeId,
    ) -> Result<()> {
        let record_type = self.program.type_entry(record_ty)?;
        let ArtifactValueShape::Record { fields } = record_type.value_shape()? else {
            return Err(Error::new(format!(
                "{field}.record type id {} must be a record type",
                record_ty.as_u32()
            )));
        };
        let expected = fields
            .iter()
            .find(|expected| expected.name == field_name)
            .ok_or_else(|| {
                Error::new(format!(
                    "{field}.field_name {field_name} is not declared by type id {}",
                    record_ty.as_u32()
                ))
            })?;
        if expected.ty != projected_ty {
            return Err(Error::new(format!(
                "{field}.type has type id {}, expected record field type id {}",
                projected_ty.as_u32(),
                expected.ty.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_list_element_projection(
        &self,
        field: &str,
        list_ty: TypeId,
        projected_ty: TypeId,
    ) -> Result<()> {
        let list_type = self.program.type_entry(list_ty)?;
        let ArtifactValueShape::List { element, .. } = list_type.value_shape()? else {
            return Err(Error::new(format!(
                "{field}.list type id {} must be a list type",
                list_ty.as_u32()
            )));
        };
        if *element != projected_ty {
            return Err(Error::new(format!(
                "{field}.type has type id {}, expected list element type id {}",
                projected_ty.as_u32(),
                element.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_list_rest_projection(
        &self,
        field: &str,
        list_ty: TypeId,
        projected_ty: TypeId,
    ) -> Result<()> {
        let list_type = self.program.type_entry(list_ty)?;
        let ArtifactValueShape::List { element, .. } = list_type.value_shape()? else {
            return Err(Error::new(format!(
                "{field}.list type id {} must be a list type",
                list_ty.as_u32()
            )));
        };
        let projected_type = self.program.type_entry(projected_ty)?;
        let ArtifactValueShape::List {
            element: projected_element,
            ..
        } = projected_type.value_shape()?
        else {
            return Err(Error::new(format!(
                "{field}.type id {} must be a list type",
                projected_ty.as_u32()
            )));
        };
        if element != projected_element {
            return Err(Error::new(format!(
                "{field}.type has list element type id {}, expected {}",
                projected_element.as_u32(),
                element.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_map_value_projection(
        &self,
        field: &str,
        map_ty: TypeId,
        key: &RuntimeValue,
        keys: &[RuntimeValue],
        projected_ty: TypeId,
    ) -> Result<()> {
        let map_type = self.program.type_entry(map_ty)?;
        let ArtifactValueShape::Map {
            key: key_type,
            value,
            ..
        } = map_type.value_shape()?
        else {
            return Err(Error::new(format!(
                "{field}.map type id {} must be a map type",
                map_ty.as_u32()
            )));
        };
        if *value != projected_ty {
            return Err(Error::new(format!(
                "{field}.type has type id {}, expected map value type id {}",
                projected_ty.as_u32(),
                value.as_u32()
            )));
        }
        self.program
            .validate_value_matches_type(&format!("{field}.key"), *key_type, key)?;
        for (index, expected_key) in keys.iter().enumerate() {
            self.program.validate_value_matches_type(
                &format!("{field}.expected_key.{index}"),
                *key_type,
                expected_key,
            )?;
        }
        Ok(())
    }

    fn validate_map_rest_projection(
        &self,
        field: &str,
        map_ty: TypeId,
        excluded_keys: &[RuntimeValue],
        projected_ty: TypeId,
    ) -> Result<()> {
        let map_type = self.program.type_entry(map_ty)?;
        let ArtifactValueShape::Map { key, value, .. } = map_type.value_shape()? else {
            return Err(Error::new(format!(
                "{field}.map type id {} must be a map type",
                map_ty.as_u32()
            )));
        };
        let projected_type = self.program.type_entry(projected_ty)?;
        let ArtifactValueShape::Map {
            key: projected_key,
            value: projected_value,
            ..
        } = projected_type.value_shape()?
        else {
            return Err(Error::new(format!(
                "{field}.type id {} must be a map type",
                projected_ty.as_u32()
            )));
        };
        if key != projected_key {
            return Err(Error::new(format!(
                "{field}.type has map key type id {}, expected {}",
                projected_key.as_u32(),
                key.as_u32()
            )));
        }
        if value != projected_value {
            return Err(Error::new(format!(
                "{field}.type has map value type id {}, expected {}",
                projected_value.as_u32(),
                value.as_u32()
            )));
        }
        for (index, excluded_key) in excluded_keys.iter().enumerate() {
            self.program.validate_value_matches_type(
                &format!("{field}.excluded_key.{index}"),
                *key,
                excluded_key,
            )?;
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
        if let Some(expected_type) = self.expected_type
            && ty != expected_type
        {
            return Err(Error::new(format!(
                "{field} has type id {}, expected {}",
                ty.as_u32(),
                expected_type.as_u32()
            )));
        }
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

    fn validate_loop_element(&self, field: &str, ty: TypeId, element: LoopElementId) -> Result<()> {
        self.program
            .validate_value_type(&format!("{field}.type"), ty)?;
        let Some(active) = self
            .loop_elements
            .iter()
            .find(|active| active.id == element)
        else {
            return Err(Error::new(format!(
                "{field} references inactive loop element id {}",
                element.as_u32()
            )));
        };
        if active.ty != ty {
            return Err(Error::new(format!(
                "{field} loop element id {} has type id {}, expected {}",
                element.as_u32(),
                active.ty.as_u32(),
                ty.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_record(
        &self,
        field: &str,
        ty: TypeId,
        fields: &[LoadedValueTemplateField],
        depth: usize,
    ) -> Result<()> {
        let type_entry = self.program.type_entry(ty)?;
        let ArtifactValueShape::Record {
            fields: expected_fields,
        } = type_entry.value_shape()?
        else {
            return Err(Error::new(format!(
                "{field}.type id {} must be a record type",
                ty.as_u32()
            )));
        };
        if fields.len() != expected_fields.len() {
            return Err(Error::new(format!(
                "{field}.field_count is {}, expected {}",
                fields.len(),
                expected_fields.len()
            )));
        }
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
            let expected = expected_fields
                .iter()
                .find(|expected| expected.name == record_field.name)
                .ok_or_else(|| {
                    Error::new(format!(
                        "{field}.field {} is not declared by type id {}",
                        record_field.name,
                        ty.as_u32()
                    ))
                })?;
            let nested = Self {
                expected_type: Some(expected.ty),
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
        ty: TypeId,
        items: &[LoadedValueTemplate],
        depth: usize,
    ) -> Result<()> {
        let type_entry = self.program.type_entry(ty)?;
        let ArtifactValueShape::List { element, capacity } = type_entry.value_shape()? else {
            return Err(Error::new(format!(
                "{field}.type id {} must be a list type",
                ty.as_u32()
            )));
        };
        if items.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.item_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        if items.len() > *capacity {
            return Err(Error::new(format!(
                "{field}.item_count is {}, capacity is {}",
                items.len(),
                capacity
            )));
        }
        let nested = Self {
            expected_type: Some(*element),
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
        ty: TypeId,
        entries: &[LoadedValueTemplateMapEntry],
        depth: usize,
    ) -> Result<()> {
        let type_entry = self.program.type_entry(ty)?;
        let ArtifactValueShape::Map {
            key: key_type,
            value: value_type,
            capacity,
        } = type_entry.value_shape()?
        else {
            return Err(Error::new(format!(
                "{field}.type id {} must be a map type",
                ty.as_u32()
            )));
        };
        if entries.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.entry_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        if entries.len() > *capacity {
            return Err(Error::new(format!(
                "{field}.entry_count is {}, capacity is {}",
                entries.len(),
                capacity
            )));
        }
        let mut keys = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            if !loaded_template_is_static_map_key(&entry.key) {
                return Err(Error::new(format!(
                    "{field}.entry.{index}.key must be a static value template"
                )));
            }
            Self {
                expected_type: Some(*key_type),
                allow_direct_process_ref: false,
                ..*self
            }
            .validate_with_depth(
                &format!("{field}.entry.{index}.key"),
                &entry.key,
                depth + 1,
            )?;
            let key = loaded_static_map_key_value(self.program, &entry.key)?;
            if keys.contains(&key) {
                return Err(Error::new(format!(
                    "{field} duplicates key {}",
                    key.label()
                )));
            }
            keys.insert(key);
            Self {
                expected_type: Some(*value_type),
                allow_direct_process_ref: false,
                ..*self
            }
            .validate_with_depth(
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
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. } => false,
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
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. } => false,
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

fn loaded_template_depends_on_loop_element(template: &LoadedValueTemplate) -> bool {
    match template {
        LoadedValueTemplate::LoopElement { .. } => true,
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::ProcessRef { .. } => false,
        LoadedValueTemplate::EnumPayload { value, .. } => {
            loaded_template_depends_on_loop_element(value)
        }
        LoadedValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_loop_element(record)
        }
        LoadedValueTemplate::ListElement { list, .. }
        | LoadedValueTemplate::ListPrefixElement { list, .. }
        | LoadedValueTemplate::ListRest { list, .. } => {
            loaded_template_depends_on_loop_element(list)
        }
        LoadedValueTemplate::MapValue { map, .. } => loaded_template_depends_on_loop_element(map),
        LoadedValueTemplate::MapRest { map, .. } => loaded_template_depends_on_loop_element(map),
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_loop_element(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_loop_element(&field.value)),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().any(loaded_template_depends_on_loop_element)
        }
        LoadedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_loop_element(&entry.key)
                || loaded_template_depends_on_loop_element(&entry.value)
        }),
    }
}
