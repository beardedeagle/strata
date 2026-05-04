# Examples

Runnable Strata examples live under `examples/`.

Read them in this order:

1. `hello.str` for the minimum source-to-runtime program.
2. `actor_ping.str` for spawning, sending, and a single worker transition.
3. `actor_sequence.str` for multiple messages and message-keyed transitions.

## Hello

`examples/hello.str` is the first source-to-runtime product gate. It checks,
builds, runs, emits `hello from Strata`, and records an observability trace.

```sh
cargo build
target/debug/strata check examples/hello.str
target/debug/strata build examples/hello.str
target/debug/mantle run target/strata/hello.mta
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
target/debug/strata check examples/actor_ping.str
target/debug/strata build examples/actor_ping.str
target/debug/mantle run target/strata/actor_ping.mta
```

Key source ideas:

- `Main` uses `spawn` before `send`.
- `WorkerMsg.Ping` is checked against `Worker`'s message type.
- `Worker` replaces `Idle` with `Handled`.
- Both processes stop normally.

## Actor Sequence

`examples/actor_sequence.str` exercises message-keyed process transitions. The
worker handles `First`, returns a whole replacement state with `Continue(...)`,
then handles `Second` and returns a whole replacement state with `Stop(...)`.

```sh
target/debug/strata check examples/actor_sequence.str
target/debug/strata build examples/actor_sequence.str
target/debug/mantle run target/strata/actor_sequence.mta
```

Key source ideas:

- `WorkerMsg` has two variants, so `Worker.step` must use `match msg`.
- The match is exhaustive.
- `Continue(SawFirst)` keeps the worker alive for the next queued message.
- `Stop(Done)` terminates the worker normally.

The runtime trace records process, message, state, and output IDs alongside
labels so that behavior can be checked without treating labels as executable
bindings.
