# Examples

Runnable Strata examples live under `examples/`.

Read them in this order:

1. `hello.str` for the minimum source-to-runtime program.
2. `actor_ping.str` for spawning, sending, and a single worker transition.
3. `actor_sequence.str` for multiple messages and message-keyed transitions.
4. `actor_instances.str` for multiple runtime instances of one process
   definition.
5. `actor_payloads.str` for typed message payloads and immutable payload
   bindings in actor step signatures.
6. `actor_reply.str` for transporting typed process references through message
   payloads.

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
then handles `Second` through the wildcard clause and returns a whole
replacement state with `Stop(...)`.

```sh
cargo run -p strata --bin strata -- check examples/actor_sequence.str
cargo run -p strata --bin strata -- build examples/actor_sequence.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta
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

## Actor Payloads

`examples/actor_payloads.str` sends a typed payload from `Main` to `Worker`.
`Worker` binds that payload in its `step` signature and returns a whole
replacement state containing the immutable payload value.

```sh
cargo run -p strata --bin strata -- check examples/actor_payloads.str
cargo run -p strata --bin strata -- build examples/actor_payloads.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payloads.mta
```

Key source ideas:

- `enum WorkerMsg { Assign(Job) }` declares a payload-bearing message variant.
- `send worker Assign(Job { phase: Ready });` sends one checked payload value.
- `Assign(job: Job)` binds the received payload as an immutable step-local
  value.
- `WorkerState { job: job }` constructs the next state as a whole value.

## Actor Reply References

`examples/actor_reply.str` passes a `ProcessRef<Sink>` as a typed immutable
message payload. `Worker` receives that reference and sends `Done` through it.

```sh
cargo run -p strata --bin strata -- check examples/actor_reply.str
cargo run -p strata --bin strata -- build examples/actor_reply.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_reply.mta
```

Key source ideas:

- `enum WorkerMsg { Work(ProcessRef<Sink>) }` declares a typed reference
  payload.
- `send worker Work(sink);` transports the spawned `Sink` reference to
  `Worker`.
- `Work(reply_to: ProcessRef<Sink>)` binds the received reference immutably.
- `send reply_to Done;` routes by the transported runtime process ID and
  admitted target process ID, not by source labels.
