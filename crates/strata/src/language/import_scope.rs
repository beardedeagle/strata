use super::ast::{
    CollectionPatternBinding, ConstructorPayloadPattern, ForEachItem, Function, FunctionBlock,
    FunctionBody, FunctionParam, Identifier, ListValue, MapValue, Match, Pattern, Process,
    ReturnExpr, Statement, TypeRef, ValueExpr,
};
use super::diagnostic::Result;
use super::import_access::{
    call_arg_type, validate_component_name, validate_enum_variant,
    validate_function_or_variant_call, validate_identifier_value,
    validate_identifier_value_expected, validate_port_name, validate_process_name,
    validate_protocol_name, validate_type_name, validate_type_ref,
};
use super::import_symbols::ImportSymbols;
use super::source_program::{ImportDependency, SourceUnit, SourceUnitId};
use super::{LIST_TYPE, MAP_TYPE};

pub(super) fn validate_import_scopes(
    units: &[SourceUnit],
    dependencies: &[ImportDependency],
) -> Result<()> {
    let symbols = ImportSymbols::new(units);
    let allowed_units = allowed_units(units, dependencies);

    for unit in units {
        validate_unit_scope(unit, &allowed_units[unit.id().index()], &symbols)?;
    }
    Ok(())
}

fn allowed_units(
    units: &[SourceUnit],
    dependencies: &[ImportDependency],
) -> Vec<Vec<SourceUnitId>> {
    let mut allowed = units.iter().map(|unit| vec![unit.id()]).collect::<Vec<_>>();

    for dependency in dependencies {
        if let Some(unit_allowed) = allowed.get_mut(dependency.importer().index()) {
            unit_allowed.push(dependency.imported());
        }
    }

    allowed
}

fn validate_unit_scope(
    unit: &SourceUnit,
    allowed: &[SourceUnitId],
    symbols: &ImportSymbols<'_>,
) -> Result<()> {
    let module = unit.module();
    let context = ImportScopeContext {
        unit,
        allowed,
        symbols,
    };

    for protocol in &module.protocols {
        validate_type_ref(&context, &protocol.message_type)?;
        validate_type_ref(&context, &protocol.authority)?;
    }
    for port in &module.ports {
        validate_protocol_name(&context, &port.protocol)?;
        validate_process_name(&context, &port.target)?;
        validate_type_ref(&context, &port.authority)?;
    }
    for component in &module.components {
        validate_port_name(&context, &component.export)?;
        for imported_port in &component.imports {
            validate_port_name(&context, imported_port)?;
        }
        validate_type_ref(&context, &component.authority)?;
    }
    for composition in &module.compositions {
        for instance in &composition.instances {
            validate_component_name(&context, &instance.component)?;
        }
        for binding in &composition.port_bindings {
            validate_port_name(&context, &binding.imported_port)?;
            validate_port_name(&context, &binding.exported_port)?;
        }
    }
    for record in &module.records {
        for field in &record.fields {
            validate_type_ref(&context, &field.ty)?;
        }
    }
    for item in &module.enums {
        for variant in &item.variants {
            if let Some(payload_type) = &variant.payload_type {
                validate_type_ref(&context, payload_type)?;
            }
        }
    }
    for function in &module.functions {
        validate_function(&context, function, None)?;
    }
    for process in &module.processes {
        validate_process(&context, process)?;
    }

    Ok(())
}

pub(super) struct ImportScopeContext<'a> {
    pub(super) unit: &'a SourceUnit,
    pub(super) allowed: &'a [SourceUnitId],
    pub(super) symbols: &'a ImportSymbols<'a>,
}

#[derive(Clone)]
pub(super) struct ValueScope<'a, 'f> {
    local_functions: Option<&'f [&'a str]>,
    bindings: Vec<&'a str>,
}

impl<'a, 'f> ValueScope<'a, 'f> {
    fn new(local_functions: Option<&'f [&'a str]>) -> Self {
        Self {
            local_functions,
            bindings: Vec::new(),
        }
    }

    fn insert_binding(&mut self, binding: &'a Identifier) {
        if !self.bindings.contains(&binding.as_str()) {
            self.bindings.push(binding.as_str());
        }
    }

    pub(super) fn is_binding(&self, name: &Identifier) -> bool {
        self.bindings.contains(&name.as_str())
    }

    pub(super) fn is_local_function(&self, name: &Identifier) -> bool {
        self.local_functions
            .is_some_and(|functions| functions.contains(&name.as_str()))
    }
}

fn validate_process(context: &ImportScopeContext<'_>, process: &Process) -> Result<()> {
    let process_functions = process
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();

    validate_type_ref(context, &process.state_type)?;
    validate_type_ref(context, &process.msg_type)?;
    for authority in &process.authorities {
        validate_type_ref(context, &authority.ty)?;
    }
    for supervisor in &process.supervisors {
        for child in &supervisor.children {
            validate_process_name(context, &child.process)?;
            validate_process_name(context, &child.spawn_target)?;
        }
    }
    validate_function(context, &process.init, Some(&process_functions))?;
    for function in &process.functions {
        validate_function(context, function, Some(&process_functions))?;
    }
    for step in &process.steps {
        validate_function(context, step, Some(&process_functions))?;
    }

    Ok(())
}

fn validate_function<'a>(
    context: &ImportScopeContext<'_>,
    function: &'a Function,
    local_functions: Option<&[&'a str]>,
) -> Result<()> {
    validate_type_ref(context, &function.return_type)?;
    let mut scope = ValueScope::new(local_functions);
    for param in &function.params {
        match param {
            FunctionParam::Binding(param) => {
                validate_type_ref(context, &param.ty)?;
                scope.insert_binding(&param.name);
            }
            FunctionParam::Pattern(pattern) => {
                validate_pattern(context, &mut scope, pattern)?;
            }
        }
    }

    let Some(body) = &function.body else {
        return Ok(());
    };
    match body {
        FunctionBody::Block(body) => {
            validate_block(context, &mut scope, body, &function.return_type)
        }
        FunctionBody::Match(match_body) => {
            validate_match(context, &scope, match_body, &function.return_type)
        }
    }
}

fn validate_block<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &mut ValueScope<'a, 'f>,
    block: &'a FunctionBlock,
    return_type: &TypeRef,
) -> Result<()> {
    for statement in &block.statements {
        validate_statement(context, scope, statement)?;
        match statement {
            Statement::LetValue { name, .. }
            | Statement::LetProcessRef { name, .. }
            | Statement::LetSpawnOutcome { name, .. }
            | Statement::LetSendOutcome { name, .. } => scope.insert_binding(name),
            Statement::Emit(_)
            | Statement::Send { .. }
            | Statement::IfElse { .. }
            | Statement::ForEach { .. } => {}
        }
    }
    validate_return_expr(context, scope, &block.returns, return_type)
}

fn validate_statement<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    statement: &'a Statement,
) -> Result<()> {
    match statement {
        Statement::Emit(_) => Ok(()),
        Statement::LetValue { ty, value, .. } => {
            validate_type_ref(context, ty)?;
            validate_value_expr_expected(context, scope, value, Some(ty))
        }
        Statement::LetProcessRef { ty, target, .. }
        | Statement::LetSpawnOutcome { ty, target, .. } => {
            validate_type_ref(context, ty)?;
            validate_process_name(context, target)
        }
        Statement::Send {
            port,
            message,
            payload,
            ..
        } => {
            validate_send_port(context, port.as_ref())?;
            validate_send_payload(context, scope, message, payload.as_ref())
        }
        Statement::LetSendOutcome {
            ty,
            port,
            message,
            payload,
            ..
        } => {
            validate_type_ref(context, ty)?;
            validate_send_port(context, port.as_ref())?;
            validate_send_payload(context, scope, message, payload.as_ref())
        }
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            validate_value_expr(context, scope, condition)?;
            validate_statement_list(context, scope.clone(), then_body)?;
            validate_statement_list(context, scope.clone(), else_body)
        }
        Statement::ForEach {
            item,
            collection,
            body,
        } => {
            let mut body_scope = scope.clone();
            validate_for_each_item(context, &mut body_scope, item)?;
            validate_value_expr(context, scope, collection)?;
            validate_statement_list(context, body_scope, body)
        }
    }
}

fn validate_send_port(context: &ImportScopeContext<'_>, port: Option<&Identifier>) -> Result<()> {
    if let Some(port) = port {
        validate_port_name(context, port)?;
    }
    Ok(())
}

fn validate_send_payload<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    message: &'a Identifier,
    payload: Option<&'a ValueExpr>,
) -> Result<()> {
    validate_enum_variant(context, scope, message)?;
    if let Some(payload) = payload {
        validate_value_expr_expected(
            context,
            scope,
            payload,
            context
                .symbols
                .enum_variant_payload_type(context.allowed, message),
        )?;
    }
    Ok(())
}

fn validate_statement_list<'a, 'f>(
    context: &ImportScopeContext<'_>,
    mut scope: ValueScope<'a, 'f>,
    statements: &'a [Statement],
) -> Result<()> {
    for statement in statements {
        validate_statement(context, &scope, statement)?;
        match statement {
            Statement::LetValue { name, .. }
            | Statement::LetProcessRef { name, .. }
            | Statement::LetSpawnOutcome { name, .. }
            | Statement::LetSendOutcome { name, .. } => scope.insert_binding(name),
            Statement::Emit(_)
            | Statement::Send { .. }
            | Statement::IfElse { .. }
            | Statement::ForEach { .. } => {}
        }
    }
    Ok(())
}

fn validate_for_each_item<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &mut ValueScope<'a, 'f>,
    item: &'a ForEachItem,
) -> Result<()> {
    match item {
        ForEachItem::Binding(name) => {
            scope.insert_binding(name);
            Ok(())
        }
        ForEachItem::RecordPattern { name, fields } => {
            validate_type_name(context, name)?;
            for field in fields {
                scope.insert_binding(&field.binding);
            }
            Ok(())
        }
    }
}

fn validate_return_expr<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    returns: &'a ReturnExpr,
    return_type: &TypeRef,
) -> Result<()> {
    match returns {
        ReturnExpr::Value(value) => {
            validate_value_expr_expected(context, scope, value, Some(return_type))
        }
        ReturnExpr::Call { name, arg } => {
            if !matches!(name.as_str(), "Stop" | "Continue" | "Panic") {
                validate_function_or_variant_call(context, scope, name)?;
            }
            validate_value_expr_expected(
                context,
                scope,
                arg,
                call_arg_type(context, name, Some(return_type)),
            )
        }
        ReturnExpr::Match(match_body) => validate_match(context, scope, match_body, return_type),
        ReturnExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_value_expr(context, scope, condition)?;
            validate_block(context, &mut scope.clone(), then_branch, return_type)?;
            validate_block(context, &mut scope.clone(), else_branch, return_type)
        }
    }
}

fn validate_match<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    match_body: &'a Match,
    return_type: &TypeRef,
) -> Result<()> {
    validate_identifier_value(context, scope, &match_body.scrutinee)?;
    for arm in &match_body.arms {
        let mut arm_scope = scope.clone();
        validate_pattern(context, &mut arm_scope, &arm.pattern)?;
        validate_block(context, &mut arm_scope, &arm.body, return_type)?;
    }
    Ok(())
}

fn validate_pattern<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &mut ValueScope<'a, 'f>,
    pattern: &'a Pattern,
) -> Result<()> {
    match pattern {
        Pattern::Constructor { name, payload } => {
            validate_enum_variant(context, scope, name)?;
            if let Some(payload) = payload {
                validate_constructor_payload_pattern(context, scope, payload)?;
            }
            Ok(())
        }
        Pattern::Record { name, fields } => {
            validate_type_name(context, name)?;
            for field in fields {
                scope.insert_binding(&field.binding);
            }
            Ok(())
        }
        Pattern::List(pattern) => {
            if let Some(element) = &pattern.element_type {
                validate_type_ref(context, element)?;
            }
            for binding in &pattern.elements {
                validate_collection_pattern_binding(context, scope, binding)?;
            }
            if let Some(rest) = &pattern.rest {
                scope.insert_binding(rest);
            }
            Ok(())
        }
        Pattern::Map(pattern) => {
            if let Some(key) = &pattern.key_type {
                validate_type_ref(context, key)?;
            }
            if let Some(value) = &pattern.value_type {
                validate_type_ref(context, value)?;
            }
            for entry in &pattern.entries {
                validate_value_expr(context, scope, &entry.key)?;
                validate_collection_pattern_binding(context, scope, &entry.binding)?;
            }
            if let Some(rest) = &pattern.rest {
                scope.insert_binding(rest);
            }
            Ok(())
        }
        Pattern::Wildcard => Ok(()),
    }
}

fn validate_constructor_payload_pattern<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &mut ValueScope<'a, 'f>,
    payload: &'a ConstructorPayloadPattern,
) -> Result<()> {
    match payload {
        ConstructorPayloadPattern::Binding(binding) => {
            validate_type_ref(context, &binding.ty)?;
            scope.insert_binding(&binding.name);
            Ok(())
        }
        ConstructorPayloadPattern::Destructure(pattern) => {
            validate_pattern(context, scope, pattern)
        }
    }
}

fn validate_collection_pattern_binding<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &mut ValueScope<'a, 'f>,
    binding: &'a CollectionPatternBinding,
) -> Result<()> {
    match binding {
        CollectionPatternBinding::Binding(name) => {
            scope.insert_binding(name);
            Ok(())
        }
        CollectionPatternBinding::Pattern(pattern) => validate_pattern(context, scope, pattern),
        CollectionPatternBinding::Wildcard => Ok(()),
    }
}

fn validate_value_expr<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    value: &'a ValueExpr,
) -> Result<()> {
    validate_value_expr_expected(context, scope, value, None)
}

fn validate_value_expr_expected<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    value: &'a ValueExpr,
    expected: Option<&'a TypeRef>,
) -> Result<()> {
    if let Some(expected) = expected {
        validate_type_ref(context, expected)?;
    }
    match value {
        ValueExpr::Identifier(name) => {
            validate_identifier_value_expected(context, scope, name, expected)
        }
        ValueExpr::ScalarLiteral(_) => Ok(()),
        ValueExpr::Call { name, arg } => {
            validate_function_or_variant_call(context, scope, name)?;
            let arg_expected = call_arg_type(context, name, expected);
            validate_value_expr_expected(context, scope, arg, arg_expected)
        }
        ValueExpr::EnumVariant { name, payload } => {
            validate_enum_variant(context, scope, name)?;
            let payload_expected = context
                .symbols
                .enum_variant_payload_type(context.allowed, name);
            validate_value_expr_expected(context, scope, payload, payload_expected)
        }
        ValueExpr::Record(record) => {
            validate_type_name(context, &record.name)?;
            for field in &record.fields {
                validate_value_expr_expected(
                    context,
                    scope,
                    &field.value,
                    context
                        .symbols
                        .record_field_type(context.allowed, &record.name, &field.name),
                )?;
            }
            Ok(())
        }
        ValueExpr::List(list) => validate_list_value(context, scope, list, expected),
        ValueExpr::Map(map) => validate_map_value(context, scope, map, expected),
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_value_expr(context, scope, condition)?;
            validate_value_expr_expected(context, scope, then_branch, expected)?;
            validate_value_expr_expected(context, scope, else_branch, expected)
        }
        ValueExpr::Equality { left, right, .. }
        | ValueExpr::ScalarArithmetic { left, right, .. }
        | ValueExpr::ScalarOrdering { left, right, .. }
        | ValueExpr::BooleanBinary { left, right, .. } => {
            validate_value_expr(context, scope, left)?;
            validate_value_expr(context, scope, right)
        }
        ValueExpr::BooleanNot { operand } => validate_value_expr(context, scope, operand),
        ValueExpr::Grouped { value: operand } => {
            validate_value_expr_expected(context, scope, operand, expected)
        }
    }
}

fn validate_list_value<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    list: &'a ListValue,
    expected: Option<&'a TypeRef>,
) -> Result<()> {
    if let Some(element) = &list.element_type {
        validate_type_ref(context, element)?;
    }
    let element_type = list
        .element_type
        .as_ref()
        .or_else(|| list_expected_type(expected));
    for item in &list.items {
        validate_value_expr_expected(context, scope, item, element_type)?;
    }
    Ok(())
}

fn validate_map_value<'a, 'f>(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'a, 'f>,
    map: &'a MapValue,
    expected: Option<&'a TypeRef>,
) -> Result<()> {
    if let Some(key) = &map.key_type {
        validate_type_ref(context, key)?;
    }
    if let Some(value) = &map.value_type {
        validate_type_ref(context, value)?;
    }
    let (key_type, value_type) = map_expected_types(expected);
    let key_type = map.key_type.as_ref().or(key_type);
    let value_type = map.value_type.as_ref().or(value_type);
    for entry in &map.entries {
        validate_value_expr_expected(context, scope, &entry.key, key_type)?;
        validate_value_expr_expected(context, scope, &entry.value, value_type)?;
    }
    Ok(())
}

fn list_expected_type(expected: Option<&TypeRef>) -> Option<&TypeRef> {
    let Some(TypeRef::Applied {
        constructor, args, ..
    }) = expected
    else {
        return None;
    };
    if constructor.as_str() == LIST_TYPE {
        args.as_slice().first()
    } else {
        None
    }
}

fn map_expected_types(expected: Option<&TypeRef>) -> (Option<&TypeRef>, Option<&TypeRef>) {
    let Some(TypeRef::Applied {
        constructor, args, ..
    }) = expected
    else {
        return (None, None);
    };
    match (constructor.as_str(), args.as_slice()) {
        (MAP_TYPE, [key, value]) => (Some(key), Some(value)),
        _ => (None, None),
    }
}
