# Artifact And Runtime Boundary

Strata owns source syntax, diagnostics, semantic checking, checked IR, and
source-visible meaning. Lowering owns conversion from checked Strata IR into
Mantle Target Artifacts. Mantle owns artifact admission, runtime execution,
process and mailbox state, host boundaries, and observability.

This separation keeps names, metadata, and runtime identity from collapsing into
one surface. Source names are useful for diagnostics and traces, but executable
runtime dispatch must use loaded typed IDs.

Mantle crates are structurally language-neutral. They may carry `source_language`
as opaque artifact metadata, but they must not own Strata source constants,
Strata output-directory defaults, `.str` examples, or source-to-runtime gates.
Strata-owned defaults live in `crates/strata`; cross-boundary gates live in
`crates/strata-mantle-acceptance`.

Artifact type identity is structural in Mantle. Lowering emits a Mantle type
table and artifact records refer to entries by `TypeId`; Mantle admission and
runtime execution do not parse source type spellings or `ProcessRef<...>` text
to decide behavior. Type labels remain diagnostics and trace metadata only.

## Admission

Mantle admits artifacts through validation, not filename trust. Before
execution, the artifact decoder and validator check:

- artifact magic, format, schema version, and source language;
- bounded process, message, state, output, transition, and action counts;
- bounded type table entries and type-kind targets;
- unique process debug names;
- unique typed state value identities per process;
- unique process reference names per process;
- either one unguarded transition per accepted message or one state-specific
  transition for each admitted state value;
- exact transition effect authority for emitted, spawned, and sent actions;
- transition references to known messages, state values, type IDs, process
  references, outputs, and process IDs.

Decode-time bounds must happen before allocation when counts come from the
artifact body.

## Execution

Mantle loads admitted transitions into indexed runtime tables. Before emitting
`ArtifactLoaded` or executing runtime side effects, Mantle validates loaded
entry metadata, state tables, transition state targets and templates, outputs,
process references, sends, payload templates, and transition effect authority.
A dequeued message selects the transition by typed message ID, and by admitted
current state ID when the transition table is state-specific. Dynamic next-state
templates resolve to an admitted state ID by typed state value identity, not by
display label text.

Transition effect metadata is admitted with the artifact, loaded as runtime
effect authority, and must exactly match the action effects that execute.
Runtime `if` conditions are admitted as typed `Bool` value templates. Mantle
validates both branch bodies before execution, executes only the selected
branch, keeps branch-local process references unavailable after the branch
unless both branches bind them, and records branch selection in the runtime
trace.

The action set covers:

- emitting declared output;
- spawning a declared process through a process reference;
- sending a declared message through a bound process reference.
- selecting a typed runtime branch over admitted action blocks.

The runtime fails closed on invalid sends, unbound process references, duplicate
process-reference bindings, mailbox exhaustion, runtime process instance budget
exhaustion, dispatch budget exhaustion, emitted-output budget exhaustion, and
trace budget exhaustion.

## Observability

Runtime traces are line-delimited JSON. They include labels for readability and
numeric IDs for process, message, state, payload type, and output identity. A
trace is evidence of runtime execution, not a substitute for running the
source-to-runtime gate.
