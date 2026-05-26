use super::*;

mod bodies;
mod collection_patterns;
mod groups;
mod match_bodies;
mod names;
mod process_ref_shadowing;
mod record_patterns;
mod substitution;
mod value_resolution;
mod values;

use super::steps::collect_message_case_process_refs;
pub(in crate::language::checker::source_functions) use bodies::{
    source_function_block, source_function_body, source_function_body_scope,
    validate_pure_source_function_block,
};
use collection_patterns::{
    validate_list_pattern_source_function_group, validate_map_pattern_source_function_group,
};
pub(in crate::language::checker::source_functions) use groups::SourceFunctionGroup;
use match_bodies::validate_binding_source_function_match_body;
pub(in crate::language::checker::source_functions) use names::{
    validate_source_pattern_binding_name, validate_source_pattern_binding_scope_conflicts,
    validate_source_value_binding_name,
};
use process_ref_shadowing::validate_source_function_process_ref_shadowing;
use record_patterns::validate_record_pattern_source_function_group;
use substitution::SourceSubstitution;
use values::validate_source_function_body_values;
pub(super) use values::{
    check_source_value_type, resolve_source_value_expr, validate_source_function_value_expr,
};

pub(super) fn validate_source_function_declarations(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let mut module_function_names = BTreeSet::new();
    validate_source_function_groups(
        module,
        semantic_index,
        "module",
        None,
        None,
        &module.functions,
    )?;
    for function in &module.functions {
        module_function_names.insert(function.name.as_str());
    }

    for (process_index, process) in module.processes.iter().enumerate() {
        for function in &process.functions {
            if module_function_names.contains(function.name.as_str()) {
                return Err(Error::new(format!(
                    "process {} function {} conflicts with module function {}",
                    process.name, function.name, function.name
                )));
            }
        }
        let process_ref_names = if process.functions.is_empty() {
            None
        } else {
            let process_id = CheckedProcessId::from_index(process_index)?;
            Some(collect_message_case_process_refs(
                process,
                process_id,
                semantic_index,
            )?)
        };
        let owner = format!("process {}", process.name);
        validate_source_function_groups(
            module,
            semantic_index,
            &owner,
            Some(process),
            process_ref_names.as_ref(),
            &process.functions,
        )?;
    }

    Ok(())
}

fn validate_source_function_groups(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    functions: &[Function],
) -> Result<()> {
    let mut groups: BTreeMap<&str, Vec<&Function>> = BTreeMap::new();
    for function in functions {
        validate_source_function_name(semantic_index, owner, &function.name)?;
        if let Some(process_refs) = process_refs {
            validate_source_function_process_ref_shadowing(owner, function, process_refs)?;
        }
        groups
            .entry(function.name.as_str())
            .or_default()
            .push(function);
    }

    for group in groups.values() {
        validate_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            process_refs,
            group,
        )?;
    }

    validate_source_function_call_cycles(owner, functions)?;

    Ok(())
}

fn validate_source_function_name(
    semantic_index: &SemanticIndex,
    owner: &str,
    name: &Identifier,
) -> Result<()> {
    if matches!(
        name.as_str(),
        "init" | "step" | "Stop" | "Continue" | "Panic"
    ) {
        return Err(Error::new(format!(
            "{owner} function {name} uses a reserved function name"
        )));
    }
    if semantic_index.process_id(name).is_ok() {
        return Err(Error::new(format!(
            "{owner} function {name} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(name) {
        return Err(Error::new(format!(
            "{owner} function {name} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}

fn validate_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let first_kind = source_function_param_kind(first)?;

    for function in functions {
        validate_source_function_contract(module, semantic_index, owner, function)?;
        if !semantic_index.same_type(&function.return_type, &first.return_type) {
            return Err(Error::new(format!(
                "{owner} function {} clauses must return {}, found {}",
                first.name, first.return_type, function.return_type
            )));
        }
        let kind = source_function_param_kind(function)?;
        if kind != first_kind {
            return Err(Error::new(format!(
                "{owner} function {} cannot mix source parameter forms",
                first.name
            )));
        }
    }

    match first_kind {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "{owner} function {} declares duplicate binding clauses",
                    first.name
                )));
            }
            validate_binding_source_function_body(
                module,
                semantic_index,
                owner,
                process,
                process_refs,
                first,
            )
        }
        SourceFunctionParamKind::EnumPattern => validate_enum_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            process_refs,
            functions,
        ),
        SourceFunctionParamKind::RecordPattern => validate_record_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            process_refs,
            functions,
        ),
        SourceFunctionParamKind::ListPattern => validate_list_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            process_refs,
            functions,
        ),
        SourceFunctionParamKind::MapPattern => validate_map_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            process_refs,
            functions,
        ),
    }
}

fn validate_source_function_contract(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
) -> Result<()> {
    if function.params.len() != 1 {
        return Err(Error::new(format!(
            "{owner} function {} must declare exactly one parameter",
            function.name
        )));
    }
    if !function.effects.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} must not declare effects",
            function.name
        )));
    }
    if !function.may.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} may-behaviors must be empty",
            function.name
        )));
    }
    if function.determinism != Determinism::Det {
        return Err(Error::new(format!(
            "{owner} function {} must be deterministic",
            function.name
        )));
    }
    if function.body.is_none() {
        return Err(Error::new(format!(
            "{owner} function {} must have a body for buildable source",
            function.name
        )));
    }
    validate_source_function_declared_value_type(
        module,
        semantic_index,
        owner,
        function,
        "return type",
        &function.return_type,
    )?;
    if let [FunctionParam::Binding(param)] = function.params.as_slice() {
        validate_source_function_declared_value_type(
            module,
            semantic_index,
            owner,
            function,
            &format!("parameter {}", param.name),
            &param.ty,
        )?;
    }
    Ok(())
}

fn validate_source_function_declared_value_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    position: &str,
    ty: &TypeRef,
) -> Result<()> {
    semantic_index
        .validate_source_value_type(module, ty)
        .map_err(|err| {
            Error::new(format!(
                "{owner} function {} {position} must use a declared record, enum, scalar, list, or map type without process-reference authority, found {ty}: {err}",
                function.name
            ))
        })
}

fn validate_source_function_call_cycles(owner: &str, functions: &[Function]) -> Result<()> {
    let function_names = functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut graph = function_names
        .iter()
        .map(|name| (*name, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();

    for function in functions {
        let mut calls = BTreeSet::new();
        collect_source_function_calls(function, &mut calls);
        let caller = function.name.as_str();
        let Some(callees) = graph.get_mut(caller) else {
            return Err(Error::new(format!(
                "{owner} function {} is not registered for cycle validation",
                function.name
            )));
        };
        for call in calls {
            if function_names.contains(call) {
                callees.insert(call);
            }
        }
    }

    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    for name in function_names {
        validate_source_function_call_cycle_from(owner, name, &graph, &mut visited, &mut stack)?;
    }
    Ok(())
}

fn validate_source_function_call_cycle_from<'a>(
    owner: &str,
    name: &'a str,
    graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    visited: &mut BTreeSet<&'a str>,
    stack: &mut Vec<&'a str>,
) -> Result<()> {
    if visited.contains(name) {
        return Ok(());
    }

    stack.push(name);
    if let Some(callees) = graph.get(name) {
        for &callee in callees {
            if let Some(position) = stack.iter().position(|candidate| *candidate == callee) {
                let mut cycle = stack[position..].to_vec();
                cycle.push(callee);
                return Err(Error::new(format!(
                    "{owner} source function call cycle {} is not supported",
                    cycle.join(" -> ")
                )));
            }
            validate_source_function_call_cycle_from(owner, callee, graph, visited, stack)?;
        }
    }
    stack.pop();
    visited.insert(name);
    Ok(())
}

fn collect_source_function_calls<'a>(function: &'a Function, calls: &mut BTreeSet<&'a str>) {
    let Some(body) = &function.body else {
        return;
    };
    match body {
        FunctionBody::Block(body) => collect_source_function_block_calls(body, calls),
        FunctionBody::Match(match_body) => {
            for arm in &match_body.arms {
                collect_source_function_block_calls(&arm.body, calls);
            }
        }
    }
}

fn collect_source_function_block_calls<'a>(body: &'a FunctionBlock, calls: &mut BTreeSet<&'a str>) {
    for statement in &body.statements {
        collect_source_statement_calls(statement, calls);
    }
    collect_source_return_expr_calls(&body.returns, calls);
}

fn collect_source_statement_calls<'a>(statement: &'a Statement, calls: &mut BTreeSet<&'a str>) {
    match statement {
        Statement::LetValue { value, .. } => collect_source_value_expr_calls(value, calls),
        Statement::Send { payload, .. } | Statement::LetSendOutcome { payload, .. } => {
            if let Some(payload) = payload {
                collect_source_value_expr_calls(payload, calls);
            }
        }
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            collect_source_value_expr_calls(condition, calls);
            for statement in then_body {
                collect_source_statement_calls(statement, calls);
            }
            for statement in else_body {
                collect_source_statement_calls(statement, calls);
            }
        }
        Statement::ForEach {
            collection, body, ..
        } => {
            collect_source_value_expr_calls(collection, calls);
            for statement in body {
                collect_source_statement_calls(statement, calls);
            }
        }
        Statement::Emit(_)
        | Statement::LetProcessRef { .. }
        | Statement::LetSpawnOutcome { .. } => {}
    }
}

fn collect_source_return_expr_calls<'a>(returns: &'a ReturnExpr, calls: &mut BTreeSet<&'a str>) {
    match returns {
        ReturnExpr::Value(value) => collect_source_value_expr_calls(value, calls),
        ReturnExpr::Call { name, arg } => {
            calls.insert(name.as_str());
            collect_source_value_expr_calls(arg, calls);
        }
        ReturnExpr::Match(match_body) => {
            for arm in &match_body.arms {
                collect_source_function_block_calls(&arm.body, calls);
            }
        }
        ReturnExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_source_value_expr_calls(condition, calls);
            collect_source_function_block_calls(then_branch, calls);
            collect_source_function_block_calls(else_branch, calls);
        }
    }
}

fn collect_source_value_expr_calls<'a>(value: &'a ValueExpr, calls: &mut BTreeSet<&'a str>) {
    match value {
        ValueExpr::Identifier(_) | ValueExpr::ScalarLiteral(_) => {}
        ValueExpr::Call { name, arg } => {
            calls.insert(name.as_str());
            collect_source_value_expr_calls(arg, calls);
        }
        ValueExpr::EnumVariant { payload, .. } => {
            collect_source_value_expr_calls(payload, calls);
        }
        ValueExpr::Record(record) => {
            for field in &record.fields {
                collect_source_value_expr_calls(&field.value, calls);
            }
        }
        ValueExpr::List(list) => {
            for item in &list.items {
                collect_source_value_expr_calls(item, calls);
            }
        }
        ValueExpr::Map(map) => {
            for entry in &map.entries {
                collect_source_value_expr_calls(&entry.key, calls);
                collect_source_value_expr_calls(&entry.value, calls);
            }
        }
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_source_value_expr_calls(condition, calls);
            collect_source_value_expr_calls(then_branch, calls);
            collect_source_value_expr_calls(else_branch, calls);
        }
        ValueExpr::Equality { left, right, .. } => {
            collect_source_value_expr_calls(left, calls);
            collect_source_value_expr_calls(right, calls);
        }
        ValueExpr::ScalarArithmetic { left, right, .. }
        | ValueExpr::ScalarOrdering { left, right, .. } => {
            collect_source_value_expr_calls(left, calls);
            collect_source_value_expr_calls(right, calls);
        }
        ValueExpr::BooleanNot { operand } => {
            collect_source_value_expr_calls(operand, calls);
        }
        ValueExpr::BooleanBinary { left, right, .. } => {
            collect_source_value_expr_calls(left, calls);
            collect_source_value_expr_calls(right, calls);
        }
        ValueExpr::Grouped { value } => {
            collect_source_value_expr_calls(value, calls);
        }
    }
}

fn source_function_param_kind(function: &Function) -> Result<SourceFunctionParamKind> {
    match function.params.as_slice() {
        [FunctionParam::Binding(_)] => Ok(SourceFunctionParamKind::Binding),
        [FunctionParam::Pattern(Pattern::Record { .. })] => {
            Ok(SourceFunctionParamKind::RecordPattern)
        }
        [FunctionParam::Pattern(Pattern::List(_))] => Ok(SourceFunctionParamKind::ListPattern),
        [FunctionParam::Pattern(Pattern::Map(_))] => Ok(SourceFunctionParamKind::MapPattern),
        [FunctionParam::Pattern(_)] => Ok(SourceFunctionParamKind::EnumPattern),
        _ => Err(Error::new(format!(
            "function {} must declare exactly one parameter",
            function.name
        ))),
    }
}

fn validate_binding_source_function_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    function: &Function,
) -> Result<()> {
    let FunctionParam::Binding(param) = &function.params[0] else {
        return Err(Error::new(format!(
            "{owner} function {} must declare a binding parameter",
            function.name
        )));
    };

    match source_function_body(function)? {
        FunctionBody::Block(body) => validate_pure_source_function_block(owner, function, body),
        FunctionBody::Match(match_body) => validate_binding_source_function_match_body(
            module,
            semantic_index,
            owner,
            function,
            param,
            match_body,
        ),
    }?;

    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        process_refs,
        semantic_index,
    };
    validate_source_value_binding_name(&scope, "source function parameter", &[], &param.name)
        .map_err(|err| Error::new(format!("{owner} function {} {err}", function.name)))?;
    validate_source_function_body_values(
        &scope,
        function,
        &[SourceValueBinding {
            name: &param.name,
            ty: &param.ty,
        }],
    )
}

fn validate_enum_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let enum_type = infer_pattern_function_enum_type(
        module,
        semantic_index,
        owner,
        functions.iter().copied(),
        functions.first().map(|function| &function.name),
    )?;
    let enum_decl = semantic_index.enum_decl(module, &enum_type)?;
    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        process_refs,
        semantic_index,
    };
    let mut explicit_arms = vec![false; enum_decl.variants.len()];
    let mut wildcard_seen = false;

    for function in functions {
        validate_pure_source_function_block(owner, function, source_function_block(function)?)?;
        let pattern_bindings = match &function.params[0] {
            FunctionParam::Pattern(pattern) => {
                let subject = format!("{owner} function {}", function.name);
                let pattern_context = PatternCheckContext {
                    module,
                    semantic_index,
                    enum_decl,
                    enum_type: &enum_type,
                    subject: &subject,
                    label: "signature",
                    payload_context: PatternPayloadContext::SourceValue,
                    binding_context: PatternBindingContext::Source { owner: &subject },
                };
                let checked_pattern = check_typed_match_pattern(&pattern_context, pattern)?;
                match checked_pattern {
                    TypedMatchPattern::Variant {
                        variant, bindings, ..
                    } => {
                        if explicit_arms[variant] {
                            return Err(Error::new(format!(
                                "{owner} function {} declares duplicate pattern for variant {}",
                                function.name, enum_decl.variants[variant].name
                            )));
                        }
                        explicit_arms[variant] = true;
                        bindings
                    }
                    TypedMatchPattern::Wildcard => {
                        if wildcard_seen {
                            return Err(Error::new(format!(
                                "{owner} function {} declares duplicate wildcard pattern",
                                function.name
                            )));
                        }
                        wildcard_seen = true;
                        Vec::new()
                    }
                }
            }
            FunctionParam::Binding(_) => {
                return Err(Error::new(format!(
                    "{owner} function {} cannot mix binding and pattern clauses",
                    function.name
                )));
            }
        };
        validate_source_pattern_binding_scope_conflicts(
            &scope,
            &format!(
                "{owner} function {} signature payload binding",
                function.name
            ),
            &pattern_bindings,
        )?;
        let body_bindings = pattern_bindings
            .iter()
            .map(|binding| SourceValueBinding {
                name: &binding.name,
                ty: &binding.ty,
            })
            .collect::<Vec<_>>();
        validate_source_function_body_values(&scope, function, &body_bindings)?;
    }

    if wildcard_seen && explicit_arms.iter().all(|is_present| *is_present) {
        return Err(Error::new(format!(
            "{owner} function {} wildcard pattern is unreachable",
            first.name
        )));
    }
    if !wildcard_seen {
        for (index, variant) in enum_decl.variants.iter().enumerate() {
            if !explicit_arms[index] {
                return Err(Error::new(format!(
                    "{owner} function {} must handle variant {}",
                    first.name, variant.name
                )));
            }
        }
    }
    Ok(())
}

fn infer_pattern_function_enum_type<'a>(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    functions: impl IntoIterator<Item = &'a Function>,
    first_function_name: Option<&Identifier>,
) -> Result<TypeRef> {
    let mut inferred = None;
    for function in functions {
        let FunctionParam::Pattern(Pattern::Constructor { name, .. }) = &function.params[0] else {
            continue;
        };
        let next = semantic_index.enum_variant_type(module, name)?;
        if let Some(existing) = &inferred {
            if !semantic_index.same_type(existing, &next) {
                return Err(Error::new(format!(
                    "{owner} function {} pattern {} belongs to {}, expected {}",
                    function.name, name, next, existing
                )));
            }
        } else {
            inferred = Some(next);
        }
    }
    inferred.ok_or_else(|| {
        Error::new(format!(
            "{owner} function {} wildcard pattern cannot infer a matched enum type",
            first_function_name
                .map(|name| name.to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ))
    })
}
