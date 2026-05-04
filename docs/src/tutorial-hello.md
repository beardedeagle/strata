# Tutorial: Hello

This tutorial walks through the smallest Strata program currently accepted by
the source-to-runtime gate.

The complete source is `examples/hello.str`.

## Module

```strata
module hello;
```

The module declaration names the source unit. It does not import anything and
does not create a package boundary yet.

## State And Message Types

```strata
record MainState;
enum MainMsg {
    Start,
}
```

`MainState` is a fieldless record. It gives the process a concrete state type
without carrying data.

`MainMsg` declares the messages accepted by `Main`. `Start` is the first variant,
so it is the entry message delivered to `Main` when Mantle starts the program.

## Main Process

```strata
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    ...
}
```

Every runnable program needs a `Main` process. The mailbox bound limits how many
messages can wait in the process mailbox.

The `State` alias points to the process state type. The `Msg` alias points to
the process message enum.

## Initial State

```strata
fn init() -> MainState ! [] ~ [] @det {
    return MainState;
}
```

`init` takes no parameters and returns the initial state. It uses no effects,
has no may-behaviors, and is deterministic.

## Step Function

```strata
fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {
    emit "hello from Strata";
    return Stop(state);
}
```

`step` receives the current state and the dequeued message. Since `MainMsg` has
one variant, the simple block form is accepted.

The body emits text and then returns `Stop(state)`. Passing `state` keeps the
current state while stopping normally.

The effect list is `[emit]` because the body uses exactly one effect.

## Run It

```sh
cargo build
target/debug/strata check examples/hello.str
target/debug/strata build examples/hello.str
target/debug/mantle run target/strata/hello.mta
```

The program prints:

```text
hello from Strata
```

Mantle also writes:

```text
target/strata/hello.observability.jsonl
```

That trace should contain `artifact_loaded`, `process_spawned`,
`message_accepted`, `message_dequeued`, `program_output`, and
`process_stopped` events.

## Common Edits

If you remove `emit "hello from Strata";`, also change the effect list from
`[emit]` to `[]`. Declaring an unused effect is rejected.

If you add a second message variant to `MainMsg`, the simple `step` body must
become an exhaustive `match msg` body.
