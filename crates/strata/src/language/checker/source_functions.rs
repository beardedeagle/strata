use super::*;

mod collection_patterns;
mod match_bodies;
mod record_patterns;
mod value_resolution;
mod values;

use collection_patterns::{
    validate_list_pattern_source_function_group, validate_map_pattern_source_function_group,
};
use match_bodies::validate_binding_source_function_match_body;
use record_patterns::validate_record_pattern_source_function_group;
use values::validate_source_function_body_values;
pub(super) use values::{
    check_source_value_type, resolve_source_value_expr, validate_source_function_value_expr,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language::checker::source_functions) struct SourceSubstitution {
    pub(in crate::language::checker::source_functions) name: Identifier,
    pub(in crate::language::checker::source_functions) value: ValueExpr,
}

impl SourceSubstitution {
    pub(in crate::language::checker::source_functions) fn new(
        name: Identifier,
        value: ValueExpr,
    ) -> Self {
        Self { name, value }
    }
}

pub(super) fn validate_source_function_declarations(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let mut module_function_names = BTreeSet::new();
    validate_source_function_groups(module, semantic_index, "module", None, &module.functions)?;
    for function in &module.functions {
        module_function_names.insert(function.name.as_str());
    }

    for process in &module.processes {
        for function in &process.functions {
            if module_function_names.contains(function.name.as_str()) {
                return Err(Error::new(format!(
                    "process {} function {} conflicts with module function {}",
                    process.name, function.name, function.name
                )));
            }
        }
        let owner = format!("process {}", process.name);
        validate_source_function_groups(
            module,
            semantic_index,
            &owner,
            Some(process),
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
    functions: &[Function],
) -> Result<()> {
    let mut groups: BTreeMap<&str, Vec<&Function>> = BTreeMap::new();
    for function in functions {
        validate_source_function_name(semantic_index, owner, &function.name)?;
        groups
            .entry(function.name.as_str())
            .or_default()
            .push(function);
    }

    for group in groups.values() {
        validate_source_function_group(module, semantic_index, owner, process, group)?;
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
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let first_kind = source_function_param_kind(first)?;

    for function in functions {
        validate_source_function_contract(semantic_index, owner, function)?;
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
            validate_binding_source_function_body(module, semantic_index, owner, process, first)
        }
        SourceFunctionParamKind::EnumPattern => validate_enum_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            functions,
        ),
        SourceFunctionParamKind::RecordPattern => validate_record_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            functions,
        ),
        SourceFunctionParamKind::ListPattern => validate_list_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            functions,
        ),
        SourceFunctionParamKind::MapPattern => validate_map_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            functions,
        ),
    }
}

fn validate_source_function_contract(
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
) -> Result<()> {
    if function.params.len() != 1 {
        return Err(Error::new(format!(
            "{owner} function {} must declare exactly one parameter in this source slice",
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
        semantic_index,
        owner,
        function,
        "return type",
        &function.return_type,
    )?;
    if let [FunctionParam::Binding(param)] = function.params.as_slice() {
        validate_source_function_declared_value_type(
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
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    position: &str,
    ty: &TypeRef,
) -> Result<()> {
    if semantic_index.is_source_value_type(ty) {
        return Ok(());
    }
    Err(Error::new(format!(
        "{owner} function {} {position} must use a declared record, enum, list, or map type, found {ty}",
        function.name
    )))
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
                    "{owner} source function call cycle {} is not supported in this source slice",
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
        FunctionBody::Block(body) => collect_source_return_expr_calls(&body.returns, calls),
        FunctionBody::Match(match_body) => {
            for arm in &match_body.arms {
                collect_source_return_expr_calls(&arm.body.returns, calls);
            }
        }
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
                collect_source_return_expr_calls(&arm.body.returns, calls);
            }
        }
    }
}

fn collect_source_value_expr_calls<'a>(value: &'a ValueExpr, calls: &mut BTreeSet<&'a str>) {
    match value {
        ValueExpr::Identifier(_) => {}
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
            "function {} must declare exactly one parameter in this source slice",
            function.name
        ))),
    }
}

fn validate_binding_source_function_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
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
        semantic_index,
    };
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
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let enum_type = infer_pattern_function_enum_type(module, semantic_index, owner, functions)?;
    let enum_decl = semantic_index.enum_decl(module, &enum_type)?;
    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
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

fn infer_pattern_function_enum_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    functions: &[&Function],
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
            functions
                .first()
                .map(|function| function.name.to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ))
    })
}

fn validate_pure_source_function_block(
    owner: &str,
    function: &Function,
    body: &FunctionBlock,
) -> Result<()> {
    if !body.statements.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} must not perform statements",
            function.name
        )));
    }
    Ok(())
}

fn source_function_block(function: &Function) -> Result<&FunctionBlock> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => Ok(body),
        FunctionBody::Match(_) => Err(Error::new(format!(
            "function {} pattern signature clauses must use block bodies",
            function.name
        ))),
    }
}

fn source_function_body(function: &Function) -> Result<&FunctionBody> {
    function.body.as_ref().ok_or_else(|| {
        Error::new(format!(
            "function {} must have a body for buildable source",
            function.name
        ))
    })
}

fn source_function_body_scope<'a>(
    scope: &SourceFunctionScope<'a>,
    function: &Function,
) -> SourceFunctionScope<'a> {
    if scope
        .module
        .functions
        .iter()
        .any(|candidate| std::ptr::eq(candidate, function))
    {
        SourceFunctionScope {
            module: scope.module,
            process_name: None,
            process_functions: &[],
            semantic_index: scope.semantic_index,
        }
    } else {
        *scope
    }
}

pub(in crate::language::checker::source_functions) fn validate_source_pattern_binding_name(
    subject: &str,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a reserved state parameter name"
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}
