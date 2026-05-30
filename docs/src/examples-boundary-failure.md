# Import, Boundary, And Failure Examples

This page covers runnable examples where source-unit visibility, boundary
contracts, or fail-closed runtime behavior are the primary evidence.

## Source-Unit Imports

`examples/imports_main.str` imports `imports_types.str` and `imports_worker.str`.
The root owns `Main`, the type unit owns `Job`, `Phase`, and `complete`, and the
worker unit owns `Worker`. Strata resolves that graph before checking and
lowering.

```sh
just run-example imports_main
```

The graph is checked in deterministic dependency-first order, direct imports
authorize visible declarations, and Mantle runs `target/strata/imports_main.mta`
without loading `.str` files or resolving import names at runtime.

## Boundary Contracts

`examples/boundary_contracts_main.str` runs the checked boundary surface in
[Boundary Contracts](boundary-contracts.md):

Run it with `just run-example boundary_contracts_main`.

## Actor Panic No Replay

`examples/actor_panic_no_replay.str` shows an abnormal transition that consumes
one queued `Ping`, returns `Panic(Failed)`, records failure evidence, and does
not replay the consumed message.

```sh
just run-example actor_panic_no_replay
```

The final command is expected to return non-zero. The trace should show two
accepted `Ping` messages, one `message_dequeued`, one `process_stepped` with
`result:"Panic"`, and one `process_failed` event.
