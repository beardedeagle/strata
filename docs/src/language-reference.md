# Language Reference

This page documents the Strata source surface accepted by the buildable implementation.
It is an authoring reference for `.str` programs, not a description of
Mantle artifact internals.

## Source Surface

| Area | Accepted Surface |
| --- | --- |
| Source unit | One `module name;` declaration per file. |
| Top-level declarations | `record`, `enum`, `fn`, and `proc`. |
| Classes | Not available. |
| Methods | Not available. |
| Top-level functions | Pure deterministic one-argument source functions with optional immutable source-local bindings. |
| Process functions | `init`, `step`, and pure deterministic one-argument process-local functions with optional immutable source-local bindings. |
| Imports | Not available. |
| Standard library | Not available. |
| Effects | `emit`, `spawn`, and `send`. |
| Process references | `let worker: ProcessRef<Worker> = spawn Worker;`, `send worker Ping;`, and `send reply_to Done;` for received typed references. |
| Effect outcomes | Immutable step-local `Result` bindings for local `send` and the current local `spawn` success shape. |
| Scalar values | Fixed-width integer source values `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, and `I64` with explicit literal suffixes. |
| Collections | Immutable `List<T,N>` and `Map<K,V,N>` source values with explicit `List[...]` and `Map[key => value]` constructors. |
| Scalar operators | `+`, `-`, `*`, `/`, `%`, `<`, `<=`, `>`, `>=`, and scalar equality over matching integer types only. |
| Boolean predicates | `!`, `&&`, and `||` over checked Bool-producing predicates. |
| Pure conditionals | Expression-only `if (condition) { value } else { value }` over explicit `enum Bool { False, True }`; concrete forms fold before lowering, and runtime-bound value forms lower as typed Mantle value templates. |
| Source-local bindings | `let name: Type = value_expr;` inside pure source function block bodies before the terminal return. |
| Runtime branching | Final-position `if (condition) { ... return ...; } else { ... return ...; }` and statement-level effect branches in `step` bodies, lowered to Mantle control flow. |
| Patterns | Constructor patterns, constructor payload bindings, nested constructor and record/list/map payload destructuring in functions, message dispatch, state matches, function return-match expressions, step return-match expressions with optional uniform and selected-arm action prefixes, and `_` wildcards. |
| Message payloads | `enum WorkerMsg { Assign(Job) }`, `enum WorkerMsg { Work(ProcessRef<Sink>) }`, collection payloads, payload sends, and payload-binding step patterns. |
| Pattern dispatch | Function signature patterns, source function match bodies, function return-match expressions, fieldless enum matches in `init`, step parameter patterns, wildcard step patterns, one whole-body `match msg` step form per process, whole-body `match state` inside message-specific step clauses, and step return-match expressions over concrete enum source bindings. Same-constructor payload-sensitive splits are accepted for source functions, whole-body `match msg`, step parameter patterns, state-match step clauses, and step return-match expressions only when nested typed predicates are provably disjoint. |
| Transition result | `ProcResult<T>` with `Continue(value)`, `Stop(value)`, and `Panic(value)`. |

The `module` declaration names a source unit. It does not create an import
namespace, package, library, or visibility boundary.

## Source Unit

A Strata source file starts with a module declaration:

```strata
module hello;
```

After the module declaration, the accepted top-level declarations are records,
enums, source functions, and processes.

```strata
module example;

record MainState;
enum MainMsg { Start }

fn identity_state(state: MainState) -> MainState ! [] ~ [] @det {
    return state;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
```

Every buildable program must declare a `Main` process. Mantle starts `Main` and
delivers the first message variant of `Main`'s message enum as the entry
message.

## Identifiers

Identifiers must start with an ASCII letter or `_`, then contain only ASCII
letters, ASCII digits, or `_`. The single `_` token is reserved for wildcard
patterns and cannot be used as an identifier.

Valid examples:

```strata
Main
Worker_1
_InternalState
```

Invalid examples:

```strata
1Main
worker-name
_
```

`_`, `as`, `bounded`, `else`, `emit`, `enum`, `fn`, `for`, `if`, `in`, `let`,
`mailbox`, `match`, `module`, `mut`, `proc`, `record`, `return`, `security`,
`send`, `spawn`, `type`, and `var` are reserved everywhere identifiers are
accepted.
`ProcResult`, `ProcessRef`, `List`, `Map`, `Unit`, `Option`, `Result`,
`SendError`, `SpawnError`, `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, and
`I64` are reserved type names because they name built-in transition,
process-reference, collection, effect outcome, and scalar value types.
Type names beginning with `__strata_checked_` are reserved for checked IR and
artifact metadata. Checked process-reference artifact labels under that prefix
are keyed by resolved process IDs, not source process names.

## Records

Records define structured state values. A fieldless record uses a semicolon:

```strata
record MainState;
```

A record with fields uses braces and does not take a semicolon after the closing
brace:

```strata
enum Phase { Idle, Done }

record WorkerState {
    phase: Phase,
}
```

Record fields are immutable. `mut` and `var` field declarations are rejected.

Record values use constructor syntax:

```strata
WorkerState { phase: Idle }
```

Fieldless record values are written as the record name:

```strata
MainState
```

Record value fields use `:`, not `=`. A braced record value must provide every
declared field exactly once; missing, duplicate, or unknown fields are rejected.

Payload-bearing enum values use constructor syntax with one immutable payload
value:

```strata
Assigned(Job { phase: Ready })
```

The checker resolves this form against the expected enum type. If the identifier
names a source function instead, it is expanded as a function call; constructor
and function names cannot collide silently.

## Scalar Integer Values

Scalar integer source values are fixed-width and explicitly typed. The accepted
types are `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, and `I64`.
Numeric value literals require suffixes:

```strata
0_u8
10_u32
-1_i16
```

Unsuffixed numeric value expressions are rejected. Capacity and mailbox-bound
numbers remain separate syntax and do not use scalar literal suffixes.

Scalar arithmetic supports `+`, `-`, `*`, `/`, and `%` over matching integer
types only. Ordering supports `<`, `<=`, `>`, and `>=` over matching integer
types and produces `Bool`. Equality supports scalar operands with the same
integer type. There is no implicit widening, narrowing, signed/unsigned mixing,
or string coercion.

Fully concrete scalar expressions fold during checking. Overflow, underflow,
statically known division by zero, statically known modulo by zero,
out-of-range literals, and malformed suffixes fail before lowering.
Runtime-bound scalar expressions lower as typed Mantle value templates with
scalar type metadata and fail-closed runtime evaluation. They do not lower as
source strings, binding names, or function names.

Scalars are ordinary source values when they do not carry authority: they can be
used in source function parameters and returns, source-local bindings, process
local pure functions, record fields, message payloads, list/map values, and
runtime `if` predicates through `Bool`.

## Built-In Result Values

The buildable value surface includes these built-in value shapes:

```strata
Unit
Option<T>
Result<T,E>
SendError<M>
SpawnError<A>
```

`Unit` has the single value `Unit`. `Option<T>` has `None` and `Some(T)`.
`Result<T,E>` has `Ok(T)` and `Err(E)`. Local send outcomes use
`Result<Unit,SendError<MessageType>>`; local spawn outcomes use
`Result<ProcessRef<TargetProcess>,SpawnError<Unit>>`.

```strata
let send_result: Result<Unit,SendError<WorkerMsg>> = send worker Work;
let spawn_result: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
```

Outcome bindings are immutable and scoped to the `step` body. They may feed
whole-value state transitions when the checker can admit the finite runtime
outcome values into the process state table. Spawn outcome successes carry
process references, so they remain step-local authority values rather than
storable source state. Outcomes may also drive follow-up effects through
ordinary immutable `if` conditions such as `send_result == Ok(Unit)` or
`spawn_result != Err(Exhausted(Unit))`. Those branch tests compare the typed
built-in variant shape, not the preserved message payload or the successful
process-reference payload. Top-level pre-state effect bindings are declaration
ordered: an outcome is usable only after its `let` statement, and outcome
bindings must appear before ordinary effect statements that would otherwise be
reordered around the commit-or-return boundary.
`SendError<M>`
preserves the original message value for pre-acceptance failures, including the
runtime metadata for a direct `ProcessRef<T>` message payload. That authority
remains a send/message boundary value and is not admitted into ordinary process
state. The admitted variants are
`Full(M)`, `Stopped(M)`, and `Crashed(M)`; the current runtime distinguishes
full, stopped, and targets already failed before
acceptance in direct runtime execution. A source transition that creates the
crash with `Panic(...)` still fails the run after recording no-replay evidence
before a later source sender can observe the target as crashed. `SpawnError<A>`
admits `Denied(A)`, `Exhausted(A)`, and
`BackendUnavailable(A)`; the current local runtime reports exhausted process
capacity.

Bare statement `send` and `spawn` remain fail-closed runtime effects: Mantle
rejects pre-acceptance delivery and process-limit failures instead of silently
drops them. Outcome-form `send` returns the typed failure result before message
acceptance and commits the message only on `Ok(Unit)`.

## Collections

Lists and maps are immutable source values with explicit numeric capacities.
They can be used as source function parameters and return values, record fields,
process state types, and message payloads when their element, key, and value
types are source value types.

```strata
List<Phase,2>[Ready, Done]
Map<Phase,Phase,1>[Ready => Done]
```

Collection constructors are explicit. Bare `[Ready, Done]` and `{ Ready: Done }`
forms are not part of the buildable language surface. Map keys are canonical source values; a
map value or map pattern that repeats a canonical key is rejected. List and map
patterns are exact by default. List rest patterns are suffix-only:

```strata
List[first, second] // exact list length
List[first, ..tail] // first must exist; tail is the unmatched suffix
```

`..tail` binds an immutable whole list containing entries after the fixed prefix.
If the matched value has type `List<T,N>` and the pattern lists `M` fixed prefix
elements, the tail binding has type `List<T,N-M>`. The actual tail value may
contain fewer entries because bounded list values may be shorter than capacity.
Arbitrary prefix/rest/suffix matching remains deferred.

A trailing `..` marker makes a map pattern a subset pattern over the listed
static keys:

```strata
Map[Ready => selected]     // exact key set
Map[Ready => selected, ..] // Ready must exist; extra keys are allowed
Map[Ready => selected, ..rest] // rest binds a map without Ready
```

Map `..rest` binds an immutable whole map containing entries except the listed
static keys. If the matched value has type `Map<K,V,N>` and the pattern lists
`M` distinct static keys, the rest binding has type `Map<K,V,N-M>`. The actual
rest value may contain fewer entries because subset patterns still match maps
that omit unlisted keys.

Overlapping exact and subset map patterns are rejected instead of relying on
source order or specificity. Subset overlap is capacity-aware: two subset
patterns overlap when one bounded map can contain both required key sets.
Runtime-bound map value keys must be static source values; dynamic-key
dictionaries remain deferred. Scalar keys are allowed when they are static
source values. Rest binding does not expose collection
iteration, order-dependent dispatch, mutation, dynamic keys, or mutable views.

Collection pattern element and map-value positions may contain nested structural
patterns, such as `List[Job { phase }, ..tail]` or
`Map[Ready => Job { phase }, ..rest]`. Map nesting still uses only the listed
static keys; dynamic-key map dispatch is not part of the buildable surface.

Record field order and map entry order are preserved in source-authored values,
emitted artifact values, labels, and traces. Projection still addresses map
entries by key, and the language does not expose source-level map iteration or
order-dependent dispatch.

## Enums

Enums define named variants:

```strata
enum MainMsg {
    Start,
}

enum WorkerState {
    Idle,
    Handled,
}

enum WorkerMsg {
    Assign(Job),
    Stop,
}
```

Enums used as process state or message types must declare at least one variant.
Duplicate variants are rejected. Payload variants are accepted for process
state and message enums. Process state payloads remain immutable whole-state
values represented through typed state IDs.

## Processes

A process declares a mailbox bound, a state type, a message type, an `init`
function, and one `step` clause for each accepted message:

```strata
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
```

Only the aliases `State` and `Msg` are accepted inside a process. Processes may
also declare pure deterministic process-local functions alongside `init` and
`step`.
Each message variant must resolve to exactly one `step` clause, selected by an
explicit constructor pattern, by one wildcard pattern, or by one whole-body
`match msg`. A message-specific step clause can also dispatch over current
state with a whole-body `match state`. A process cannot mix parameter-pattern
or state-match step clauses with a `match msg` step body.

An `init` function returns one immutable whole state value. It may use a
whole-body `match` or a pure `return match` over one fieldless enum constructor
when the checker can select one arm before lowering:

```strata
fn init() -> MainState ! [] ~ [] @det {
    return match Warm {
        Cold => {
            return MainState { readiness: ColdReady };
        }
        Warm => {
            return MainState { readiness: WarmReady };
        }
    };
}
```

This is not runtime dispatch. The checker proves the fieldless enum scrutinee,
checks exhaustiveness and arm shapes, and emits the selected initial state as the
existing typed state ID. `init return match` arms must be statement-free and must
return whole state values; nested return matches and payload-binding
materialization into the initial state are rejected.

## Function Signatures

The accepted function signature shape is:

```strata
fn name(params...) -> ReturnType ! [effects] ~ [may_behaviors] @det {
    ...
}
```

Buildable source requires:

| Function | Required Shape |
| --- | --- |
| `init` | No parameters, returns the process state type, uses `! [] ~ [] @det`. |
| parameter-pattern `step` | Parameters exactly `state: StateType, MessagePattern`, returns `ProcResult<StateType>`, uses `~ [] @det`. |
| match `step` | Parameters exactly `state: StateType, msg: MsgType`, returns `ProcResult<StateType>`, uses `~ [] @det`, and has a whole-body `match msg`. |
| state-match `step` | Parameters exactly `state: StateType, MessagePattern`, returns `ProcResult<StateType>`, uses `~ [] @det`, and has a whole-body `match state`. |
| source function | One binding parameter or one pattern parameter, returns a source value type, uses `! [] ~ [] @det`, and has no runtime statements. |

The parser recognizes `@nondet`, but buildable source rejects it. The
may-behavior list after `~` must be empty.

Normal source functions are checked before lowering and expanded into the
value positions where they are called. They do not become Mantle runtime
dispatch entries and cannot perform runtime effects. A process-local function is
visible only inside that process. A module function is visible throughout the
module. Recursive function call cycles are rejected.

Source function block bodies may introduce sequential immutable source-local
values before the terminal return:

```strata
fn route(work: Work) -> Phase ! [] ~ [] @det {
    let current: Phase = status(work);
    let selected: Phase = if (current == Active) { Active } else { Idle };
    return selected;
}
```

Each binding uses `let name: Type = value_expr;`. The annotated type must be a
declared source value type without process-reference authority: a record, enum,
`List<T,N>`, or `Map<K,V,N>` whose contained types are also source values. A
message enum that carries a direct `ProcessRef<T>` payload is valid as a message
type, but it is not a pure source value type. The right-hand side is a pure
source value expression and may refer to parameters, pattern bindings, earlier
source-local bindings, source function calls, source-time conditionals,
return-match values, record/list/map constructors, and Bool/equality predicates.
Bindings are immutable and cannot be reassigned.

Source-local bindings may not shadow parameters, pattern bindings, earlier local
bindings, constructors, types, processes, or visible source functions. They
cannot use `ProcessRef<T>`, `spawn`, `send`, `emit`, runtime `for`, mutation, or
runtime-only state/payload forms. The checker resolves these names during source
function expansion; lowering emits only typed values, templates, typed IDs, and
validated artifact data. Source-local binding names do not become Mantle dispatch
keys or executable runtime bindings.

Source-function pattern bindings use the same immutable checker namespace. They
must not shadow source functions or process-reference bindings introduced by the
owning process.

Function signature patterns can author enum dispatch:

```strata
fn readiness(Cold) -> Readiness ! [] ~ [] @det {
    return ColdReady;
}

fn readiness(Warm) -> Readiness ! [] ~ [] @det {
    return WarmReady;
}

fn status(Assigned(job: Job)) -> WorkStatus ! [] ~ [] @det {
    return Active(job);
}
```

A source function signature may also destructure fields from an immutable record
value:

```strata
fn phase_of(Job { phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
}

fn renamed_phase(Job { phase: current }) -> JobPhase ! [] ~ [] @det {
    return current;
}
```

Source function signatures may also dispatch on exact immutable collection
shapes:

```strata
fn first(List<Phase,2>[phase, _]) -> Phase ! [] ~ [] @det {
    return phase;
}

fn lookup(Map<Phase,Phase,1>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}
```

Nested function patterns compose constructor payloads, records, and collection
element/value projections:

```strata
fn routed_phase(Assign(Job { phase })) -> Phase ! [] ~ [] @det {
    return phase;
}

fn listed_phase(List<Routed,1>[Assign(Job { phase })]) -> Phase ! [] ~ [] @det {
    return phase;
}
```

A function may also use a whole-body match over its typed binding parameter:

```strata
fn readiness_body(mode: StartupMode) -> Readiness ! [] ~ [] @det {
    match mode {
        Cold => {
            return ColdReady;
        }
        Warm => {
            return WarmReady;
        }
    }
}
```

Whole-body function matches and function return-match expressions may split one
top-level constructor when nested typed enum predicates are provably disjoint.
The split remains checker-time source dispatch; function expansion resolves the
concrete source value before lowering:

```strata
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    }
}
```

Whole-body function matches may also destructure a concrete record binding:

```strata
fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { phase } => {
            return phase;
        }
    }
}
```

Whole-body function matches can destructure exact list patterns, suffix-only list
rest patterns, exact map patterns, and map subset/rest patterns. A wildcard arm
may provide a fallback for collection shapes that are not listed:

```strata
fn first_or_unknown(items: List<Phase,1>) -> Phase ! [] ~ [] @det {
    match items {
        List[phase] => {
            return phase;
        }
        _ => {
            return Unknown;
        }
    }
}
```

Or a block body may return a match over an in-scope source value binding:

```strata
fn status(work: Work) -> WorkStatus ! [] ~ [] @det {
    return match work {
        Empty => {
            return Idle;
        }
        Assigned(job: Job) => {
            return Active(job);
        }
    };
}
```

The same function return-match form may destructure a concrete record binding:

```strata
fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    return match job {
        Job { phase } => {
            return phase;
        }
    };
}
```

Collection return matches use the same collection patterns:

```strata
fn ready_value(items: Map<Phase,Phase,2>) -> Phase ! [] ~ [] @det {
    return match items {
        Map[Ready => selected, ..rest] => {
            return selected;
        }
        _ => {
            return Unknown;
        }
    };
}
```

Enum matches are exhaustive and immutable. Source function whole-body matches and
function return-match expressions may repeat a top-level constructor only when the
nested typed enum predicates are provably disjoint; identical predicates,
unguarded constructor arms, and unproven overlaps are rejected. Source function
signature groups still keep one clause per top-level constructor. Record body
matches and return matches use one record pattern arm for the matched record type.
Collection patterns match exact list length unless they use the trailing
`..tail` suffix rest binding. Map patterns match exact key sets unless they use
the trailing `..` subset or rest marker. `_` remains available as a collection
fallback in function match bodies and return matches.
Payload-bearing source function patterns and record/list/map destructuring
patterns bind immutable source values. A function call must provide a concrete
enum constructor value for signature-pattern, whole-body match, or function
return-match dispatch. Record and collection destructuring functions require a
concrete value argument after source function expansion. Functions are still
expanded before lowering and do not become runtime dispatch entries.

## Pure Conditionals

Source functions and pure value expressions can use a source-time value-level
conditional:

```strata
enum Bool { False, True }

fn readiness(flag: Bool) -> Readiness ! [] ~ [] @det {
    return if (flag) { WarmReady } else { ColdReady };
}
```

The equivalent braced pure return form is also supported in source functions:

```strata
fn readiness(flag: Bool) -> Readiness ! [] ~ [] @det {
    if (flag) {
        return WarmReady;
    } else {
        return ColdReady;
    }
}
```

The condition type is exactly the declared fieldless enum
`Bool { False, True }`. Both expression-form branches are source value
expressions checked against the same expected return, field, state, or payload
type. Braced source function return-if branches may contain immutable
source-local bindings before the terminal pure return. Branches cannot perform
`emit`, `send`, `spawn`, runtime `for`, branch-local process-reference binding,
mutation, or any other runtime statement.

Concrete conditionals are selected during source checking and source function
expansion, so lowering sees only the selected value. Runtime-bound value
conditionals over `Bool` lower as typed Mantle value templates containing the
condition template and both typed branch value templates. Mantle does not
receive a conditional runtime dispatch entry, a source function name, or a
source-string branch key.

## Typed Equality Predicates

The equality surface is `==` and `!=` over these operand families:

- `Bool`, with the exact `enum Bool { False, True }` contract;
- fixed-width scalar integer values with matching scalar types;
- fieldless values of the same payload-free enum type;
- built-in `Option`, `Result`, `SendError`, and `SpawnError` values only when one
  side is a safe built-in variant pattern such as `None`, `Ok(Unit)`, or
  `Err(Exhausted(Unit))`.

Both operands must have the same checked type. Concrete source operands fold
during checking, so lowering sees only the selected `True` or `False` value.
Runtime-dependent operands lower as typed Mantle value templates and Mantle
evaluates them from typed values. Equality does not dispatch through
source names, function names, debug labels, or parser strings.

String equality, structural record/list/map equality, payload enum equality,
process-reference equality, direct equality between two spawn outcomes, and
matching a preserved message payload by equality are not part of the buildable
equality surface.

## Boolean Predicate Composition

Bool-producing predicates can be composed with grouping, unary `!`, binary
`&&`, and binary `||`:

```strata
if ((flag == True) && !(status == Done)) {
    emit "still active";
} else {
    emit "complete";
}
```

Every composed operand must have the exact `Bool` contract. The supported
operands are direct `Bool` values or templates, typed equality predicates,
typed scalar-ordering predicates, and nested Boolean predicate composition.
Fully concrete source predicates fold during checking. Runtime-dependent
predicates lower into typed Mantle value templates; Mantle validates the typed
tree, validates all operands, evaluates it from typed runtime values, and
records the selected branch through the existing `branch_selected` trace event.

Predicate composition does not add floats, string equality, structural
equality, payload enum equality, process-reference equality, assignment,
mutation, or authority.

Runtime branching, runtime iteration, statements, effects, step patterns, state transitions, and limits are documented in [Runtime Reference](runtime-reference.md).
