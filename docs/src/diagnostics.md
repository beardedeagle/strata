# Diagnostics

Strata diagnostics are intended to reject invalid source close to the layer
that can explain it. Parser errors describe source shape. Checker errors
describe semantic rules. Runtime errors describe admitted execution failures.

## Reading A Diagnostic

Run:

```sh
cargo run -p strata --bin strata -- check examples/hello.str
```

If checking fails, fix the first reported error first. Later errors may be a
result of the first invalid shape.

## Common Source Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `expected record, enum, or proc declaration` | A top-level item is not accepted. | Use only `record`, `enum`, or `proc` after `module`. |
| `entry process Main is not declared` | The program has no `Main` process. | Add `proc Main ...`. |
| `process ... must declare type State` | A process is missing its state alias. | Add `type State = StateType;`. |
| `process ... must declare type Msg` | A process is missing its message alias. | Add `type Msg = MessageEnum;`. |
| `unsupported process function` | A process declares a function other than `init` or `step`. | Move the logic into `init` or `step`; general functions are not available yet. |
| `init must declare no parameters` | `init` has parameters. | Use `fn init() -> StateType ...`. |
| `init body must not perform statements` | `init` uses `emit`, `spawn`, or `send`. | Return only the initial state. |
| `step must declare state and msg parameters` | `step` has the wrong parameter count. | Use `state: StateType, msg: MsgType`. |
| `step returns ..., expected ProcResult<...>` | `step` return type is wrong. | Return `ProcResult<StateType>`. |
| `step may-behaviors must be empty` | The `~ [...]` list is not empty. | Use `~ []`. |
| `step must be deterministic` | `step` uses `@nondet`. | Use `@det`. |
| `declares effect ... but does not use it` | The effect list is wider than the body. | Remove the unused effect. |
| `uses effect ... but does not declare it` | The body uses an undeclared effect. | Add the effect to `! [...]`. |

## Message Handling Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `step with multiple messages must use match msg` | A process message enum has multiple variants, but `step` uses a simple block. | Replace the body with exhaustive `match msg`. |
| `step must match msg` | The match scrutinee is not `msg`. | Use `match msg`. |
| `duplicate match arm for message` | A message variant has more than one arm. | Keep one arm per variant. |
| `step match must cover message` | A message variant is missing. | Add an arm for the missing message. |
| `sends message ... not accepted by ...` | The target process message enum has no such variant. | Send a declared target message variant. |

## State Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `value ... is not a variant of enum ...` | A returned enum value does not belong to the expected enum. | Return a variant from the process state enum. |
| `record constructor ... does not match expected record ...` | A record value constructor does not match the expected state type. | Construct the expected record type. |
| `record value fields use ':'` | A record value used assignment syntax. | Use `field: value`, not `field = value`. |
| `state value state conflicts` | A state enum variant is named `state`. | Rename the variant. |

## Process And Mailbox Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `spawns itself` | A process tries to spawn itself. | Spawn another declared process. |
| `conflicts with a process declaration` | A process handle uses the same name as a process definition. | Use a distinct handle name. |
| `undeclared process handle` | A send references a handle that is never spawned by that process. | Add a matching `spawn Process as handle;` statement. |
| `unbound process handle` | A runnable path sends through a handle before it is bound. | Spawn the handle before sending on that path. |
| `duplicates process handle id` | A transition tries to bind the same handle twice. | Use two distinct handles or bind once. |
| `mailbox would exceed bound` | A send would overflow the target mailbox. | Increase the mailbox bound or send fewer messages before the target runs. |
| `would retain ... unhandled message` | A process can stop while messages remain in its mailbox. | Continue until queued messages are handled or avoid queuing them. |
| `mailbox_bound must be no greater than` | The mailbox bound exceeds the admitted limit. | Lower the bound. |

## Runtime Errors

Runtime errors are emitted by Mantle after artifact admission starts. Common
causes include invalid artifacts, blocked trace paths, mailbox exhaustion, trace
size exhaustion, and dispatch budget exhaustion.

Use the source gate first:

```sh
cargo run -p strata --bin strata -- check path/to/program.str
cargo run -p strata --bin strata -- build path/to/program.str
```

Then run Mantle:

```sh
cargo run -p mantle-runtime --bin mantle -- run target/strata/program.mta
```

If source checking passes but Mantle rejects an artifact, inspect the artifact
and runtime boundary docs before changing runtime behavior.
