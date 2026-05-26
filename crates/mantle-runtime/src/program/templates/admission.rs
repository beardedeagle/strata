use super::support::*;
use static_keys::{loaded_static_map_key_value, loaded_template_is_static_map_key};

mod predicates;
mod projections;
mod static_keys;

pub(in crate::program) struct LoadedTemplateAdmission<'a> {
    pub(in crate::program) expected_type: Option<TypeId>,
    pub(in crate::program) received_payload_type: Option<TypeId>,
    pub(in crate::program) current_state_payload_type: Option<TypeId>,
    pub(in crate::program) allow_direct_process_ref: bool,
    pub(in crate::program) allow_process_ref_effect_outcome: bool,
    pub(in crate::program) loop_elements: &'a [LoadedLoopElement],
    pub(in crate::program) effect_outcomes: &'a [(EffectOutcomeId, TypeId)],
    pub(in crate::program) program: &'a LoadedProgram,
    pub(in crate::program) process: &'a LoadedProcess,
    pub(in crate::program) spawned_refs: &'a [bool],
}

impl LoadedTemplateAdmission<'_> {
    pub(in crate::program) fn validate(
        &self,
        field: &str,
        template: &LoadedValueTemplate,
    ) -> Result<()> {
        self.validate_with_depth(field, template, 0)
    }

    fn nested(&self) -> Self {
        Self {
            expected_type: None,
            allow_direct_process_ref: false,
            allow_process_ref_effect_outcome: false,
            ..*self
        }
    }

    fn with_expected_type(self, expected_type: Option<TypeId>) -> Self {
        Self {
            expected_type,
            ..self
        }
    }

    fn with_process_ref_effect_outcome(self, allowed: bool) -> Self {
        Self {
            allow_process_ref_effect_outcome: allowed,
            ..self
        }
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
                let nested = self.nested();
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
                let nested = self.nested();
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
                let nested = self.nested();
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
                let nested = self.nested();
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
                let nested = self.nested();
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
                let nested = self.nested();
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
                let nested = self.nested();
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
            LoadedValueTemplate::EffectOutcome { ty, outcome } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                if !self.allow_process_ref_effect_outcome {
                    self.reject_type_containing_process_ref(field, *ty)?;
                }
                self.validate_effect_outcome(field, *ty, *outcome)
            }
            LoadedValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_enum_variant_payload(field, *ty, *variant, payload.result_type())?;
                let nested = self.nested();
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
            LoadedValueTemplate::IfElse {
                ty,
                condition,
                then_value,
                else_value,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                let bool_ty = condition.result_type();
                self.validate_bool_contract_type(&format!("{field}.condition.type"), bool_ty)?;
                let condition_nested = self.nested().with_expected_type(Some(bool_ty));
                condition_nested.validate_with_depth(
                    &format!("{field}.condition"),
                    condition,
                    depth + 1,
                )?;
                let branch_nested = self.nested().with_expected_type(Some(*ty));
                branch_nested.validate_with_depth(
                    &format!("{field}.then"),
                    then_value,
                    depth + 1,
                )?;
                branch_nested.validate_with_depth(&format!("{field}.else"), else_value, depth + 1)
            }
            LoadedValueTemplate::Equality {
                ty,
                operand_ty,
                left,
                right,
                ..
            } => {
                self.validate_bool_contract_type(&format!("{field}.type"), *ty)?;
                let equality_admission =
                    self.validate_equality_operands(field, *operand_ty, left, right)?;
                self.validate_equality_operand_template(field, "left", *operand_ty, left)?;
                self.validate_equality_operand_template(field, "right", *operand_ty, right)?;
                let nested = self
                    .nested()
                    .with_expected_type(Some(*operand_ty))
                    .with_process_ref_effect_outcome(
                        equality_admission.allow_process_ref_effect_outcome,
                    );
                nested.validate_with_depth(&format!("{field}.left"), left, depth + 1)?;
                nested.validate_with_depth(&format!("{field}.right"), right, depth + 1)
            }
            LoadedValueTemplate::ScalarArithmetic {
                ty, left, right, ..
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_scalar_value_type(&format!("{field}.type"), *ty)?;
                self.validate_equality_operand_template(field, "left", *ty, left)?;
                self.validate_equality_operand_template(field, "right", *ty, right)?;
                let nested = self.nested().with_expected_type(Some(*ty));
                nested.validate_with_depth(&format!("{field}.left"), left, depth + 1)?;
                nested.validate_with_depth(&format!("{field}.right"), right, depth + 1)
            }
            LoadedValueTemplate::ScalarOrdering {
                ty,
                operand_ty,
                left,
                right,
                ..
            } => {
                self.validate_bool_contract_type(&format!("{field}.type"), *ty)?;
                self.validate_scalar_value_type(&format!("{field}.operand_type_id"), *operand_ty)?;
                self.validate_equality_operand_template(field, "left", *operand_ty, left)?;
                self.validate_equality_operand_template(field, "right", *operand_ty, right)?;
                let nested = self.nested().with_expected_type(Some(*operand_ty));
                nested.validate_with_depth(&format!("{field}.left"), left, depth + 1)?;
                nested.validate_with_depth(&format!("{field}.right"), right, depth + 1)
            }
            LoadedValueTemplate::BooleanNot { ty, operand } => {
                self.validate_bool_contract_type(&format!("{field}.type"), *ty)?;
                self.validate_boolean_operand_template(field, "operand", *ty, operand)?;
                let nested = self.nested().with_expected_type(Some(*ty));
                nested.validate_with_depth(&format!("{field}.operand"), operand, depth + 1)
            }
            LoadedValueTemplate::BooleanBinary {
                ty, left, right, ..
            } => {
                self.validate_bool_contract_type(&format!("{field}.type"), *ty)?;
                self.validate_boolean_operand_template(field, "left", *ty, left)?;
                self.validate_boolean_operand_template(field, "right", *ty, right)?;
                let nested = self.nested().with_expected_type(Some(*ty));
                nested.validate_with_depth(&format!("{field}.left"), left, depth + 1)?;
                nested.validate_with_depth(&format!("{field}.right"), right, depth + 1)
            }
        }
    }

    fn validate_effect_outcome(
        &self,
        field: &str,
        ty: TypeId,
        outcome: EffectOutcomeId,
    ) -> Result<()> {
        let Some((_, expected_ty)) = self.effect_outcomes.iter().find(|(id, _)| *id == outcome)
        else {
            return Err(Error::new(format!(
                "{field} references unbound effect outcome id {}",
                outcome.as_u32()
            )));
        };
        if *expected_ty != ty {
            return Err(Error::new(format!(
                "{field} effect outcome id {} has type id {}, expected {}",
                outcome.as_u32(),
                ty.as_u32(),
                expected_ty.as_u32()
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
        for (index, record_field) in fields.iter().enumerate() {
            validate_loaded_ident_field(&format!("{field}.field"), &record_field.name)?;
            if fields[..index]
                .iter()
                .any(|previous| previous.name == record_field.name)
            {
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
            let nested = self.nested().with_expected_type(Some(expected.ty));
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
        let nested = self.nested().with_expected_type(Some(*element));
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
        let mut keys = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            if !loaded_template_is_static_map_key(&entry.key) {
                return Err(Error::new(format!(
                    "{field}.entry.{index}.key must be a static value template"
                )));
            }
            self.nested()
                .with_expected_type(Some(*key_type))
                .validate_with_depth(
                    &format!("{field}.entry.{index}.key"),
                    &entry.key,
                    depth + 1,
                )?;
            let key = loaded_static_map_key_value(self.program, &entry.key)?;
            if keys.iter().any(|previous| previous == &key) {
                return Err(Error::new(format!(
                    "{field} duplicates key {}",
                    key.label()
                )));
            }
            keys.push(key);
            self.nested()
                .with_expected_type(Some(*value_type))
                .validate_with_depth(
                    &format!("{field}.entry.{index}.value"),
                    &entry.value,
                    depth + 1,
                )?;
        }
        Ok(())
    }
}
