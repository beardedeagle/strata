# Runtime Traces

Mantle writes runtime traces as line-delimited JSON. Each line is one runtime
event.

By default, running:

```sh
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta
```

writes:

```text
target/strata/actor_sequence.observability.jsonl
```

The trace is evidence that Mantle admitted and executed the artifact. It is not
a substitute for running the source-to-runtime gate.

## Identity Fields

Trace events include both labels and numeric IDs.

Labels are for reading:

- `process`;
- `message`;
- `state`;
- `text`.

IDs are for stable runtime identity:

- `pid`, the runtime process instance ID;
- `process_id`, the admitted process definition ID;
- `message_id`, the admitted message case ID;
- `state_id`, the admitted typed state value ID;
- `payload_type_id`, the admitted artifact type ID when a payload is present;
- `output_id`, the admitted output literal ID.

Do not treat labels as runtime dispatch keys. Runtime execution uses admitted
typed IDs and typed state value identities.

When multiple runtime instances are spawned from one process definition, the
instances share `process_id` and label metadata but have different `pid` values.
`examples/actor_instances.str` exercises that shape.

## Event Types

| Event | Meaning |
| --- | --- |
| `artifact_loaded` | Mantle admits an artifact and loads its entry metadata. |
| `process_spawned` | Mantle creates a runtime process instance. |
| `message_accepted` | Mantle accepts a message into a process mailbox. |
| `message_dequeued` | A process dequeued a message for handling. |
| `branch_selected` | Mantle selected a typed runtime control-flow branch. |
| `process_stepped` | A transition ran for a message. |
| `state_updated` | A process state changes to another admitted state value. |
| `program_output` | A process emitted declared output. |
| `process_stopped` | A process stopped normally. |
| `process_failed` | A process failed abnormally after a consumed message. |

## Artifact Loaded

Example shape:

```json
{"event":"artifact_loaded","format":"mantle-target-artifact","schema_version":"1","source_language":"strata","module":"actor_sequence","entry_process_id":0,"entry_process":"Main","entry_message_id":0,"process_count":2}
```

Important fields:

- `format` and `schema_version` identify the artifact schema;
- `source_language` identifies the frontend that produced the artifact;
- `entry_process_id` and `entry_message_id` identify the runtime entrypoint.

## Process Spawned

Example shape:

```json
{"event":"process_spawned","pid":2,"process_id":1,"process":"Worker","state_id":0,"state":"Waiting","mailbox_bound":2,"spawned_by_pid":1}
```

`pid` is the runtime process instance. `process_id` is the admitted process
definition. `spawned_by_pid` is present when another process spawned this one.

## Message Accepted And Dequeued

Example shape:

```json
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","queue_depth":1,"sender_pid":1}
{"event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","queue_depth":1}
```

`message_accepted` records mailbox admission. `message_dequeued` records the
message selected for the next transition.

Payload-bearing messages keep the stable admitted message label and add
`payload_type_id` plus `payload` fields:

```json
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign","payload_type_id":2,"payload":"Job{phase:Ready}","queue_depth":1,"sender_pid":1}
```

Runtime dispatch always uses the numeric `message_id`; labels are trace
metadata. For transitions without payload guards, payload values are recorded
only as trace data. Payload-sensitive dispatch, when present, uses the admitted
payload type ID and exact typed payload value identity, not source strings or
debug labels. Payload type identity is the numeric ID from the admitted artifact
type table, not a source type string.

For state-specific transitions produced by `match state`, runtime dispatch
uses the current admitted `state_id` together with the admitted `message_id`.
The trace continues to record the resulting state ID and label; labels are not
used to select the transition.

When a payload is a transported process reference, the trace also includes the
admitted target process ID and runtime process ID:

```json
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work","payload_type_id":2,"payload":"type2#3","payload_process_id":2,"payload_pid":3,"queue_depth":1,"sender_pid":1}
```

## Branch Selected

Example shape:

```json
{"event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","condition_type_id":0,"condition":"True"}
```

`branch` is `then` or `else`. `condition_type_id` is the admitted artifact type
ID for the checked `Bool` condition, and `condition` is trace metadata for the
evaluated value. Runtime branch selection uses the admitted typed condition,
not source strings, helper names, or debug labels.

## Process Stepped

Example shape:

```json
{"event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst"}
```

`result` is `Continue`, `Stop`, or `Panic`. `state_id` and `state` are the
transition target state. Payload-bearing steps include the same
`payload_type_id` and `payload` fields as mailbox events.

## State Updated

Example shape:

```json
{"event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Waiting","to_state_id":1,"to":"SawFirst"}
```

State updates are whole-value replacements. The trace records the previous and
new admitted state values, including payload-bearing enum state values such as
`Working(Job{phase:Ready})`.

## Program Output

Example shape:

```json
{"event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker handled First"}
```

`output_id` identifies the admitted output literal. `text` is the readable
output.

## Process Stopped

Example shape:

```json
{"event":"process_stopped","pid":2,"process_id":1,"process":"Worker","reason":"normal"}
```

The stop reason is `normal`.

## Process Failed

Example shape:

```json
{"event":"process_failed","pid":2,"process_id":1,"process":"Worker","state_id":1,"state":"Failed","reason":"panic"}
```

`Panic(value)` records a `process_stepped` event with `result:"Panic"`, then a
`process_failed` event. The dequeued message is already consumed and is not
replayed. The failure reason is `panic`.
