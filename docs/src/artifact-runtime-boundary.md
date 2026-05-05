# Artifact And Runtime Boundary

Strata owns source syntax, diagnostics, semantic checking, checked IR, and
source-visible meaning. Lowering owns conversion from checked Strata IR into
Mantle Target Artifacts. Mantle owns artifact admission, runtime execution,
process and mailbox state, host boundaries, and observability.

This separation keeps names, metadata, and runtime identity from collapsing into
one surface. Source names are useful for diagnostics and traces, but executable
runtime dispatch must use loaded typed IDs.

## Admission

Mantle admits artifacts through validation, not filename trust. Before
execution, the artifact decoder and validator check:

- artifact magic, format, schema version, and source language;
- bounded process, message, state, output, transition, and action counts;
- unique process debug names;
- unique state values per process;
- unique process handle names per process;
- exactly one transition per accepted message;
- transition references to known messages, state values, process handles,
  outputs, and process IDs.

Decode-time bounds must happen before allocation when counts come from the
artifact body.

## Execution

Mantle loads admitted transitions into indexed runtime tables. A dequeued
message selects the transition by typed message ID. The runtime then applies the
transition as a whole-value state replacement and executes admitted actions.

The current action set covers:

- emitting declared output;
- spawning a declared process through a process handle;
- sending a declared message through a bound process handle.

The runtime fails closed on invalid sends, unbound process handles, process
handle rebinding, mailbox exhaustion, runtime process instance budget
exhaustion, dispatch budget exhaustion, emitted-output budget exhaustion, and
trace budget exhaustion.

## Observability

Runtime traces are line-delimited JSON. They include labels for readability and
numeric IDs for process, message, state, and output identity. A trace is
evidence of runtime execution, not a substitute for running the product gate.
