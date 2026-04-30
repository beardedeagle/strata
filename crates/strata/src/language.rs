use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    source_hash_fnv1a64, ArtifactAction, ArtifactProcess, Error, MantleArtifact, MessageId,
    OutputId, ProcessId, Result, StateId, StepResult, ARTIFACT_FORMAT, ARTIFACT_VERSION,
    MAX_FIELD_VALUE_BYTES, MAX_OUTPUT_LITERALS, STRATA_SOURCE_LANGUAGE,
};

const STATIC_RUNTIME_DISPATCH_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub records: Vec<Record>,
    pub enums: Vec<Enum>,
    pub processes: Vec<Process>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub mailbox_bound: usize,
    pub state_type: String,
    pub msg_type: String,
    pub init: Function,
    pub step: Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
    pub effects: Vec<String>,
    pub may: Vec<String>,
    pub determinism: Determinism,
    pub body: Option<FunctionBody>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
    pub returns: ReturnExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Emit(String),
    Spawn(String),
    Send { target: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Determinism {
    Det,
    Nondet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnExpr {
    TypeValue(String),
    Variable(String),
    Call { name: String, arg: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub module: Module,
    pub entry_process: ProcessId,
    pub entry_message: MessageId,
    pub outputs: Vec<String>,
    pub processes: Vec<ArtifactProcess>,
}

impl CheckedProgram {
    pub fn to_artifact(&self, source: &str) -> Result<MantleArtifact> {
        let artifact = MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            format_version: ARTIFACT_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: self.module.name.clone(),
            entry_process: self.entry_process,
            entry_message: self.entry_message,
            outputs: self.outputs.clone(),
            processes: self.processes.clone(),
            source_hash_fnv1a64: source_hash_fnv1a64(source),
        };
        artifact.validate()?;
        Ok(artifact)
    }
}

pub fn parse_source(source: &str) -> Result<Module> {
    Parser::new(source)?.parse_module()
}

pub fn check_source(source: &str) -> Result<CheckedProgram> {
    let module = parse_source(source)?;
    let checked = check_module(module)?;
    checked.to_artifact(source)?;
    Ok(checked)
}

struct Symbols<'a> {
    process_ids: BTreeMap<&'a str, ProcessId>,
}

impl<'a> Symbols<'a> {
    fn build(module: &'a Module) -> Result<Self> {
        let mut process_ids = BTreeMap::new();
        for (index, process) in module.processes.iter().enumerate() {
            process_ids.insert(process.name.as_str(), ProcessId::from_index(index)?);
        }
        Ok(Self { process_ids })
    }

    fn process_id(&self, name: &str) -> Result<ProcessId> {
        self.process_ids
            .get(name)
            .copied()
            .ok_or_else(|| Error::new(format!("process {name} is not declared")))
    }
}

struct OutputPool {
    values: Vec<String>,
    by_text: BTreeMap<String, OutputId>,
}

impl OutputPool {
    fn new() -> Self {
        Self {
            values: Vec::new(),
            by_text: BTreeMap::new(),
        }
    }

    fn intern(&mut self, value: &str) -> Result<OutputId> {
        if let Some(id) = self.by_text.get(value) {
            return Ok(*id);
        }
        if self.values.len() >= MAX_OUTPUT_LITERALS {
            return Err(Error::new(format!(
                "program emits more than {MAX_OUTPUT_LITERALS} distinct output literals"
            )));
        }
        let id = OutputId::from_index(self.values.len())?;
        self.values.push(value.to_string());
        self.by_text.insert(value.to_string(), id);
        Ok(id)
    }

    fn into_values(self) -> Vec<String> {
        self.values
    }
}

pub fn check_module(module: Module) -> Result<CheckedProgram> {
    if module.records.is_empty() {
        return Err(Error::new("expected at least one record declaration"));
    }
    if module.enums.is_empty() {
        return Err(Error::new("expected at least one enum declaration"));
    }
    if module.processes.is_empty() {
        return Err(Error::new("expected at least one process declaration"));
    }

    validate_unique_names(
        "record",
        module.records.iter().map(|record| record.name.as_str()),
    )?;
    validate_unique_names("enum", module.enums.iter().map(|item| item.name.as_str()))?;
    validate_unique_type_names(&module)?;
    for item in &module.enums {
        validate_unique_names(
            &format!("variant in enum {}", item.name),
            item.variants.iter().map(String::as_str),
        )?;
    }
    validate_unique_names(
        "process",
        module.processes.iter().map(|process| process.name.as_str()),
    )?;

    let entry = module
        .processes
        .iter()
        .find(|candidate| candidate.name == "Main")
        .or_else(|| module.processes.first())
        .ok_or_else(|| Error::new("expected an entry process"))?;

    let symbols = Symbols::build(&module)?;
    let entry_process = symbols.process_id(&entry.name)?;
    let mut outputs = OutputPool::new();
    let mut checked_processes = Vec::with_capacity(module.processes.len());
    for (index, process) in module.processes.iter().enumerate() {
        let process_id = ProcessId::from_index(index)?;
        checked_processes.push(check_process(
            &module,
            process,
            process_id,
            &symbols,
            &mut outputs,
        )?);
    }

    validate_action_references(&checked_processes, &entry_process)?;

    let entry_message = MessageId::new(0);
    let entry_process_definition = checked_processes
        .get(entry_process.index())
        .ok_or_else(|| Error::new("entry process id is not defined"))?;
    if entry_process_definition.message_variants.is_empty() {
        return Err(Error::new(format!(
            "entry process {} has no messages",
            entry_process_definition.debug_name
        )));
    }

    Ok(CheckedProgram {
        module,
        entry_process,
        entry_message,
        outputs: outputs.into_values(),
        processes: checked_processes,
    })
}

fn check_process(
    module: &Module,
    process: &Process,
    process_id: ProcessId,
    symbols: &Symbols<'_>,
    outputs: &mut OutputPool,
) -> Result<ArtifactProcess> {
    if process.mailbox_bound == 0 {
        return Err(Error::new(format!(
            "process {} mailbox bound must be greater than zero",
            process.name
        )));
    }

    let state_values = state_values_for_type(module, &process.state_type)?;
    let msg_enum = require_enum(module, &process.msg_type)?;
    if msg_enum.variants.is_empty() {
        return Err(Error::new(format!(
            "enum {} must declare at least one variant",
            msg_enum.name
        )));
    }

    let init_state = check_init(process, &state_values)?;
    let (step_result, final_state, actions) = check_step(
        module,
        process,
        process_id,
        symbols,
        &state_values,
        init_state,
        outputs,
    )?;

    Ok(ArtifactProcess {
        debug_name: process.name.clone(),
        state_type: process.state_type.clone(),
        state_values,
        message_type: process.msg_type.clone(),
        message_variants: msg_enum.variants.clone(),
        mailbox_bound: process.mailbox_bound,
        init_state,
        step_result,
        final_state,
        actions,
    })
}

fn validate_unique_names<'a>(kind: &str, names: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(Error::new(format!("duplicate {kind} declaration {name}")));
        }
    }
    Ok(())
}

fn validate_unique_type_names(module: &Module) -> Result<()> {
    let mut seen = BTreeMap::new();
    for record in &module.records {
        seen.insert(record.name.as_str(), "record");
    }
    for item in &module.enums {
        if let Some(previous_kind) = seen.insert(item.name.as_str(), "enum") {
            return Err(Error::new(format!(
                "duplicate type declaration {} used by {} and enum",
                item.name, previous_kind
            )));
        }
    }
    Ok(())
}

fn require_record<'a>(module: &'a Module, name: &str) -> Result<&'a Record> {
    module
        .records
        .iter()
        .find(|record| record.name == name)
        .ok_or_else(|| Error::new(format!("type {name} is not declared as a record")))
}

fn require_enum<'a>(module: &'a Module, name: &str) -> Result<&'a Enum> {
    module
        .enums
        .iter()
        .find(|item| item.name == name)
        .ok_or_else(|| Error::new(format!("type {name} is not declared as an enum")))
}

fn state_values_for_type(module: &Module, name: &str) -> Result<Vec<String>> {
    if require_record(module, name).is_ok() {
        return Ok(vec![name.to_string()]);
    }
    if let Ok(item) = require_enum(module, name) {
        if item.variants.is_empty() {
            return Err(Error::new(format!(
                "enum {} must declare at least one variant",
                item.name
            )));
        }
        return Ok(item.variants.clone());
    }
    Err(Error::new(format!(
        "state type {name} must be declared as a record or enum"
    )))
}

fn check_init(process: &Process, state_values: &[String]) -> Result<StateId> {
    let init = &process.init;
    if !init.params.is_empty() {
        return Err(Error::new("init must declare no parameters"));
    }
    if init.return_type != process.state_type {
        return Err(Error::new(format!(
            "init returns {}, expected {}",
            init.return_type, process.state_type
        )));
    }
    if !init.may.is_empty() {
        return Err(Error::new("init may-behaviors must be empty"));
    }
    if init.determinism != Determinism::Det {
        return Err(Error::new("init must be deterministic"));
    }

    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    if !body.statements.is_empty() {
        return Err(Error::new(
            "init body must not perform statements in this slice",
        ));
    }
    validate_effects("init", &init.effects, BTreeSet::new())?;

    let ReturnExpr::TypeValue(value) = &body.returns else {
        return Err(Error::new(format!(
            "init body must return a value of {}",
            process.state_type
        )));
    };
    state_id_for_value(state_values, value).map_err(|_| {
        Error::new(format!(
            "init body returns {}, which is not a value of {}",
            value, process.state_type
        ))
    })
}

fn state_id_for_value(state_values: &[String], value: &str) -> Result<StateId> {
    state_values
        .iter()
        .position(|candidate| candidate == value)
        .map(StateId::from_index)
        .transpose()?
        .ok_or_else(|| Error::new(format!("unknown state value {value}")))
}

fn check_step(
    module: &Module,
    process: &Process,
    process_id: ProcessId,
    symbols: &Symbols<'_>,
    state_values: &[String],
    init_state: StateId,
    outputs: &mut OutputPool,
) -> Result<(StepResult, StateId, Vec<ArtifactAction>)> {
    let step = &process.step;
    if step.params.len() != 2 {
        return Err(Error::new("step must declare state and msg parameters"));
    }
    let state_param = &step.params[0];
    let msg_param = &step.params[1];
    if state_param.name != "state" || state_param.ty != process.state_type {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    if msg_param.name != "msg" || msg_param.ty != process.msg_type {
        return Err(Error::new(format!(
            "step second parameter must be msg: {}",
            process.msg_type
        )));
    }

    let expected_return = format!("ProcResult<{}>", process.state_type);
    if step.return_type != expected_return {
        return Err(Error::new(format!(
            "step returns {}, expected {}",
            step.return_type, expected_return
        )));
    }
    if !step.may.is_empty() {
        return Err(Error::new("step may-behaviors must be empty"));
    }
    if step.determinism != Determinism::Det {
        return Err(Error::new("step must be deterministic"));
    }

    let Some(body) = &step.body else {
        return Err(Error::new("step must have a body for buildable source"));
    };

    let mut used_effects = BTreeSet::new();
    let mut actions = Vec::with_capacity(body.statements.len());
    for statement in &body.statements {
        match statement {
            Statement::Emit(text) => {
                validate_emit_text(text)?;
                used_effects.insert("emit");
                actions.push(ArtifactAction::Emit {
                    output: outputs.intern(text)?,
                });
            }
            Statement::Spawn(target) => {
                used_effects.insert("spawn");
                actions.push(ArtifactAction::Spawn {
                    target: symbols.process_id(target)?,
                });
            }
            Statement::Send { target, message } => {
                let target_id = symbols.process_id(target)?;
                let message_id = message_id_for_process(module, &process.name, target_id, message)?;
                used_effects.insert("send");
                actions.push(ArtifactAction::Send {
                    target: target_id,
                    message: message_id,
                });
            }
        }
    }
    validate_effects("step", &step.effects, used_effects)?;

    let (step_result, state_arg) = match &body.returns {
        ReturnExpr::Call { name, arg } if name == "Stop" => (StepResult::Stop, arg.as_str()),
        ReturnExpr::Call { name, arg } if name == "Continue" => {
            (StepResult::Continue, arg.as_str())
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(<state value>) or Continue(<state value>)",
            ))
        }
    };
    let final_state = if state_arg == "state" {
        init_state
    } else {
        state_id_for_value(state_values, state_arg).map_err(|_| {
            Error::new(format!(
                "step returns state value {}, which is not a value of {}",
                state_arg, process.state_type
            ))
        })?
    };

    reject_unsupported_self_send(process, process_id, &actions)?;

    Ok((step_result, final_state, actions))
}

fn message_id_for_process(
    module: &Module,
    sender_process: &str,
    process_id: ProcessId,
    message: &str,
) -> Result<MessageId> {
    let process = module.processes.get(process_id.index()).ok_or_else(|| {
        Error::new(format!(
            "process id {} is not declared",
            process_id.as_u32()
        ))
    })?;
    let msg_enum = require_enum(module, &process.msg_type)?;
    msg_enum
        .variants
        .iter()
        .position(|variant| variant == message)
        .map(MessageId::from_index)
        .transpose()?
        .ok_or_else(|| {
            Error::new(format!(
                "process {} sends message {} not accepted by {}",
                sender_process, message, process.name
            ))
        })
}

fn validate_effects(
    function_name: &str,
    declared_effects: &[String],
    used_effects: BTreeSet<&'static str>,
) -> Result<()> {
    let mut declared = BTreeSet::new();
    for effect in declared_effects {
        match effect.as_str() {
            "emit" | "spawn" | "send" => {}
            _ => {
                return Err(Error::new(format!(
                    "{function_name} declares unsupported effect {effect}"
                )))
            }
        }
        if !declared.insert(effect.as_str()) {
            return Err(Error::new(format!(
                "{function_name} declares duplicate effect {effect}"
            )));
        }
    }

    for used in &used_effects {
        if !declared.contains(used) {
            return Err(Error::new(format!(
                "{function_name} uses effect {used} but does not declare it"
            )));
        }
    }
    for declared_effect in declared {
        if !used_effects.contains(declared_effect) {
            return Err(Error::new(format!(
                "{function_name} declares effect {declared_effect} but does not use it"
            )));
        }
    }
    Ok(())
}

fn validate_action_references(
    processes: &[ArtifactProcess],
    entry_process: &ProcessId,
) -> Result<()> {
    let mut spawned_targets = BTreeMap::new();
    for (process_index, process) in processes.iter().enumerate() {
        let process_id = ProcessId::from_index(process_index)?;
        for action in &process.actions {
            match action {
                ArtifactAction::Emit { .. } => {}
                ArtifactAction::Spawn { target } => {
                    if target.index() >= processes.len() {
                        return Err(Error::new(format!(
                            "process {} spawns undefined process id {}",
                            process.debug_name,
                            target.as_u32()
                        )));
                    }
                    if target == entry_process {
                        return Err(Error::new(format!(
                            "process {} spawns entry process {}, which is already started",
                            process.debug_name,
                            process_label(processes, *target)?
                        )));
                    }
                    if *target == process_id {
                        return Err(Error::new(format!(
                            "process {} spawns itself, which is not supported in this source slice",
                            process.debug_name
                        )));
                    }
                    if let Some(previous_process) = spawned_targets.insert(*target, process_id) {
                        return Err(Error::new(format!(
                            "process {} duplicates spawn target {} already spawned by {}",
                            process.debug_name,
                            process_label(processes, *target)?,
                            process_label(processes, previous_process)?
                        )));
                    }
                }
                ArtifactAction::Send { target, message } => {
                    let Some(target_process) = processes.get(target.index()) else {
                        return Err(Error::new(format!(
                            "process {} sends to undefined process id {}",
                            process.debug_name,
                            target.as_u32()
                        )));
                    };
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name,
                            message.as_u32(),
                            target_process.debug_name
                        )));
                    }
                }
            }
        }
    }
    validate_static_runtime_order(processes, *entry_process)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticProcessStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StaticProcessInstance {
    process_id: ProcessId,
    status: StaticProcessStatus,
    mailbox_depth: usize,
}

fn validate_static_runtime_order(
    processes: &[ArtifactProcess],
    entry_process: ProcessId,
) -> Result<()> {
    let mut instances = vec![StaticProcessInstance {
        process_id: entry_process,
        status: StaticProcessStatus::Running,
        mailbox_depth: 1,
    }];
    let mut dispatches = 0usize;

    while let Some(process_index) = next_static_runnable(&instances) {
        if dispatches >= STATIC_RUNTIME_DISPATCH_LIMIT {
            return Err(Error::new(format!(
                "static runtime validation exceeded {STATIC_RUNTIME_DISPATCH_LIMIT} process step(s)"
            )));
        }

        let process_id = instances[process_index].process_id;
        let process = process_by_id(processes, process_id)?;
        instances[process_index].mailbox_depth -= 1;

        for action in &process.actions {
            match action {
                ArtifactAction::Emit { .. } => {}
                ArtifactAction::Spawn { target } => {
                    let target_process = process_by_id(processes, *target)?;
                    if instances
                        .iter()
                        .any(|instance| instance.process_id == *target)
                    {
                        return Err(Error::new(format!(
                            "process {} spawns process {}, which is already spawned",
                            process.debug_name, target_process.debug_name
                        )));
                    }
                    instances.push(StaticProcessInstance {
                        process_id: *target,
                        status: StaticProcessStatus::Running,
                        mailbox_depth: 0,
                    });
                }
                ArtifactAction::Send { target, message } => {
                    let target_process = process_by_id(processes, *target)?;
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name,
                            message.as_u32(),
                            target_process.debug_name
                        )));
                    }

                    let Some(target_index) = instances
                        .iter()
                        .position(|instance| instance.process_id == *target)
                    else {
                        return Err(Error::new(format!(
                            "process {} sends to {} before it is spawned",
                            process.debug_name, target_process.debug_name
                        )));
                    };

                    if instances[target_index].status != StaticProcessStatus::Running {
                        return Err(Error::new(format!(
                            "process {} sends to {}, which is not running",
                            process.debug_name, target_process.debug_name
                        )));
                    }
                    if instances[target_index].mailbox_depth >= target_process.mailbox_bound {
                        return Err(Error::new(format!(
                            "process {} sends to {}, but its mailbox would exceed bound {}",
                            process.debug_name,
                            target_process.debug_name,
                            target_process.mailbox_bound
                        )));
                    }
                    instances[target_index].mailbox_depth += 1;
                }
            }
        }

        if process.step_result == StepResult::Stop {
            instances[process_index].status = StaticProcessStatus::Stopped;
        }
        dispatches += 1;
    }

    for instance in &instances {
        if instance.mailbox_depth != 0 {
            return Err(Error::new(format!(
                "process {} would retain {} unhandled message(s)",
                process_label(processes, instance.process_id)?,
                instance.mailbox_depth
            )));
        }
    }

    Ok(())
}

fn next_static_runnable(instances: &[StaticProcessInstance]) -> Option<usize> {
    instances.iter().position(|instance| {
        instance.status == StaticProcessStatus::Running && instance.mailbox_depth > 0
    })
}

fn process_by_id(processes: &[ArtifactProcess], process_id: ProcessId) -> Result<&ArtifactProcess> {
    processes
        .get(process_id.index())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}

fn process_label(processes: &[ArtifactProcess], process_id: ProcessId) -> Result<&str> {
    processes
        .get(process_id.index())
        .map(|process| process.debug_name.as_str())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}

fn reject_unsupported_self_send(
    process: &Process,
    process_id: ProcessId,
    actions: &[ArtifactAction],
) -> Result<()> {
    if actions.iter().any(|action| {
        matches!(
            action,
            ArtifactAction::Send { target, .. } if *target == process_id
        )
    }) {
        return Err(Error::new(format!(
            "process {} sends to itself, which is not supported in this source slice",
            process.name
        )));
    }

    Ok(())
}

fn validate_emit_text(output: &str) -> Result<()> {
    if output.is_empty() {
        return Err(Error::new("emit output must not be empty"));
    }
    if output.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "emit output exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if output.chars().any(char::is_control) {
        return Err(Error::new(
            "emit output must not contain control characters",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Number(String),
    StringLiteral(String),
    Symbol(char),
    Arrow,
    AtIdent(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    offset: usize,
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        while let Some((offset, ch)) = self.peek_char() {
            if ch.is_whitespace() {
                self.bump_char();
                continue;
            }
            if ch == '/' && self.peek_next_char() == Some('/') {
                self.bump_char();
                self.bump_char();
                while let Some((_, next)) = self.peek_char() {
                    self.bump_char();
                    if next == '\n' {
                        break;
                    }
                }
                continue;
            }
            if ch == '-' && self.peek_next_char() == Some('>') {
                self.bump_char();
                self.bump_char();
                tokens.push(Token {
                    kind: TokenKind::Arrow,
                    offset,
                });
                continue;
            }
            if ch == '@' {
                self.bump_char();
                match self.peek_char() {
                    Some((_, next)) if is_ident_start(next) => {}
                    _ => {
                        return Err(Error::new(format!(
                            "expected identifier after '@' at byte {offset}"
                        )));
                    }
                }
                let ident = self.read_ident()?;
                tokens.push(Token {
                    kind: TokenKind::AtIdent(ident),
                    offset,
                });
                continue;
            }
            if ch == '"' {
                let literal = self.read_string_literal(offset)?;
                tokens.push(Token {
                    kind: TokenKind::StringLiteral(literal),
                    offset,
                });
                continue;
            }
            if is_ident_start(ch) {
                let ident = self.read_ident()?;
                tokens.push(Token {
                    kind: TokenKind::Ident(ident),
                    offset,
                });
                continue;
            }
            if ch.is_ascii_digit() {
                let number = self.read_number();
                tokens.push(Token {
                    kind: TokenKind::Number(number),
                    offset,
                });
                continue;
            }
            if "{}()[];:,=<>!~".contains(ch) {
                self.bump_char();
                tokens.push(Token {
                    kind: TokenKind::Symbol(ch),
                    offset,
                });
                continue;
            }
            return Err(Error::new(format!(
                "unsupported character {ch:?} at byte {offset}"
            )));
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            offset: self.source.len(),
        });
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<(usize, char)> {
        self.source[self.offset..]
            .char_indices()
            .next()
            .map(|(local, ch)| (self.offset + local, ch))
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.offset..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.source[self.offset..].chars().next()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn read_ident(&mut self) -> Result<String> {
        let mut ident = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if is_ident_continue(ch) {
                ident.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        if ident.is_empty() {
            Err(Error::new(format!(
                "expected identifier at byte {}",
                self.offset
            )))
        } else {
            Ok(ident)
        }
    }

    fn read_number(&mut self) -> String {
        let mut number = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if ch.is_ascii_digit() {
                number.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        number
    }

    fn read_string_literal(&mut self, start: usize) -> Result<String> {
        self.bump_char();
        let mut literal = String::new();
        while let Some((offset, ch)) = self.peek_char() {
            match ch {
                '"' => {
                    self.bump_char();
                    return Ok(literal);
                }
                '\n' | '\r' => {
                    return Err(Error::new(format!(
                        "unterminated string literal at byte {start}"
                    )));
                }
                '\\' => {
                    return Err(Error::new(format!(
                        "string escapes are not supported in this source slice at byte {offset}"
                    )));
                }
                _ => {
                    literal.push(ch);
                    self.bump_char();
                }
            }
        }
        Err(Error::new(format!(
            "unterminated string literal at byte {start}"
        )))
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self> {
        Ok(Self {
            tokens: Lexer::new(source).tokenize()?,
            index: 0,
        })
    }

    fn parse_module(mut self) -> Result<Module> {
        self.expect_keyword("module")?;
        let name = self.expect_ident()?;
        self.expect_symbol(';')?;

        let mut records = Vec::new();
        let mut enums = Vec::new();
        let mut processes = Vec::new();

        while !self.at_eof() {
            if self.peek_keyword("record") {
                records.push(self.parse_record()?);
            } else if self.peek_keyword("enum") {
                enums.push(self.parse_enum()?);
            } else if self.peek_keyword("proc") {
                processes.push(self.parse_process()?);
            } else if self.peek_keyword("security") {
                self.skip_statement()?;
            } else {
                return Err(self.error_here("expected record, enum, or proc declaration"));
            }
        }

        Ok(Module {
            name,
            records,
            enums,
            processes,
        })
    }

    fn parse_record(&mut self) -> Result<Record> {
        self.expect_keyword("record")?;
        let name = self.expect_ident()?;
        if self.consume_symbol(';') {
            return Ok(Record { name });
        }
        if self.consume_symbol('{') {
            self.skip_balanced_body('{', '}')?;
            self.expect_symbol(';')?;
            return Ok(Record { name });
        }
        Err(self.error_here("expected ';' or record field body"))
    }

    fn parse_enum(&mut self) -> Result<Enum> {
        self.expect_keyword("enum")?;
        let name = self.expect_ident()?;
        self.expect_symbol('{')?;
        let mut variants = Vec::new();
        while !self.consume_symbol('}') {
            variants.push(self.expect_ident()?);
            let _ = self.consume_symbol(',');
        }
        self.expect_symbol(';')?;
        Ok(Enum { name, variants })
    }

    fn parse_process(&mut self) -> Result<Process> {
        self.expect_keyword("proc")?;
        let name = self.expect_ident()?;
        self.expect_keyword("mailbox")?;
        self.expect_keyword("bounded")?;
        self.expect_symbol('(')?;
        let mailbox_bound = self
            .expect_number()?
            .parse::<usize>()
            .map_err(|_| Error::new(format!("process {name} mailbox bound must fit in usize")))?;
        self.expect_symbol(')')?;
        self.expect_symbol('{')?;

        let mut state_type = None;
        let mut msg_type = None;
        let mut init = None;
        let mut step = None;

        while !self.consume_symbol('}') {
            if self.peek_keyword("type") {
                self.expect_keyword("type")?;
                let alias = self.expect_ident()?;
                self.expect_symbol('=')?;
                let ty = self.parse_type()?;
                self.expect_symbol(';')?;
                match alias.as_str() {
                    "State" => state_type = Some(ty),
                    "Msg" => msg_type = Some(ty),
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported process type alias {alias}; expected State or Msg"
                        )));
                    }
                }
            } else if self.peek_keyword("fn") {
                let function = self.parse_function()?;
                match function.name.as_str() {
                    "init" => init = Some(function),
                    "step" => step = Some(function),
                    other => {
                        return Err(Error::new(format!(
                            "unsupported process function {other}; expected init or step"
                        )));
                    }
                }
            } else {
                return Err(self.error_here("expected process type alias or function"));
            }
        }

        Ok(Process {
            name: name.clone(),
            mailbox_bound,
            state_type: state_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type State")))?,
            msg_type: msg_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type Msg")))?,
            init: init.ok_or_else(|| Error::new(format!("process {name} must declare init")))?,
            step: step.ok_or_else(|| Error::new(format!("process {name} must declare step")))?,
        })
    }

    fn parse_function(&mut self) -> Result<Function> {
        self.expect_keyword("fn")?;
        let name = self.expect_ident()?;
        self.expect_symbol('(')?;
        let mut params = Vec::new();
        while !self.consume_symbol(')') {
            let param_name = self.expect_ident()?;
            self.expect_symbol(':')?;
            let ty = self.parse_type()?;
            params.push(Param {
                name: param_name,
                ty,
            });
            let _ = self.consume_symbol(',');
        }
        self.expect_arrow()?;
        let return_type = self.parse_type()?;
        self.expect_symbol('!')?;
        let effects = self.parse_list()?;
        self.expect_symbol('~')?;
        let may = self.parse_list()?;
        let determinism = match self.expect_at_ident()?.as_str() {
            "det" => Determinism::Det,
            "nondet" => Determinism::Nondet,
            other => {
                return Err(Error::new(format!(
                    "unsupported determinism @{other}; expected @det or @nondet"
                )));
            }
        };

        let body = if self.consume_symbol(';') {
            None
        } else {
            self.expect_symbol('{')?;
            let mut statements = Vec::new();
            while !self.peek_keyword("return") {
                statements.push(self.parse_function_statement()?);
            }
            self.expect_keyword("return")?;
            let returns = self.parse_return_expr()?;
            self.expect_symbol(';')?;
            self.expect_symbol('}')?;
            Some(FunctionBody {
                statements,
                returns,
            })
        };

        Ok(Function {
            name,
            params,
            return_type,
            effects,
            may,
            determinism,
            body,
        })
    }

    fn parse_function_statement(&mut self) -> Result<Statement> {
        if self.peek_keyword("emit") {
            self.expect_keyword("emit")?;
            let text = self.expect_string_literal()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Emit(text));
        }
        if self.peek_keyword("spawn") {
            self.expect_keyword("spawn")?;
            let target = self.expect_ident()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Spawn(target));
        }
        if self.peek_keyword("send") {
            self.expect_keyword("send")?;
            let target = self.expect_ident()?;
            let message = self.expect_ident()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Send { target, message });
        }
        Err(self.error_here("expected emit, spawn, send, or return statement"))
    }

    fn parse_type(&mut self) -> Result<String> {
        let name = self.expect_ident()?;
        if !self.consume_symbol('<') {
            return Ok(name);
        }
        let mut args = Vec::new();
        while !self.consume_symbol('>') {
            args.push(self.parse_type()?);
            let _ = self.consume_symbol(',');
        }
        Ok(format!("{name}<{}>", args.join(",")))
    }

    fn parse_list(&mut self) -> Result<Vec<String>> {
        self.expect_symbol('[')?;
        let mut values = Vec::new();
        while !self.consume_symbol(']') {
            values.push(self.expect_ident()?);
            let _ = self.consume_symbol(',');
        }
        Ok(values)
    }

    fn parse_return_expr(&mut self) -> Result<ReturnExpr> {
        let name = self.expect_ident()?;
        if self.consume_symbol('(') {
            let arg = self.expect_ident()?;
            self.expect_symbol(')')?;
            return Ok(ReturnExpr::Call { name, arg });
        }
        if self.consume_symbol('{') {
            self.skip_balanced_body('{', '}')?;
            return Ok(ReturnExpr::TypeValue(name));
        }
        if name
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
        {
            Ok(ReturnExpr::TypeValue(name))
        } else {
            Ok(ReturnExpr::Variable(name))
        }
    }

    fn skip_statement(&mut self) -> Result<()> {
        while !self.at_eof() {
            if self.consume_symbol(';') {
                return Ok(());
            }
            self.index += 1;
        }
        Err(self.error_here("expected ';'"))
    }

    fn skip_balanced_body(&mut self, open: char, close: char) -> Result<()> {
        let mut depth = 1usize;
        while !self.at_eof() {
            if self.consume_symbol(open) {
                depth += 1;
                continue;
            }
            if self.consume_symbol(close) {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
                continue;
            }
            self.index += 1;
        }
        Err(self.error_here("unterminated balanced body"))
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        match self.next_kind() {
            TokenKind::Ident(value) if value == keyword => Ok(()),
            _ => Err(self.error_previous(format!("expected keyword {keyword}"))),
        }
    }

    fn peek_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(value) if value == keyword)
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::Ident(value) => Ok(value),
            _ => Err(self.error_previous("expected identifier")),
        }
    }

    fn expect_number(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::Number(value) => Ok(value),
            _ => Err(self.error_previous("expected number")),
        }
    }

    fn expect_string_literal(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::StringLiteral(value) => Ok(value),
            _ => Err(self.error_previous("expected string literal")),
        }
    }

    fn expect_at_ident(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::AtIdent(value) => Ok(value),
            _ => Err(self.error_previous("expected @identifier")),
        }
    }

    fn expect_arrow(&mut self) -> Result<()> {
        match self.next_kind() {
            TokenKind::Arrow => Ok(()),
            _ => Err(self.error_previous("expected ->")),
        }
    }

    fn expect_symbol(&mut self, symbol: char) -> Result<()> {
        if self.consume_symbol(symbol) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected symbol {symbol:?}")))
        }
    }

    fn consume_symbol(&mut self, symbol: char) -> bool {
        if matches!(self.peek_kind(), TokenKind::Symbol(value) if *value == symbol) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next_kind(&mut self) -> TokenKind {
        let kind = self.peek_kind().clone();
        if !matches!(kind, TokenKind::Eof) {
            self.index += 1;
        }
        kind
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error_here(&self, message: impl Into<String>) -> Error {
        Error::new(format!(
            "{} at byte {}",
            message.into(),
            self.tokens[self.index].offset
        ))
    }

    fn error_previous(&self, message: impl Into<String>) -> Error {
        let offset = self
            .tokens
            .get(self.index.saturating_sub(1))
            .map(|token| token.offset)
            .unwrap_or(0);
        Error::new(format!("{} at byte {offset}", message.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

    const ACTOR_PING: &str = r#"
module actor_ping;

record MainState;
enum MainMsg { Start };
enum WorkerState { Idle, Handled };
enum WorkerMsg { Ping };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        spawn Worker;
        send Worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#;

    #[test]
    fn parses_and_checks_hello() {
        let checked = check_source(HELLO).expect("hello should check");

        assert_eq!(checked.module.name, "hello");
        assert_eq!(checked.entry_process, ProcessId::new(0));
        assert_eq!(checked.entry_message, MessageId::new(0));
        assert_eq!(checked.outputs, ["hello from Strata"]);
        assert_eq!(checked.processes.len(), 1);
        assert_eq!(checked.processes[0].step_result, StepResult::Stop);
        assert_eq!(
            checked.processes[0].actions,
            [ArtifactAction::Emit {
                output: OutputId::new(0)
            }]
        );
    }

    #[test]
    fn parses_and_checks_actor_ping() {
        let checked = check_source(ACTOR_PING).expect("actor ping should check");

        assert_eq!(checked.module.name, "actor_ping");
        assert_eq!(checked.entry_process, ProcessId::new(0));
        assert_eq!(checked.entry_message, MessageId::new(0));
        assert_eq!(checked.outputs, ["worker handled Ping"]);
        assert_eq!(checked.processes.len(), 2);

        let main = checked
            .processes
            .iter()
            .find(|process| process.debug_name == "Main")
            .expect("Main should be checked");
        assert_eq!(
            main.actions,
            [
                ArtifactAction::Spawn {
                    target: ProcessId::new(1)
                },
                ArtifactAction::Send {
                    target: ProcessId::new(1),
                    message: MessageId::new(0)
                }
            ]
        );

        let worker = checked
            .processes
            .iter()
            .find(|process| process.debug_name == "Worker")
            .expect("Worker should be checked");
        assert_eq!(worker.init_state, StateId::new(0));
        assert_eq!(worker.final_state, StateId::new(1));
    }

    #[test]
    fn rejects_declaration_only_entry_points() {
        let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det;
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det;
}
"#;

        let err = check_source(source).expect_err("declaration-only source should be rejected");
        assert!(err.to_string().contains("init must have a body"));
    }

    #[test]
    fn rejects_emit_without_declared_effect() {
        let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

        let err = check_source(source).expect_err("undeclared emit should be rejected");
        assert!(err
            .to_string()
            .contains("step uses effect emit but does not declare it"));
    }

    #[test]
    fn rejects_spawn_without_declared_effect() {
        let source = ACTOR_PING.replace("! [spawn, send]", "! [send]");

        let err = check_source(&source).expect_err("undeclared spawn should be rejected");

        assert!(err
            .to_string()
            .contains("step uses effect spawn but does not declare it"));
    }

    #[test]
    fn rejects_duplicate_static_spawn_target() {
        let source = ACTOR_PING.replace("spawn Worker;", "spawn Worker;\n        spawn Worker;");

        let err = check_source(&source).expect_err("duplicate spawn should be rejected");

        assert!(err.to_string().contains("duplicates spawn target Worker"));
    }

    #[test]
    fn rejects_static_self_spawn() {
        let source = ACTOR_PING
            .replace("! [emit] ~ [] @det", "! [spawn] ~ [] @det")
            .replace(r#"emit "worker handled Ping";"#, "spawn Worker;");

        let err = check_source(&source).expect_err("self-spawn should be rejected");

        assert!(err.to_string().contains("process Worker spawns itself"));
    }

    #[test]
    fn rejects_send_before_static_spawn() {
        let source = ACTOR_PING.replace(
            "spawn Worker;\n        send Worker Ping;",
            "send Worker Ping;\n        spawn Worker;",
        );

        let err = check_source(&source).expect_err("send before spawn should be rejected");

        assert!(err
            .to_string()
            .contains("sends to Worker before it is spawned"));
    }

    #[test]
    fn rejects_send_without_static_spawn() {
        let source = ACTOR_PING
            .replace("! [spawn, send] ~ [] @det", "! [send] ~ [] @det")
            .replace("        spawn Worker;\n", "");

        let err = check_source(&source).expect_err("send without spawn should be rejected");

        assert!(err
            .to_string()
            .contains("sends to Worker before it is spawned"));
    }

    #[test]
    fn rejects_send_to_stopped_process() {
        let source = ACTOR_PING
            .replace("! [emit] ~ [] @det", "! [send] ~ [] @det")
            .replace(r#"emit "worker handled Ping";"#, "send Main Start;");

        let err = check_source(&source).expect_err("send to stopped process should be rejected");

        assert!(err
            .to_string()
            .contains("sends to Main, which is not running"));
    }

    #[test]
    fn rejects_send_to_unknown_message() {
        let source = ACTOR_PING.replace("send Worker Ping;", "send Worker Unknown;");

        let err = check_source(&source).expect_err("unknown message should be rejected");

        assert!(err
            .to_string()
            .contains("sends message Unknown not accepted by Worker"));
    }

    #[test]
    fn rejects_continue_after_self_send() {
        let source = HELLO
            .replace("! [emit]", "! [send]")
            .replace(r#"emit "hello from Strata";"#, "send Main Start;")
            .replace("return Stop(state);", "return Continue(state);");

        let err = check_source(&source).expect_err("self-send continuation should be rejected");

        assert!(err
            .to_string()
            .contains("sends to itself, which is not supported"));
    }

    #[test]
    fn rejects_emit_output_too_large_for_artifacts() {
        let output = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
        let source = HELLO.replace("hello from Strata", &output);

        let err = check_source(&source).expect_err("oversized emit output should fail");

        assert!(err
            .to_string()
            .contains("emit output exceeds maximum length"));
    }

    #[test]
    fn rejects_bare_concrete_state_return_with_accurate_message() {
        let source = ACTOR_PING.replace("return Stop(Handled);", "return Handled;");

        let err = check_source(&source).expect_err("bare state return should be rejected");

        let message = err.to_string();
        assert!(message
            .contains("step body must return Stop(<state value>) or Continue(<state value>)"));
        assert!(!message.contains("or a concrete state value"));
    }

    #[test]
    fn rejects_duplicate_enum_variants() {
        let source = HELLO.replace("enum MainMsg { Start };", "enum MainMsg { Start, Start };");

        let err = check_source(&source).expect_err("duplicate variant should be rejected");

        assert!(err
            .to_string()
            .contains("duplicate variant in enum MainMsg declaration Start"));
    }

    #[test]
    fn rejects_record_enum_type_name_collision() {
        let source = HELLO.replace("enum MainMsg { Start };", "enum MainState { Start };");

        let err = check_source(&source).expect_err("type name collision should be rejected");

        assert!(err
            .to_string()
            .contains("duplicate type declaration MainState used by record and enum"));
    }

    #[test]
    fn rejects_invalid_annotation_identifier_start() {
        let source = HELLO.replacen("@det", "@1", 1);

        let err = parse_source(&source).expect_err("invalid annotation should fail lexing");

        assert!(err.to_string().contains("expected identifier after '@'"));
    }
}
