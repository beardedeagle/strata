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
| Patterns | Constructor patterns, constructor payload bindings, and `_` wildcards. |
| Message payloads | `enum WorkerMsg { Assign(Job) }`, `enum WorkerMsg { Work(ProcessRef<Sink>) }`, payload sends, and payload-binding step patterns. |
| Pattern dispatch | Function signature patterns, source function match bodies, fieldless enum matches in `init`, step parameter patterns, wildcard step patterns, and one whole-body `match msg` step form per process. |
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

`as`, `let`, `mut`, and `var` are reserved everywhere identifiers are accepted.
`ProcResult` and `ProcessRef` are reserved type names because they name built-in
transition and process-reference types.
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

Record value fields use `:`, not `=`.

Payload-bearing enum values use constructor syntax with one immutable payload
value:

```strata
Assigned(Job { phase: Ready })
```

The checker resolves this form against the expected enum type. If the identifier
names a source helper instead, it is expanded as a helper call; constructor and
helper names cannot collide silently.

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
message enums. State enum payload variants are rejected in the buildable slice.

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
match. A process cannot mix step parameter patterns with a match step body in
this slice.

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

These matches are exhaustive, duplicate-free, and immutable. Payload-bearing
source helper patterns bind payloads as immutable values. A helper call must
provide a concrete enum constructor value for signature-pattern or whole-body
match dispatch; helpers are still expanded before lowering and do not become
runtime dispatch entries.

## Statements

The accepted statements are:

```strata
emit "text";
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
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

Patterns are source-level syntax for typed value decomposition. The current
runnable subset admits constructor patterns, constructor payload bindings, and
wildcards. Normal source helpers may match concrete enum values, `init` may use
one whole-body match over a fieldless enum constructor to select the initial
state, and actor message dispatch may use one whole-body match over the typed
message parameter:

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
patterns, including typed payload bindings. Checking resolves each arm into
typed message-keyed transitions before lowering. Mantle still dispatches by
admitted message IDs and payload type IDs, not by source strings. In this
buildable step subset the match scrutinee must be the typed message parameter,
and match arms are block-delimited without comma separators.

An `init` match is checked against the enum that owns the scrutinee constructor.
It must be exhaustive, duplicate-free, and statement-free; each arm returns an
immutable whole state value. Payload-bearing enum variants can be covered by
explicit patterns or `_`, but `init` arms cannot materialize payload bindings in
the returned state because the initial state lowers to one static state ID.

## Effects

The `! [...]` effect list is source-level authority for the runtime effects used
by each `step` clause. It must exactly match the clause actions. For a
match step, the one effect list applies to every generated transition,
so each arm must use exactly those effects. Missing, duplicate, and unused
declared effects are rejected before lowering.

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

Every accepted message variant must resolve to exactly one clause. Explicit
constructor clauses handle their named variants. One wildcard clause may cover
variants that do not have explicit clauses. Duplicate explicit clauses,
duplicate wildcard clauses, missing coverage, and unreachable wildcard clauses
are rejected. Parameter patterns are compile-time dispatch only: Mantle
dequeues one message at a time and dispatches by typed message ID.
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

Passing a record value or enum variant creates an explicit whole-value state
replacement:

```strata
return Continue(WorkerState { phase: Idle });
return Stop(Handled);
return Panic(Failed);
```

State changes are immutable whole-value transitions. There is no assignment
statement and no source-visible field mutation.

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
