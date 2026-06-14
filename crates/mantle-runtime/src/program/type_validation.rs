use super::*;

impl LoadedProgram {
    pub(in crate::program) fn validate_type_shape(
        &self,
        type_index: usize,
        ty: &ArtifactType,
    ) -> Result<()> {
        match ty.kind {
            ArtifactTypeKind::Value => {
                let Some(shape) = &ty.shape else {
                    return Err(Error::new(format!(
                        "loaded type.{type_index} value type must declare a value shape"
                    )));
                };
                self.validate_value_shape(type_index, shape)
            }
            ArtifactTypeKind::ProcessRef { target } => {
                if ty.shape.is_some() {
                    return Err(Error::new(format!(
                        "loaded type.{type_index} process reference type must not declare a value shape"
                    )));
                }
                self.process(target)?;
                Ok(())
            }
        }
    }

    fn validate_value_shape(&self, type_index: usize, shape: &ArtifactValueShape) -> Result<()> {
        match shape {
            ArtifactValueShape::Atom => Ok(()),
            ArtifactValueShape::Primitive { .. } => Ok(()),
            ArtifactValueShape::Scalar { .. } => Ok(()),
            ArtifactValueShape::Record { fields } => {
                if fields.is_empty() || fields.len() > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.field_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                let mut seen = BTreeSet::new();
                for (field_index, field) in fields.iter().enumerate() {
                    validate_loaded_ident_field(
                        &format!("loaded type.{type_index}.field.{field_index}.name"),
                        &field.name,
                    )?;
                    if !seen.insert(field.name.as_str()) {
                        return Err(Error::new(format!(
                            "loaded type.{type_index} duplicates field {}",
                            field.name
                        )));
                    }
                    self.validate_value_type(
                        &format!("loaded type.{type_index}.field.{field_index}.type_id"),
                        field.ty,
                    )?;
                }
                Ok(())
            }
            ArtifactValueShape::Enum { variants } => {
                if variants.is_empty() || variants.len() > MAX_ENUM_VARIANTS_PER_TYPE {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.enum_variant_count must be between 1 and {MAX_ENUM_VARIANTS_PER_TYPE}"
                    )));
                }
                let mut seen = BTreeSet::new();
                for (variant_index, variant) in variants.iter().enumerate() {
                    validate_loaded_ident_field(
                        &format!("loaded type.{type_index}.enum_variant.{variant_index}"),
                        &variant.label,
                    )?;
                    if variant.payload_type.is_some() {
                        validate_loaded_payload_enum_variant_label(type_index, &variant.label)?;
                    }
                    if !seen.insert(variant.label.as_str()) {
                        return Err(Error::new(format!(
                            "loaded type.{type_index} duplicates enum variant {}",
                            variant.label
                        )));
                    }
                    if let Some(payload_type) = variant.payload_type {
                        self.type_entry(payload_type)?;
                    }
                }
                Ok(())
            }
            ArtifactValueShape::List { element, capacity } => {
                if *capacity > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                self.validate_value_type(
                    &format!("loaded type.{type_index}.element_type_id"),
                    *element,
                )
            }
            ArtifactValueShape::Map {
                key,
                value,
                capacity,
            } => {
                if *capacity > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                self.validate_value_type(&format!("loaded type.{type_index}.key_type_id"), *key)?;
                self.validate_value_type(&format!("loaded type.{type_index}.value_type_id"), *value)
            }
        }
    }

    pub(in crate::program) fn validate_value_matches_type_at_depth(
        &self,
        field: &str,
        ty: TypeId,
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum typed value depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        let type_entry = self.type_entry(ty)?;
        self.validate_value_type(field, ty)?;
        values::validate_non_process_ref_value(field, value)?;
        match type_entry.value_shape()? {
            ArtifactValueShape::Atom => self.validate_atom_value(field, ty, type_entry, value),
            ArtifactValueShape::Primitive { primitive } => {
                self.validate_primitive_value(field, ty, type_entry, *primitive, value)
            }
            ArtifactValueShape::Scalar { scalar } => {
                self.validate_scalar_value(field, ty, type_entry, *scalar, value)
            }
            ArtifactValueShape::Enum { variants } => {
                self.validate_enum_value(field, ty, type_entry, variants, value, depth)
            }
            ArtifactValueShape::Record { fields } => {
                self.validate_record_value(field, ty, type_entry, fields, value, depth)
            }
            ArtifactValueShape::List { element, capacity } => {
                self.validate_list_value(field, *element, *capacity, value, depth)
            }
            ArtifactValueShape::Map {
                key,
                value: item,
                capacity,
            } => self.validate_map_value(field, *key, *item, *capacity, value, depth),
        }
    }

    fn validate_atom_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        value: &RuntimeValue,
    ) -> Result<()> {
        if matches!(value, RuntimeValue::Atom(_)) {
            return Ok(());
        }
        Err(Error::new(format!(
            "{field} value {} does not match atom type {} (type id {})",
            value.label(),
            type_entry.label,
            ty.as_u32()
        )))
    }

    fn validate_primitive_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        expected: ArtifactPrimitiveType,
        value: &RuntimeValue,
    ) -> Result<()> {
        match (expected, value) {
            (ArtifactPrimitiveType::String, RuntimeValue::String(_))
            | (ArtifactPrimitiveType::Bytes, RuntimeValue::Bytes(_)) => Ok(()),
            _ => Err(Error::new(format!(
                "{field} value {} does not match primitive type {} (type id {})",
                value.label(),
                type_entry.label,
                ty.as_u32()
            ))),
        }
    }

    fn validate_scalar_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        expected: ArtifactScalarType,
        value: &RuntimeValue,
    ) -> Result<()> {
        let RuntimeValue::Scalar(value) = value else {
            return Err(Error::new(format!(
                "{field} value {} does not match scalar type {} (type id {})",
                value.label(),
                type_entry.label,
                ty.as_u32()
            )));
        };
        if value.ty() != expected {
            return Err(Error::new(format!(
                "{field} scalar value {} does not match scalar type {} (type id {})",
                value.label(),
                type_entry.label,
                ty.as_u32()
            )));
        }
        expected.validate_value(field, value.value())
    }

    fn validate_enum_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        variants: &[ArtifactEnumVariant],
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        match value {
            RuntimeValue::Atom(label) => {
                let Some(variant) = variants.iter().find(|variant| variant.label == *label) else {
                    return Err(runtime_value_not_member_error(field, ty, type_entry, value));
                };
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "{field} enum variant {label} requires a payload"
                    )));
                }
                Ok(())
            }
            RuntimeValue::EnumVariant { variant, payload } => {
                let Some(entry) = variants.iter().find(|entry| entry.label == *variant) else {
                    return Err(runtime_value_not_member_error(field, ty, type_entry, value));
                };
                let Some(payload_type) = entry.payload_type else {
                    return Err(Error::new(format!(
                        "{field} enum variant {variant} must not carry a payload"
                    )));
                };
                self.validate_value_matches_type_at_depth(
                    &format!("{field}.payload"),
                    payload_type,
                    payload,
                    depth + 1,
                )
            }
            RuntimeValue::Record { .. }
            | RuntimeValue::List(_)
            | RuntimeValue::Map(_)
            | RuntimeValue::String(_)
            | RuntimeValue::Bytes(_)
            | RuntimeValue::Scalar(_)
            | RuntimeValue::ProcessRef { .. } => {
                Err(runtime_value_not_member_error(field, ty, type_entry, value))
            }
        }
    }

    fn validate_record_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        expected_fields: &[ArtifactTypeField],
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::Record {
            constructor,
            fields,
        } = value
        else {
            return Err(Error::new(format!(
                "{field} value {} does not match record type {} (type id {})",
                value.label(),
                type_entry.label,
                ty.as_u32()
            )));
        };
        if constructor != &type_entry.label {
            return Err(Error::new(format!(
                "{field} record constructor {constructor} does not match type {} (type id {})",
                type_entry.label,
                ty.as_u32()
            )));
        }
        if fields.len() != expected_fields.len() {
            return Err(Error::new(format!(
                "{field} record field_count is {}, expected {} for type {} (type id {})",
                fields.len(),
                expected_fields.len(),
                type_entry.label,
                ty.as_u32()
            )));
        }
        let mut seen = BTreeSet::new();
        for actual in fields {
            if !seen.insert(actual.name.as_str()) {
                return Err(Error::new(format!(
                    "{field} record duplicates field {}",
                    actual.name
                )));
            }
            let Some(expected) = expected_fields
                .iter()
                .find(|expected| expected.name == actual.name)
            else {
                return Err(Error::new(format!(
                    "{field} record field {} is not declared by type {} (type id {})",
                    actual.name,
                    type_entry.label,
                    ty.as_u32()
                )));
            };
            self.validate_value_matches_type_at_depth(
                &format!("{field}.field.{}", actual.name),
                expected.ty,
                &actual.value,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn validate_list_value(
        &self,
        field: &str,
        element: TypeId,
        capacity: usize,
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::List(items) = value else {
            return Err(Error::new(format!(
                "{field} value {} does not match list type",
                value.label()
            )));
        };
        if items.len() > capacity {
            return Err(Error::new(format!(
                "{field} list item_count is {}, capacity is {}",
                items.len(),
                capacity
            )));
        }
        for (index, item) in items.iter().enumerate() {
            self.validate_value_matches_type_at_depth(
                &format!("{field}.item.{index}"),
                element,
                item,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn validate_map_value(
        &self,
        field: &str,
        key: TypeId,
        item: TypeId,
        capacity: usize,
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::Map(entries) = value else {
            return Err(Error::new(format!(
                "{field} value {} does not match map type",
                value.label()
            )));
        };
        if entries.len() > capacity {
            return Err(Error::new(format!(
                "{field} map entry_count is {}, capacity is {}",
                entries.len(),
                capacity
            )));
        }
        let mut seen = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            self.validate_value_matches_type_at_depth(
                &format!("{field}.entry.{index}.key"),
                key,
                &entry.key,
                depth + 1,
            )?;
            if !seen.insert(&entry.key) {
                return Err(Error::new(format!(
                    "{field} duplicates map key {}",
                    entry.key.label()
                )));
            }
            self.validate_value_matches_type_at_depth(
                &format!("{field}.entry.{index}.value"),
                item,
                &entry.value,
                depth + 1,
            )?;
        }
        Ok(())
    }
}

fn validate_loaded_payload_enum_variant_label(type_index: usize, variant: &str) -> Result<()> {
    for primitive in ArtifactPrimitiveType::ALL {
        if variant == primitive.source_name() {
            return Err(Error::new(format!(
                "loaded type.{type_index} payload-bearing enum variant {variant} collides with reserved primitive value label"
            )));
        }
    }
    Ok(())
}

fn runtime_value_not_member_error(
    field: &str,
    ty: TypeId,
    type_entry: &ArtifactType,
    value: &RuntimeValue,
) -> Error {
    Error::new(format!(
        "{field} value {} is not a member of enum type {} (type id {})",
        value.label(),
        type_entry.label,
        ty.as_u32()
    ))
}
