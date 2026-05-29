use mantle_artifact::ArtifactScalarType;

use super::ast::{Identifier, TypeRef};
use super::diagnostic::{Error, Result};
use super::import_scope::{ImportScopeContext, ValueScope};
use super::import_symbols::{NamedOwner, owner_of};
use super::source_program::SourceUnitId;
use super::{
    CAP_TYPE, COMPONENT_EXPORT_TYPE, LIST_TYPE, MAP_TYPE, OPTION_TYPE, PORT_CONNECT_TYPE,
    PROC_RESULT_TYPE, PROCESS_REF_TYPE, PROTOCOL_BOUNDARY_TYPE, RESULT_TYPE, SEND_ERROR_TYPE,
    SPAWN_ERROR_TYPE, SPAWN_TYPE, UNIT_TYPE,
};

pub(super) fn validate_identifier_value(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'_, '_>,
    name: &Identifier,
) -> Result<()> {
    validate_identifier_value_expected(context, scope, name, None)
}

pub(super) fn validate_identifier_value_expected(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'_, '_>,
    name: &Identifier,
    expected: Option<&TypeRef>,
) -> Result<()> {
    if let Some(expected) = expected {
        validate_expected_value_type_access(context, expected)?;
        if validate_expected_fieldless_record_constructor(context, name, expected)?.is_some() {
            return Ok(());
        }
        if validate_expected_enum_variant(context, name, expected)?.is_some() {
            return Ok(());
        }
    }
    if scope.is_binding(name) {
        return Ok(());
    }
    validate_fieldless_record_constructor(context, name)?;
    validate_enum_variant(context, scope, name)
}

fn validate_expected_value_type_access(
    context: &ImportScopeContext<'_>,
    expected: &TypeRef,
) -> Result<()> {
    validate_type_ref(context, expected)
}

fn validate_expected_fieldless_record_constructor(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
    expected: &TypeRef,
) -> Result<Option<()>> {
    let TypeRef::Named(expected_name) = expected else {
        return Ok(None);
    };
    if expected_name.as_str() != name.as_str() {
        return Ok(None);
    }
    let Some(owner) = owner_of(
        &context.symbols.fieldless_record_constructors,
        expected_name.as_str(),
    ) else {
        return Ok(None);
    };
    if context.allowed.contains(&owner) {
        Ok(Some(()))
    } else {
        Err(unimported_symbol_error(
            context,
            "record constructor",
            name,
            owner,
        ))
    }
}

fn validate_expected_enum_variant(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
    expected: &TypeRef,
) -> Result<Option<()>> {
    let TypeRef::Named(expected_name) = expected else {
        return Ok(None);
    };
    let Some(type_owner) = owner_of(&context.symbols.types, expected_name.as_str()) else {
        return Ok(None);
    };
    for entry in &context.symbols.enum_variants {
        if entry.name != name.as_str() || entry.owner != type_owner {
            continue;
        }
        if context.allowed.contains(&entry.owner) {
            return Ok(Some(()));
        }
        return Err(unimported_symbol_error(
            context,
            "enum variant",
            name,
            entry.owner,
        ));
    }
    Ok(None)
}

fn validate_fieldless_record_constructor(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    let Some(owner) = owner_of(
        &context.symbols.fieldless_record_constructors,
        name.as_str(),
    ) else {
        return Ok(());
    };
    if context.allowed.contains(&owner) {
        Ok(())
    } else {
        Err(unimported_symbol_error(
            context,
            "record constructor",
            name,
            owner,
        ))
    }
}

pub(super) fn validate_function_or_variant_call(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'_, '_>,
    name: &Identifier,
) -> Result<()> {
    if scope.is_local_function(name) || is_builtin_value_constructor(name.as_str()) {
        return Ok(());
    }
    validate_function_name(context, name)?;
    validate_enum_variant(context, scope, name)
}

fn validate_function_name(context: &ImportScopeContext<'_>, name: &Identifier) -> Result<()> {
    let Some(owner) = context.symbols.function_owner(name) else {
        return Ok(());
    };
    if context.allowed.contains(&owner) {
        Ok(())
    } else {
        Err(unimported_symbol_error(context, "function", name, owner))
    }
}

pub(super) fn call_arg_type<'a>(
    context: &ImportScopeContext<'a>,
    name: &Identifier,
    expected: Option<&'a TypeRef>,
) -> Option<&'a TypeRef> {
    context
        .symbols
        .single_allowed_function_arg_type(context.allowed, name)
        .or_else(|| builtin_constructor_arg_type(name, expected))
}

fn builtin_constructor_arg_type<'a>(
    name: &Identifier,
    expected: Option<&'a TypeRef>,
) -> Option<&'a TypeRef> {
    let expected = expected?;
    let TypeRef::Applied {
        constructor, args, ..
    } = expected
    else {
        return None;
    };
    match (name.as_str(), constructor.as_str(), args.as_slice()) {
        ("Some", OPTION_TYPE, [payload]) => Some(payload),
        ("Ok", RESULT_TYPE, [payload, _]) => Some(payload),
        ("Err", RESULT_TYPE, [_, payload]) => Some(payload),
        ("Stop" | "Continue" | "Panic", PROC_RESULT_TYPE, [payload]) => Some(payload),
        _ => None,
    }
}

pub(super) fn validate_type_ref(context: &ImportScopeContext<'_>, ty: &TypeRef) -> Result<()> {
    match ty {
        TypeRef::Named(name) => validate_type_name(context, name),
        TypeRef::Applied {
            constructor, args, ..
        } => match constructor.as_str() {
            PROCESS_REF_TYPE | SPAWN_TYPE => {
                if let [TypeRef::Named(target)] = args.as_slice() {
                    validate_process_name(context, target)?;
                }
                Ok(())
            }
            CAP_TYPE => validate_capability_type_ref(context, args),
            _ if is_builtin_type_constructor(constructor.as_str()) => {
                for arg in args {
                    validate_type_ref(context, arg)?;
                }
                Ok(())
            }
            _ => {
                validate_type_name(context, constructor)?;
                for arg in args {
                    validate_type_ref(context, arg)?;
                }
                Ok(())
            }
        },
    }
}

pub(super) fn validate_type_name(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    if is_builtin_type_constructor(name.as_str()) {
        return Ok(());
    }
    validate_imported_symbol(context, "type", name, &context.symbols.types)
}

pub(super) fn validate_process_name(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    validate_imported_symbol(context, "process", name, &context.symbols.processes)
}

pub(super) fn validate_protocol_name(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    validate_imported_symbol(context, "protocol", name, &context.symbols.protocols)
}

pub(super) fn validate_port_name(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    validate_imported_symbol(context, "port", name, &context.symbols.ports)
}

pub(super) fn validate_component_name(
    context: &ImportScopeContext<'_>,
    name: &Identifier,
) -> Result<()> {
    validate_imported_symbol(context, "component", name, &context.symbols.components)
}

pub(super) fn validate_enum_variant(
    context: &ImportScopeContext<'_>,
    scope: &ValueScope<'_, '_>,
    name: &Identifier,
) -> Result<()> {
    if scope.is_binding(name) || is_builtin_value_constructor(name.as_str()) {
        return Ok(());
    }
    let mut first_owner = None;
    for entry in &context.symbols.enum_variants {
        if entry.name != name.as_str() {
            continue;
        }
        first_owner.get_or_insert(entry.owner);
        if context.allowed.contains(&entry.owner) {
            return Ok(());
        }
    }
    if let Some(owner) = first_owner {
        Err(unimported_symbol_error(
            context,
            "enum variant",
            name,
            owner,
        ))
    } else {
        Ok(())
    }
}

fn validate_imported_symbol(
    context: &ImportScopeContext<'_>,
    kind: &str,
    name: &Identifier,
    owners: &[NamedOwner<'_>],
) -> Result<()> {
    let Some(owner) = owner_of(owners, name.as_str()) else {
        return Ok(());
    };
    if context.allowed.contains(&owner) {
        Ok(())
    } else {
        Err(unimported_symbol_error(context, kind, name, owner))
    }
}

fn validate_capability_type_ref(context: &ImportScopeContext<'_>, args: &[TypeRef]) -> Result<()> {
    match args {
        [
            TypeRef::Applied {
                constructor,
                args,
                const_args,
            },
        ] if const_args.is_empty() && args.len() == 1 => match constructor.as_str() {
            SPAWN_TYPE => {
                if let [TypeRef::Named(target)] = args.as_slice() {
                    validate_process_name(context, target)?;
                }
                Ok(())
            }
            PROTOCOL_BOUNDARY_TYPE => {
                if let [TypeRef::Named(target)] = args.as_slice() {
                    validate_protocol_name(context, target)?;
                }
                Ok(())
            }
            PORT_CONNECT_TYPE => {
                if let [TypeRef::Named(target)] = args.as_slice() {
                    validate_port_name(context, target)?;
                }
                Ok(())
            }
            COMPONENT_EXPORT_TYPE => {
                if let [TypeRef::Named(target)] = args.as_slice() {
                    validate_component_name(context, target)?;
                }
                Ok(())
            }
            _ => {
                for arg in args {
                    validate_type_ref(context, arg)?;
                }
                Ok(())
            }
        },
        _ => {
            for arg in args {
                validate_type_ref(context, arg)?;
            }
            Ok(())
        }
    }
}

fn unimported_symbol_error(
    context: &ImportScopeContext<'_>,
    kind: &str,
    name: &Identifier,
    owner: SourceUnitId,
) -> Error {
    Error::new(format!(
        "source unit {} references {kind} {name} from module {} without importing {}",
        context.unit.module().name,
        context.symbols.module_name(owner),
        context.symbols.module_name(owner)
    ))
}

fn is_builtin_type_constructor(name: &str) -> bool {
    matches!(
        name,
        PROC_RESULT_TYPE
            | PROCESS_REF_TYPE
            | CAP_TYPE
            | SPAWN_TYPE
            | LIST_TYPE
            | MAP_TYPE
            | UNIT_TYPE
            | OPTION_TYPE
            | RESULT_TYPE
            | SEND_ERROR_TYPE
            | SPAWN_ERROR_TYPE
    ) || ArtifactScalarType::parse_source_name(name).is_some()
}

fn is_builtin_value_constructor(name: &str) -> bool {
    matches!(
        name,
        "Unit"
            | "None"
            | "Some"
            | "Ok"
            | "Err"
            | "Full"
            | "Stopped"
            | "Crashed"
            | "MailboxClosed"
            | "Denied"
            | "Exhausted"
            | "BackendUnavailable"
    )
}
