# Examples

Runnable Strata examples live under `examples/`.

Read them in this order:

1. `hello.str` for the minimum source-to-runtime program.
2. `actor_ping.str` for spawning, sending, and a single worker transition.
3. `actor_sequence.str` for multiple messages and message-keyed transitions.
4. `actor_match.str` for whole-body match authoring that checks
   into typed message-keyed transitions.
5. `init_match.str` for whole-body match authoring in `init`.
6. `init_return_match.str` for pure init return-match expressions.
7. `function_match.str` for module functions, process-local helpers, and
   pattern matching outside actor dispatch.
8. `function_payload_match.str` for payload-bearing enum construction and
   matching in normal source helpers.
9. `function_if_else.str` for pure value-level conditionals selected before
   lowering.
10. `function_collection_match.str` for immutable list/map source values and
   collection patterns in normal source helpers.
11. `function_return_match.str` for helper return-match expressions.
12. `process_return_match.str` for process step return-match expressions with
    uniform effect prefixes.
13. `function_record_pattern.str` for source helper record destructuring
   patterns.
14. `function_record_return_match.str` for helper return-match record
   destructuring.
15. `function_record_body_match.str` for whole-body helper match record
   destructuring.
16. `state_payload_enum.str` for payload-bearing process state enum transitions.
17. `collection_state.str` for immutable collection state and payload-dependent
   collection next-state templates.
18. `state_payload_match.str` for matching immutable current process state
   payloads.
19. `actor_instances.str` for multiple runtime instances of one process
   definition.
20. `actor_payloads.str` for typed message payloads and immutable payload
   bindings in actor step parameter patterns.
21. `runtime_if_else.str` for Mantle-backed runtime branching over a message
   payload.
22. `runtime_payload_projection_if.str` for Mantle-backed runtime branching over
   a projected field from an immutable received record payload.
23. `runtime_payload_projection_next_state.str` for Mantle-backed runtime
   next-state branching over a projected field from an immutable received record
   payload.
24. `runtime_state_payload_projection_next_state.str` for Mantle-backed runtime
   next-state branching over a projected field from an immutable current-state
   record payload.
25. `runtime_guard_noop.str` for omitted `else` and explicit no-op runtime
   branch behavior.
26. `runtime_for_each.str` for Mantle-backed bounded runtime iteration over a
   typed list payload.
27. `runtime_for_each_empty.str` for the zero-iteration runtime collection case.
28. `runtime_for_each_if.str` for Mantle-backed runtime branch selection inside
   bounded loop bodies.
29. `runtime_guarded_for_each.str` for guarding a whole bounded runtime loop.
30. `runtime_guarded_ref_loop.str` for routing a guarded bounded loop through a
   received direct process reference.
31. `runtime_guarded_ref_loop_jobs.str` for routing ordinary immutable `Job`
   values through a guarded loop and received direct process reference.
32. `runtime_loop_element_projection.str` for projecting immutable record
   fields from guarded runtime loop elements.
33. `actor_payload_match.str` for the same payload binding through a whole-body
   `match msg`.
34. `actor_payload_split_match.str` for payload-sensitive same-message
   splitting inside a whole-body `match msg`.
35. `actor_payload_split_signature.str` for payload-sensitive same-message
   splitting across step parameter patterns.
36. `actor_payload_split_signature_wildcard.str` for payload-sensitive
   step-signature wildcard fallback over discovered concrete payload cases.
37. `actor_payload_state_match_split.str` for payload-sensitive same-message
   splitting across state-match step clauses.
38. `actor_payload_state_match_wildcard.str` for payload-sensitive state-match
   wildcard fallback over discovered concrete payload cases.
39. `nested_patterns.str` for nested immutable constructor, record, list, and
   map payload destructuring.
40. `actor_reply.str` for transporting typed process references through message
   payloads.
41. `actor_emit_spawn_send.str` for one transition with declared emit, spawn,
   and send authority.
42. `actor_panic_no_replay.str` for fail-closed actor failure and no replay
   after message dequeue.

## Hello

`examples/hello.str` is the first source-to-runtime gate. It checks,
builds, runs, emits `hello from Strata`, and records an observability trace.

```sh
cargo build
cargo run -p strata --bin strata -- check examples/hello.str
cargo run -p strata --bin strata -- build examples/hello.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
```

Key source ideas:

- `Main` is the entry process.
- `MainMsg.Start` is the entry message.
- `emit` is declared in the `step` effect list.
- `Stop(state)` terminates normally without changing state.

## Actor Ping

`examples/actor_ping.str` is the first actor/runtime gate. It spawns a worker,
sends a message, handles that message, updates state, terminates normally, and
records the runtime trace.

```sh
cargo run -p strata --bin strata -- check examples/actor_ping.str
cargo run -p strata --bin strata -- build examples/actor_ping.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_ping.mta
```

Key source ideas:

- `Main` uses `let worker: ProcessRef<Worker> = spawn Worker;` before `send worker Ping;`.
- `WorkerMsg.Ping` is checked against `Worker`'s message type.
- `Worker` replaces `Idle` with `Handled`.
- Both processes stop normally.

## Actor Sequence

`examples/actor_sequence.str` exercises message-keyed process transitions. The
worker handles `First`, returns a whole replacement state with `Continue(...)`,
then handles `Second` through the wildcard clause and returns a whole
replacement state with `Stop(...)`.

```sh
cargo run -p strata --bin strata -- check examples/actor_sequence.str
cargo run -p strata --bin strata -- build examples/actor_sequence.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta
```

Key source ideas:

- `WorkerMsg` has two variants, and each variant resolves to exactly one
  `step` clause.
- The explicit `First` clause handles `First`; `_` covers the remaining
  accepted message variants.
- `Continue(SawFirst)` keeps the worker alive for the next queued message.
- `Stop(Done)` terminates the worker normally.

The runtime trace records process, message, state, and output IDs alongside
labels so that behavior can be checked without treating labels as executable
bindings.

## Actor Match

`examples/actor_match.str` exercises the whole-body `match msg`
authoring form. The checker resolves each arm into the same typed transition
table used by step parameter patterns.

```sh
cargo run -p strata --bin strata -- check examples/actor_match.str
cargo run -p strata --bin strata -- build examples/actor_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_match.mta
```

Key source ideas:

- `fn step(state: WorkerState, msg: WorkerMsg)` declares a typed message
  parameter.
- `match msg` must be the whole function body in this source slice.
- Each arm returns a whole replacement state through `Continue(...)` or
  `Stop(...)`.
- The generated Mantle artifact still dispatches by typed message IDs.

## Init Match

`examples/init_match.str` exercises a non-step whole-body match in `init`. The
checker resolves the fieldless enum scrutinee, proves the arms are exhaustive,
and selects the typed initial state before lowering.

```sh
cargo run -p strata --bin strata -- check examples/init_match.str
cargo run -p strata --bin strata -- build examples/init_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/init_match.mta
```

Key source ideas:

- `match Warm` is checked against `StartupMode`.
- Both `Cold` and `Warm` arms return immutable whole `MainState` record values.
- The Mantle trace starts `Main` in `MainState{readiness:WarmReady}`, proving
  the selected initial state reached runtime admission.

## Init Return Match

`examples/init_return_match.str` exercises a pure `return match` expression in
`init`. The checker resolves the fieldless enum scrutinee, proves the arms are
exhaustive, selects one whole initial state value, and lowers that state through
the existing typed artifact state table.

```sh
cargo run -p strata --bin strata -- check examples/init_return_match.str
cargo run -p strata --bin strata -- build examples/init_return_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/init_return_match.mta
```

Key source ideas:

- `return match Warm { ... };` is accepted only because `Warm` is a fieldless
  enum constructor.
- Each arm is statement-free and returns one immutable whole `MainState` value.
- Mantle receives the selected typed initial state ID; it does not dispatch on
  the source match arm names.

## Function Match

`examples/function_match.str` exercises normal source functions outside actor
dispatch. It uses module-level functions and process-local helpers, including
signature-pattern dispatch and a whole-body match helper.

```sh
cargo run -p strata --bin strata -- check examples/function_match.str
cargo run -p strata --bin strata -- build examples/function_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_match.mta
```

Key source ideas:

- `readiness_sig(Cold)` and `readiness_sig(Warm)` are module-level function
  clauses selected by typed signature patterns.
- `readiness_body(mode: StartupMode)` uses a whole-body `match mode` outside
  an actor `step`.
- `Main` and `Worker` declare process-local helper functions for state
  construction.
- `send worker Assign(ready_job(Ready))` proves helper calls are expanded for
  message payload discovery and lowering.
- Mantle sees typed state IDs, message IDs, and payload templates, not source
  helper dispatch names.

## Function Payload Match

`examples/function_payload_match.str` extends normal source helpers to
payload-bearing enum values. It constructs source-visible enum payload values,
matches them through signature patterns and whole-body helper matches, and
lowers a received actor payload through a process-local helper into an enum
payload state template.

```sh
cargo run -p strata --bin strata -- check examples/function_payload_match.str
cargo run -p strata --bin strata -- build examples/function_payload_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_payload_match.mta
```

Key source ideas:

- `Assigned(Job { phase: Ready })` is resolved as a typed enum value, not a
  helper call.
- `status_sig(Assigned(job: Job))` binds the enum payload immutably in a normal
  source helper signature pattern.
- `status_body(work: Work)` matches the typed helper parameter and binds the
  payload inside the selected arm.
- `state_for(Assigned(job))` proves a process-local helper can wrap a received
  immutable payload into a source enum value before lowering.

## Function If Else

`examples/function_if_else.str` uses a pure value-level conditional in normal
source helpers. The checker resolves the explicit `Bool { False, True }`
condition and selects one immutable branch before lowering.

```sh
cargo run -p strata --bin strata -- check examples/function_if_else.str
cargo run -p strata --bin strata -- build examples/function_if_else.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_if_else.mta
```

Key source ideas:

- `enum Bool { False, True }` is an exact source contract for conditionals.
- `if (flag) { WarmReady } else { ColdReady }` is a pure value expression, not
  a statement block.
- Both branches are checked against the same expected type.
- Mantle receives selected typed state values, not a conditional branch key or
  source helper dispatch name.

## Function Collection Match

`examples/function_collection_match.str` uses immutable `List<T,N>` and
`Map<K,V,N>` source values in normal source helpers.

```sh
cargo run -p strata --bin strata -- check examples/function_collection_match.str
cargo run -p strata --bin strata -- build examples/function_collection_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_collection_match.mta
```

Key source ideas:

- `List<Phase,2>[Ready, Done]` and `Map<Phase,Phase,2>[Ready => Done, Unknown => Unknown]` are typed,
  bounded immutable collection values.
- `fn first(List<Phase,2>[phase, _])` dispatches on exact list length and binds
  one immutable element.
- Helper body and return matches can use exact list patterns, list rest patterns
  such as `List[_, ..tail]`, exact map patterns, and subset map patterns such as
  `Map[Ready => selected, ..rest]` with `_` fallback arms.
- The helper expansion leaves Mantle with a resolved `MainState` value, not
  source helper dispatch names.

## Function Return Match

`examples/function_return_match.str` uses a helper `return match` expression to
select an immutable result from an in-scope source value binding.

```sh
cargo run -p strata --bin strata -- check examples/function_return_match.str
cargo run -p strata --bin strata -- build examples/function_return_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_return_match.mta
```

Key source ideas:

- `return match work { ... };` is checked as a pure helper return expression.
- The selected arm binds enum payloads immutably before helper expansion.
- Mantle receives the resolved `MainState{status:Active(Job{phase:Ready})}`
  state value, not source helper dispatch.

## Process Return Match

`examples/process_return_match.str` uses a process `step return match` over a
concrete enum payload binding after a uniform `emit` prefix.

```sh
cargo run -p strata --bin strata -- check examples/process_return_match.str
cargo run -p strata --bin strata -- build examples/process_return_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/process_return_match.mta
```

Key source ideas:

- `Envelope(Assign(phase: Phase))` binds an immutable enum payload whose
  concrete value is already proven by payload-sensitive dispatch.
- `emit "process return match uniform prefix";` lowers as the same typed action
  prefix on every selected transition.
- `return match phase { ... };` is checked and reduced to a typed
  `Continue(...)` or `Stop(...)` transition before lowering.
- Mantle executes the emitted typed transition IDs and payload guards; it does
  not dispatch on source strings or helper names.

## Function Record Pattern

`examples/function_record_pattern.str` destructures an immutable record value in
a normal source helper signature.

```sh
cargo run -p strata --bin strata -- check examples/function_record_pattern.str
cargo run -p strata --bin strata -- build examples/function_record_pattern.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_record_pattern.mta
```

Key source ideas:

- `fn phase_of(Job { phase })` binds the `phase` field as an immutable source
  value.
- `field: binding` may rename a record field binding in the helper signature.
- The helper expands before lowering; Mantle admits the resolved
  `MainState{phase:Ready}` state value.

## Function Record Return Match

`examples/function_record_return_match.str` destructures an immutable record value
inside a helper `return match` expression.

```sh
cargo run -p strata --bin strata -- check examples/function_record_return_match.str
cargo run -p strata --bin strata -- build examples/function_record_return_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_record_return_match.mta
```

Key source ideas:

- `return match job { Job { phase } => { return phase; } };` binds the
  `phase` field as an immutable source value inside the selected arm.
- Record return-match destructuring requires the scrutinee to be an in-scope
  source binding with a concrete record value.
- The helper expands before lowering; Mantle admits the resolved
  `MainState{phase:Ready}` state value.

## Function Record Body Match

`examples/function_record_body_match.str` destructures an immutable record value
inside a whole-body source helper match.

```sh
cargo run -p strata --bin strata -- check examples/function_record_body_match.str
cargo run -p strata --bin strata -- build examples/function_record_body_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/function_record_body_match.mta
```

Key source ideas:

- `match job { Job { phase } => { return phase; } }` binds the `phase` field
  as an immutable source value inside the selected arm.
- Record body-match destructuring requires one record pattern arm over the
  helper parameter's concrete record value.
- The helper expands before lowering; Mantle admits the resolved
  `MainState{phase:Ready}` state value.

## State Payload Enum

`examples/state_payload_enum.str` admits payload-bearing process state enum
values. It starts a worker in fieldless `Idle`, receives an immutable `Job`
payload, and transitions to `Working(Job { phase: Ready })`.

```sh
cargo run -p strata --bin strata -- check examples/state_payload_enum.str
cargo run -p strata --bin strata -- build examples/state_payload_enum.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/state_payload_enum.mta
```

Key source ideas:

- `enum WorkerState { Idle, Working(Job) }` declares a payload-bearing state
  variant.
- `return Stop(Working(job));` constructs a whole replacement state from an
  immutable received payload.
- Mantle receives a next-state value template and admits the resulting
  `Working(Job{phase:Ready})` only because it is present in the typed state
  table.

## Collection State

`examples/collection_state.str` admits immutable `List<Phase,1>` and
`Map<Phase,Phase,2>` process states and lowers received payloads, including list
rest and subset map payload projections, into collection next-state templates.

```sh
cargo run -p strata --bin strata -- check examples/collection_state.str
cargo run -p strata --bin strata -- build examples/collection_state.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/collection_state.mta
```

Key source ideas:

- `type State = List<Phase,1>;` and `type State = Map<Phase,Phase,2>;` make
  worker states collection values rather than records or enums.
- `return Stop(tail);` and
  `return Stop(Map<Phase,Phase,2>[Ready => next, Done => Ready]);` create new
  immutable whole collection states from received payload bindings.
- Mantle receives typed list-rest and map value templates and evaluates them
  during runtime execution.

## State Payload Match

`examples/state_payload_match.str` matches the current process state as typed
immutable data. The worker first enters `Working(Job { phase: Ready })`; a later
`Complete` message dispatches over the current state and binds `job` inside the
selected state arm.

```sh
cargo run -p strata --bin strata -- check examples/state_payload_match.str
cargo run -p strata --bin strata -- build examples/state_payload_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/state_payload_match.mta
```

Key source ideas:

- `fn step(state: WorkerState, Complete)` can use a whole-body `match state`.
- `Working(job: Job)` binds the state enum payload immutably for that transition
  arm only.
- `return Stop(Done(job));` returns a whole replacement state; it does not mutate
  the current state in place.
- The Mantle artifact carries state-specific transitions keyed by admitted
  message ID plus admitted current state ID, and the payload-derived next state
  uses a typed current-state payload template.

## Actor Instances

`examples/actor_instances.str` proves process references and instance-aware sends.
`Main` spawns the `Worker` process definition twice, binds each runtime instance
to a different process reference, and sends `Ping` through both references.

```sh
cargo run -p strata --bin strata -- check examples/actor_instances.str
cargo run -p strata --bin strata -- build examples/actor_instances.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_instances.mta
```

Key source ideas:

- `let first: ProcessRef<Worker> = spawn Worker;` and
  `let second: ProcessRef<Worker> = spawn Worker;` create two runtime worker
  instances.
- `send first Ping;` and `send second Ping;` dispatch by reference, not by process
  definition label.
- The runtime trace records two different `pid` values with the same
  `process_id` for `Worker`.

## Actor Payloads

`examples/actor_payloads.str` sends a typed payload from `Main` to `Worker`.
`Worker` binds that payload with a `step` parameter pattern and returns a whole
replacement state containing the immutable payload value.

```sh
cargo run -p strata --bin strata -- check examples/actor_payloads.str
cargo run -p strata --bin strata -- build examples/actor_payloads.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payloads.mta
```

Key source ideas:

- `enum WorkerMsg { Assign(Job) }` declares a payload-bearing message variant.
- `send worker Assign(Job { phase: Ready });` sends one checked payload value.
- `Assign(job: Job)` binds the received payload as an immutable step-local
  value.
- `WorkerState { job: job }` constructs the next state as a whole value.

## Runtime If Else

`examples/runtime_if_else.str` branches inside `Worker.step` over a received
`Bool` payload. The branch is not selected by Strata during checking; it lowers
as typed Mantle control flow and executes when each worker handles its message.

```sh
cargo run -p strata --bin strata -- check examples/runtime_if_else.str
cargo run -p strata --bin strata -- build examples/runtime_if_else.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_if_else.mta
```

Key source ideas:

- `Branch(True)` and `Branch(False)` send two different runtime payloads.
- `if (flag == True) { ... } else { ... }` lowers to Mantle branch control
  flow through a typed equality template.
- Each branch emits its own declared output and returns a whole immutable state.
- The runtime trace records `branch_selected` for both `then` and `else` paths.

## Runtime Payload Projection If

`examples/runtime_payload_projection_if.str` branches inside `Worker.step` over a
field destructured from a received `Job` record payload. The source binding is
immutable step-local syntax; the runtime branch lowers through a typed Mantle
record-field projection over the received payload.

```sh
cargo run -p strata --bin strata -- check examples/runtime_payload_projection_if.str
cargo run -p strata --bin strata -- build examples/runtime_payload_projection_if.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_payload_projection_if.mta
```

Key source ideas:

- `Assign(Job { phase: assigned_phase })` destructures the received record
  payload without introducing mutation.
- `if (assigned_phase == Ready) { ... } else { ... }` lowers as typed Mantle
  branch control through `ReceivedPayload<Job>`, `RecordField("phase")`, and a
  `Phase` equality predicate.
- Only the ready payload emits output; the done payload takes the empty branch.

## Runtime Payload Projection Next State

`examples/runtime_payload_projection_next_state.str` branches inside
`Worker.step` over a field destructured from a received `Job` record payload.
The branch controls the final next-state result, and the state change remains a
whole immutable value returned through `Continue`.

```sh
cargo run -p strata --bin strata -- check examples/runtime_payload_projection_next_state.str
cargo run -p strata --bin strata -- build examples/runtime_payload_projection_next_state.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_payload_projection_next_state.mta
```

Key source ideas:

- `Assign(Job { phase: assigned_phase })` is source syntax for an immutable
  payload destructuring binding.
- The final-position `if (assigned_phase == Ready) { ... } else { ... }`
  lowers to Mantle `NextState::IfElse` through `ReceivedPayload<Job>`,
  `RecordField("phase")`, and a `Phase` equality predicate.
- Both branches return whole `WorkerState` enum values; the artifact does not
  use the source alias as an executable runtime path.

## Runtime State Payload Projection Next State

`examples/runtime_state_payload_projection_next_state.str` stores immutable
`Job` records in process state, later destructures the current state payload,
and uses the projected field to choose a final next state inside Mantle.

```sh
cargo run -p strata --bin strata -- check examples/runtime_state_payload_projection_next_state.str
cargo run -p strata --bin strata -- build examples/runtime_state_payload_projection_next_state.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_state_payload_projection_next_state.mta
```

Key source ideas:

- `Assign(job: Job)` stores the whole received job as `Holding(job)`.
- `Holding(Job { phase: held_phase })` destructures the immutable current-state
  payload for the selected `Decide` transition arm.
- The final-position `if (held_phase == Ready) { ... } else { ... }` lowers to
  Mantle `NextState::IfElse` through `CurrentStatePayload<Job>`,
  `RecordField("phase")`, and a `Phase` equality predicate.
- Both branches return whole `WorkerState` enum values; the artifact does not
  use the source alias as an executable runtime path.

## Runtime Guard Noop

`examples/runtime_guard_noop.str` shows statement-level runtime branches where
one selected branch intentionally performs no effects. The conditions are still
checked `Bool` predicates and lower into typed Mantle branch actions.

```sh
cargo run -p strata --bin strata -- check examples/runtime_guard_noop.str
cargo run -p strata --bin strata -- build examples/runtime_guard_noop.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guard_noop.mta
```

Key source ideas:

- `if (flag == True) { ... }` lowers an omitted `else` as an explicit empty
  Mantle branch.
- `else {}` is an explicit no-op branch; `{}` on one side is allowed when the
  sibling branch has an admitted effect.
- Empty selected branches do not emit, send, acquire authority, or change state,
  but they still record `branch_selected`.
- Both branches empty are rejected before runnable behavior is admitted.

## Runtime For Each

`examples/runtime_for_each.str` iterates inside `BatchWorker.step` over a
received `List<Bool,2>` payload. The loop is not unrolled or selected by Strata
during checking; it lowers as typed Mantle loop control flow and executes once
per runtime element.

```sh
cargo run -p strata --bin strata -- check examples/runtime_for_each.str
cargo run -p strata --bin strata -- build examples/runtime_for_each.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each.mta
```

`examples/runtime_for_each_empty.str` uses the same shape with
`List<Bool,0>[]` and proves that Mantle records loop start/completion without
executing the body.

```sh
cargo run -p strata --bin strata -- check examples/runtime_for_each_empty.str
cargo run -p strata --bin strata -- build examples/runtime_for_each_empty.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each_empty.mta
```

Key source ideas:

- `for item in items { ... }` requires `items` to be a typed runtime list
  binding.
- `item` is an immutable per-iteration value binding lowered to a typed loop
  element ID.
- The loop body sends `Branch(item)` in collection order.
- The runtime trace records `loop_started`, one `loop_iteration` per item, and
  `loop_completed`.

`examples/runtime_for_each_if.str` extends the same runtime loop with
statement-level branch control inside the loop body.

```sh
cargo run -p strata --bin strata -- check examples/runtime_for_each_if.str
cargo run -p strata --bin strata -- build examples/runtime_for_each_if.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each_if.mta
```

Key source ideas:

- `if ((item != False) && !(item == False)) { ... } else { ... }` runs in
  Mantle for each loop iteration through typed Boolean predicate templates.
- The branch condition uses the admitted typed loop element ID; branch payloads
  remain typed loop element values.
- One branch may be empty when the sibling branch has admitted effects.
  Branches can emit and send, but they cannot return, spawn, loop, or nest
  another statement-level branch in this slice.
- The runtime trace records `loop_iteration`, `branch_selected`, branch effects,
  and `loop_completed` in deterministic collection order.

`examples/runtime_guarded_for_each.str` guards a whole bounded runtime loop with
a statement-level branch.

```sh
cargo run -p strata --bin strata -- check examples/runtime_guarded_for_each.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_for_each.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_for_each.mta
```

Key source ideas:

- `if (enabled == True) { for item in items { ... } } else {}` lowers as a
  typed Mantle branch whose selected `then` branch contains a bounded loop.
- The disabled selected branch records `branch_selected` but emits no loop
  events and performs no branch-local work.
- The enabled selected branch records `branch_selected`, then `loop_started`,
  ordered `loop_iteration` body effects, and `loop_completed`.
- The guarded branch and loop body keep the same restrictions: no nested loops,
  no `spawn`, no `return`, no assignment, and no process-reference loop element
  type. A statement-level branch body still cannot directly contain another
  statement-level branch.

`examples/runtime_guarded_ref_loop.str` routes that guarded-loop send through a
direct `ProcessRef<Worker>` received as the current message payload.

```sh
cargo run -p strata --bin strata -- check examples/runtime_guarded_ref_loop.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_ref_loop.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_ref_loop.mta
```

Key source ideas:

- `BatchWorker` stores only value data in state. The worker reference remains a
  direct message payload on `Route(ProcessRef<Worker>)`.
- The selected enabled branch sends `Branch(item)` through the received
  reference from inside the guarded loop body.
- The disabled branch records only `branch_selected`; it emits no loop events
  and performs no branch-local or loop-local authority acquisition.
- Lowering emits a typed received-payload send target. Runtime dispatch uses the
  transported runtime process ID plus admitted target process ID, not the source
  payload binding name.

## Runtime Guarded Ref Loop Jobs

`examples/runtime_guarded_ref_loop_jobs.str` keeps the same received direct
`ProcessRef<Worker>` routing shape, but the guarded loop iterates over
ordinary immutable `Job` record values.

```sh
cargo run -p strata --bin strata -- check examples/runtime_guarded_ref_loop_jobs.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_ref_loop_jobs.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_ref_loop_jobs.mta
```

Key source ideas:

- The guard remains a `Bool` predicate over current `BatchRequest` state.
- The loop collection is `List<Job,2>`, and `Job` is plain value data.
- The worker reference remains direct message authority on
  `Route(ProcessRef<Worker>)`; it is not stored in state or nested inside the
  job list.
- Lowering emits the `jobs` projection as a current-state value template, the
  loop element as `Job`, and the send target as a typed received-payload
  process reference.

## Runtime Loop Element Projection

`examples/runtime_loop_element_projection.str` projects immutable `Job.phase`
data from each loop element, branches on the typed `Phase`, and sends only the
`Ready` phase through the received direct `ProcessRef<Worker>`.

```sh
cargo run -p strata --bin strata -- check examples/runtime_loop_element_projection.str
cargo run -p strata --bin strata -- build examples/runtime_loop_element_projection.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_loop_element_projection.mta
```

Key source ideas:

- The loop item uses a record pattern, `Job { phase: routed_phase }`, to bind an
  immutable field value inside the loop body.
- Lowering emits `RecordField(LoopElement(...), "phase")` typed templates for
  the inner branch condition and send payload.
- Runtime execution uses admitted loop element IDs, type IDs, and received
  process-ref payload targets. The source binding alias is not executable
  dispatch metadata.

## Actor Payload Match

`examples/actor_payload_match.str` proves the same immutable payload binding
works from a whole-body `match msg` arm, not only from a `step` signature.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_match.str
cargo run -p strata --bin strata -- build examples/actor_payload_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_match.mta
```

Key source ideas:

- `fn step(state: WorkerState, msg: WorkerMsg)` binds the message parameter.
- `match msg` dispatches through the same typed pattern validation used by
  signature patterns.
- `Assign(job: Job)` binds the received payload immutably inside the match arm.
- Runtime still dispatches by admitted message IDs and payload type IDs.

## Actor Payload Split Match

`examples/actor_payload_split_match.str` proves that one top-level message
variant can be split inside a whole-body `match msg` by disjoint nested typed
payload predicates.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_split_match.str
cargo run -p strata --bin strata -- build examples/actor_payload_split_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_split_match.mta
```

Key source ideas:

- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` share the same
  top-level message constructor and differ only by nested payload identity.
- The checker admits the split only because the nested typed predicates are
  provably disjoint over discovered concrete payload cases.
- Lowering emits exact typed payload guards in Mantle transition records.
- Runtime dispatch uses admitted message IDs, current state IDs when applicable,
  and exact typed payload identity, not source strings or debug labels.

## Actor Payload Split Signature

`examples/actor_payload_split_signature.str` proves the same payload-sensitive
same-message split through step parameter patterns rather than a whole-body
`match msg`.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_split_signature.str
cargo run -p strata --bin strata -- build examples/actor_payload_split_signature.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_split_signature.mta
```

Key source ideas:

- Multiple `fn step(state, Envelope(Assign(...)))` clauses can share one
  top-level message constructor when the nested typed predicates are provably
  disjoint.
- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` lower to two typed
  payload-guarded Mantle transitions for the same admitted message ID.
- Mantle selects the transition by admitted message ID, current state ID when
  applicable, and exact typed payload identity.
- `actor_payload_split_match.str` exercises the equivalent whole-body
  `match msg` authoring form.

## Actor Payload Split Signature Wildcard

`examples/actor_payload_split_signature_wildcard.str` proves that a
payload-sensitive step-signature split can use `_` as fallback for discovered
concrete payload cases not handled by explicit nested predicates.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_split_signature_wildcard.str
cargo run -p strata --bin strata -- build examples/actor_payload_split_signature_wildcard.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_split_signature_wildcard.mta
```

Key source ideas:

- `Envelope(Assign(Ready))` handles the explicitly guarded payload case.
- `_` handles the discovered `Envelope(Assign(Done))` case, and lowering emits
  a typed payload guard for `Assign(Done)` rather than an open runtime catch-all.
- The fallback remains bounded to discovered concrete payload cases admitted by
  checking.
- Runtime dispatch still uses admitted message IDs and exact typed payload
  identity, not source strings or debug labels.

## Actor Payload State-Match Split

`examples/actor_payload_state_match_split.str` proves that state-match step
clauses can share one top-level message constructor by disjoint nested typed
payload predicates.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_state_match_split.str
cargo run -p strata --bin strata -- build examples/actor_payload_state_match_split.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_state_match_split.mta
```

Key source ideas:

- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` are explicit,
  discovered payload cases for the same admitted message constructor.
- Each payload case expands across the admitted `Idle`, `SawReady`, and `Done`
  current-state cases from `match state`.
- Lowering emits typed Mantle transitions keyed by message ID, current state ID,
  and exact typed payload guard.
- State changes remain immutable whole-value returns through `Continue(...)` or
  `Stop(...)`; runtime dispatch does not use source strings or debug labels.

## Actor Payload State-Match Wildcard

`examples/actor_payload_state_match_wildcard.str` proves that a
payload-sensitive state-match split can use `_` as fallback for discovered
concrete payload cases not handled by explicit state-match clauses.

```sh
cargo run -p strata --bin strata -- check examples/actor_payload_state_match_wildcard.str
cargo run -p strata --bin strata -- build examples/actor_payload_state_match_wildcard.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_state_match_wildcard.mta
```

Key source ideas:

- `Envelope(Assign(Ready))` handles the explicitly guarded payload case.
- The wildcard state-match step handles the discovered `Envelope(Assign(Done))`
  case; lowering emits `Assign(Done)` as an exact typed payload guard rather
  than an unguarded payload catch-all.
- Each explicit and fallback payload case expands across the admitted `Idle`,
  `SawReady`, and `Done` current-state cases from its `match state` body.
- State changes remain immutable whole-value returns through `Continue(...)` or
  `Stop(...)`; runtime dispatch does not use source strings or debug labels.

## Nested Patterns

`examples/nested_patterns.str` composes immutable destructuring across
constructor payloads, records, list elements/rest, and map values/rest.

```sh
cargo run -p strata --bin strata -- check examples/nested_patterns.str
cargo run -p strata --bin strata -- build examples/nested_patterns.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/nested_patterns.mta
```

Key source ideas:

- `AssignEnvelope(Assign(Job { phase }))` binds a nested record field from a
  constructor payload through typed projection paths.
- `HoldEnvelope(Hold(List[Job { phase }, ..tail]))` binds an immutable list
  suffix whole value, not a mutable view.
- `LookupEnvelope(Lookup(Map[Ready => Job { phase }, ..rest]))` keeps map
  matching on static keys while binding nested values and an immutable rest map.
- Lowering emits typed Mantle value templates; runtime execution does not use
  source strings as executable references.

## Actor Reply References

`examples/actor_reply.str` passes a `ProcessRef<Sink>` as a typed immutable
message payload. `Worker` receives that reference and sends `Done` through it.

```sh
cargo run -p strata --bin strata -- check examples/actor_reply.str
cargo run -p strata --bin strata -- build examples/actor_reply.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_reply.mta
```

Key source ideas:

- `enum WorkerMsg { Work(ProcessRef<Sink>) }` declares a typed reference
  payload.
- `send worker Work(sink);` transports the spawned `Sink` reference to
  `Worker`.
- `Work(reply_to: ProcessRef<Sink>)` binds the received reference immutably.
- `send reply_to Done;` routes by the transported runtime process ID and
  admitted target process ID, not by source labels.

## Actor Emit Spawn Send

`examples/actor_emit_spawn_send.str` combines `emit`, `spawn`, and `send` in
one admitted transition. `Main` declares each effect, spawns `Worker`, sends
`Ping` through the typed process reference, and stops with a whole replacement
state.

```sh
cargo run -p strata --bin strata -- check examples/actor_emit_spawn_send.str
cargo run -p strata --bin strata -- build examples/actor_emit_spawn_send.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_emit_spawn_send.mta
```

Key source ideas:

- `fn step(...) -> ProcResult<MainState> ! [emit, spawn, send]` declares the
  exact authority used by the body.
- `let worker: ProcessRef<Worker> = spawn Worker;` creates a typed process
  reference.
- `send worker Ping;` dispatches by the admitted process-reference target and
  message ID after lowering, not by source text.
- `return Stop(MainState { phase: Done });` preserves immutable whole-state
  transition semantics.

## Actor Panic No Replay

`examples/actor_panic_no_replay.str` admits an explicit abnormal transition.
`Main` queues two `Ping` messages to `Worker`; `Worker` dequeues one message,
returns `Panic(Failed)`, records failure evidence, and the consumed message is
not replayed.

```sh
cargo run -p strata --bin strata -- check examples/actor_panic_no_replay.str
cargo run -p strata --bin strata -- build examples/actor_panic_no_replay.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_panic_no_replay.mta
```

The final command is expected to return non-zero. The runtime trace should show
two accepted `Ping` messages, one `message_dequeued` for `Worker`, one
`process_stepped` event with `result:"Panic"`, and one `process_failed` event.
