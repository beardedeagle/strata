# Runtime Traces

Mantle writes runtime traces as line-delimited JSON. Each line is one runtime
event.

By default, running:

```sh
just mantle-run target/strata/actor_sequence.mta
```

writes:

```text
target/strata/actor_sequence.observability.jsonl
```

The trace is evidence that Mantle admitted and executed the artifact. It is not
a substitute for running the source-to-runtime gate.

## Trace Schema

Every event includes Mantle-owned trace schema metadata:

- `trace_schema`, currently `mantle-runtime-observability`;
- `trace_schema_version`, currently `1`.

These fields describe the JSONL observability contract only. They do not
identify a `.mta` artifact schema, carry executable bindings, select runtime
behavior, or move runtime semantics into Strata.

Mantle validates traces with a read-only schema check when repository tests ask
for trace evidence. Validation checks event kind support, required fields,
string-vs-number field shapes, exact per-event field sets, payload and loop
context field grouping, strict unsigned integer syntax, numeric branch-path
segments that match Mantle's emitted branch-path encoding, schema identity,
`artifact_loaded` first/no-repeat ordering, closed runtime enum value domains,
u32 artifact typed-ID width, u16 branch-path segment width, branch-path length,
artifact process-ID bounds from `process_count`, Mantle-contiguous spawned PID
sequencing, non-entry spawn parent evidence, runtime PID-to-process-ID
correlation, supervisor-child slot causality for child starts and restart
decisions, restart-window numeric bounds and decision coupling, and process
lifecycle causality boundaries after terminal stop/fail events. It never
dispatches from trace data and never turns labels, source names, or debug
strings into runtime inputs.

The validator fails closed under explicit byte, event-count, and runtime-process
limits. The default validation limits match Mantle's default 8 MiB trace-byte
budget, cap validation at 100,000 events, and cap spawned runtime process
correlation at 10,000 process IDs; callers that expose trace validation outside
the repository gates can pass caller-specific positive limits. Limits bound
validation work only. They do not make trace JSON executable, trusted source
semantics, or an artifact boundary.

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
- `outcome_id`, the admitted effect-outcome binding ID for a pre-state
  outcome action;
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
| `spawn_authority_checked` | Mantle checked an admitted spawn-site authority before spawn acceptance. |
| `boundary_send_checked` | Mantle accepted a typed-port boundary send on the mailbox acceptance path. |
| `effect_outcome_bound` | Mantle bound a typed effect outcome and recorded its source-visible result category. |
| `message_accepted` | Mantle accepts a message into a process mailbox. |
| `message_dequeued` | A process dequeued a message for handling. |
| `branch_selected` | Mantle selected a typed runtime control-flow branch. |
| `loop_started` | Mantle started a bounded runtime collection loop. |
| `loop_iteration` | Mantle started one ordered runtime loop body iteration. |
| `loop_completed` | Mantle completed a bounded runtime collection loop. |
| `process_stepped` | A transition ran for a message. |
| `state_updated` | A process state changes to another admitted state value. |
| `program_output` | A process emitted declared output. |
| `process_stopped` | A process stopped normally. |
| `process_failed` | A process failed abnormally after a consumed message. |
| `supervisor_child_started` | A supervisor started a declared lexical child. |
| `supervisor_restart_decision` | A supervisor accepted, denied, or skipped a child restart. |

## Artifact Loaded

Example shape:

```json
{"event":"artifact_loaded","format":"mantle-target-artifact","schema_version":"6","source_language":"strata","module":"actor_sequence","entry_process_id":0,"entry_process":"Main","entry_message_id":0,"process_count":2,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

Important fields:

- `format` and `schema_version` identify the artifact schema;
- `source_language` identifies the frontend that produced the artifact;
- `entry_process_id` and `entry_message_id` identify the runtime entrypoint.

## Process Spawned

Example shape:

```json
{"event":"process_spawned","pid":2,"process_id":1,"process":"Worker","state_id":0,"state":"Waiting","mailbox_bound":2,"spawned_by_pid":1,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`pid` is the runtime process instance. `process_id` is the admitted process
definition. `spawned_by_pid` is present when another process spawned this one.
When present, the spawning PID must still be live; a stopped or failed process
cannot create a later runtime process.

## Spawn Authority Checked

Example shape:

```json
{"event":"spawn_authority_checked","pid":1,"process_id":0,"process":"Main","target_process_id":1,"spawn_site_id":0,"authority_id":0,"spawn_kind":"dynamic_local","authority_result":"accepted","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`spawn_site_id` and `authority_id` are admitted typed table IDs. `spawn_kind`
currently records `dynamic_local`. `authority_result` is `accepted` or
`denied`; a denied spawn outcome returns `Err(Denied(Unit))` before the target
process is accepted.

## Boundary Send Checked

Example shape:

```json
{"event":"boundary_send_checked","pid":1,"process_id":0,"process":"Main","port_id":0,"port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","target_process_id":1,"target_process":"Worker","message_id":0,"message":"Work","boundary_result":"accepted","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`port_id`, `protocol_id`, `target_process_id`, and `message_id` are admitted
typed IDs. Labels are diagnostics and trace metadata only. A
`boundary_result` of `accepted` is emitted only for typed-port sends that proceed
on the same runtime path as mailbox acceptance. A typed send outcome that returns
`Full`, `Stopped`, `Crashed`, or `MailboxClosed` before mailbox acceptance
does not emit an accepted boundary event. Invalid or denied boundary shapes fail artifact or
loaded-program admission before runtime dispatch.

## Effect Outcome Bound

Example shapes:

```json
{"event":"effect_outcome_bound","pid":1,"process_id":0,"process":"Main","outcome_id":0,"action":"spawn","target_process_id":1,"spawn_site_id":0,"outcome_result":"backend_unavailable","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
{"event":"effect_outcome_bound","pid":1,"process_id":0,"process":"Main","outcome_id":1,"action":"send","target_process_id":1,"message_id":0,"outcome_result":"mailbox_closed","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`outcome_id`, `target_process_id`, `spawn_site_id`, `message_id`, and
`port_id` are admitted typed IDs. `action` is `spawn` or `send`.
`outcome_result` is a closed Mantle result category: `ok`, `denied`,
`exhausted`, or `backend_unavailable` for spawn outcomes, and `ok`, `full`,
`stopped`, `crashed`, or `mailbox_closed` for send outcomes. Spawn outcome events require `spawn_site_id` and do not carry a
message or port ID. Send outcome events require `message_id` and may carry
`port_id` when the outcome action was tied to an admitted typed port.

This event records the Mantle-owned cause of the immutable source-visible
`Result` value. It does not add executable source bindings, does not dispatch
from labels, and does not turn trace JSON into a Strata or `.mta` input.

## Message Accepted And Dequeued

Example shape:

```json
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","queue_depth":1,"sender_pid":1,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
{"event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","queue_depth":0,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`message_accepted` records mailbox admission. `message_dequeued` records the
message selected for the next transition. When `sender_pid` is present, it
must reference a live runtime process at the admission event; payload process
references remain typed evidence and are not executable dispatch handles.

Payload-bearing messages keep the stable admitted message label and add
`payload_type_id` plus `payload` fields:

```json
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign","payload_type_id":2,"payload":"Job{phase:Ready}","queue_depth":1,"sender_pid":1,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
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
{"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work","payload_type_id":2,"payload":"type2#3","payload_process_id":2,"payload_pid":3,"queue_depth":1,"sender_pid":1,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

## Branch Selected

Example shape:

```json
{"event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action","branch_path":[0],"condition_type_id":0,"condition":"True","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`branch` is `then` or `else`. `scope` is `action` for action-level branch
execution, including statement-level branches and branch action prefixes lowered
from final-position runtime `if`, and `next_state` for branch selection used to
resolve the transition result state. `branch_path` is a stable numeric path
assigned from the admitted runtime artifact structure; it is trace identity, not
source syntax. Its segments must be values the Mantle runtime encoder can emit:
action indexes, selected branch action indexes, selected loop-body action
indexes, or selected next-state branch sentinels. `condition_type_id` is the
admitted artifact type ID for the checked `Bool` condition, and `condition` is
trace metadata for the evaluated value. Runtime branch selection uses the
admitted typed condition, not source strings, function names, or debug labels.
For composed predicates, this metadata is still only the final evaluated Bool
value; the branch path remains the trace identity for the admitted artifact node.
Statement-level branches record this event even when the selected branch is an
admitted no-op branch with no actions.
When a selected statement-level branch contains the single admitted nested
runtime branch action, the nested branch records its own `branch_selected` event
with a child `branch_path` derived from the admitted outer branch path and
selected branch action index.
When a statement-level branch guards a bounded runtime loop, the outer
`branch_selected` event is recorded before any loop events. If the selected
branch is empty, no `loop_started`, `loop_iteration`, or `loop_completed` event
is emitted for that branch.

When a branch runs inside a bounded runtime loop body, `branch_selected` appears
after that iteration's `loop_iteration` event and before the selected branch
effects. The event also includes `loop_element_id` and `loop_index` so the
selected branch is tied directly to the active immutable loop element binding.
The condition may be the evaluated active loop element value.

## Runtime Loop Events

Example shape:

```json
{"event":"loop_started","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","element_id":0,"collection_type_id":0,"max_items":2,"item_count":2,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
{"event":"loop_iteration","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","element_id":0,"index":0,"element_type_id":1,"element":"True","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
{"event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","element_id":0,"iteration_count":2,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`element_id`, `collection_type_id`, and `element_type_id` are admitted artifact
IDs. `index`, `item_count`, and `iteration_count` record the bounded runtime
execution. `element` is trace metadata for the evaluated element value. Runtime
loop execution uses the admitted typed collection template and loop element ID,
not the source binding name.

## Process Stepped

Example shape:

```json
{"event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`result` is `Continue`, `Stop`, or `Panic`. `state_id` and `state` are the
transition target state. Payload-bearing steps include the same
`payload_type_id` and `payload` fields as mailbox events.

## State Updated

Example shape:

```json
{"event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Waiting","to_state_id":1,"to":"SawFirst","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

State updates are whole-value replacements. The trace records the previous and
new admitted state values, including payload-bearing enum state values such as
`Working(Job{phase:Ready})`.

## Program Output

Example shape:

```json
{"event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker handled First","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`output_id` identifies the admitted output literal. `text` is the readable
output.

## Process Stopped

Example shape:

```json
{"event":"process_stopped","pid":2,"process_id":1,"process":"Worker","reason":"normal","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

The stop reason is `normal` for source-level `Stop(...)` termination,
`supervisor_shutdown` when a running supervisor stops its child tree during
orderly shutdown, and `supervisor_failure` when a failed supervisor or failed
supervised process forces its child tree to stop.

## Process Failed

Example shape:

```json
{"event":"process_failed","pid":2,"process_id":1,"process":"Worker","state_id":1,"state":"Failed","reason":"panic","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`Panic(value)` records a `process_stepped` event with `result:"Panic"`, then a
`process_failed` event. The dequeued message is already consumed and is not
replayed. The failure reason is `panic`. Supervisor-scope failures also use
`process_failed`; current supervisor failure reasons are
`supervisor_restart_intensity_exceeded`,
`supervisor_restart_capacity_exceeded`, and `supervisor_restart_throttled`.
These `process_failed.reason` values are detailed failure-class metadata. For
supervisor restart causality, every `process_failed` terminal event has the
supervisor exit class `panic`.

## Supervisor Lifecycle Events

Example shapes:

```json
{"event":"supervisor_child_started","supervisor_pid":1,"supervisor_process_id":0,"supervisor_process":"Main","supervisor_id":0,"child_id":0,"child":"worker","child_pid":2,"child_process_id":1,"child_process":"Worker","spawn_site_id":0,"spawn_kind":"lexical_supervisor_child","trace_schema":"mantle-runtime-observability","trace_schema_version":1}
{"event":"supervisor_restart_decision","supervisor_pid":1,"supervisor_process_id":0,"supervisor_process":"Main","supervisor_id":0,"child_id":0,"child":"worker","child_pid":2,"child_process_id":1,"child_process":"Worker","reason":"panic","decision":"restarted","restart_time_ms":0,"restart_window_count":1,"restart_window_limit":3,"restart_window_ms":1000,"new_child_pid":3,"trace_schema":"mantle-runtime-observability","trace_schema_version":1}
```

`supervisor_id`, `child_id`, and `spawn_site_id` are admitted typed IDs.
`spawn_kind` records the lexical supervisor-child classification.
`child`, `supervisor_process`, and `child_process` are trace labels only.
`restart_time_ms` is the sampled monotonic runtime time for restart-window
decisions. `restarted` and `denied` decisions carry numeric restart time;
`not_restarted` records `null` because no restart-window sample is needed. The
restart window fields record the observed count and configured limit/window
after stale entries are pruned. The configured limit and window duration must be
greater than zero, and the observed count must not exceed the configured limit.
`restarted` records a nonzero observed count; `not_restarted` records zero
because no restart was attempted. Only a `restarted` decision carries a numeric
`new_child_pid`; `not_restarted` and `denied` decisions set `new_child_pid` to
`null`. Supervisor lifecycle events must be caused by a live supervisor PID.
Child-start events require a live child PID that was spawned by the supervisor.
Restart decisions require prior `supervisor_child_started` evidence for the
same typed supervisor/child slot, require `child_pid` to be that slot's current
child, require a prior `process_stopped`/`process_failed` event whose terminal
exit class matches the restart decision reason (`process_stopped` `normal`
matches `normal`; any `process_failed` matches `panic`), and for `restarted`
decisions require a distinct live `new_child_pid` spawned by the supervisor. A
denied restart fails the supervisor scope when intensity, capacity, or the
default same-monotonic-tick throttle prevents a restart.
Runtime supervision uses admitted supervisor tables and typed child IDs, not
labels.
