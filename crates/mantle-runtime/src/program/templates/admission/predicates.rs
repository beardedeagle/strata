use super::*;
use mantle_artifact::ArtifactEnumVariant;

impl LoadedTemplateAdmission<'_> {
    pub(super) fn validate_bool_contract_type(&self, field: &str, ty: TypeId) -> Result<()> {
        let type_entry = self.program.type_entry(ty)?;
        if matches!(type_entry.value_shape(), Ok(shape) if is_bool_contract_shape(shape)) {
            return Ok(());
        }
        Err(Error::new(format!(
            "{field} must have type enum Bool {{ False, True }}"
        )))
    }

    pub(super) fn validate_equality_operand_type(
        &self,
        field: &str,
        operand_ty: TypeId,
    ) -> Result<()> {
        self.validate_equality_operand_type_at_depth(field, operand_ty, 0)
    }

    pub(super) fn validate_equality_operands(
        &self,
        field: &str,
        operand_ty: TypeId,
        left: &LoadedValueTemplate,
        right: &LoadedValueTemplate,
    ) -> Result<EqualityOperandAdmission> {
        let full_error = match self.validate_equality_operand_type(field, operand_ty) {
            Ok(()) => {
                return Ok(EqualityOperandAdmission {
                    allow_process_ref_effect_outcome: false,
                });
            }
            Err(err) => err,
        };
        if self.template_is_builtin_variant_pattern(operand_ty, left)?
            || self.template_is_builtin_variant_pattern(operand_ty, right)?
        {
            Ok(EqualityOperandAdmission {
                allow_process_ref_effect_outcome: true,
            })
        } else {
            Err(full_error)
        }
    }

    fn validate_equality_operand_type_at_depth(
        &self,
        field: &str,
        operand_ty: TypeId,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field}.operand_type_id nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        let type_entry = self.program.type_entry(operand_ty)?;
        match type_entry.kind {
            ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
                "{field}.operand_type_id must be Bool, a scalar value type, or a fieldless enum value type"
            ))),
            ArtifactTypeKind::Value => match type_entry.value_shape()? {
                ArtifactValueShape::Atom if type_entry.label == "Unit" => Ok(()),
                ArtifactValueShape::Scalar { .. } => Ok(()),
                ArtifactValueShape::Enum { variants }
                    if variants
                        .iter()
                        .all(|variant| variant.payload_type.is_none()) =>
                {
                    Ok(())
                }
                ArtifactValueShape::Enum { variants }
                    if is_recognized_builtin_equality_enum(variants) =>
                {
                    for variant in variants {
                        if let Some(payload_ty) = variant.payload_type {
                            self.validate_equality_operand_type_at_depth(
                                field,
                                payload_ty,
                                depth + 1,
                            )?;
                        }
                    }
                    Ok(())
                }
                _ => Err(Error::new(format!(
                    "{field}.operand_type_id must be Bool, a scalar value type, or a fieldless enum value type"
                ))),
            },
        }
    }

    pub(super) fn validate_scalar_value_type(&self, field: &str, ty: TypeId) -> Result<()> {
        let type_entry = self.program.type_entry(ty)?;
        match &type_entry.kind {
            ArtifactTypeKind::Value => match type_entry.value_shape()? {
                ArtifactValueShape::Scalar { .. } => Ok(()),
                _ => Err(Error::new(format!("{field} must be a scalar value type"))),
            },
            _ => Err(Error::new(format!("{field} must be a scalar value type"))),
        }
    }

    pub(super) fn validate_equality_operand_template(
        &self,
        field: &str,
        side: &str,
        operand_ty: TypeId,
        operand: &LoadedValueTemplate,
    ) -> Result<()> {
        if operand.result_type() != operand_ty {
            return Err(Error::new(format!(
                "{field}.{side} has type id {}, expected {}",
                operand.result_type().as_u32(),
                operand_ty.as_u32()
            )));
        }
        Ok(())
    }

    pub(super) fn validate_boolean_operand_template(
        &self,
        field: &str,
        side: &str,
        bool_ty: TypeId,
        operand: &LoadedValueTemplate,
    ) -> Result<()> {
        if operand.result_type() != bool_ty {
            return Err(Error::new(format!(
                "{field}.{side} has type id {}, expected {}",
                operand.result_type().as_u32(),
                bool_ty.as_u32()
            )));
        }
        Ok(())
    }

    fn template_is_builtin_variant_pattern(
        &self,
        operand_ty: TypeId,
        template: &LoadedValueTemplate,
    ) -> Result<bool> {
        match template {
            LoadedValueTemplate::Literal { ty, value } if *ty == operand_ty => {
                self.value_is_builtin_variant_pattern(operand_ty, value)
            }
            LoadedValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } if *ty == operand_ty => {
                let type_entry = self.program.type_entry(operand_ty)?;
                let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
                    return Ok(false);
                };
                if !is_recognized_builtin_equality_enum(variants) {
                    return Ok(false);
                }
                let Some(variant_entry) = variants.get(variant.index()) else {
                    return Ok(false);
                };
                let Some(payload_ty) = variant_entry.payload_type else {
                    return Ok(false);
                };
                self.payload_template_pattern_is_safe(payload_ty, payload)
            }
            _ => Ok(false),
        }
    }

    fn value_is_builtin_variant_pattern(
        &self,
        operand_ty: TypeId,
        value: &RuntimeValue,
    ) -> Result<bool> {
        let type_entry = self.program.type_entry(operand_ty)?;
        let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
            return Ok(false);
        };
        if !is_recognized_builtin_equality_enum(variants) {
            return Ok(false);
        }
        match value {
            RuntimeValue::Atom(label) => Ok(variants
                .iter()
                .any(|variant| variant.label == *label && variant.payload_type.is_none())),
            RuntimeValue::EnumVariant { variant, payload } => {
                let Some(variant_entry) = variants.iter().find(|entry| entry.label == *variant)
                else {
                    return Ok(false);
                };
                let Some(payload_ty) = variant_entry.payload_type else {
                    return Ok(false);
                };
                self.payload_value_pattern_is_safe(payload_ty, payload)
            }
            _ => Ok(false),
        }
    }

    fn payload_template_pattern_is_safe(
        &self,
        payload_ty: TypeId,
        payload: &LoadedValueTemplate,
    ) -> Result<bool> {
        if self
            .validate_equality_operand_type("equality payload", payload_ty)
            .is_ok()
        {
            return Ok(true);
        }
        self.template_is_builtin_variant_pattern(payload_ty, payload)
    }

    fn payload_value_pattern_is_safe(
        &self,
        payload_ty: TypeId,
        payload: &RuntimeValue,
    ) -> Result<bool> {
        if self
            .validate_equality_operand_type("equality payload", payload_ty)
            .is_ok()
        {
            return Ok(true);
        }
        self.value_is_builtin_variant_pattern(payload_ty, payload)
    }
}

pub(super) struct EqualityOperandAdmission {
    pub(super) allow_process_ref_effect_outcome: bool,
}

fn is_recognized_builtin_equality_enum(variants: &[ArtifactEnumVariant]) -> bool {
    match variants {
        [none, some] => {
            none.label == "None"
                && none.payload_type.is_none()
                && some.label == "Some"
                && some.payload_type.is_some()
                || none.label == "Ok"
                    && none.payload_type.is_some()
                    && some.label == "Err"
                    && some.payload_type.is_some()
        }
        [first, second, third] => {
            first.payload_type.is_some()
                && second.payload_type.is_some()
                && third.payload_type.is_some()
                && ((first.label == "Full"
                    && second.label == "Stopped"
                    && third.label == "Crashed")
                    || (first.label == "Denied"
                        && second.label == "Exhausted"
                        && third.label == "BackendUnavailable"))
        }
        _ => false,
    }
}

fn is_bool_contract_shape(shape: &ArtifactValueShape) -> bool {
    matches!(
        shape,
        ArtifactValueShape::Enum { variants }
            if variants.len() == 2
                && variants[0].label == "False"
                && variants[0].payload_type.is_none()
                && variants[1].label == "True"
                && variants[1].payload_type.is_none()
    )
}
