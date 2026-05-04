# Language Concepts

Strata is a source language for explicit state, explicit effects, typed
messages, and runtime-observable process execution.

The current implementation is intentionally small. It is still a language
surface: a `.str` file declares source-level records, enums, processes, message
types, state types, and process transitions. Mantle executes the admitted
artifact produced from that source.

## Source Versus Runtime

Strata source is where author-visible meaning lives:

- names;
- records;
- enums;
- process declarations;
- message declarations;
- state transition expressions;
- declared effects.

Mantle runtime is where admitted execution lives:

- process instances;
- process handles;
- mailboxes;
- loaded typed IDs;
- transition tables;
- runtime events;
- trace output.

The two surfaces are related, but not interchangeable. Source labels are useful
for diagnostics and traces. Runtime dispatch uses loaded typed IDs.

## Processes

A Strata program is organized around processes. A process declares:

- a bounded mailbox;
- a state type;
- a message type;
- an `init` function that creates the initial state;
- a `step` function that handles messages and returns a transition result.

The entry process is named `Main`. Mantle starts `Main` and delivers its first
message variant as the entry message.

Spawning a process creates a runtime process instance and binds it to a process
handle:

```strata
spawn Worker as worker;
```

The handle names that instance for the process that spawned it. Multiple
handles may target the same process definition, which creates multiple runtime
instances.

## Messages

A process message type is an enum. Each variant is a message the process can
accept.

```strata
enum WorkerMsg {
    Ping,
}
```

Sends are statically checked against the target process message enum:

```strata
send worker Ping;
```

The current source form sends message variants through process handles only.
Message payloads are not available yet.

## State

Process state is immutable at the source level. A transition returns a whole
replacement state or the current state.

```strata
return Continue(SawFirst);
return Stop(state);
```

There is no assignment statement and no field update expression. Record state is
constructed as a new whole value:

```strata
return Continue(WorkerState { phase: Idle });
```

## Effects

Effects must be visible in the function signature. The current effects are:

- `emit`;
- `spawn`;
- `send`.

The declared effect list must exactly match the effects used by `step`.

```strata
fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
    spawn Worker as worker;
    send worker Ping;
    return Stop(state);
}
```

This is deliberately stricter than "allow anything in the list." A declared but
unused effect is rejected.

## Determinism And May-Behaviors

Function signatures include determinism and may-behavior positions:

```strata
! [effects] ~ [may_behaviors] @det
```

The current buildable source requires `~ [] @det`. The parser recognizes
`@nondet`, but accepted buildable programs are deterministic today.

## Execution Shape

The source-to-runtime path is:

```text
.str source -> parse -> check -> lower -> .mta artifact -> admit -> run -> trace
```

Each phase has a different responsibility. Parser success alone does not mean a
program is accepted. A program must also pass semantic checking, artifact
validation, and runtime admission.
