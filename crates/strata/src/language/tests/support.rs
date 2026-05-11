pub(super) use super::super::ast::EnumVariant;
pub(super) use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedOutputId, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedSendTarget, CheckedStateId, CheckedStepResult,
    CheckedTransition, CheckedTypeKind, CheckedValueTemplate,
};
pub(super) use super::super::lexer::{Lexer, TokenKind};
pub(super) use super::super::*;
pub(super) use mantle_artifact::{
    ArtifactAction, ArtifactEffect, ArtifactMessageVariant, ArtifactSendTarget, ArtifactTypeKind,
    ArtifactValue, ArtifactValueTemplate, MAX_ACTIONS_PER_PROCESS, MAX_FIELD_VALUE_BYTES,
    MAX_IDENTIFIER_BYTES, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact,
    ProcessId, ProcessRefId, StepResult, TypeId,
};

pub(super) use super::fixtures::*;

pub(super) fn nested_record_value_source(depth: usize) -> String {
    let mut value = "Leaf".to_string();
    for index in (0..depth).rev() {
        value = format!("State{index} {{ next: {value} }}");
    }
    value
}

pub(super) fn payload_source_with(send_statement: &str, step_header: &str) -> String {
    format!(
        r#"
module actor_payloads;

record MainState;
record Job {{ phase: JobPhase }}
record WorkerState {{ job: Job }}
enum MainMsg {{ Start }}
enum JobPhase {{ Ready, Done }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        {send_statement}
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return WorkerState {{ job: Job {{ phase: Done }} }};
    }}

    {step_header} -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(WorkerState {{ job: job }});
    }}
}}
"#
    )
}

pub(super) fn checked_type_count_overflow_module() -> Module {
    let payload_variants_per_process = MAX_MESSAGE_VARIANTS_PER_PROCESS - 1;
    let mut process_count = 1usize;
    while process_count * (payload_variants_per_process + 2) <= MAX_TYPE_COUNT {
        process_count += 1;
    }
    assert!(process_count <= MAX_PROCESS_COUNT);

    let mut records = Vec::new();
    let mut enums = Vec::new();
    let mut processes = Vec::new();

    for process_index in 0..process_count {
        let state_name = format!("State{process_index}");
        let msg_name = format!("Msg{process_index}");
        records.push(Record {
            name: ident(state_name.as_str()),
            fields: Vec::new(),
        });

        let mut variants = vec![EnumVariant {
            name: ident("Start"),
            payload_type: None,
        }];
        for payload_index in 0..payload_variants_per_process {
            let payload_name = format!("Payload{process_index}_{payload_index}");
            records.push(Record {
                name: ident(&payload_name),
                fields: Vec::new(),
            });
            variants.push(EnumVariant {
                name: ident(format!("M{process_index}_{payload_index}")),
                payload_type: Some(TypeRef::Named(ident(payload_name))),
            });
        }
        enums.push(Enum {
            name: ident(msg_name.as_str()),
            variants,
        });

        let process_name = if process_index == 0 {
            "Main".to_string()
        } else {
            format!("P{process_index}")
        };
        let state_type = TypeRef::Named(ident(state_name.as_str()));
        processes.push(Process {
            name: ident(process_name),
            mailbox_bound: 1,
            state_type: state_type.clone(),
            msg_type: TypeRef::Named(ident(msg_name)),
            init: Function {
                name: ident("init"),
                params: Vec::new(),
                return_type: state_type.clone(),
                effects: Vec::new(),
                may: Vec::new(),
                determinism: Determinism::Det,
                body: Some(FunctionBody::Block(FunctionBlock {
                    statements: Vec::new(),
                    returns: ReturnExpr::Value(ValueExpr::Identifier(ident(state_name.as_str()))),
                })),
            },
            functions: Vec::new(),
            steps: vec![Function {
                name: ident("step"),
                params: vec![
                    FunctionParam::Binding(Param {
                        name: ident("state"),
                        ty: state_type.clone(),
                    }),
                    FunctionParam::Pattern(Pattern::Wildcard),
                ],
                return_type: TypeRef::Applied {
                    constructor: ident("ProcResult"),
                    args: vec![state_type],
                    const_args: Vec::new(),
                },
                effects: Vec::new(),
                may: Vec::new(),
                determinism: Determinism::Det,
                body: Some(FunctionBody::Block(FunctionBlock {
                    statements: Vec::new(),
                    returns: ReturnExpr::Call {
                        name: ident("Stop"),
                        arg: ValueExpr::Identifier(ident("state")),
                    },
                })),
            }],
        });
    }

    Module {
        name: ident("type_count_overflow"),
        records,
        enums,
        functions: Vec::new(),
        processes,
    }
}

pub(super) fn ident(value: impl Into<String>) -> Identifier {
    Identifier::new(value).expect("test identifier should be valid")
}

pub(super) fn payload_message_label_overflow_source() -> String {
    let field_names = payload_overflow_field_names();
    let record_fields = field_names
        .iter()
        .map(|name| format!("    {name}: Phase,\n"))
        .collect::<String>();
    let payload_fields = field_names
        .iter()
        .map(|name| format!("            {name}: Ready,\n"))
        .collect::<String>();

    format!(
        r#"
module payload_label_limit;

record MainState;
record WorkerState;
enum Phase {{ Ready }}
record Job {{
{record_fields}}}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(16) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job {{
{payload_fields}        }});
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(16) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return WorkerState;
    }}

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

pub(super) fn payload_overflow_field_names() -> Vec<String> {
    let mut field_names = (0..MAX_VALUE_TEMPLATE_FIELDS)
        .map(|index| format!("f{index}"))
        .collect::<Vec<_>>();
    let target_payload_len = MAX_FIELD_VALUE_BYTES - "Assign()".len() + 1;
    let mut payload_len = payload_record_label(&field_names).len();

    for field_name in &mut field_names {
        while payload_len < target_payload_len && field_name.len() < MAX_IDENTIFIER_BYTES {
            field_name.push('x');
            payload_len += 1;
        }
        if payload_len == target_payload_len {
            break;
        }
    }

    let payload_label = payload_record_label(&field_names);
    let message_label = format!("Assign({payload_label})");
    assert!(payload_label.len() <= MAX_FIELD_VALUE_BYTES);
    assert!(message_label.len() > MAX_FIELD_VALUE_BYTES);

    field_names
}

pub(super) fn payload_record_label(field_names: &[String]) -> String {
    let fields = field_names
        .iter()
        .map(|name| format!("{name}:Ready"))
        .collect::<Vec<_>>()
        .join(",");
    format!("Job{{{fields}}}")
}

pub(super) fn checked_process_id(index: usize) -> CheckedProcessId {
    CheckedProcessId::from_index(index).expect("valid checked process id")
}

pub(super) fn checked_process_ref_id(index: usize) -> CheckedProcessRefId {
    CheckedProcessRefId::from_index(index).expect("valid checked process reference id")
}

pub(super) fn checked_state_id(index: usize) -> CheckedStateId {
    CheckedStateId::from_index(index).expect("valid checked state id")
}

pub(super) fn checked_message_id(index: usize) -> CheckedMessageId {
    CheckedMessageId::from_index(index).expect("valid checked message id")
}

pub(super) fn checked_output_id(index: usize) -> CheckedOutputId {
    CheckedOutputId::from_index(index).expect("valid checked output id")
}

pub(super) fn checked_state_labels(process: &CheckedProcess) -> Vec<&str> {
    process
        .state_values()
        .iter()
        .map(|state| state.label())
        .collect()
}

pub(super) fn artifact_state_labels(process: &mantle_artifact::ArtifactProcess) -> Vec<&str> {
    process
        .state_values
        .iter()
        .map(|state| state.label.as_str())
        .collect()
}

pub(super) fn artifact_type_id(artifact: &MantleArtifact, label: &str) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.label == label)
        .unwrap_or_else(|| panic!("artifact type {label} should exist"));
    TypeId::from_index(index).expect("artifact type index should fit")
}

pub(super) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(super) fn artifact_process_ref_type_id(artifact: &MantleArtifact, target: ProcessId) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.kind == ArtifactTypeKind::ProcessRef { target })
        .unwrap_or_else(|| {
            panic!(
                "artifact process reference type targeting process {} should exist",
                target.as_u32()
            )
        });
    TypeId::from_index(index).expect("artifact type index should fit")
}

pub(super) fn repeated_emit_statements(count: usize, indent: usize) -> String {
    let padding = " ".repeat(indent);
    let mut statements = String::new();
    for _ in 0..count {
        statements.push_str(&padding);
        statements.push_str("emit \"hello from Strata\";\n");
    }
    statements
}

pub(super) fn only_transition(process: &CheckedProcess) -> &CheckedTransition {
    assert_eq!(
        process.transitions().len(),
        1,
        "expected exactly one checked transition for {}",
        process.debug_name()
    );
    &process.transitions()[0]
}
