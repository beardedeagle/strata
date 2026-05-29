use super::ast::{Enum, Function, FunctionParam, Identifier, TypeRef};
use super::source_program::{SourceUnit, SourceUnitId};

pub(super) struct ImportSymbols<'a> {
    units: &'a [SourceUnit],
    module_names: Vec<&'a str>,
    pub(super) types: Vec<NamedOwner<'a>>,
    pub(super) functions: Vec<FunctionOwner<'a>>,
    pub(super) processes: Vec<NamedOwner<'a>>,
    pub(super) protocols: Vec<NamedOwner<'a>>,
    pub(super) ports: Vec<NamedOwner<'a>>,
    pub(super) components: Vec<NamedOwner<'a>>,
    pub(super) enum_variants: Vec<EnumVariantOwner<'a>>,
    pub(super) fieldless_record_constructors: Vec<NamedOwner<'a>>,
}

pub(super) struct NamedOwner<'a> {
    pub(super) name: &'a str,
    pub(super) owner: SourceUnitId,
}

pub(super) struct FunctionOwner<'a> {
    pub(super) name: &'a str,
    pub(super) owner: SourceUnitId,
    function: &'a Function,
}

pub(super) struct EnumVariantOwner<'a> {
    pub(super) name: &'a str,
    pub(super) owner: SourceUnitId,
}

impl<'a> ImportSymbols<'a> {
    pub(super) fn new(units: &'a [SourceUnit]) -> Self {
        let mut module_names = Vec::with_capacity(units.len());
        let mut counts = SymbolCounts::default();
        for unit in units {
            let module = unit.module();
            counts.protocols += module.protocols.len();
            counts.ports += module.ports.len();
            counts.components += module.components.len();
            counts.types += module.records.len() + module.enums.len();
            counts.functions += module.functions.len();
            counts.processes += module.processes.len();
            counts.enum_variants += module
                .enums
                .iter()
                .map(|item| item.variants.len())
                .sum::<usize>();
            counts.fieldless_record_constructors += module
                .records
                .iter()
                .filter(|record| record.fields.is_empty())
                .count();
        }
        let mut types = Vec::with_capacity(counts.types);
        let mut functions = Vec::with_capacity(counts.functions);
        let mut processes = Vec::with_capacity(counts.processes);
        let mut protocols = Vec::with_capacity(counts.protocols);
        let mut ports = Vec::with_capacity(counts.ports);
        let mut components = Vec::with_capacity(counts.components);
        let mut enum_variants = Vec::with_capacity(counts.enum_variants);
        let mut fieldless_record_constructors =
            Vec::with_capacity(counts.fieldless_record_constructors);

        for unit in units {
            let module = unit.module();
            module_names.push(module.name.as_str());
            for protocol in &module.protocols {
                protocols.push(NamedOwner {
                    name: protocol.name.as_str(),
                    owner: unit.id(),
                });
            }
            for port in &module.ports {
                ports.push(NamedOwner {
                    name: port.name.as_str(),
                    owner: unit.id(),
                });
            }
            for component in &module.components {
                components.push(NamedOwner {
                    name: component.name.as_str(),
                    owner: unit.id(),
                });
            }
            for record in &module.records {
                types.push(NamedOwner {
                    name: record.name.as_str(),
                    owner: unit.id(),
                });
                if record.fields.is_empty() {
                    fieldless_record_constructors.push(NamedOwner {
                        name: record.name.as_str(),
                        owner: unit.id(),
                    });
                }
            }
            for item in &module.enums {
                let enum_owner = unit.id();
                push_enum_type(&mut types, item, enum_owner);
                for variant in &item.variants {
                    enum_variants.push(EnumVariantOwner {
                        name: variant.name.as_str(),
                        owner: enum_owner,
                    });
                }
            }
            for function in &module.functions {
                functions.push(FunctionOwner {
                    name: function.name.as_str(),
                    owner: unit.id(),
                    function,
                });
            }
            for process in &module.processes {
                processes.push(NamedOwner {
                    name: process.name.as_str(),
                    owner: unit.id(),
                });
            }
        }

        Self {
            units,
            module_names,
            types,
            functions,
            processes,
            protocols,
            ports,
            components,
            enum_variants,
            fieldless_record_constructors,
        }
    }

    pub(super) fn module_name(&self, id: SourceUnitId) -> &str {
        self.module_names
            .get(id.index())
            .copied()
            .unwrap_or("<unknown>")
    }

    pub(super) fn single_allowed_function_arg_type(
        &self,
        allowed: &[SourceUnitId],
        name: &Identifier,
    ) -> Option<&'a TypeRef> {
        let mut arg_type = None;
        for entry in &self.functions {
            if entry.name != name.as_str() || !allowed.contains(&entry.owner) {
                continue;
            }
            let [FunctionParam::Binding(param)] = entry.function.params.as_slice() else {
                return None;
            };
            if let Some(existing) = arg_type
                && existing != &param.ty
            {
                return None;
            }
            arg_type = Some(&param.ty);
        }
        arg_type
    }

    pub(super) fn record_field_type(
        &self,
        allowed: &[SourceUnitId],
        record: &Identifier,
        field: &Identifier,
    ) -> Option<&'a TypeRef> {
        for unit in self.units {
            if !allowed.contains(&unit.id()) {
                continue;
            }
            let Some(record) = unit
                .module()
                .records
                .iter()
                .find(|entry| entry.name.as_str() == record.as_str())
            else {
                continue;
            };
            return record
                .fields
                .iter()
                .find_map(|entry| (entry.name.as_str() == field.as_str()).then_some(&entry.ty));
        }
        None
    }

    pub(super) fn enum_variant_payload_type(
        &self,
        allowed: &[SourceUnitId],
        name: &Identifier,
    ) -> Option<&'a TypeRef> {
        let mut payload_type = None;
        for unit in self.units {
            if !allowed.contains(&unit.id()) {
                continue;
            }
            for item in &unit.module().enums {
                for variant in &item.variants {
                    if variant.name.as_str() != name.as_str() {
                        continue;
                    }
                    let Some(ty) = &variant.payload_type else {
                        continue;
                    };
                    if let Some(existing) = payload_type
                        && existing != ty
                    {
                        return None;
                    }
                    payload_type = Some(ty);
                }
            }
        }
        payload_type
    }

    pub(super) fn function_owner(&self, name: &Identifier) -> Option<SourceUnitId> {
        self.functions
            .iter()
            .find_map(|entry| (entry.name == name.as_str()).then_some(entry.owner))
    }
}

#[derive(Default)]
struct SymbolCounts {
    types: usize,
    functions: usize,
    processes: usize,
    protocols: usize,
    ports: usize,
    components: usize,
    enum_variants: usize,
    fieldless_record_constructors: usize,
}

fn push_enum_type<'a>(types: &mut Vec<NamedOwner<'a>>, item: &'a Enum, owner: SourceUnitId) {
    types.push(NamedOwner {
        name: item.name.as_str(),
        owner,
    });
}

pub(super) fn owner_of(owners: &[NamedOwner<'_>], name: &str) -> Option<SourceUnitId> {
    owners
        .iter()
        .find_map(|entry| (entry.name == name).then_some(entry.owner))
}
