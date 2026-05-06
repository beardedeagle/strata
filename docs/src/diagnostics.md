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
| `step must declare state parameter and message pattern` | `step` has the wrong parameter count. | Use `state: StateType, MessageVariant` or `state: StateType, _`. |
| `step second parameter must be a message variant pattern or wildcard pattern` | The second `step` parameter is a typed binding instead of a message pattern. | Replace `msg: MsgType` with a message variant or `_`. |
| `step returns ..., expected ProcResult<...>` | `step` return type is wrong. | Return `ProcResult<StateType>`. |
| `step may-behaviors must be empty` | The `~ [...]` list is not empty. | Use `~ []`. |
| `step must be deterministic` | `step` uses `@nondet`. | Use `@det`. |
| `declares effect ... but does not use it` | The effect list is wider than the body. | Remove the unused effect. |
| `uses effect ... but does not declare it` | The body uses an undeclared effect. | Add the effect to `! [...]`. |

## Message Handling Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `step pattern message ... is not accepted` | A `step` signature names a message variant outside the process message enum. | Use a declared message variant. |
| `duplicate step pattern for message` | A message variant has more than one explicit `step` clause. | Keep one explicit clause per variant. |
| `duplicate wildcard step pattern` | More than one `step` clause uses `_`. | Keep one wildcard clause. |
| `wildcard step pattern is unreachable` | Explicit clauses already cover every accepted message variant. | Remove the wildcard clause or remove an explicit clause that it should cover. |
| `must declare step pattern for message` | A message variant is not covered by an explicit or wildcard `step` clause. | Add a `step` clause for the missing message or add one `_` clause. |
| `message match bodies are not supported` | A function body uses `match msg`. | Declare `step` clauses with message patterns instead. |
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
| `conflicts with a process declaration` | A process reference uses the same name as a process definition. | Use a distinct reference name. |
| `undeclared process reference` | A send references a name that is never spawned in the process. | Add a matching `let worker: ProcessRef<Worker> = spawn Worker;` statement. |
| `unbound process reference` | A transition sends through a reference before it is bound. | Spawn the reference before sending through it. |
| `duplicates process reference id` | A transition binds the same reference twice. | Use two distinct references or bind once. |
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
