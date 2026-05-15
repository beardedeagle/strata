# Implementation Architecture

This page maps the source-to-runtime implementation for contributors. It is not
required for writing simple Strata programs.

## Crate Layout

| Path | Responsibility |
| --- | --- |
| `crates/strata` | Strata CLI, parser, AST, checker, checked IR, and lowering. |
| `crates/mantle-artifact` | Mantle Target Artifact encode, decode, validation, limits, and typed artifact IDs. |
| `crates/mantle-runtime` | Mantle admission, runtime process state, mailboxes, dispatch, output, and traces. |
| `crates/strata-mantle-acceptance` | Workspace-owned `.str` to `.mta` to Mantle execution acceptance gates and boundary ownership checks. |
| `examples` | Runnable Strata programs used by source-to-runtime gates. |
| `fuzz` | Fuzz targets for parser/checker/lowering, artifact decode, and runtime admission paths. |
| `tools` | Editor and MIME metadata. |

## Source Path

```text
source text
  -> lexer
  -> parser
  -> AST
  -> semantic checker
  -> checked IR
  -> lowering
  -> Mantle Target Artifact
```

The parser accepts source shape. The checker assigns source-visible meaning,
resolves names, validates process/message/state rules, and produces checked IR.
It resolves process reference names and source type references into checked IDs,
including checked type identities for value and process-reference payloads.
Lowering converts checked IR into Mantle artifact tables by mapping checked
process, message, state, output, and type IDs to artifact IDs.

## Runtime Path

```text
artifact text
  -> decode
  -> validate
  -> load typed runtime tables
  -> spawn Main
  -> deliver entry message
  -> dispatch by message ID
  -> execute actions by typed process references and IDs
  -> write JSONL trace
```

Mantle must validate artifacts before execution. Runtime dispatch uses loaded
typed IDs, not source strings. Payload and state type identity is admitted
through Mantle type-table IDs; source type labels are metadata only.

## Important Boundaries

Strata owns:

- source syntax;
- diagnostics;
- AST;
- semantic checking;
- checked IR;
- source-visible meaning.

Lowering owns:

- conversion from checked Strata IR into Mantle artifact records;
- mapping checked process, message, state, type, and output IDs to artifact IDs.

Mantle owns:

- artifact decoding and validation;
- admitted runtime tables;
- process instances and mailboxes;
- action execution;
- runtime traces.

Workspace source-to-runtime acceptance gates own the cross-boundary proof that a
Strata program can be checked, lowered, admitted by Mantle, executed, and
traced.

Do not move source-only assumptions into Mantle as trusted runtime behavior.
Do not make Mantle dispatch through labels that exist only for diagnostics or
trace readability.
Do not move Strata source constants, default output paths, examples, or
source-to-runtime gates into Mantle-owned crates.

## Adding A Language Feature

A source-facing feature usually needs changes across several layers:

- parser;
- AST;
- checker;
- checked IR;
- lowering;
- artifact schema or validation, when runtime representation changes;
- runtime execution, when behavior changes;
- diagnostics;
- examples;
- docs;
- positive tests;
- negative tests;
- source-to-runtime gates, when user-visible execution changes.

Parser acceptance alone is not enough. If another layer can construct or admit
the same invalid state, that layer needs its own validation.

## Existing Source-To-Runtime Gates

The runnable examples are:

- `examples/hello.str`;
- `examples/actor_ping.str`;
- `examples/actor_sequence.str`;
- `examples/actor_match.str`;
- `examples/init_match.str`;
- `examples/init_return_match.str`;
- `examples/function_match.str`;
- `examples/function_payload_match.str`;
- `examples/function_collection_match.str`;
- `examples/function_return_match.str`;
- `examples/process_return_match.str`;
- `examples/function_record_pattern.str`;
- `examples/function_record_return_match.str`;
- `examples/function_record_body_match.str`;
- `examples/state_payload_enum.str`;
- `examples/collection_state.str`;
- `examples/state_payload_match.str`;
- `examples/actor_instances.str`;
- `examples/actor_payloads.str`;
- `examples/actor_payload_match.str`;
- `examples/actor_payload_split_match.str`;
- `examples/actor_payload_split_signature.str`;
- `examples/actor_payload_split_signature_wildcard.str`;
- `examples/actor_payload_state_match_split.str`;
- `examples/actor_payload_state_match_wildcard.str`;
- `examples/actor_reply.str`;
- `examples/actor_emit_spawn_send.str`;
- `examples/actor_panic_no_replay.str`.

The integration tests in
`crates/strata-mantle-acceptance/tests/source_to_runtime_gates.rs` mirror the
same source check, artifact build, and runtime execution sequence:

```sh
cargo run -p strata --bin strata -- check examples/hello.str
cargo run -p strata --bin strata -- build examples/hello.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
```

The same pattern applies to the actor examples. Fail-closed examples check and
build, but their runtime gate expects `mantle run` to return non-zero after
emitting trace evidence.

## Closure Rule

A change that affects source syntax, artifact schema, runtime behavior,
diagnostics, examples, or gates should update this book and pass:

```sh
just quality
```

Docs explain the contract; runnable gates prove the behavior.
