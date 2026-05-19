# Language Reference

This page documents the Strata source surface accepted by the buildable slice.
It is an authoring reference for `.str` programs, not a description of
Mantle artifact internals.

## Source Surface

| Area | Accepted Surface |
| --- | --- |
| Source unit | One `module name;` declaration per file. |
| Top-level declarations | `record`, `enum`, `fn`, and `proc`. |
| Classes | Not available. |
| Methods | Not available. |
| Top-level functions | Pure deterministic one-argument source helpers. |
| Process functions | `init`, `step`, and pure deterministic one-argument process-local helpers. |
| Imports | Not available. |
| Standard library | Not available. |
| Effects | `emit`, `spawn`, and `send`. |
| Process references | `let worker: ProcessRef<Worker> = spawn Worker;`, `send worker Ping;`, and `send reply_to Done;` for received typed references. |
| Collections | Immutable `List<T,N>` and `Map<K,V,N>` source values with explicit `List[...]` and `Map[key => value]` constructors. |
| Boolean predicates | `!`, `&&`, and `||` over admitted Bool-producing predicates. |
| Pure conditionals | Source-time expression-only `if (condition) { value } else { value }` over explicit `enum Bool { False, True }`. |
| Runtime branching | Final-position `if (condition) { ... return ...; } else { ... return ...; }` and statement-level effect branches in `step` bodies, lowered to Mantle control flow. |
| Patterns | Constructor patterns, constructor payload bindings, nested constructor and record/list/map payload destructuring in helpers, message dispatch, state matches, helper return-match expressions, step return-match expressions with optional uniform action prefixes, and `_` wildcards. |
| Message payloads | `enum WorkerMsg { Assign(Job) }`, `enum WorkerMsg { Work(ProcessRef<Sink>) }`, collection payloads, payload sends, and payload-binding step patterns. |
| Pattern dispatch | Function signature patterns, source function match bodies, helper return-match expressions, fieldless enum matches in `init`, step parameter patterns, wildcard step patterns, one whole-body `match msg` step form per process, whole-body `match state` inside message-specific step clauses, and step return-match expressions over concrete enum source bindings. Same-constructor payload-sensitive splits are accepted for helpers, whole-body `match msg`, step parameter patterns, state-match step clauses, and step return-match expressions only when nested typed predicates are provably disjoint. |
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
`ProcResult`, `ProcessRef`, `List`, and `Map` are reserved type names because
they name built-in transition, process-reference, and collection types.
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
names a source helper instead, it is expanded as a helper call; constructor and
helper names cannot collide silently.

## Collections

Lists and maps are immutable source values with explicit numeric capacities.
They can be used as source helper parameters and return values, record fields,
process state types, and message payloads when their element, key, and value
types are source value types.

```strata
List<Phase,2>[Ready, Done]
Map<Phase,Phase,1>[Ready => Done]
```

Collection constructors are explicit. Bare `[Ready, Done]` and `{ Ready: Done }`
forms are not admitted in this slice. Map keys are canonical source values; a
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
Runtime-bound map value keys must be static source values in this slice;
dynamic-key dictionaries remain deferred. Rest binding does not expose collection
iteration, order-dependent dispatch, mutation, dynamic keys, or mutable views.

Collection pattern element and map-value positions may contain nested structural
patterns, such as `List[Job { phase }, ..tail]` or
`Map[Ready => Job { phase }, ..rest]`. Map nesting still uses only the listed
static keys; dynamic-key map dispatch remains outside this source slice.

Record field order and map entry order are preserved in source-authored values,
emitted artifact values, labels, and traces. Projection still addresses map
entries by key, and this slice does not expose source-level map iteration or
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
values admitted through typed state IDs.

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
also declare pure deterministic helper functions alongside `init` and `step`.
Each message variant must resolve to exactly one `step` clause, selected by an
explicit constructor pattern, by one wildcard pattern, or by one whole-body
`match msg`. A message-specific step clause can also dispatch over current
state with a whole-body `match state`. A process cannot mix parameter-pattern
or state-match step clauses with a `match msg` step body in this slice.

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
materialization into the initial state are rejected in this source slice.

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
| source helper | One binding parameter or one pattern parameter, returns a source value type, uses `! [] ~ [] @det`, and has no runtime statements. |

The parser recognizes `@nondet`, but buildable source rejects it. The
may-behavior list after `~` must be empty.

Normal source functions are checked before lowering and expanded into the
value positions where they are called. They do not become Mantle runtime
dispatch entries and cannot perform runtime effects. A process-local helper is
visible only inside that process. A module helper is visible throughout the
module. Recursive helper call cycles are rejected in this source slice.

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

A source helper signature may also destructure fields from an immutable record
value:

```strata
fn phase_of(Job { phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
}

fn renamed_phase(Job { phase: current }) -> JobPhase ! [] ~ [] @det {
    return current;
}
```

Source helper signatures may also dispatch on exact immutable collection
shapes:

```strata
fn first(List<Phase,2>[phase, _]) -> Phase ! [] ~ [] @det {
    return phase;
}

fn lookup(Map<Phase,Phase,1>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}
```

Nested helper patterns compose constructor payloads, records, and collection
element/value projections:

```strata
fn routed_phase(Assign(Job { phase })) -> Phase ! [] ~ [] @det {
    return phase;
}

fn listed_phase(List<Routed,1>[Assign(Job { phase })]) -> Phase ! [] ~ [] @det {
    return phase;
}
```

A helper may also use a whole-body match over its typed binding parameter:

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

Whole-body helper matches and helper return-match expressions may split one
top-level constructor when nested typed enum predicates are provably disjoint.
The split remains checker-time source dispatch; helper expansion resolves the
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

Whole-body helper matches may also destructure a concrete record binding:

```strata
fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { phase } => {
            return phase;
        }
    }
}
```

Whole-body helper matches can destructure exact list patterns, suffix-only list
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

The same helper return-match form may destructure a concrete record binding:

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

Enum matches are exhaustive and immutable. Source helper whole-body matches and
helper return-match expressions may repeat a top-level constructor only when the
nested typed enum predicates are provably disjoint; identical predicates,
unguarded constructor arms, and unproven overlaps are rejected. Source helper
signature groups still keep one clause per top-level constructor. Record body
matches and return matches use one record pattern arm for the matched record type.
Collection patterns match exact list length unless they use the trailing
`..tail` suffix rest binding. Map patterns match exact key sets unless they use
the trailing `..` subset or rest marker. `_` remains available as a collection
fallback in helper match bodies and return matches.
Payload-bearing source helper patterns and record/list/map destructuring
patterns bind immutable source values. A helper call must provide a concrete
enum constructor value for signature-pattern, whole-body match, or enum helper
return-match dispatch. Record and collection destructuring helpers require a
concrete value argument after source helper expansion. Helpers are still
expanded before lowering and do not become runtime dispatch entries.

## Pure Conditionals

Source helpers and pure value expressions can use a source-time value-level
conditional:

```strata
enum Bool { False, True }

fn readiness(flag: Bool) -> Readiness ! [] ~ [] @det {
    return if (flag) { WarmReady } else { ColdReady };
}
```

The condition type is exactly the declared fieldless enum
`Bool { False, True }`. Both branches are source value expressions checked
against the same expected return, field, state, or payload type. Branches cannot
perform statements or effects.

Conditionals are selected during source checking and helper expansion. The
selected branch is what lowering sees; Mantle does not receive a conditional
runtime dispatch entry, a source function name, or a source-string branch key.
If the condition is not a concrete `True` or `False` value after source helper
expansion, checking fails closed.

## Typed Equality Predicates

The admitted equality surface is `==` and `!=` over two operand families:

- `Bool`, with the exact `enum Bool { False, True }` contract;
- fieldless values of the same payload-free enum type.

Both operands must have the same checked type. Concrete source operands fold
during checking, so lowering sees only the selected `True` or `False` value.
Runtime-dependent operands lower as typed Mantle value templates and Mantle
evaluates them from admitted typed values. Equality does not dispatch through
source names, helper names, debug labels, or parser strings.

This slice does not admit string equality, structural record/list/map equality,
payload enum equality, process-reference equality, ordering, arithmetic,
assignment, or mutation.

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

Every composed operand must have the exact `Bool` contract. The admitted
operands are direct `Bool` values or templates, typed equality predicates, and
nested Boolean predicate composition. Fully concrete source predicates fold
during checking. Runtime-dependent predicates lower into typed Mantle value
templates; Mantle admits the typed tree, validates all operands, evaluates it
from admitted runtime values, and records the selected branch through the
existing `branch_selected` trace event.

Predicate composition does not add arithmetic, ordering, string equality,
structural equality, payload enum equality, process-reference equality,
assignment, mutation, or authority.

## Runtime Branching

Step bodies can use a final-position runtime `if` whose condition is a checked
`Bool` value template, or a statement-level runtime `if` whose branches run
effects and then continue to the enclosing final return:

```strata
fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag == True) {
        emit "worker took warm branch";
        return Stop(WarmReady);
    } else {
        emit "worker took cold branch";
        return Stop(ColdReady);
    }
}
```

```strata
fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag != False) {
        emit "worker handled true";
    } else {
        emit "worker handled false";
    }
    return Continue(state);
}
```

Statement-level runtime branches also admit guard/no-op shapes:

```strata
fn step(state: WorkerState, Check(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag == True) {
        emit "guard saw true";
    }
    if (!(flag == False)) {
        emit "guard enabled";
    } else {
    }
    if (flag == True) {
    } else {
        emit "guard saw false";
    }
    return Continue(state);
}
```

This is ordinary runtime branching. If the condition depends on the received
payload or current state payload, Strata lowers the checked condition, branch
actions, and any branch next states into the Mantle artifact. Mantle admits the
typed condition, validates both branches, executes only the selected branch, and
records `branch_selected` trace events with a stable admitted-artifact
`branch_path`. Branch effects must be declared by the step effect list. Runtime
branch statement prefixes cannot bind process references or contain direct
nested branches in this slice. Branches may contain bounded runtime `for`
statements; each loop body remains the same admitted runtime-loop body surface.
An omitted statement-level `else` lowers as an explicit empty branch. Empty
statement-level branches perform no effects, no state change, no authority
acquisition, and no hidden work; branch selection is still observable through
`branch_selected`. One branch may be empty when the sibling branch contains at
least one admitted action. Both branches empty are rejected at source checking,
artifact admission, and loaded-runtime admission. Final-position runtime
branches must return the same step result from both branches. Statement-level
runtime branches cannot return; state changes still occur only through the
enclosing immutable whole-value `Continue`, `Stop`, or `Panic` return.

This slice does not add ordering comparisons, arithmetic, unbounded loops,
imports, or a standard library.

## Runtime Iteration

Step bodies can use a bounded runtime `for` loop over an immutable typed list
binding:

```strata
fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn, send] ~ [] @det {
    let worker: ProcessRef<Worker> = spawn Worker;
    for item in items {
        send worker Branch(item);
    }
    return Stop(state);
}
```

This is ordinary runtime iteration. The collection source must be an identifier
binding that checks as `List<Element,N>` and remains runtime-bound, such as a
message payload binding. Strata lowers the checked collection template, typed
loop element ID, immutable element binding, maximum item count, and body actions
into the Mantle artifact. Mantle admits the loop structure, validates the body,
executes the body once per runtime element in collection order, enforces the
collection length and runtime fuel limits, and records `loop_started`,
`loop_iteration`, and `loop_completed` trace events.

The loop element is immutable and may be used only as a typed value template in
the loop body. Loop bodies may use statement-level runtime `if` over the active
loop element or another checked `Bool` template, including the same one-sided
no-op branch shapes admitted outside loops. Mantle selects the branch during
execution and records `branch_selected` inside the loop trace with the active
loop element ID and iteration index. Bounded loops may also appear inside
statement-level runtime branches, allowing a guard to select or skip a whole
loop. If the selected branch is empty, Mantle records only the outer
`branch_selected` event and emits no loop events. If the selected branch contains
the loop, trace order is `branch_selected`, `loop_started`, ordered
`loop_iteration` body effects, and `loop_completed`.

The loop body is still intentionally narrow in this slice: no nested loops, no
`spawn`, no `return`, no assignment, no branch nested inside another branch,
and no process-reference element type. A loop body may contain the admitted
statement-level runtime `if` / no-op shape, including when the loop is itself
inside a selected guard branch. Send targets may be process references bound
before the loop or direct `ProcessRef<T>` payloads received by the current step.
Declare every body effect in the enclosing step effect list.

## Statements

The accepted statements are:

```strata
emit "text";
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
if (flag == True) { emit "true"; } else { emit "false"; }
if (flag == True) { emit "true"; }
if (flag == True) {} else { emit "false"; }
for item in items { send worker Branch(item); }
for item in items { if ((item != False) && !(item == False)) { send worker Branch(item); } }
return Stop(state);
return Continue(next_state);
return Panic(failed_state);
```

`emit` records and prints an output literal. Output literals must be non-empty,
must not contain control characters, and do not support string escapes in this
slice.

`spawn` starts another declared process and returns an immutable typed process
reference. The reference binding is local to the transition and must be
typed as `ProcessRef<TargetProcess>`.

`send` queues a message through a process reference spawned in the same
transition or through a received `ProcessRef<T>` payload binding. The message
must be accepted by the reference target's process message enum. Static
validation rejects
self-spawn, spawning the already-started entry process, duplicate
process-reference binding in one transition, sends before the reference is
bound, mailbox overflow, and messages left unhandled after a target stops
normally.

Payload messages use the variant constructor at the send site:

```strata
send worker Assign(Job { phase: Ready });
```

The payload value is checked against the target message variant's payload type.
Unit message variants reject payload arguments, and payload variants require
one payload argument.

Process references can be payloads when the message variant declares a typed
reference:

```strata
enum WorkerMsg { Work(ProcessRef<Sink>) }
send worker Work(sink);
```

The received reference is immutable and can be used as a send target:

```strata
fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
    send reply_to Done;
    return Stop(state);
}
```

Runtime dispatch uses the transported runtime process ID and admitted target
process ID. Source names remain diagnostics and trace metadata.

Received references can also be used as send targets inside admitted
statement-level branches and bounded loop bodies. This does not make process
references general source values: they remain direct message payload authority,
and lowering emits typed received-payload send targets rather than source
binding names.

Current process-reference boundaries:

| Surface | Current status |
| --- | --- |
| `let worker: ProcessRef<Worker> = spawn Worker;` | Admitted as an immutable transition-local binding. The checker resolves the target process before lowering. |
| `send worker Ping;` | Admitted for a process reference spawned earlier in the same transition. Lowering emits a process-reference table ID. |
| `enum WorkerMsg { Work(ProcessRef<Sink>) }` | Admitted only as a direct message payload type. |
| `send worker Work(sink);` and `send reply_to Done;` | Admitted for direct process-reference payload forwarding. Mantle routes by admitted target process ID and runtime process ID. |
| Multiple `ProcessRef<Worker>` bindings to one process definition | Admitted. Each spawn creates a separate runtime process instance. |
| Process references directly in record fields or collection element/key/value types | Rejected. Process references are runtime authority, not general immutable data values in this slice. |
| Process references nested inside record, enum, list, map, or next-state payload templates | Rejected. A process reference must be the direct payload of a message that declares `ProcessRef<T>`. |
| Process references in process state values or next-state templates | Rejected. Process states remain immutable source values without embedded runtime authority. |
| Sending by process definition names, source strings, registries, dynamic worker pools, supervisor child sets, stale-reference semantics, restart semantics | Future actor-topology semantics. They require separate authority, lifetime, failure, and observability rules. |

Patterns are source-level syntax for typed value decomposition. The current
runnable subset admits constructor patterns, constructor payload bindings,
nested constructor and record/list/map payload destructuring, helper return-match
expressions, and wildcards. Normal source helpers may match concrete enum values
or destructure concrete record/list/map values, `init` may use one whole-body
match over a fieldless enum constructor to select the initial state, and actor
message dispatch may use one whole-body match over the typed message parameter:

```strata
fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    match msg {
        First => {
            emit "worker matched First";
            return Continue(SawFirst);
        }
        Second => {
            emit "worker matched Second";
            return Stop(Done);
        }
    }
}
```

Step `match` is an authoring form for the same semantics as step parameter
patterns, including typed payload bindings and nested record/list/map payload
destructuring. Whole-body `match msg` arms and step parameter patterns may split
one top-level message constructor by exact nested typed payload predicates when
those predicates are provably disjoint over discovered concrete payload cases.
Checking resolves each arm into typed message-keyed transitions, typed payload
guards when needed, and typed projection templates before lowering. Mantle still
dispatches by admitted message IDs, current state IDs when a transition is
state-specific, payload type IDs, exact typed payload identity, and loaded
template structure, not by source strings. In this buildable step subset the
match scrutinee must be the typed message parameter, and match arms are
block-delimited without comma separators.

A message-specific `step` may instead match the current state parameter when
the process state type is an enum:

```strata
fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    match state {
        Idle => {
            emit "worker had no job";
            return Stop(Idle);
        }
        Working(job: Job) => {
            emit "worker completed job";
            return Stop(Done(job));
        }
        Done(job: Job) => {
            emit "worker already done";
            return Stop(Done(job));
        }
    }
}
```

State-match arms resolve against the declared process state enum and are
exhaustive over its variants. Payload-bearing state variants may bind the whole
payload with the declared payload type or destructure a concrete record/list/map
payload. Each binding is immutable and local to that transition arm. Lowering
emits typed Mantle transitions keyed by admitted message ID plus admitted
current state ID, and runtime selection fails closed if the current state is not
admitted.

An `init` match is checked against the enum that owns the scrutinee constructor.
It must be exhaustive, duplicate-free, and statement-free; each arm returns an
immutable whole state value. Payload-bearing enum variants can be covered by
explicit patterns or `_`, but `init` arms cannot materialize payload bindings in
the returned state because the initial state lowers to one static state ID.

## Effects

The `! [...]` effect list is source-level authority for the runtime effects used
by each `step` clause. It must exactly match the clause actions. For a
`match msg`, `match state`, or `step return match` step, the one effect list
applies to every generated transition. Match-body arms must use exactly those
effects, and a return-match uniform prefix must lower the same actions onto each
selected transition. Missing, duplicate, and unused declared effects are
rejected before lowering.

| Effect | Statement |
| --- | --- |
| `emit` | `emit "text";` |
| `spawn` | `let worker: ProcessRef<Worker> = spawn Worker;` |
| `send` | `send worker Ping;` or `send reply_to Done;` |

`init` cannot perform statements in the buildable slice and therefore uses an
empty effect list.

## Step Patterns

A `step` parameter pattern handles one message constructor:

```strata
fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
    emit "hello from Strata";
    return Stop(state);
}
```

Payload constructors can bind the received payload in a `step` parameter pattern
or a whole-body `match msg` arm:

```strata
fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Stop(WorkerState { job: job });
}

fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
    match msg {
        Assign(job: Job) => {
            return Stop(WorkerState { job: job });
        }
    }
}
```

The binding is immutable and local to that transition. It can be used where a
value of the bound payload type is expected, including whole-value state
returns, record fields, downstream message payload sends, and send targets when
the payload type is `ProcessRef<T>`. Payload bindings cannot shadow the `state`
parameter, process declarations, type names, or value constructors.
Process-reference bindings in the same transition cannot shadow a payload
binding.

Record, list, and map payloads can also be destructured directly in step
parameter patterns, `match msg` arms, and `match state` arms:

```strata
fn step(state: WorkerState, Assign(Job { phase })) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Items(List[phase, ..tail])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Lookup(Map[Ready => phase, ..rest])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Envelope(Assign(Job { phase }))) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Holding(List[Job { phase }, ..tail])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { tail: tail });
}
```

These bindings are immutable projections of the concrete payload value. A
constructor payload, record field, list element, list rest, map value, or map
rest can be used in whole-value state returns and downstream payloads, but
process references still remain valid only as direct message payload bindings.
Fieldless nested enum constructors such as `Envelope(Assign(Ready))` are
accepted as typed shape predicates; they do not introduce bindings.
Shape-only collection payload patterns such as `Items(List[_])`,
`Lookup(Map[Ready => _])`, or `Lookup(Map[..])` are not admitted in this slice;
use the constructor pattern without destructuring when the payload is ignored.
Multiple parameter-pattern or state-match `step` clauses may share one
top-level message constructor only when exact nested typed payload predicates
are provably disjoint. For state-match clauses, each accepted payload case
expands across the admitted current-state cases from the `match state` body, and
lowering emits typed transitions keyed by message ID, current state ID, and exact
typed payload guard. A wildcard step pattern may cover discovered concrete
payload cases not matched by explicit payload-sensitive step-signature or
state-match clauses. The fallback lowers to exact typed payload-guarded
transitions for those discovered cases; it is not an open-ended runtime
catch-all for future payload values. State-match fallback cases additionally
expand across the admitted current-state cases from the fallback `match state`
body before lowering.

If a process accepts more than one message, it can declare explicit clauses for
specific constructors and one wildcard clause for the remaining variants:

```strata
fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "worker handled First";
    return Continue(SawFirst);
}

fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "worker handled Second";
    return Stop(Done);
}
```

Every accepted message variant, or every discovered concrete payload case inside
a payload-sensitive split, must resolve to exactly one generated transition.
Explicit constructor clauses handle their named variants. One wildcard clause
may cover variants that do not have explicit clauses. When payload-sensitive
step-signature or state-match splitting is active for a variant, the same
wildcard may cover only discovered concrete payload cases not matched by
explicit payload predicates. Duplicate explicit clauses, overlapping payload
predicates, missing coverage, duplicate wildcard clauses, missing variant
coverage, and unreachable wildcard clauses are rejected.
Parameter patterns are compile-time dispatch only: Mantle dequeues one message
at a time and dispatches by typed message ID, current state ID when a transition
is state-specific, and exact typed payload identity when a payload guard exists.
Payload-bearing variants keep one stable admitted message case, and their
immutable values travel in runtime message envelopes.

## State Transitions

`step` returns `ProcResult<StateType>`:

```strata
return Continue(next_state);
return Stop(final_state);
return Panic(failed_state);
```

`Continue(value)` replaces the process state and keeps the process running.
`Stop(value)` replaces the process state and terminates the process normally.
`Panic(value)` replaces the process state, marks the process failed, records
failure trace evidence, and fails the run without replaying the consumed
message.

Passing the `state` parameter preserves the supplied state:

```strata
return Stop(state);
```

Passing a record value, enum variant, list, or map creates an explicit
whole-value state replacement:

```strata
return Continue(WorkerState { phase: Idle });
return Continue(Working(Job { phase: Ready }));
return Continue(List<Phase,2>[Ready, Done]);
return Continue(Map<Phase,Phase,1>[Ready => Done]);
return Stop(Handled);
return Panic(Failed);
```

A `step` body may also use `return match` over a concrete enum source value
binding when the checker can reduce the match to one typed transition before
lowering. Statements before the `return match` are a uniform action prefix; the
checker lowers that same typed action list onto every generated transition:

```strata
fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "return-match prefix";
    return match phase {
        Ready => {
            return Continue(SawReady);
        }
        Done => {
            return Stop(Done);
        }
    };
}
```

This form is not runtime dispatch. The checker requires an immutable enum source
binding with a concrete value proven during step clause or state-match expansion,
checks every arm as a `ProcResult<StateType>`, selects the matching arm, and
lowers the selected arm to the existing typed transition shape. Uniform prefix
effects occur before the return selection in source program order and remain
committed runtime actions. Return-match arms remain statement-free, so per-arm
effects are not admitted. Matching `state`, matching non-enum values, and
dynamic payload catch-all dispatch are not admitted.

Current pattern-matching closure boundaries:

| Surface | Current status |
| --- | --- |
| `step return match` after uniform pre-return effects | Admitted. Prefix actions lower identically onto each selected typed transition. |
| `step return match state` | Rejected. Use whole-body `match state`, then match a concrete state-payload binding when one is proven. |
| `init match` over payload-bearing arm constructors | Admitted only when every arm returns a state value that does not materialize payload bindings. |
| `init return match` over payload-bearing constructors | Rejected in this slice. `init return match` selects a static initial state from a fieldless enum scrutinee. |
| Init arms materializing payload bindings in the initial state | Rejected. The initial state lowers to one static typed state ID. |
| Shape-only collection predicates such as `Items(List[_])`, `Lookup(Map[Ready => _])`, or `Lookup(Map[..])` | Rejected. Bind at least one immutable projected value, or match only the enclosing constructor when the payload is ignored. |
| Dynamic-key map matching | Rejected. Map pattern keys are static source values in this slice. |
| Arbitrary/general match expressions outside admitted helper, `init`, `step`, `match msg`, and `match state` forms | Future language semantics, not part of the current buildable surface. |

State changes are immutable whole-value transitions. There is no assignment
statement and no source-visible field mutation.

Payload-bearing process states can be observed only through checked immutable
state patterns such as `Working(job: Job)`. Returning `Done(job)` creates a new
whole state value; it does not rewrite the payload inside the existing state.

## Limits

The buildable source slice enforces bounded sizes:

| Limit | Value |
| --- | --- |
| Source bytes | 1 MiB |
| Identifier bytes | 128 |
| Distinct checked artifact types | 4096 |
| Output literal bytes | 16 KiB |
| Processes | 256 |
| State values per process | 1024 |
| Message variants per process | 1024 |
| Static process-reference bindings per process definition | 4096 |
| Distinct output literals | 4096 |
| Actions per process | 4096 |
| Mailbox bound | 65,536 |
| Type nesting | 32 |
| Value nesting | 32 |

These limits are part of the admitted artifact and runtime boundary.
