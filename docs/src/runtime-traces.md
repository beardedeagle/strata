# Runtime Traces

Mantle writes runtime traces as line-delimited JSON. Each line is one runtime
event.

By default, running:

```sh
target/debug/mantle run target/strata/actor_sequence.mta
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
- `message_id`, the admitted message variant ID;
- `state_id`, the admitted state value ID;
- `output_id`, the admitted output literal ID.

Do not treat labels as runtime dispatch keys. Runtime execution uses admitted
typed IDs.

## Event Types

| Event | Meaning |
| --- | --- |
| `artifact_loaded` | Mantle admitted an artifact and loaded its entry metadata. |
| `process_spawned` | A runtime process instance was created. |
| `message_accepted` | A message was accepted into a process mailbox. |
| `message_dequeued` | A process dequeued a message for handling. |
| `process_stepped` | A transition ran for a message. |
| `state_updated` | A process state changed to another admitted state value. |
| `program_output` | A process emitted declared output. |
| `process_stopped` | A process stopped normally. |

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

## Process Stepped

Example shape:

```json
{"event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst"}
```

`result` is `Continue` or `Stop`. `state_id` and `state` are the transition
target state.

## State Updated

Example shape:

```json
{"event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Waiting","to_state_id":1,"to":"SawFirst"}
```

State updates are whole-value replacements. The trace records the previous and
new admitted state values.

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

The current stop reason is `normal`.
