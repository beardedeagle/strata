# Function And Return-Match Examples

These examples cover pure source functions, immutable source-local computation, source patterns, and checked return-match forms.

## Function Match

`examples/function_match.str` exercises normal source functions outside actor
dispatch. It uses module-level functions and process-local functions, including
signature-pattern dispatch and a whole-body match function.

```sh
just run-example function_match
```

Key source ideas:

- `readiness_sig(Cold)` and `readiness_sig(Warm)` are module-level function
  clauses selected by typed signature patterns.
- `readiness_body(mode: StartupMode)` uses a whole-body `match mode` outside
  an actor `step`.
- `Main` and `Worker` declare process-local functions for state
  construction.
- `send worker Assign(ready_job(Ready))` proves function calls are expanded for
  message payload discovery and lowering.
- Mantle sees typed state IDs, message IDs, and payload templates, not source
  function dispatch names.

## Function Payload Match

`examples/function_payload_match.str` extends normal source functions to
payload-bearing enum values. It constructs source-visible enum payload values,
matches them through signature patterns and whole-body function matches, and
lowers a received actor payload through a process-local function into an enum
payload state template.

```sh
just run-example function_payload_match
```

Key source ideas:

- `Assigned(Job { phase: Ready })` is resolved as a typed enum value, not a
  function call.
- `status_sig(Assigned(job: Job))` binds the enum payload immutably in a normal
  source function signature pattern.
- `status_body(work: Work)` matches the typed function parameter and binds the
  payload inside the selected arm.
- `state_for(Assigned(job))` proves a process-local function can wrap a received
  immutable payload into a source enum value before lowering.

## Function If Else

`examples/function_if_else.str` uses pure source-time conditionals in normal
source functions. The checker resolves the explicit `Bool { False, True }`
condition and selects one immutable branch before lowering. The example includes
both expression-form `return if (...) { value } else { value };` and braced pure
return branches.

```sh
just run-example function_if_else
```

Key source ideas:

- `enum Bool { False, True }` is an exact source contract for conditionals.
- `if (flag) { WarmReady } else { ColdReady }` is a pure value expression, not
  a statement block.
- `if (flag) { return WarmReady; } else { return ColdReady; }` is an equivalent
  pure source-function return branch shape, not a runtime branch.
- Both branches are checked against the same expected type.
- Mantle receives selected typed state values, not a conditional branch key or
  source function dispatch name.

## Function Local Bindings

`examples/function_local_bindings.str` uses sequential immutable bindings in
normal source functions and process-local pure functions. The checker resolves
each typed source-local value before lowering.

```sh
just run-example function_local_bindings
```

Key source ideas:

- `let current_local: Phase = status(work);` binds an immutable source value,
  not a runtime action.
- Later source-local bindings can use earlier bindings in source function calls,
  pure conditionals, record/list/map constructors, and equality predicates.
- Braced pure return-if branches can introduce branch-local immutable values
  before their terminal return.
- Function return-match arms can use source-local bindings before their terminal
  pure return.
- Process-local pure functions use the same source-local binding rules.
- Message enums that carry direct `ProcessRef<T>` payloads remain authority
  surfaces and are rejected as source-local binding types.
- Mantle receives typed values and templates, not source-local binding names or
  source function dispatch names.

## Function Collection Match

`examples/function_collection_match.str` uses immutable `List<T,N>` and
`Map<K,V,N>` source values in normal source functions.

```sh
just run-example function_collection_match
```

Key source ideas:

- `List<Phase,2>[Ready, Done]` and `Map<Phase,Phase,2>[Ready => Done, Unknown => Unknown]` are typed,
  bounded immutable collection values.
- `fn first(List<Phase,2>[phase, _])` dispatches on exact list length and binds
  one immutable element.
- Function body and return matches can use exact list patterns, list rest patterns
  such as `List[_, ..tail]`, exact map patterns, and subset map patterns such as
  `Map[Ready => selected, ..rest]` with `_` fallback arms.
- Function expansion leaves Mantle with a resolved `MainState` value, not
  source function dispatch names.

## Function Return Match

`examples/function_return_match.str` uses a function `return match` expression to
select an immutable result from an in-scope source value binding.

```sh
just run-example function_return_match
```

Key source ideas:

- `return match work { ... };` is checked as a pure function return expression.
- The selected arm binds enum payloads immutably before function expansion.
- Mantle receives the resolved `MainState{status:Active(Job{phase:Ready})}`
  state value, not source function dispatch.

## Process Return Match

`examples/process_return_match.str` uses a process `step return match` over a
concrete enum payload binding after a uniform `emit` prefix.

```sh
just run-example process_return_match
```

Key source ideas:

- `Envelope(Assign(phase: Phase))` binds an immutable enum payload whose
  concrete value is already proven by payload-sensitive dispatch.
- `emit "process return match uniform prefix";` lowers as the same typed action
  prefix on every selected transition.
- `return match phase { ... };` is checked and reduced to a typed
  `Continue(...)` or `Stop(...)` transition before lowering.
- Mantle executes the emitted typed transition IDs and payload guards; it does
  not dispatch on source strings or function names.

## Process Return Match Arm Prefix

`examples/process_return_match_arm_prefix.str` extends `step return match` with
selected arm-local `emit` and typed `send` prefixes before the terminal result.

```sh
just run-example process_return_match_arm_prefix
```

Key source ideas:

- The checker still selects one concrete return-match arm before lowering.
- The uniform prefix lowers onto every selected transition before the arm-local
  prefix.
- Only the selected arm's typed actions lower onto that transition; the arm-local
  `send` prefixes also seed the receiver's payload-sensitive transitions before
  lowering.
- Arm labels and source binding names are not executable runtime dispatch keys.
- Arm-local `spawn`, process-reference binding, final-position runtime `if`,
  nested return matches, and mutable source state changes remain rejected.

## Process Return Match Arm Runtime If Prefix

`examples/process_return_match_arm_runtime_if_prefix.str` extends selected
`step return match` arm prefixes with statement-level runtime `if` actions.

```sh
just run-example process_return_match_arm_runtime_if_prefix
```

Key source ideas:

- The checker still source-selects one concrete return-match arm before
  lowering.
- The checked arm-local runtime branch lowers as a typed Mantle action inside
  that selected arm prefix, after any uniform prefix actions and before the
  terminal `Continue(...)` or `Stop(...)`.
- This example keeps branch bodies action-only with local `emit` and in-scope
  direct `send`; `process_return_match_arm_if_for_prefix.str` also covers
  bounded runtime `for` actions inside selected runtime branches.
- Branch-local `spawn`, process-reference binding, branch returns, final-position
  runtime `if`, and runtime branch nesting beyond the artifact bound remain
  rejected.
- Mantle selects the runtime branch from typed artifact templates; source arm
  labels and source binding names are not executable dispatch keys.

## Process Return Match Arm For Prefix

`examples/process_return_match_arm_for_prefix.str` extends selected
`step return match` arm prefixes with bounded runtime `for` over a
checked runtime `List<T,N>` binding.

```sh
just run-example process_return_match_arm_for_prefix
```

Key source ideas:

- The checker still source-selects one concrete return-match arm before
  lowering.
- The selected arm loop lowers as a typed Mantle `for_each` action after any
  uniform and arm-local checked action prefixes, before the terminal
  `Continue(...)` or `Stop(...)`.
- Loop collections must be checked runtime `List<T,N>` bindings. Loop element
  projections lower as typed templates, not source-name dispatch.
- Loop bodies remain action-only: local `emit` and in-scope direct `send`
  actions are supported, while `spawn`, process-reference binding, nested loops,
  and branch-local process-reference binding remain rejected.

## Process Return Match Arm For-If Prefix

`examples/process_return_match_arm_for_if_prefix.str` extends selected
`step return match` arm-local bounded loops with loop-body runtime `if` actions.

```sh
just run-example process_return_match_arm_for_if_prefix
```

Key source ideas:

- The checker still source-selects one concrete return-match arm before
  lowering.
- The selected arm loop lowers as a typed Mantle `for_each` action, and the
  direct loop-body runtime branch lowers as a typed Mantle branch action inside
  that loop body.
- The branch condition and branch sends project from typed immutable loop
  element bindings, not source aliases or debug strings.
- Loop-body branches remain action-only: local `emit` and in-scope direct
  `send` actions are supported, while branch returns, branch-local `spawn`,
  process-reference binding, nested runtime `for`, and runtime branch nesting
  beyond the artifact bound remain rejected.

## Process Return Match Arm If-For Prefix

`examples/process_return_match_arm_if_for_prefix.str` extends selected
`step return match` arm-local runtime branches with bounded runtime `for` actions
inside branches.

```sh
just run-example process_return_match_arm_if_for_prefix
```

Key source ideas:

- The checker still source-selects one concrete return-match arm before
  lowering.
- The selected arm runtime branch lowers as a typed Mantle branch action, and a
  selected branch may contain bounded typed `for_each` actions.
- Loop collections remain checked runtime `List<T,N>` bindings, and loop element
  projections lower through typed templates rather than source aliases.
- Branch-local loops remain action-only and bounded: local `emit`, in-scope
  direct `send`, and loop-body runtime branch actions are accepted;
  branch returns, branch-local `spawn`, process-reference binding, nested
  runtime `for`, and runtime branch nesting beyond the artifact bound remain
  rejected.

## Process Return Match Arm Action Block

`examples/process_return_match_arm_action_block.str` exercises selected
`step return match` arms as ordinary typed step action blocks before their
terminal result.

```sh
just run-example process_return_match_arm_action_block
```

Key source ideas:

- Each selected arm contains multiple statement-level runtime `if` actions and
  multiple sibling bounded runtime `for` actions in source order.
- Nested statement-level runtime branches are supported only up to the shared
  artifact/runtime branch-action bound.
- Runtime loops remain bounded over checked `List<T,N>` bindings, and nested
  runtime loops remain rejected.
- Arm-local `spawn`, branch-local process-reference binding, final-position
  runtime `if`, nested return matches, mutation, and source-string dispatch
  remain rejected.

## Function Record Pattern

`examples/function_record_pattern.str` destructures an immutable record value in
a normal source function signature.

```sh
just run-example function_record_pattern
```

Key source ideas:

- `fn phase_of(Job { phase })` binds the `phase` field as an immutable source
  value.
- `field: binding` may rename a record field binding in the function signature.
- The function expands before lowering; Mantle receives the resolved
  `MainState{phase:Ready}` state value.

## Function Record Return Match

`examples/function_record_return_match.str` destructures an immutable record value
inside a function `return match` expression.

```sh
just run-example function_record_return_match
```

Key source ideas:

- `return match job { Job { phase } => { return phase; } };` binds the
  `phase` field as an immutable source value inside the selected arm.
- Record return-match destructuring requires the scrutinee to be an in-scope
  source binding with a concrete record value.
- The function expands before lowering; Mantle receives the resolved
  `MainState{phase:Ready}` state value.

## Function Record Body Match

`examples/function_record_body_match.str` destructures an immutable record value
inside a whole-body source function match.

```sh
just run-example function_record_body_match
```

Key source ideas:

- `match job { Job { phase } => { return phase; } }` binds the `phase` field
  as an immutable source value inside the selected arm.
- Record body-match destructuring requires one record pattern arm over the
  function parameter's concrete record value.
- The function expands before lowering; Mantle receives the resolved
  `MainState{phase:Ready}` state value.
