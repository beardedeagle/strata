# Examples

Runnable Strata examples live under `examples/`.

Read them in this order:

1. `hello.str` for the minimum source-to-runtime program.
2. `actor_ping.str` for spawning, sending, and a single worker transition.
3. `actor_sequence.str` for multiple messages and message-keyed transitions.
4. `actor_instances.str` for multiple runtime instances of one process
   definition.

## Hello

`examples/hello.str` is the first source-to-runtime product gate. It checks,
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
then handles `Second` and returns a whole replacement state with `Stop(...)`.

```sh
cargo run -p strata --bin strata -- check examples/actor_sequence.str
cargo run -p strata --bin strata -- build examples/actor_sequence.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta
```

Key source ideas:

- `WorkerMsg` has two variants, so `Worker` declares two `step` clauses.
- The step patterns are exhaustive.
- `Continue(SawFirst)` keeps the worker alive for the next queued message.
- `Stop(Done)` terminates the worker normally.

The runtime trace records process, message, state, and output IDs alongside
labels so that behavior can be checked without treating labels as executable
bindings.

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
