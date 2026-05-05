# Language Reference

This page documents the Strata source surface accepted by the current buildable
slice. It is an authoring reference for `.str` programs, not a description of
Mantle artifact internals.

## Current Surface

| Area | Available Today |
| --- | --- |
| Source unit | One `module name;` declaration per file. |
| Top-level declarations | `record`, `enum`, and `proc`. |
| Classes | Not available. |
| Methods | Not available. |
| Top-level functions | Not available. |
| Process functions | `init` and `step` only. |
| Imports | Not available. |
| Standard library | Not available. |
| Effects | `emit`, `spawn`, and `send`. |
| Process references | `let worker: ProcessRef<Worker> = spawn Worker;` and `send worker Ping;`. |
| Transition result | `ProcResult<T>` with `Stop(value)` and `Continue(value)`. |

The current `module` declaration names a source unit. It does not create an
import namespace, package, library, or visibility boundary yet.

## Source Unit

A Strata source file starts with a module declaration:

```strata
module hello;
```

After the module declaration, the accepted top-level declarations are records,
enums, and processes.

```strata
module example;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
```

Every buildable program must declare a `Main` process. Mantle starts `Main` and
delivers the first message variant of `Main`'s message enum as the entry
message.

## Identifiers

Identifiers must start with an ASCII letter or `_`, then contain only ASCII
letters, ASCII digits, or `_`.

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
```

`as`, `let`, `mut`, and `var` are reserved everywhere identifiers are accepted.
`ProcResult` and `ProcessRef` are reserved type names because they name built-in
transition and process-reference types.

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
```

Enums used as process state or message types must declare at least one variant.
Duplicate variants are rejected.

## Processes

A process declares a mailbox bound, a state type, a message type, an `init`
function, and a `step` function:

```strata
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
```

Only the aliases `State` and `Msg` are accepted inside a process. Only the
functions `init` and `step` are accepted inside a process.

## Function Signatures

The current function signature shape is:

```strata
fn name(params...) -> ReturnType ! [effects] ~ [may_behaviors] @det {
    ...
}
```

Buildable source currently requires:

| Function | Required Shape |
| --- | --- |
| `init` | No parameters, returns the process state type, uses `! [] ~ [] @det`. |
| `step` | Parameters exactly `state: StateType, msg: MsgType`, returns `ProcResult<StateType>`, uses `~ [] @det`. |

The parser recognizes `@nondet`, but buildable source currently rejects it.
The may-behavior list after `~` must currently be empty.

## Statements

The accepted statements are:

```strata
emit "text";
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
return Stop(state);
return Continue(next_state);
```

`emit` records and prints an output literal. Output literals must be non-empty,
must not contain control characters, and do not support string escapes in this
slice.

`spawn` starts another declared process and returns an immutable typed process
reference. The reference binding is local to the current transition and must be
typed as `ProcessRef<TargetProcess>`.

`send` queues a message through a previously spawned process reference. The
message must be accepted by the reference target's process message enum. Static
validation rejects self-spawn, spawning the already-started entry process,
duplicate process-reference binding in one transition, sends before the
reference is bound, mailbox overflow, and messages left unhandled after a target
stops.

## Effects

The `! [...]` effect list must exactly match the effects used by `step`.
Missing effects and unused declared effects are both rejected.

| Effect | Statement |
| --- | --- |
| `emit` | `emit "text";` |
| `spawn` | `let worker: ProcessRef<Worker> = spawn Worker;` |
| `send` | `send worker Ping;` |

`init` cannot perform statements in the current buildable slice and therefore
uses an empty effect list.

## Step Bodies

If a process accepts exactly one message, `step` can use a simple block:

```strata
fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {
    emit "hello from Strata";
    return Stop(state);
}
```

If a process accepts more than one message, `step` must use exhaustive
`match msg`:

```strata
fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    match msg {
        First => {
            emit "worker handled First";
            return Continue(SawFirst);
        }
        Second => {
            emit "worker handled Second";
            return Stop(Done);
        }
    }
}
```

Each message variant must have exactly one arm. Matching on anything other than
`msg` is rejected.

## State Transitions

`step` returns `ProcResult<StateType>`:

```strata
return Continue(next_state);
return Stop(final_state);
```

`Continue(value)` replaces the process state and keeps the process running.
`Stop(value)` replaces the process state and terminates the process normally.

Passing the `state` parameter keeps the current state:

```strata
return Stop(state);
```

Passing a record value or enum variant creates an explicit whole-value state
replacement:

```strata
return Continue(WorkerState { phase: Idle });
return Stop(Handled);
```

State changes are immutable whole-value transitions. There is no assignment
statement and no source-visible field mutation.

## Current Limits

The buildable source slice enforces bounded sizes:

| Limit | Value |
| --- | --- |
| Source bytes | 1 MiB |
| Identifier bytes | 128 |
| Output literal bytes | 16 KiB |
| Processes | 256 |
| State values per process | 1024 |
| Message variants per process | 1024 |
| Process references per process | 4096 |
| Distinct output literals | 4096 |
| Actions per process | 4096 |
| Mailbox bound | 65,536 |
| Type nesting | 32 |
| Value nesting | 32 |

These limits are part of the current admitted artifact and runtime boundary.
