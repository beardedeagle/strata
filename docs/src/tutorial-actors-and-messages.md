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
spawn Worker;
send Worker Ping;
return Stop(state);
```

The order matters in this source surface. Sending to a process before it is
spawned is rejected.

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
fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
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

When a process accepts multiple messages, `step` must use exhaustive
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

Each message variant needs one arm. Missing arms and duplicate arms are
rejected.

## Continue Versus Stop

`Continue(SawFirst)` means:

- replace the worker state with `SawFirst`;
- keep the worker running;
- allow later queued messages to be handled.

`Stop(Done)` means:

- replace the worker state with `Done`;
- terminate the worker normally.

The runtime trace records both steps with message IDs and state IDs.

## Run The Actor Examples

```sh
cargo build

target/debug/strata check examples/actor_ping.str
target/debug/strata build examples/actor_ping.str
target/debug/mantle run target/strata/actor_ping.mta

target/debug/strata check examples/actor_sequence.str
target/debug/strata build examples/actor_sequence.str
target/debug/mantle run target/strata/actor_sequence.mta
```

For `actor_sequence`, the trace should show `Worker` dequeuing `First`, stepping
with `Continue`, then later dequeuing `Second` and stepping with `Stop`.
