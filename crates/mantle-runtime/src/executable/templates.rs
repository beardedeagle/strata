use mantle_artifact::{
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactTypeKind,
    ArtifactValueBooleanOperator, ArtifactValueEqualityOperator, EffectOutcomeId, EnumVariantId,
    Error, LoopElementId, MapProjectionMode, ProcessId, ProcessRefId, RecordFieldId, Result,
    StateId, TypeId,
};

use super::compact::{CompactList, CompactListBuilder};
use super::counts::count_loaded_program_templates;
use super::executable_process_ref;
use crate::program::{
    LoadedNextState, LoadedProcess, LoadedProgram, LoadedValueTemplate, LoadedValueTemplateField,
    LoadedValueTemplateMapEntry, RuntimeValue,
};

mod scope;
pub(super) use scope::ExecutableTemplateScope;

#[derive(Debug)]
pub(crate) struct ExecutableTemplateProgram<'program> {
    values: CompactList<ExecutableValueTemplate<'program>>,
}

impl<'program> ExecutableTemplateProgram<'program> {
    pub(crate) fn get(
        &self,
        template: ExecutableValueTemplateRef,
    ) -> Result<&ExecutableValueTemplate<'program>> {
        self.values.get(template.index()).ok_or_else(|| {
            Error::new(format!(
                "executable value template ref {} is not loaded",
                template.as_u32()
            ))
        })
    }

    pub(crate) fn result_type(&self, template: ExecutableValueTemplateRef) -> Result<TypeId> {
        Ok(self.get(template)?.result_type())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.values.as_slice().len()
    }
}

#[derive(Debug)]
pub(super) struct ExecutableTemplateProgramBuilder<'program> {
    program: &'program LoadedProgram,
    values: CompactListBuilder<ExecutableValueTemplate<'program>>,
    value_count: usize,
}

impl<'program> ExecutableTemplateProgramBuilder<'program> {
    pub(super) fn new(program: &'program LoadedProgram) -> Self {
        Self {
            program,
            values: CompactListBuilder::with_expected_len(count_loaded_program_templates(program)),
            value_count: 0,
        }
    }

    pub(super) fn append(
        &mut self,
        process: &'program LoadedProcess,
        template: &'program LoadedValueTemplate,
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<ExecutableValueTemplateRef> {
        let value = self.compile_value(process, template, scope)?;
        let index = self.value_count;
        let reference = ExecutableValueTemplateRef::from_index(index)?;
        self.value_count = self
            .value_count
            .checked_add(1)
            .ok_or_else(|| Error::new("executable value template count overflowed"))?;
        self.values.push(value);
        Ok(reference)
    }

    pub(super) fn finish(self) -> ExecutableTemplateProgram<'program> {
        ExecutableTemplateProgram {
            values: self.values.finish(),
        }
    }

    fn compile_value(
        &mut self,
        process: &'program LoadedProcess,
        template: &'program LoadedValueTemplate,
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<ExecutableValueTemplate<'program>> {
        self.program.type_entry(template.result_type())?;
        match template {
            LoadedValueTemplate::Literal { ty, value } => {
                Ok(ExecutableValueTemplate::Literal { ty: *ty, value })
            }
            LoadedValueTemplate::ReceivedPayload { ty } => {
                scope.validate_received_payload(*ty)?;
                self.reject_indirect_process_ref_payload(
                    scope,
                    *ty,
                    "executable received payload template",
                )?;
                Ok(ExecutableValueTemplate::ReceivedPayload { ty: *ty })
            }
            LoadedValueTemplate::CurrentStatePayload { ty } => {
                scope.validate_current_state_payload(*ty)?;
                Ok(ExecutableValueTemplate::CurrentStatePayload { ty: *ty })
            }
            LoadedValueTemplate::EnumPayload { ty, value, variant } => {
                self.reject_projected_process_ref_type(
                    *ty,
                    "executable enum payload projection template",
                )?;
                let value_ref = self.append(process, value, scope.nested())?;
                let source_ty = self.value_type(value_ref)?;
                self.program.enum_variant_label(source_ty, *variant)?;
                Ok(ExecutableValueTemplate::EnumPayload {
                    ty: *ty,
                    value: value_ref,
                    variant: *variant,
                })
            }
            LoadedValueTemplate::RecordField { ty, record, field } => {
                self.reject_projected_process_ref_type(
                    *ty,
                    "executable record field projection template",
                )?;
                let record_ref = self.append(process, record, scope.nested())?;
                let record_ty = self.value_type(record_ref)?;
                self.program.record_field(record_ty, *field)?;
                Ok(ExecutableValueTemplate::RecordField {
                    ty: *ty,
                    record: record_ref,
                    field: *field,
                })
            }
            LoadedValueTemplate::ListElement {
                ty,
                list,
                index,
                len,
            } => Ok(ExecutableValueTemplate::ListElement {
                ty: *ty,
                list: {
                    self.reject_projected_process_ref_type(
                        *ty,
                        "executable list element projection template",
                    )?;
                    self.append(process, list, scope.nested())?
                },
                index: *index,
                len: *len,
            }),
            LoadedValueTemplate::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => Ok(ExecutableValueTemplate::ListPrefixElement {
                ty: *ty,
                list: {
                    self.reject_projected_process_ref_type(
                        *ty,
                        "executable list prefix projection template",
                    )?;
                    self.append(process, list, scope.nested())?
                },
                index: *index,
                prefix_len: *prefix_len,
            }),
            LoadedValueTemplate::ListRest {
                ty,
                list,
                prefix_len,
            } => Ok(ExecutableValueTemplate::ListRest {
                ty: *ty,
                list: {
                    self.reject_projected_process_ref_type(
                        *ty,
                        "executable list rest projection template",
                    )?;
                    self.append(process, list, scope.nested())?
                },
                prefix_len: *prefix_len,
            }),
            LoadedValueTemplate::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => Ok(ExecutableValueTemplate::MapValue {
                ty: *ty,
                map: {
                    self.reject_projected_process_ref_type(
                        *ty,
                        "executable map value projection template",
                    )?;
                    self.append(process, map, scope.nested())?
                },
                key,
                keys,
                projection: *projection,
            }),
            LoadedValueTemplate::MapRest {
                ty,
                map,
                excluded_keys,
            } => Ok(ExecutableValueTemplate::MapRest {
                ty: *ty,
                map: {
                    self.reject_projected_process_ref_type(
                        *ty,
                        "executable map rest projection template",
                    )?;
                    self.append(process, map, scope.nested())?
                },
                excluded_keys,
            }),
            LoadedValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => {
                self.program.validate_process_ref_type_id_target(
                    "executable process reference template type",
                    *ty,
                    *target_process,
                )?;
                let executable_ref = executable_process_ref(process, *process_ref)?;
                if executable_ref.target_process != *target_process {
                    return Err(Error::new(format!(
                        "process {} executable process reference template id {} targets process id {}, expected {}",
                        process.debug_name,
                        process_ref.as_u32(),
                        executable_ref.target_process.as_u32(),
                        target_process.as_u32()
                    )));
                }
                scope.validate_process_ref(process, *process_ref)?;
                Ok(ExecutableValueTemplate::ProcessRef {
                    ty: *ty,
                    target_process: *target_process,
                    process_ref: *process_ref,
                })
            }
            LoadedValueTemplate::LoopElement { ty, element } => {
                self.program
                    .validate_value_type("executable loop element type", *ty)?;
                scope.validate_loop_element(*ty, *element)?;
                Ok(ExecutableValueTemplate::LoopElement {
                    ty: *ty,
                    element: *element,
                })
            }
            LoadedValueTemplate::EffectOutcome { ty, outcome } => {
                self.program
                    .validate_value_type("executable effect outcome type", *ty)?;
                scope.validate_effect_outcome(*ty, *outcome)?;
                Ok(ExecutableValueTemplate::EffectOutcome {
                    ty: *ty,
                    outcome: *outcome,
                })
            }
            LoadedValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => Ok(ExecutableValueTemplate::EnumVariant {
                ty: *ty,
                variant: *variant,
                payload: self.append(process, payload, scope.nested())?,
            }),
            LoadedValueTemplate::Record { ty, fields } => Ok(ExecutableValueTemplate::Record {
                ty: *ty,
                fields: self.compile_fields(process, *ty, fields, scope.nested())?,
            }),
            LoadedValueTemplate::List { ty, items } => Ok(ExecutableValueTemplate::List {
                ty: *ty,
                items: self.compile_values(process, items, scope.nested())?,
            }),
            LoadedValueTemplate::Map { ty, entries } => Ok(ExecutableValueTemplate::Map {
                ty: *ty,
                entries: self.compile_entries(process, entries, scope.nested())?,
            }),
            LoadedValueTemplate::IfElse {
                ty,
                condition,
                then_value,
                else_value,
            } => Ok(ExecutableValueTemplate::IfElse {
                ty: *ty,
                condition: self.append(process, condition, scope.nested())?,
                then_value: self.append(process, then_value, scope.nested())?,
                else_value: self.append(process, else_value, scope.nested())?,
            }),
            LoadedValueTemplate::Equality {
                ty,
                operand_ty,
                operator,
                left,
                right,
            } => Ok(ExecutableValueTemplate::Equality {
                ty: *ty,
                operand_ty: *operand_ty,
                operator: *operator,
                left: self.append(process, left, scope.nested())?,
                right: self.append(process, right, scope.nested())?,
            }),
            LoadedValueTemplate::ScalarArithmetic {
                ty,
                operator,
                left,
                right,
            } => Ok(ExecutableValueTemplate::ScalarArithmetic {
                ty: *ty,
                operator: *operator,
                left: self.append(process, left, scope.nested())?,
                right: self.append(process, right, scope.nested())?,
            }),
            LoadedValueTemplate::ScalarOrdering {
                ty,
                operand_ty,
                operator,
                left,
                right,
            } => Ok(ExecutableValueTemplate::ScalarOrdering {
                ty: *ty,
                operand_ty: *operand_ty,
                operator: *operator,
                left: self.append(process, left, scope.nested())?,
                right: self.append(process, right, scope.nested())?,
            }),
            LoadedValueTemplate::BooleanNot { ty, operand } => {
                Ok(ExecutableValueTemplate::BooleanNot {
                    ty: *ty,
                    operand: self.append(process, operand, scope.nested())?,
                })
            }
            LoadedValueTemplate::BooleanBinary {
                ty,
                operator,
                left,
                right,
            } => Ok(ExecutableValueTemplate::BooleanBinary {
                ty: *ty,
                operator: *operator,
                left: self.append(process, left, scope.nested())?,
                right: self.append(process, right, scope.nested())?,
            }),
        }
    }

    fn compile_fields(
        &mut self,
        process: &'program LoadedProcess,
        ty: TypeId,
        fields: &'program [LoadedValueTemplateField],
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<Box<[ExecutableValueTemplateField]>> {
        fields
            .iter()
            .map(|field| {
                self.program.record_field(ty, field.field)?;
                Ok(ExecutableValueTemplateField {
                    field: field.field,
                    value: self.append(process, &field.value, scope)?,
                })
            })
            .collect()
    }

    fn compile_values(
        &mut self,
        process: &'program LoadedProcess,
        values: &'program [LoadedValueTemplate],
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<Box<[ExecutableValueTemplateRef]>> {
        values
            .iter()
            .map(|value| self.append(process, value, scope))
            .collect()
    }

    fn compile_entries(
        &mut self,
        process: &'program LoadedProcess,
        entries: &'program [LoadedValueTemplateMapEntry],
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<Box<[ExecutableValueTemplateMapEntry]>> {
        entries
            .iter()
            .map(|entry| {
                Ok(ExecutableValueTemplateMapEntry {
                    key: self.append(process, &entry.key, scope)?,
                    value: self.append(process, &entry.value, scope)?,
                })
            })
            .collect()
    }

    fn reject_indirect_process_ref_payload(
        &self,
        scope: ExecutableTemplateScope<'_>,
        ty: TypeId,
        field: &str,
    ) -> Result<()> {
        if !scope.allows_direct_process_ref()
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

    fn reject_projected_process_ref_type(&self, ty: TypeId, field: &str) -> Result<()> {
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

    fn value_type(&self, template: ExecutableValueTemplateRef) -> Result<TypeId> {
        let Some(value) = self.values.get(template.index()) else {
            return Err(Error::new(format!(
                "executable value template ref {} was not constructed",
                template.as_u32()
            )));
        };
        Ok(value.result_type())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ExecutableValueTemplateRef(u32);

impl ExecutableValueTemplateRef {
    fn from_index(index: usize) -> Result<Self> {
        let id = u32::try_from(index).map_err(|_| {
            Error::new(format!(
                "executable value template index {index} exceeds u32"
            ))
        })?;
        Ok(Self(id))
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutableNextState {
    Current,
    Value(StateId),
    Template(ExecutableValueTemplateRef),
    IfElse {
        condition: ExecutableValueTemplateRef,
        then_state: Box<ExecutableNextState>,
        else_state: Box<ExecutableNextState>,
    },
}

impl ExecutableNextState {
    pub(super) fn from_loaded<'program>(
        builder: &mut ExecutableTemplateProgramBuilder<'program>,
        process: &'program LoadedProcess,
        next_state: &'program LoadedNextState,
        scope: ExecutableTemplateScope<'_>,
    ) -> Result<Self> {
        match next_state {
            LoadedNextState::Current => Ok(Self::Current),
            LoadedNextState::Value(state) => Ok(Self::Value(*state)),
            LoadedNextState::Template(template) => {
                Ok(Self::Template(builder.append(process, template, scope)?))
            }
            LoadedNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => Ok(Self::IfElse {
                condition: builder.append(process, condition, scope)?,
                then_state: Box::new(Self::from_loaded(builder, process, then_state, scope)?),
                else_state: Box::new(Self::from_loaded(builder, process, else_state, scope)?),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutableValueTemplate<'program> {
    Literal {
        ty: TypeId,
        value: &'program RuntimeValue,
    },
    ReceivedPayload {
        ty: TypeId,
    },
    CurrentStatePayload {
        ty: TypeId,
    },
    EnumPayload {
        ty: TypeId,
        value: ExecutableValueTemplateRef,
        variant: EnumVariantId,
    },
    RecordField {
        ty: TypeId,
        record: ExecutableValueTemplateRef,
        field: RecordFieldId,
    },
    ListElement {
        ty: TypeId,
        list: ExecutableValueTemplateRef,
        index: usize,
        len: usize,
    },
    ListPrefixElement {
        ty: TypeId,
        list: ExecutableValueTemplateRef,
        index: usize,
        prefix_len: usize,
    },
    ListRest {
        ty: TypeId,
        list: ExecutableValueTemplateRef,
        prefix_len: usize,
    },
    MapValue {
        ty: TypeId,
        map: ExecutableValueTemplateRef,
        key: &'program RuntimeValue,
        keys: &'program [RuntimeValue],
        projection: MapProjectionMode,
    },
    MapRest {
        ty: TypeId,
        map: ExecutableValueTemplateRef,
        excluded_keys: &'program [RuntimeValue],
    },
    ProcessRef {
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    LoopElement {
        ty: TypeId,
        element: LoopElementId,
    },
    EffectOutcome {
        ty: TypeId,
        outcome: EffectOutcomeId,
    },
    EnumVariant {
        ty: TypeId,
        variant: EnumVariantId,
        payload: ExecutableValueTemplateRef,
    },
    Record {
        ty: TypeId,
        fields: Box<[ExecutableValueTemplateField]>,
    },
    List {
        ty: TypeId,
        items: Box<[ExecutableValueTemplateRef]>,
    },
    Map {
        ty: TypeId,
        entries: Box<[ExecutableValueTemplateMapEntry]>,
    },
    IfElse {
        ty: TypeId,
        condition: ExecutableValueTemplateRef,
        then_value: ExecutableValueTemplateRef,
        else_value: ExecutableValueTemplateRef,
    },
    Equality {
        ty: TypeId,
        operand_ty: TypeId,
        operator: ArtifactValueEqualityOperator,
        left: ExecutableValueTemplateRef,
        right: ExecutableValueTemplateRef,
    },
    ScalarArithmetic {
        ty: TypeId,
        operator: ArtifactScalarArithmeticOperator,
        left: ExecutableValueTemplateRef,
        right: ExecutableValueTemplateRef,
    },
    ScalarOrdering {
        ty: TypeId,
        operand_ty: TypeId,
        operator: ArtifactScalarOrderingOperator,
        left: ExecutableValueTemplateRef,
        right: ExecutableValueTemplateRef,
    },
    BooleanNot {
        ty: TypeId,
        operand: ExecutableValueTemplateRef,
    },
    BooleanBinary {
        ty: TypeId,
        operator: ArtifactValueBooleanOperator,
        left: ExecutableValueTemplateRef,
        right: ExecutableValueTemplateRef,
    },
}

impl ExecutableValueTemplate<'_> {
    pub(crate) const fn result_type(&self) -> TypeId {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::EnumPayload { ty, .. }
            | Self::RecordField { ty, .. }
            | Self::ListElement { ty, .. }
            | Self::ListPrefixElement { ty, .. }
            | Self::ListRest { ty, .. }
            | Self::MapValue { ty, .. }
            | Self::MapRest { ty, .. }
            | Self::ProcessRef { ty, .. }
            | Self::LoopElement { ty, .. }
            | Self::EffectOutcome { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. }
            | Self::List { ty, .. }
            | Self::Map { ty, .. }
            | Self::IfElse { ty, .. }
            | Self::Equality { ty, .. }
            | Self::ScalarArithmetic { ty, .. }
            | Self::ScalarOrdering { ty, .. }
            | Self::BooleanNot { ty, .. }
            | Self::BooleanBinary { ty, .. } => *ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableValueTemplateField {
    pub(crate) field: RecordFieldId,
    pub(crate) value: ExecutableValueTemplateRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableValueTemplateMapEntry {
    pub(crate) key: ExecutableValueTemplateRef,
    pub(crate) value: ExecutableValueTemplateRef,
}
