# Basic Examples

These examples cover the first source-to-runtime programs and initial process dispatch forms.

## Hello

`examples/hello.str` is the first source-to-runtime gate. It checks,
builds, runs, emits `hello from Strata`, and records an observability trace.

```sh
just build
just run-example hello
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
just run-example actor_ping
```

Key source ideas:

- `Main` declares `authority spawn_worker: Cap<Spawn<Worker>>;`, then uses
  `let worker: ProcessRef<Worker> = spawn Worker;` before `send worker Ping;`.
- `WorkerMsg.Ping` is checked against `Worker`'s message type.
- `Worker` replaces `Idle` with `Handled`.
- Both processes stop normally.

## Actor Sequence

`examples/actor_sequence.str` exercises message-keyed process transitions. The
worker handles `First`, returns a whole replacement state with `Continue(...)`,
then handles `Second` through the wildcard clause and returns a whole
replacement state with `Stop(...)`.

```sh
just run-example actor_sequence
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
just run-example actor_match
```

Key source ideas:

- `fn step(state: WorkerState, msg: WorkerMsg)` declares a typed message
  parameter.
- `match msg` must be the whole function body.
- Each arm returns a whole replacement state through `Continue(...)` or
  `Stop(...)`.
- The generated Mantle artifact still dispatches by typed message IDs.

## Init Match

`examples/init_match.str` exercises a non-step whole-body match in `init`. The
checker resolves the fieldless enum scrutinee, proves the arms are exhaustive,
and selects the typed initial state before lowering.

```sh
just run-example init_match
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
just run-example init_return_match
```

Key source ideas:

- `return match Warm { ... };` is accepted only because `Warm` is a fieldless
  enum constructor.
- Each arm is statement-free and returns one immutable whole `MainState` value.
- Mantle receives the selected typed initial state ID; it does not dispatch on
  the source match arm names.
