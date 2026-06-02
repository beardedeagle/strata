# Runtime And Actor Examples

These examples cover runtime state transitions, branching, loops, payload dispatch, process references, and failure behavior.

## Runtime Scalar Priority

`examples/runtime_scalar_priority.str` sends a typed `U32` payload to a worker,
computes an immutable scalar priority in process-local pure functions, and lets
Mantle select the runtime branch and the runtime-bound priority value from
typed scalar templates.

```sh
just run-example runtime_scalar_priority
```

Key source ideas:

- `Assign(U32)` is a typed scalar message payload, not a string payload.
- `compute_level` uses immutable `U32` source-local bindings and checked scalar
  arithmetic.
- `compute_level(weight) >= 10_u32` lowers into typed Mantle scalar templates;
  `high_priority(weight)` is the pure Bool function used by the runtime branch.
- `classify(weight)` returns `if (urgent) { High } else { Normal }`; when
  `urgent` depends on the message payload, lowering emits a typed Mantle
  value-if template for the selected state field.
- The selected branch emits `scalar priority high`, proving Mantle executed the
  scalar branch path.

## State Payload Enum

`examples/state_payload_enum.str` shows payload-bearing process state enum
values. It starts a worker in fieldless `Idle`, receives an immutable `Job`
payload, and transitions to `Working(Job { phase: Ready })`.

```sh
just run-example state_payload_enum
```

Key source ideas:

- `enum WorkerState { Idle, Working(Job) }` declares a payload-bearing state
  variant.
- `return Stop(Working(job));` constructs a whole replacement state from an
  immutable received payload.
- Mantle receives a next-state value template and resolves the resulting
  `Working(Job{phase:Ready})` only because it is present in the typed state
  table.

## Collection State

`examples/collection_state.str` shows immutable `List<Phase,1>` and
`Map<Phase,Phase,2>` process states and lowers received payloads, including list
rest and subset map payload projections, into collection next-state templates.

```sh
just run-example collection_state
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
just run-example state_payload_match
```

Key source ideas:

- `fn step(state: WorkerState, Complete)` can use a whole-body `match state`.
- `Working(job: Job)` binds the state enum payload immutably for that transition
  arm only.
- `return Stop(Done(job));` returns a whole replacement state; it does not mutate
  the current state in place.
- The Mantle artifact carries state-specific transitions keyed by typed
  message ID plus checked current state ID, and the payload-derived next state
  uses a typed current-state payload template.

## Actor Instances

`examples/actor_instances.str` proves process references and instance-aware sends.
`Main` spawns the `Worker` process definition twice, binds each runtime instance
to a different process reference, and sends `Ping` through both references.

```sh
just run-example actor_instances
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
just run-example actor_payloads
```

Key source ideas:

- `enum WorkerMsg { Assign(Job) }` declares a payload-bearing message variant.
- `send worker Assign(Job { phase: Ready });` sends one checked payload value.
- `Assign(job: Job)` binds the received payload as an immutable step-local
  value.
- `WorkerState { job: job }` constructs the next state as a whole value.

## Component Composition

`examples/component_composition_main.str` imports
`examples/component_composition_worker.str`, declares a `MainComponent` that
imports `WorkerPort`, and admits a local `AppComposition` binding the import to
the worker component export. The binding is checked as a typed graph edge before
lowering; Mantle admits typed component-instance and port IDs as
metadata/admission data, while runtime send dispatch still uses loaded typed
process, message, and port IDs.

```sh
just run-example component_composition_main
```

## Typed Effect Outcomes

`examples/effect_outcomes.str` binds local spawn and send results as immutable
`Result` values, branches on typed outcome variant patterns, and stores the
finite send result in the next state as whole-value data.

```sh
just run-example effect_outcomes
```

Key source ideas:

- `let spawn_result: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;`
  returns the committed process reference on accepted local spawn.
- `let send_result: Result<Unit,SendError<WorkerMsg>> = send worker Work;`
  returns `Ok(Unit)` after Mantle accepts the message, or typed
  `SendError<WorkerMsg>` before acceptance.
- Outcome branches such as `spawn_result != Err(Exhausted(Unit))` and
  `send_result == Ok(Unit)` use immutable typed variants, not process-reference
  identity comparisons.
- The next state stores `send_result`; `spawn_result` stays step-local because
  success carries process-reference authority rather than ordinary source state.
- The artifact uses typed outcome IDs and value templates, not source binding
  names, for runtime meaning.
- `effect_outcome_mailbox_full`, `effect_outcome_stopped_target`, and
  `effect_outcome_crashed_target` cover pre-acceptance failure, stopped/already
  failed targets, and the current fail-closed source-created panic boundary.
- `effect_outcome_spawn_denied` covers denied admitted spawn authority before a
  `Worker` instance is accepted.
- `effect_outcome_spawn_exhausted` covers process-capacity exhaustion before a
  `Worker` is accepted under `--max-runtime-processes 1`; the exhausted branch
  emits without admitting or executing the child. The runtime trace records
  `effect_outcome_bound` with `outcome_result:"exhausted"`.
- The local supervision examples cover lexical child sends, explicit restart
  intensity, child modes, stopped- and crashed-child send outcomes, and restart
  traces without replaying consumed messages; crashed-child send outcomes
  record `effect_outcome_bound` with `outcome_result:"crashed"`.

## Runtime If Else

`examples/runtime_if_else.str` branches inside `Worker.step` over a received
`Bool` payload. The branch is not selected by Strata during checking; it lowers
as typed Mantle control flow and executes when each worker handles its message.

```sh
just run-example runtime_if_else
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
just run-example runtime_payload_projection_if
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
just run-example runtime_payload_projection_next_state
```

Key source ideas:

- `Assign(Job { phase: assigned_phase })` is source syntax for an immutable
  payload destructuring binding.
- The final-position `if (assigned_phase == Ready) { ... } else { ... }`
  lowers to Mantle `NextState::IfElse` through `ReceivedPayload<Job>`,
  `RecordField("phase")`, and a `Phase` equality predicate.
- Both branches return whole `WorkerState` enum values; the artifact does not
  use the source alias as an executable runtime path.

## Runtime State Payload Projection If

`examples/runtime_state_payload_projection_if.str` stores immutable `Job`
records in process state, later destructures the current state payload, and uses
the projected field to select a statement-level runtime branch action inside
Mantle.

```sh
just run-example runtime_state_payload_projection_if
```

Key source ideas:

- `Assign(job: Job)` stores the whole received job as `Holding(job)`.
- `Holding(Job { phase: held_phase })` destructures the immutable current-state
  payload for the selected `Decide` transition arm.
- The statement-level `if (held_phase == Ready) { ... } else { ... }` lowers to
  Mantle `ArtifactAction::IfElse` through `CurrentStatePayload<Job>`,
  `RecordField("phase")`, and a `Phase` equality predicate.
- Both branches emit from typed runtime action selection, then return
  `Continue(state)` as a whole-value continuation; the artifact does not use the
  source alias as an executable runtime path.

## Runtime State Payload Projection Next State

`examples/runtime_state_payload_projection_next_state.str` stores immutable
`Job` records in process state, later destructures the current state payload,
and uses the projected field to choose a final next state inside Mantle.

```sh
just run-example runtime_state_payload_projection_next_state
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

## Runtime Nested If Actions

`examples/runtime_nested_if_actions.str` sends immutable `CheckFlags` record
payloads to three workers and uses one nested statement-level runtime branch to
choose effect-only action output inside Mantle.

```sh
just run-example runtime_nested_if_actions
```

Key source ideas:

- `Check(CheckFlags { outer_flag: primary, inner_flag: secondary })`
  destructures the received record payload into immutable source bindings.
- The outer and inner `if` conditions lower through typed
  `RecordField(ReceivedPayload<CheckFlags>, ...)` Bool equality templates.
- The nested branch is an typed Mantle action with a stable nested
  `branch_path`; source aliases such as `primary` and `secondary` are not
  executable dispatch paths.
- State remains a whole-value `Continue(state)` after the effect-only nested
  branch action; branch-local `spawn`, process-reference binding, `return`,
  nested loops, and deeper direct branch nesting remain rejected.

## Runtime Final If Guarded Loop

`examples/runtime_final_if_guarded_loop.str` sends immutable `CheckFlags` record
payloads to three workers and uses a bounded loop action prefix inside a
final-position runtime branch before returning the whole-value step result.

```sh
just run-example runtime_final_if_guarded_loop
```

Key source ideas:

- The final-position `if (enabled == True) { ... } else { ... }` lowers both
  branch action prefixes and branch next states through typed Mantle control
  flow.
- The selected enabled branch records `branch_selected`, runs the bounded loop,
  and then returns `Continue(state)`.
- The selected disabled branch emits `worker disabled`, returns the same
  whole-value step result shape, and does not execute loop actions.
- Loop element access lowers through the typed loop element ID; source aliases
  such as `item` are not executable runtime dispatch paths.
- Branch-local `spawn`, branch-local process-reference binding, nested loops,
  deeper direct branch-action nesting, and mutable state updates remain rejected.

## Runtime Final If Nested If Actions

`examples/runtime_final_if_nested_if_actions.str` sends immutable `CheckFlags`
record payloads to four workers and uses one direct nested statement-level
runtime branch action as an action prefix inside each selected final-position
runtime branch. Each outer final-position branch still ends in the same
whole-value `Continue(state)` result.

```sh
just run-example runtime_final_if_nested_if_actions
```

Key source ideas:

- The final-position `if (outer == True) { ... } else { ... }` lowers both
  branch action prefixes and branch next states through typed Mantle control
  flow.
- The direct nested `if (inner == True) { ... } else { ... }` in each selected
  final-position branch lowers as a typed Mantle `ArtifactAction::IfElse`.
- Nested branch sends use the declared `reporter: ProcessRef<Reporter>` binding
  created before the final-position branch and typed `ReceivedPayload` record
  field templates for the immutable `inner_flag` value.
- Runtime execution records the final-position branch selection, nested branch
  selection, selected emit/send effects, and final whole-value result behavior
  in order.
- Third-level runtime branch actions, nested loops, branch-local `spawn`,
  branch-local process-reference binding, statement-branch `return`, and mutable
  state updates remain rejected.

## Runtime Final If Nested Terminal If

`examples/runtime_final_if_nested_terminal_if.str` sends immutable `CheckFlags`
record payloads to four workers and uses one direct nested final-position
runtime branch as the terminal return inside each selected final-position
runtime branch. Each nested leaf returns a whole-value `Continue(...)` state.

```sh
just run-example runtime_final_if_nested_terminal_if
```

Key source ideas:

- The outer `if (outer == True) { ... } else { ... }` lowers to a typed Mantle
  `NextState::IfElse`.
- The terminal nested `if (inner == True) { ... } else { ... }` in each selected
  branch lowers to a direct nested typed Mantle `NextState::IfElse`.
- Branch conditions lower through typed `ReceivedPayload` record-field
  templates for immutable `outer_flag` and `inner_flag`, not source aliases.
- Runtime execution records outer and nested `next_state` branch selections,
  selected output effects, and the final whole-value state transition in order.
- Third-level terminal runtime branches, deeper branch actions, nested loops,
  branch-local `spawn`, branch-local process-reference binding, statement-branch
  `return`, and mutable state updates remain rejected.

## Runtime Guard Noop

`examples/runtime_guard_noop.str` shows statement-level runtime branches where
one selected branch intentionally performs no effects. The conditions are still
checked `Bool` predicates and lower into typed Mantle branch actions.

```sh
just run-example runtime_guard_noop
```

Key source ideas:

- `if (flag == True) { ... }` lowers an omitted `else` as an explicit empty
  Mantle branch.
- `else {}` is an explicit no-op branch; `{}` on one side is allowed when the
  sibling branch has an declared effect.
- Empty selected branches do not emit, send, acquire authority, or change state,
  but they still record `branch_selected`.
- Both branches empty are rejected before runtime execution.

## Runtime For Each

`examples/runtime_for_each.str` iterates inside `BatchWorker.step` over a
received `List<Bool,2>` payload. The loop is not unrolled or selected by Strata
during checking; it lowers as typed Mantle loop control flow and executes once
per runtime element.

```sh
just run-example runtime_for_each
```

`examples/runtime_for_each_empty.str` uses the same shape with
`List<Bool,0>[]` and proves that Mantle records loop start/completion without
executing the body.

```sh
just run-example runtime_for_each_empty
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
just run-example runtime_for_each_if
```

Key source ideas:

- `if ((item != False) && !(item == False)) { ... } else { ... }` runs in
  Mantle for each loop iteration through typed Boolean predicate templates.
- The branch condition uses the typed loop element ID; branch payloads
  remain typed loop element values.
- One branch may be empty when the sibling branch has declared effects.
  Branches can emit and send, and may contain one direct nested
  statement-level branch. They cannot return, spawn, loop, or nest more deeply.
- The runtime trace records `loop_iteration`, `branch_selected`, branch effects,
  and `loop_completed` in deterministic collection order.

`examples/runtime_for_each_nested_if_actions.str` projects immutable fields from
each `CheckFlags` loop element and selects one direct nested statement-level
runtime branch inside the selected outer loop-body branch.

```sh
just run-example runtime_for_each_nested_if_actions
```

Key source ideas:

- The loop item record pattern binds immutable `outer` and `inner` field values.
- Lowering emits typed `RecordField(LoopElement(...), "...")` templates for the
  outer condition, nested condition, and send payload.
- Runtime execution records the loop iteration, outer branch selection, nested
  branch selection, and selected branch effects in order.
- Third-level runtime branch actions, nested loops, branch-local `spawn`, and
  branch-local process-reference binding remain rejected.

`examples/runtime_guarded_for_each.str` guards a whole bounded runtime loop with
a statement-level branch.

```sh
just run-example runtime_guarded_for_each
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
  type. Statement-level branch nesting still cannot exceed the single
  nested layer.

`examples/runtime_guarded_ref_loop.str` routes that guarded-loop send through a
direct `ProcessRef<Worker>` received as the current message payload.

```sh
just run-example runtime_guarded_ref_loop
```

Key source ideas:

- `BatchWorker` stores only value data in state. The worker reference remains a
  direct message payload on `Route(ProcessRef<Worker>)`.
- The selected enabled branch sends `Branch(item)` through the received
  reference from inside the guarded loop body.
- The disabled branch records only `branch_selected`; it emits no loop events
  and performs no branch-local or loop-local authority acquisition.
- Lowering emits a typed received-payload send target. Runtime dispatch uses the
  transported runtime process ID plus checked target process ID, not the source
  payload binding name.

## Runtime Guarded Ref Loop Jobs

`examples/runtime_guarded_ref_loop_jobs.str` keeps the same received direct
`ProcessRef<Worker>` routing shape, but the guarded loop iterates over
ordinary immutable `Job` record values.

```sh
just run-example runtime_guarded_ref_loop_jobs
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
just run-example runtime_loop_element_projection
```

Key source ideas:

- The loop item uses a record pattern, `Job { phase: routed_phase }`, to bind an
  immutable field value inside the loop body.
- Lowering emits `RecordField(LoopElement(...), "phase")` typed templates for
  the inner branch condition and send payload.
- Runtime execution uses typed loop element IDs, type IDs, and received
  process-ref payload targets. The source binding alias is not executable
  dispatch metadata.

## Actor Payload Match

`examples/actor_payload_match.str` proves the same immutable payload binding
works from a whole-body `match msg` arm, not only from a `step` signature.

```sh
just run-example actor_payload_match
```

Key source ideas:

- `fn step(state: WorkerState, msg: WorkerMsg)` binds the message parameter.
- `match msg` dispatches through the same typed pattern validation used by
  signature patterns.
- `Assign(job: Job)` binds the received payload immutably inside the match arm.
- Runtime still dispatches by typed message IDs and payload type IDs.

## Actor Payload Split Match

`examples/actor_payload_split_match.str` proves that one top-level message
variant can be split inside a whole-body `match msg` by disjoint nested typed
payload predicates.

```sh
just run-example actor_payload_split_match
```

Key source ideas:

- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` share the same
  top-level message constructor and differ only by nested payload identity.
- The checker accepts the split only because the nested typed predicates are
  provably disjoint over discovered concrete payload cases.
- Lowering emits exact typed payload guards in Mantle transition records.
- Runtime dispatch uses typed message IDs, current state IDs when applicable,
  and exact typed payload identity, not source strings or debug labels.

## Actor Payload Split Signature

`examples/actor_payload_split_signature.str` proves the same payload-sensitive
same-message split through step parameter patterns rather than a whole-body
`match msg`.

```sh
just run-example actor_payload_split_signature
```

Key source ideas:

- Multiple `fn step(state, Envelope(Assign(...)))` clauses can share one
  top-level message constructor when the nested typed predicates are provably
  disjoint.
- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` lower to two typed
  payload-guarded Mantle transitions for the same typed message ID.
- Mantle selects the transition by typed message ID, current state ID when
  applicable, and exact typed payload identity.
- `actor_payload_split_match.str` exercises the equivalent whole-body
  `match msg` authoring form.

## Actor Payload Split Signature Wildcard

`examples/actor_payload_split_signature_wildcard.str` proves that a
payload-sensitive step-signature split can use `_` as fallback for discovered
concrete payload cases not handled by explicit nested predicates.

```sh
just run-example actor_payload_split_signature_wildcard
```

Key source ideas:

- `Envelope(Assign(Ready))` handles the explicitly guarded payload case.
- `_` handles the discovered `Envelope(Assign(Done))` case, and lowering emits
  a typed payload guard for `Assign(Done)` rather than an open runtime catch-all.
- The fallback remains bounded to discovered concrete payload cases discovered by
  checking.
- Runtime dispatch still uses typed message IDs and exact typed payload
  identity, not source strings or debug labels.

## Actor Payload State-Match Split

`examples/actor_payload_state_match_split.str` proves that state-match step
clauses can share one top-level message constructor by disjoint nested typed
payload predicates.

```sh
just run-example actor_payload_state_match_split
```

Key source ideas:

- `Envelope(Assign(Ready))` and `Envelope(Assign(Done))` are explicit,
  discovered payload cases for the same message constructor.
- Each payload case expands across the checked `Idle`, `SawReady`, and `Done`
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
just run-example actor_payload_state_match_wildcard
```

Key source ideas:

- `Envelope(Assign(Ready))` handles the explicitly guarded payload case.
- The wildcard state-match step handles the discovered `Envelope(Assign(Done))`
  case; lowering emits `Assign(Done)` as an exact typed payload guard rather
  than an unguarded payload catch-all.
- Each explicit and fallback payload case expands across the checked `Idle`,
  `SawReady`, and `Done` current-state cases from its `match state` body.
- State changes remain immutable whole-value returns through `Continue(...)` or
  `Stop(...)`; runtime dispatch does not use source strings or debug labels.

## Nested Patterns

`examples/nested_patterns.str` composes immutable destructuring across
constructor payloads, records, list elements/rest, and map values/rest.

```sh
just run-example nested_patterns
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
just run-example actor_reply
```

Key source ideas:

- `enum WorkerMsg { Work(ProcessRef<Sink>) }` declares a typed reference
  payload.
- `send worker Work(sink);` transports the spawned `Sink` reference to
  `Worker`.
- `Work(reply_to: ProcessRef<Sink>)` binds the received reference immutably.
- `send reply_to Done;` routes by the transported runtime process ID and
  checked target process ID, not by source labels.

## Actor Emit Spawn Send

`examples/actor_emit_spawn_send.str` combines `emit`, `spawn`, and `send` in
one checked transition. `Main` declares each effect and exact
`Cap<Spawn<Worker>>` authority, spawns `Worker`, sends `Ping` through the typed
process reference, and stops with a whole replacement state.

```sh
just run-example actor_emit_spawn_send
```

Key source ideas:

- `authority spawn_worker: Cap<Spawn<Worker>>;` declares the exact local spawn
  capability.
- `fn step(...) -> ProcResult<MainState> ! [emit, spawn, send]` declares the
  exact effects used by the body.
- `let worker: ProcessRef<Worker> = spawn Worker;` creates a typed process
  reference.
- `send worker Ping;` dispatches by the checked process-reference target and
  message ID after lowering, not by source text.
- `return Stop(MainState { phase: Done });` preserves immutable whole-state
  transition semantics.
