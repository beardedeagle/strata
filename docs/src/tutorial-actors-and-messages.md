# Tutorial: Actors And Messages

This tutorial introduces multiple processes, message sends, and message-keyed
state transitions.

Read Tutorial: Hello first if the basic `Main` process shape is unfamiliar.

## Process Roles

`examples/actor_ping.str` has two processes:

- `Main`, the entry process;
- `Worker`, a process spawned by `Main`.

`Main` starts the worker, sends `Ping`, and stops. `Worker` receives `Ping`,
emits output, transitions to `Handled`, and stops.

## Worker Message Type

```strata
enum WorkerMsg {
    Ping,
}
```

This says `Worker` accepts one message: `Ping`.

`Main` can send that message after spawning `Worker`:

```strata
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
return Stop(state);
```

`worker` is an immutable process reference for the spawned runtime instance. The
order matters in this source surface. Sending through a reference before it is
bound is
rejected.

## Worker State Type

```strata
enum WorkerState {
    Idle,
    Handled,
}
```

`Worker` starts in `Idle`:

```strata
fn init() -> WorkerState ! [] ~ [] @det {
    return Idle;
}
```

After handling `Ping`, it stops in `Handled`:

```strata
fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "worker handled Ping";
    return Stop(Handled);
}
```

The transition is a whole state replacement. `Handled` is the new state value.

## Multiple Messages

`examples/actor_sequence.str` extends the pattern with two worker messages:

```strata
enum WorkerMsg {
    First,
    Second,
}
```

When a process accepts multiple messages, each message must resolve to one
`step` clause. Explicit patterns handle named variants, and `_` handles the
remaining accepted variants:

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

Missing coverage, duplicate explicit patterns, duplicate wildcard patterns, and
wildcards that cannot cover any remaining message are rejected.

## Continue Versus Stop

`Continue(SawFirst)` means:

- replace the worker state with `SawFirst`;
- keep the worker running;
- allow later queued messages to be handled.

`Stop(Done)` means:

- replace the worker state with `Done`;
- terminate the worker normally.

The runtime trace records both steps with message IDs and state IDs.

## Multiple Instances

`examples/actor_instances.str` spawns two runtime instances of the same process
definition:

```strata
let first: ProcessRef<Worker> = spawn Worker;
let second: ProcessRef<Worker> = spawn Worker;
send first Ping;
send second Ping;
```

`first` and `second` are separate process references. Mantle assigns each
spawned worker a different runtime `pid`, and the trace records both messages
and both worker steps with the same process definition ID but different process
instance IDs.

## Run The Actor Examples

```sh
cargo build

cargo run -p strata --bin strata -- check examples/actor_ping.str
cargo run -p strata --bin strata -- build examples/actor_ping.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_ping.mta

cargo run -p strata --bin strata -- check examples/actor_sequence.str
cargo run -p strata --bin strata -- build examples/actor_sequence.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta

cargo run -p strata --bin strata -- check examples/actor_instances.str
cargo run -p strata --bin strata -- build examples/actor_instances.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_instances.mta
```

For `actor_sequence`, the trace should show `Worker` dequeuing `First`, stepping
with `Continue`, then later dequeuing `Second` and stepping with `Stop`.
