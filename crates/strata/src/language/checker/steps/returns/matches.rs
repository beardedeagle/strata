use super::super::super::source_functions::validate_source_function_value_expr;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepReturnSubstitution<'a> {
    name: &'a Identifier,
    value: ValueExpr,
}

pub(super) fn resolve_step_return_match(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    input: &StepReturnInput<'_>,
    match_body: &Match,
) -> Result<ResolvedStepReturn> {
    let scrutinee_binding = source_bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete enum source value binding",
                process.name, match_body.scrutinee
            ))
        })?;
    let enum_decl = semantic_index
        .enum_decl(module, scrutinee_binding.ty)
        .map_err(|_| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete enum source value binding",
                process.name, match_body.scrutinee
            ))
        })?;
    let scrutinee_value =
        concrete_step_binding_value(module, process, semantic_index, input, scrutinee_binding)?;
    let (selected_variant, selected_payload) =
        concrete_artifact_enum_value(process, enum_decl, &match_body.scrutinee, &scrutinee_value)?;

    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl,
        enum_type: scrutinee_binding.ty,
        subject: &subject,
        label: "step return match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_payload_sensitive_typed_match_arms(&pattern_context, &match_body.arms)?;
    let mut wildcard = None;
    for (arm, source_arm) in arms.iter().zip(&match_body.arms) {
        validate_step_return_match_arm(
            process,
            function_scope,
            source_bindings,
            &arm.pattern,
            source_arm,
        )?;
        match &arm.pattern {
            TypedMatchPattern::Variant {
                variant,
                payload_guard,
                bindings,
            } => {
                if *variant != selected_variant {
                    continue;
                }
                if !artifact_payload_matches_guard(
                    module,
                    semantic_index,
                    selected_payload,
                    payload_guard.as_ref(),
                )? {
                    continue;
                }
                let substitutions = step_return_match_substitutions(
                    module,
                    semantic_index,
                    process,
                    enum_decl,
                    *variant,
                    selected_payload,
                    bindings,
                )?;
                return resolved_step_return_match_arm(&source_arm.body, &substitutions);
            }
            TypedMatchPattern::Wildcard => {
                wildcard = Some(&source_arm.body);
            }
        }
    }

    if let Some(body) = wildcard {
        return resolved_step_return_match_arm(body, &[]);
    }

    Err(Error::new(format!(
        "process {} step return match has no matching pattern for concrete {}",
        process.name,
        scrutinee_value.label()
    )))
}

fn validate_step_return_match_arm(
    process: &Process,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    pattern: &TypedMatchPattern,
    arm: &MatchArm,
) -> Result<()> {
    if !arm.body.statements.is_empty() {
        return Err(Error::new(format!(
            "process {} step return match arms must not perform statements",
            process.name
        )));
    }
    let resolved = match &arm.body.returns {
        ReturnExpr::Call { name, arg } => step_result_call(name, arg, "step return match arm")?,
        ReturnExpr::Match(_) | ReturnExpr::Value(_) | ReturnExpr::IfElse { .. } => {
            return Err(step_return_shape_error("step return match arm"));
        }
    };
    let mut arm_bindings = Vec::new();
    let validation_bindings = match pattern {
        TypedMatchPattern::Variant { bindings, .. } => {
            for binding in bindings {
                if source_bindings
                    .iter()
                    .any(|existing| existing.name == &binding.name)
                {
                    return Err(Error::new(format!(
                        "process {} step return match payload binding {} conflicts with an existing source value binding",
                        process.name, binding.name
                    )));
                }
            }
            if bindings.is_empty() {
                source_bindings
            } else {
                arm_bindings.reserve_exact(source_bindings.len().saturating_add(bindings.len()));
                arm_bindings.extend_from_slice(source_bindings);
                arm_bindings.extend(bindings.iter().map(|binding| SourceValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                }));
                arm_bindings.as_slice()
            }
        }
        TypedMatchPattern::Wildcard => source_bindings,
    };

    if matches!(&resolved.state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(());
    }
    validate_source_function_value_expr(
        function_scope,
        &process.state_type,
        &resolved.state_arg,
        validation_bindings,
    )
}

fn resolved_step_return_match_arm(
    body: &FunctionBlock,
    substitutions: &[StepReturnSubstitution<'_>],
) -> Result<ResolvedStepReturn> {
    let ReturnExpr::Call { name, arg } = &body.returns else {
        return Err(step_return_shape_error("step return match arm"));
    };
    let mut resolved = step_result_call(name, arg, "step return match arm")?;
    if !substitutions.is_empty() {
        resolved.state_arg = substitute_step_return_bindings(resolved.state_arg, substitutions);
    }
    Ok(resolved)
}

fn concrete_step_binding_value(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    input: &StepReturnInput<'_>,
    binding: &SourceValueBinding<'_>,
) -> Result<ArtifactValue> {
    if let Some(payload_binding) = input
        .payload_bindings
        .iter()
        .find(|candidate| candidate.name == *binding.name)
    {
        let payload = input.payload_guard.ok_or_else(|| {
            Error::new(format!(
                "process {} step return match scrutinee {} requires a discovered concrete message payload case",
                process.name, binding.name
            ))
        })?;
        let value = checked_payload_binding(
            module,
            semantic_index,
            payload,
            &PatternPayloadParam {
                name: payload_binding.name.clone(),
                ty: payload_binding.ty.clone(),
                path: payload_binding.path.clone(),
            },
        )?
        .and_then(|(_, value)| value)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete source value binding",
                process.name, binding.name
            ))
        })?;
        return Ok(value);
    }

    if let Some(state_binding) = input
        .state_payload_bindings
        .iter()
        .find(|candidate| candidate.name == *binding.name)
    {
        return Ok(state_binding.value.clone());
    }

    Err(Error::new(format!(
        "process {} step return match scrutinee {} must be a concrete enum source value binding",
        process.name, binding.name
    )))
}

fn concrete_artifact_enum_value<'a>(
    process: &Process,
    enum_decl: &Enum,
    scrutinee: &Identifier,
    value: &'a ArtifactValue,
) -> Result<(usize, Option<&'a ArtifactValue>)> {
    match value {
        ArtifactValue::Atom(actual) => {
            let (index, variant) = enum_decl
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name.as_str() == actual)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} step return match scrutinee {} value {} is not a variant of enum {}",
                        process.name, scrutinee, actual, enum_decl.name
                    ))
                })?;
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "process {} step return match scrutinee {} fieldless value {} is missing a payload",
                    process.name, scrutinee, actual
                )));
            }
            Ok((index, None))
        }
        ArtifactValue::EnumVariant {
            variant: actual,
            payload,
        } => {
            let (index, variant) = enum_decl
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name.as_str() == actual)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} step return match scrutinee {} value {} is not a variant of enum {}",
                        process.name, scrutinee, actual, enum_decl.name
                    ))
                })?;
            if variant.payload_type.is_none() {
                return Err(Error::new(format!(
                    "process {} step return match scrutinee {} value {} carries an unsupported payload",
                    process.name, scrutinee, actual
                )));
            }
            Ok((index, Some(payload)))
        }
        ArtifactValue::Record { .. }
        | ArtifactValue::List(_)
        | ArtifactValue::Map(_)
        | ArtifactValue::ProcessRef { .. } => Err(Error::new(format!(
            "process {} step return match scrutinee {} must be a concrete enum source value binding",
            process.name, scrutinee
        ))),
    }
}

fn artifact_payload_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: Option<&ArtifactValue>,
    guard: Option<&PatternPayloadGuard>,
) -> Result<bool> {
    let Some(guard) = guard else {
        return Ok(true);
    };
    let Some(payload) = payload else {
        return Ok(false);
    };
    artifact_value_matches_guard(module, semantic_index, payload, guard)
}

fn step_return_match_substitutions<'a>(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    enum_decl: &Enum,
    variant: usize,
    selected_payload: Option<&ArtifactValue>,
    bindings: &'a [PatternPayloadParam],
) -> Result<Vec<StepReturnSubstitution<'a>>> {
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    let variant_decl = enum_decl.variants.get(variant).ok_or_else(|| {
        Error::new(format!(
            "process {} step return match selected missing variant id {}",
            process.name, variant
        ))
    })?;
    if variant_decl.payload_type.is_none() {
        return Err(Error::new(format!(
            "process {} step return match pattern {} does not carry a payload",
            process.name, variant_decl.name
        )));
    }
    let Some(payload) = selected_payload else {
        return Err(Error::new(format!(
            "process {} step return match pattern {} requires a payload value",
            process.name, variant_decl.name
        )));
    };

    let mut substitutions = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let value =
            payload_binding_value(module, semantic_index, payload, binding)?.ok_or_else(|| {
                Error::new(format!(
                    "process {} step return match payload {} does not match binding {}",
                    process.name,
                    payload.label(),
                    binding.name
                ))
            })?;
        substitutions.push(StepReturnSubstitution {
            name: &binding.name,
            value: artifact_to_source_value(module, semantic_index, &binding.ty, &value)?,
        });
    }
    Ok(substitutions)
}

fn artifact_to_source_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ArtifactValue,
) -> Result<ValueExpr> {
    if semantic_index
        .process_ref_target_type(expected_type)?
        .is_some()
    {
        return Err(Error::new(
            "process references must be direct message payloads",
        ));
    }
    if let Ok(record) = semantic_index.record_decl(module, expected_type) {
        return artifact_record_to_source_value(module, semantic_index, record, value);
    }
    if let Some(collection) = semantic_index.collection_type(expected_type)? {
        return artifact_collection_to_source_value(module, semantic_index, collection, value);
    }
    artifact_enum_to_source_value(module, semantic_index, expected_type, value)
}

fn artifact_enum_to_source_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ArtifactValue,
) -> Result<ValueExpr> {
    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
    match value {
        ArtifactValue::Atom(actual) => {
            let variant = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name.as_str() == actual)
                .ok_or_else(|| {
                    Error::new(format!(
                        "artifact value {} is not a variant of enum {}",
                        actual, enum_decl.name
                    ))
                })?;
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "artifact value {} is missing payload for enum {}",
                    actual, enum_decl.name
                )));
            }
            Ok(ValueExpr::Identifier(variant.name.clone()))
        }
        ArtifactValue::EnumVariant {
            variant: actual,
            payload,
        } => {
            let variant = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name.as_str() == actual)
                .ok_or_else(|| {
                    Error::new(format!(
                        "artifact value {} is not a variant of enum {}",
                        actual, enum_decl.name
                    ))
                })?;
            let payload_type = variant.payload_type.as_ref().ok_or_else(|| {
                Error::new(format!(
                    "artifact value {} carries payload for fieldless enum {}",
                    actual, enum_decl.name
                ))
            })?;
            Ok(ValueExpr::EnumVariant {
                name: variant.name.clone(),
                payload: Box::new(artifact_to_source_value(
                    module,
                    semantic_index,
                    payload_type,
                    payload,
                )?),
            })
        }
        ArtifactValue::Record { .. }
        | ArtifactValue::List(_)
        | ArtifactValue::Map(_)
        | ArtifactValue::ProcessRef { .. } => Err(Error::new(format!(
            "artifact value {} is not an enum value of type {}",
            value.label(),
            expected_type
        ))),
    }
}

fn artifact_record_to_source_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    record: &Record,
    value: &ArtifactValue,
) -> Result<ValueExpr> {
    if record.fields.is_empty() {
        return match value {
            ArtifactValue::Atom(actual) if actual == record.name.as_str() => {
                Ok(ValueExpr::Identifier(record.name.clone()))
            }
            _ => Err(Error::new(format!(
                "artifact value {} is not a value of record {}",
                value.label(),
                record.name
            ))),
        };
    }

    let ArtifactValue::Record {
        constructor,
        fields,
    } = value
    else {
        return Err(Error::new(format!(
            "artifact value {} is not a value of record {}",
            value.label(),
            record.name
        )));
    };
    if constructor != record.name.as_str() {
        return Err(Error::new(format!(
            "artifact record {} does not match expected record {}",
            constructor, record.name
        )));
    }
    for value_field in fields {
        if !record
            .fields
            .iter()
            .any(|field| value_field.name == field.name.as_str())
        {
            return Err(Error::new(format!(
                "artifact record {} carries unexpected field {}",
                record.name, value_field.name
            )));
        }
    }

    let mut source_fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let value_field = fields
            .iter()
            .find(|candidate| candidate.name == field.name.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "artifact record {} is missing field {}",
                    record.name, field.name
                ))
            })?;
        source_fields.push(RecordValueField {
            name: field.name.clone(),
            value: artifact_to_source_value(module, semantic_index, &field.ty, &value_field.value)?,
        });
    }
    Ok(ValueExpr::Record(RecordValue {
        name: record.name.clone(),
        fields: source_fields,
    }))
}

fn artifact_collection_to_source_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    collection: CollectionType<'_>,
    value: &ArtifactValue,
) -> Result<ValueExpr> {
    match (collection, value) {
        (CollectionType::List { element, capacity }, ArtifactValue::List(items)) => {
            let mut source_items = Vec::with_capacity(items.len());
            for item in items {
                source_items.push(artifact_to_source_value(
                    module,
                    semantic_index,
                    element,
                    item,
                )?);
            }
            Ok(ValueExpr::List(ListValue {
                element_type: Some(element.clone()),
                capacity: Some(capacity),
                items: source_items,
            }))
        }
        (
            CollectionType::Map {
                key,
                value: item_type,
                capacity,
            },
            ArtifactValue::Map(entries),
        ) => {
            let mut source_entries = Vec::with_capacity(entries.len());
            for entry in entries {
                source_entries.push(MapValueEntry {
                    key: artifact_to_source_value(module, semantic_index, key, &entry.key)?,
                    value: artifact_to_source_value(
                        module,
                        semantic_index,
                        item_type,
                        &entry.value,
                    )?,
                });
            }
            Ok(ValueExpr::Map(MapValue {
                key_type: Some(key.clone()),
                value_type: Some(item_type.clone()),
                capacity: Some(capacity),
                entries: source_entries,
            }))
        }
        (_, _) => Err(Error::new(format!(
            "artifact value {} does not match collection type",
            value.label()
        ))),
    }
}

fn substitute_step_return_bindings(
    value: ValueExpr,
    bindings: &[StepReturnSubstitution<'_>],
) -> ValueExpr {
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|binding| (binding.name == &name).then(|| binding.value.clone()))
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_step_return_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_step_return_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_step_return_bindings(field.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::List(list) => ValueExpr::List(ListValue {
            element_type: list.element_type,
            capacity: list.capacity,
            items: list
                .items
                .into_iter()
                .map(|item| substitute_step_return_bindings(item, bindings))
                .collect(),
        }),
        ValueExpr::Map(map) => ValueExpr::Map(MapValue {
            key_type: map.key_type,
            value_type: map.value_type,
            capacity: map.capacity,
            entries: map
                .entries
                .into_iter()
                .map(|entry| MapValueEntry {
                    key: substitute_step_return_bindings(entry.key, bindings),
                    value: substitute_step_return_bindings(entry.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => ValueExpr::Equality {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::BooleanNot { operand } => ValueExpr::BooleanNot {
            operand: Box::new(substitute_step_return_bindings(*operand, bindings)),
        },
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => ValueExpr::BooleanBinary {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::Grouped { value } => ValueExpr::Grouped {
            value: Box::new(substitute_step_return_bindings(*value, bindings)),
        },
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => ValueExpr::IfElse {
            condition: Box::new(substitute_step_return_bindings(*condition, bindings)),
            then_branch: Box::new(substitute_step_return_bindings(*then_branch, bindings)),
            else_branch: Box::new(substitute_step_return_bindings(*else_branch, bindings)),
        },
    }
}
