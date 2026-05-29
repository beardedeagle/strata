use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{SourceHashFnv1a64, source_hash_fnv1a64};

use super::ast::{Identifier, Module};
use super::checked::CheckedProgram;
use super::checker::check_module;
use super::diagnostic::{Error, Result};
use super::import_scope::validate_import_scopes;
use super::parser::parse_source;
use super::{MAX_SOURCE_PROGRAM_BYTES, MAX_SOURCE_UNIT_COUNT};

const SOURCE_PROGRAM_HASH_INPUT_HEADER: &str = "strata-source-program-v2\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProvenanceHash {
    fnv1a64: String,
}

impl SourceProvenanceHash {
    pub fn from_source(source: &str) -> Self {
        Self {
            fnv1a64: source_hash_fnv1a64(source),
        }
    }

    pub fn fnv1a64(&self) -> &str {
        &self.fnv1a64
    }

    pub fn into_fnv1a64(self) -> String {
        self.fnv1a64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceUnitId(u32);

impl SourceUnitId {
    pub fn from_index(index: usize) -> Result<Self> {
        let value = u32::try_from(index)
            .map_err(|_| Error::new(format!("source unit index {index} is too large")))?;
        Ok(Self(value))
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUnit {
    id: SourceUnitId,
    source: String,
    module: Module,
}

impl SourceUnit {
    pub fn parse(id: SourceUnitId, source: String) -> Result<Self> {
        let module = parse_source(&source)?;
        Ok(Self { id, source, module })
    }

    pub fn id(&self) -> SourceUnitId {
        self.id
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn module(&self) -> &Module {
        &self.module
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportDependency {
    importer: SourceUnitId,
    imported: SourceUnitId,
}

impl ImportDependency {
    fn new(importer: SourceUnitId, imported: SourceUnitId) -> Self {
        Self { importer, imported }
    }

    pub fn importer(self) -> SourceUnitId {
        self.importer
    }

    pub fn imported(self) -> SourceUnitId {
        self.imported
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProgram {
    root: SourceUnitId,
    units: Vec<SourceUnit>,
    dependencies: Vec<ImportDependency>,
    dependency_order: Vec<SourceUnitId>,
}

impl SourceProgram {
    pub fn new(root: SourceUnitId, units: Vec<SourceUnit>) -> Result<Self> {
        validate_source_unit_count(units.len())?;
        let root_unit = units.get(root.index()).ok_or_else(|| {
            Error::new(format!(
                "root source unit id {} is not declared",
                root.as_u32()
            ))
        })?;
        validate_root_entry_process(root_unit)?;
        validate_source_unit_ids(&units)?;
        validate_total_source_bytes(&units)?;

        let module_ids = module_id_index(&units)?;
        let dependencies = collect_import_dependencies(&units, &module_ids)?;
        validate_cross_unit_names(&units)?;
        validate_import_scopes(&units, &dependencies)?;
        let dependency_order = dependency_order(root, &units, &dependencies)?;
        validate_all_units_reachable(root, &units, &dependency_order)?;

        Ok(Self {
            root,
            units,
            dependencies,
            dependency_order,
        })
    }

    pub fn root(&self) -> SourceUnitId {
        self.root
    }

    pub fn root_unit(&self) -> &SourceUnit {
        &self.units[self.root.index()]
    }

    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    pub fn dependencies(&self) -> &[ImportDependency] {
        &self.dependencies
    }

    pub fn dependency_order(&self) -> &[SourceUnitId] {
        &self.dependency_order
    }

    #[cfg(test)]
    pub fn source_hash_input(&self) -> String {
        let input_len = self.source_hash_input_len();
        let mut input = String::with_capacity(input_len);
        self.write_source_hash_input_chunks(|chunk| input.push_str(chunk));
        input
    }

    pub fn source_provenance_hash(&self) -> SourceProvenanceHash {
        let mut hasher = SourceHashFnv1a64::new();
        self.write_source_hash_input_chunks(|chunk| hasher.update(chunk.as_bytes()));
        SourceProvenanceHash {
            fnv1a64: hasher.finish_hex(),
        }
    }

    #[cfg(test)]
    fn source_hash_input_len(&self) -> usize {
        if self.dependency_order.len() == 1 {
            return self.units[self.dependency_order[0].index()].source.len();
        }
        self.dependency_order
            .iter()
            .map(|id| {
                let unit = &self.units[id.index()];
                decimal_len(unit.source.len()) + "\n".len() + unit.source.len()
            })
            .sum::<usize>()
            + SOURCE_PROGRAM_HASH_INPUT_HEADER.len()
            + decimal_len(self.dependency_order.len())
            + "\n".len()
    }

    fn write_source_hash_input_chunks(&self, mut write: impl FnMut(&str)) {
        if self.dependency_order.len() == 1 {
            write(self.units[self.dependency_order[0].index()].source.as_str());
            return;
        }
        write(SOURCE_PROGRAM_HASH_INPUT_HEADER);
        write_decimal_line(&mut write, self.dependency_order.len());
        for id in &self.dependency_order {
            let unit = &self.units[id.index()];
            write_decimal_line(&mut write, unit.source.len());
            write(unit.source.as_str());
        }
    }

    fn into_flattened_module(self) -> Result<Module> {
        let root_name = self.units[self.root.index()].module.name.clone();
        let mut protocols = Vec::new();
        let mut ports = Vec::new();
        let mut components = Vec::new();
        let mut records = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        let mut processes = Vec::new();
        let mut units = self.units.into_iter().map(Some).collect::<Vec<_>>();

        for id in self.dependency_order {
            let Some(unit) = units.get_mut(id.index()).and_then(Option::take) else {
                return Err(Error::new(format!(
                    "source unit id {} appears more than once in dependency order",
                    id.as_u32()
                )));
            };
            protocols.extend(unit.module.protocols);
            ports.extend(unit.module.ports);
            components.extend(unit.module.components);
            records.extend(unit.module.records);
            enums.extend(unit.module.enums);
            functions.extend(unit.module.functions);
            processes.extend(unit.module.processes);
        }

        Ok(Module {
            name: root_name,
            imports: Vec::new(),
            protocols,
            ports,
            components,
            records,
            enums,
            functions,
            processes,
        })
    }
}

pub fn check_source_program(program: SourceProgram) -> Result<CheckedProgram> {
    check_module(program.into_flattened_module()?)
}

fn validate_source_unit_count(count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::new(
            "source program must contain at least one source unit",
        ));
    }
    if count > MAX_SOURCE_UNIT_COUNT {
        return Err(Error::new(format!(
            "source_unit_count must be no greater than {MAX_SOURCE_UNIT_COUNT}"
        )));
    }
    Ok(())
}

fn validate_source_unit_ids(units: &[SourceUnit]) -> Result<()> {
    for (index, unit) in units.iter().enumerate() {
        let expected = SourceUnitId::from_index(index)?;
        if unit.id != expected {
            return Err(Error::new(format!(
                "source unit id {} does not match index {index}",
                unit.id.as_u32()
            )));
        }
    }
    Ok(())
}

fn validate_total_source_bytes(units: &[SourceUnit]) -> Result<()> {
    let mut total = 0usize;
    for unit in units {
        total = total
            .checked_add(unit.source.len())
            .ok_or_else(|| Error::new("source program byte count overflowed"))?;
    }
    if total > MAX_SOURCE_PROGRAM_BYTES {
        return Err(Error::new(format!(
            "source program exceeds maximum size of {MAX_SOURCE_PROGRAM_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_root_entry_process(root_unit: &SourceUnit) -> Result<()> {
    if root_unit
        .module
        .processes
        .iter()
        .any(|process| process.name.as_str() == "Main")
    {
        Ok(())
    } else {
        Err(Error::new(format!(
            "root source unit {} must declare entry process Main",
            root_unit.module.name
        )))
    }
}

fn module_id_index(units: &[SourceUnit]) -> Result<BTreeMap<&str, SourceUnitId>> {
    let mut module_ids = BTreeMap::new();
    for unit in units {
        let module_name = unit.module.name.as_str();
        if let Some(previous) = module_ids.insert(module_name, unit.id) {
            return Err(Error::new(format!(
                "duplicate module identity {module_name} declared by source units {} and {}",
                previous.as_u32(),
                unit.id.as_u32()
            )));
        }
    }
    Ok(module_ids)
}

fn collect_import_dependencies(
    units: &[SourceUnit],
    module_ids: &BTreeMap<&str, SourceUnitId>,
) -> Result<Vec<ImportDependency>> {
    let mut dependencies = Vec::new();
    for unit in units {
        let mut seen = BTreeSet::new();
        for import in &unit.module.imports {
            let import_name = import.module.as_str();
            if !seen.insert(import_name) {
                return Err(Error::new(format!(
                    "source unit {} imports module {import_name} more than once",
                    unit.module.name
                )));
            }
            let imported = module_ids.get(import_name).copied().ok_or_else(|| {
                Error::new(format!(
                    "source unit {} imports missing module {import_name}",
                    unit.module.name
                ))
            })?;
            dependencies.push(ImportDependency::new(unit.id, imported));
        }
    }
    Ok(dependencies)
}

fn validate_cross_unit_names(units: &[SourceUnit]) -> Result<()> {
    let mut types = BTreeMap::new();
    let mut functions = BTreeMap::new();
    let mut processes = BTreeMap::new();
    let mut protocols = BTreeMap::new();
    let mut ports = BTreeMap::new();
    let mut components = BTreeMap::new();
    let mut enum_variants = BTreeMap::new();
    for unit in units {
        for protocol in &unit.module.protocols {
            insert_cross_unit_name(&mut protocols, "protocol", unit, &protocol.name)?;
        }
        for port in &unit.module.ports {
            insert_cross_unit_name(&mut ports, "port", unit, &port.name)?;
        }
        for component in &unit.module.components {
            insert_cross_unit_name(&mut components, "component", unit, &component.name)?;
        }
        for record in &unit.module.records {
            insert_cross_unit_name(&mut types, "type", unit, &record.name)?;
        }
        for item in &unit.module.enums {
            insert_cross_unit_name(&mut types, "type", unit, &item.name)?;
            for variant in &item.variants {
                insert_cross_unit_name(&mut enum_variants, "enum variant", unit, &variant.name)?;
            }
        }

        let mut unit_functions = BTreeSet::new();
        for function in &unit.module.functions {
            if unit_functions.insert(function.name.as_str()) {
                insert_cross_unit_name(&mut functions, "function", unit, &function.name)?;
            }
        }

        for process in &unit.module.processes {
            insert_cross_unit_name(&mut processes, "process", unit, &process.name)?;
        }
    }
    validate_cross_unit_callable_names(units, &functions)?;
    Ok(())
}

#[cfg(test)]
fn decimal_len(value: usize) -> usize {
    if value == 0 {
        return 1;
    }
    let mut len = 0usize;
    let mut remaining = value;
    while remaining > 0 {
        len += 1;
        remaining /= 10;
    }
    len
}

fn write_decimal_line(write: &mut impl FnMut(&str), value: usize) {
    let mut buffer = [0u8; 20];
    let mut remaining = value;
    let mut index = buffer.len();
    loop {
        index -= 1;
        buffer[index] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    let digits = std::str::from_utf8(&buffer[index..]).expect("decimal digits are valid UTF-8");
    write(digits);
    write("\n");
}

fn insert_cross_unit_name<'a>(
    names: &mut BTreeMap<&'a str, (SourceUnitId, &'a str)>,
    kind: &str,
    unit: &'a SourceUnit,
    name: &'a Identifier,
) -> Result<()> {
    if let Some((previous_id, previous_module)) =
        names.insert(name.as_str(), (unit.id, unit.module.name.as_str()))
    {
        if previous_id != unit.id {
            return Err(Error::new(format!(
                "ambiguous imported {kind} name {name} declared by modules {} and {}",
                previous_module, unit.module.name
            )));
        }
    }
    Ok(())
}

fn validate_cross_unit_callable_names<'a>(
    units: &'a [SourceUnit],
    functions: &BTreeMap<&'a str, (SourceUnitId, &'a str)>,
) -> Result<()> {
    for unit in units {
        for item in &unit.module.enums {
            for variant in &item.variants {
                let Some((function_id, function_module)) = functions.get(variant.name.as_str())
                else {
                    continue;
                };
                if *function_id != unit.id {
                    return Err(Error::new(format!(
                        "ambiguous imported callable name {} declared by modules {} and {}",
                        variant.name, unit.module.name, function_module
                    )));
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

fn dependency_order(
    root: SourceUnitId,
    units: &[SourceUnit],
    dependencies: &[ImportDependency],
) -> Result<Vec<SourceUnitId>> {
    let mut adjacency = vec![Vec::new(); units.len()];
    for dependency in dependencies {
        adjacency[dependency.importer.index()].push(dependency.imported);
    }

    let mut states = vec![VisitState::Unvisited; units.len()];
    let mut stack = Vec::new();
    let mut order = Vec::new();
    visit_unit(root, units, &adjacency, &mut states, &mut stack, &mut order)?;
    Ok(order)
}

fn visit_unit(
    unit_id: SourceUnitId,
    units: &[SourceUnit],
    adjacency: &[Vec<SourceUnitId>],
    states: &mut [VisitState],
    stack: &mut Vec<SourceUnitId>,
    order: &mut Vec<SourceUnitId>,
) -> Result<()> {
    match states[unit_id.index()] {
        VisitState::Visited => return Ok(()),
        VisitState::Visiting => return Err(import_cycle_error(unit_id, units, stack)),
        VisitState::Unvisited => {}
    }

    states[unit_id.index()] = VisitState::Visiting;
    stack.push(unit_id);
    for imported in &adjacency[unit_id.index()] {
        visit_unit(*imported, units, adjacency, states, stack, order)?;
    }
    stack.pop();
    states[unit_id.index()] = VisitState::Visited;
    order.push(unit_id);
    Ok(())
}

fn import_cycle_error(
    repeated: SourceUnitId,
    units: &[SourceUnit],
    stack: &[SourceUnitId],
) -> Error {
    let start = stack
        .iter()
        .position(|candidate| *candidate == repeated)
        .unwrap_or(0);
    let mut names = stack[start..]
        .iter()
        .map(|id| units[id.index()].module.name.to_string())
        .collect::<Vec<_>>();
    names.push(units[repeated.index()].module.name.to_string());
    Error::new(format!(
        "import cycle {} is not supported",
        names.join(" -> ")
    ))
}

fn validate_all_units_reachable(
    root: SourceUnitId,
    units: &[SourceUnit],
    dependency_order: &[SourceUnitId],
) -> Result<()> {
    let reachable = dependency_order.iter().copied().collect::<BTreeSet<_>>();
    if reachable.len() == units.len() {
        return Ok(());
    }
    for unit in units {
        if !reachable.contains(&unit.id) {
            return Err(Error::new(format!(
                "source unit {} is not reachable from root module {}",
                unit.module.name,
                units[root.index()].module.name
            )));
        }
    }
    Ok(())
}
