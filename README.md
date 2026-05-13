# Strata

Strata is a systems language for programs whose authority, effects,
concurrency, and state transitions should be visible in source code and checked
before execution.

Mantle is the runtime target. Strata source files are written as `.str`; the
Strata frontend checks those files and builds language-neutral Mantle Target
Artifacts as `.mta`; Mantle validates and executes the artifacts.

The design goal is not just to run code. The goal is to make runtime behavior
part of the checked interface of the program:

- which effects a function or transition may perform;
- which process can send which message;
- which state transitions are admitted;
- which values can carry process authority;
- which operations are deterministic;
- which runtime events prove what happened.

## Status

This repository is a buildable source-to-runtime implementation slice. A real
`.str` program can be checked, lowered into a `.mta` artifact, and executed by
Mantle with an observability trace.

It is not a complete language release. The accepted source surface is documented
in the mdBook, especially:

- [Language reference](docs/src/language-reference.md)
- [Syntax reference](docs/src/syntax-reference.md)
- [Source-to-runtime gates](docs/src/source-to-runtime-gates.md)
- [Artifact/runtime boundary](docs/src/artifact-runtime-boundary.md)

## Final Direction

The final Strata/Mantle shape is a statically checked language and runtime pair
for local and distributed systems where authority, effects, ownership, state
transitions, distribution, failure, and evidence are part of the checked program
contract.

Strata owns source syntax, type meaning, diagnostics, semantic checking, checked
IR, exact effects, determinism classes, capability types, process declarations,
protocol and component declarations, archive semantics, post-quantum
cryptographic obligations, policy-facing semantic artifacts, provenance, and
reproducibility rules. Lowering owns the explicit conversion from checked Strata
IR into Mantle target artifacts.

Mantle owns the operational contract for admitted artifacts: process identity
and scheduling, bounded mailboxes, isolated process state, supervision, effect
dispatch, runtime capability validation, artifact admission, archive validation,
repository/code-distribution behavior, host boundaries, transport, membership,
partition observation, revocation propagation, federation, and observability.
Mantle can choose an implementation strategy for schedulers, queues, transports,
allocators, repositories, and drivers, but it must not widen or weaken
Strata-visible meaning.

The intended language surface includes:

- explicit authority rather than ambient access to filesystems, networks,
  environment, arguments, standard I/O, clocks, randomness, cluster membership,
  remote spawn, or cross-cluster federation;
- ownership, borrowing, deterministic destruction, process isolation, and no
  shared mutable state across processes or nodes;
- typed local and remote process references, typed messages, typed init bundles,
  explicit send/spawn sites, bounded mailboxes, and supervisor-declared failure
  topology;
- component and protocol boundaries with declared authority, typed ports,
  explicit failure behavior, and checked component composition;
- exact effects, declared blocking/nondeterminism, deterministic artifact
  generation, and allocation treated as an operational effect unless proven
  fixed-frame, stack-local, static, or caller-provided;
- distribution as a typed semantic fact, not a transparent call boundary:
  transport reachability is never authority, remote failure is typed, partition
  behavior is observable, and cross-cluster operations require explicit
  admitted capability and policy;
- post-quantum cryptographic trust as a core semantic and admission
  requirement: ML-DSA signatures, ML-KEM or policy-admitted hybrid key
  establishment, crypto manifests, and active policy checks for signed
  artifacts, capability attestations, node identity, cluster membership,
  repository admission, artifact admission, and node-to-node sessions;
- classical-only cryptography excluded from the core trust path, with hybrid
  establishment treated as a policy-declared transition mechanism rather than a
  fallback to classical-only security;
- fail-closed operational behavior: accepted messages are not silently dropped,
  dequeued messages and completed effects are not automatically replayed, and
  runtime retries cannot hide commit-or-return outcomes;
- optional high-assurance profiles for information-flow control,
  constant-time checking, revocable future-use capability leases, upgrade, and
  federation, with profile-specific evidence and explicit non-claims;
- canonical typed artifacts, manifests, semantic hashes, archive/type hashes,
  provenance, attestation, publication bundles, and redaction policy that bind
  claims to reproducible source-to-runtime evidence.

Source names are for syntax, diagnostics, traces, provenance, and metadata.
Executable semantics cross the Strata/Mantle boundary as typed IDs and typed
artifact structures, not as source strings. Runtime dispatch uses admitted
typed tables and loaded runtime identities, not source text.

The end state is not proof artifacts in place of behavior. The closure bar is a
checked `.str` program lowered to a language-neutral `.mta` artifact, admitted
and executed by Mantle, with observability and evidence that explain both
accepted and rejected outcomes.

## Language Tour

### Source Units

A Strata file starts with a module declaration and defines records, enums,
source helpers, and processes:

```strata
module hello;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
```

`Main` is the entry process. Mantle starts it and delivers the first message
variant of its message enum.

### Explicit Effects

Effects are declared in function signatures:

```strata
fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det
```

The current buildable surface includes `emit`, `spawn`, and `send`. Undeclared
effects are rejected before artifact execution.

### Actors And Messages

Processes declare a state type, a message type, and message-keyed transitions:

```strata
enum WorkerState { Idle, Done }
enum WorkerMsg { Ping }

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Done);
    }
}
```

State changes are immutable whole-value transitions through `Continue(value)`,
`Stop(value)`, or `Panic(value)`.

### Typed Process References

Spawning returns a typed process reference. Sends use that typed reference, not a
string process name:

```strata
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
```

Process references can also travel as typed immutable message payloads when the
payload type admits them.

### Records, Enums, And Payloads

Records and enum payloads are immutable values:

```strata
enum Phase { Ready, Done }

record Job {
    phase: Phase,
}

enum WorkerMsg {
    Assign(Job),
}

enum WorkerState {
    Idle,
    Working(Job),
}
```

Payloads can be bound directly in step signatures or in `match msg` bodies:

```strata
fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(Working(job));
}
```

### Pattern Matching

Strata supports checked pattern dispatch in source helpers, step signatures,
whole-body message matches, state matches, and helper return-match expressions.

```strata
enum JobStatus {
    Assigned(Job),
}

fn status(Assigned(job: Job)) -> Phase ! [] ~ [] @det {
    return match job {
        Job { phase } => {
            return phase;
        }
    };
}
```

Patterns bind immutable local values. Nested patterns compose constructor
payloads, records, and collection element/value projections:

```strata
fn step(state: WorkerState, Envelope(Assign(Job { phase }))) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(Seen(phase));
}

fn step(state: WorkerState, Holding(List[Job { phase }, ..tail])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(Held(tail));
}
```

Helper matches may repeat a top-level constructor only when nested typed enum
predicates are provably disjoint. Invalid, duplicate, unreachable, and
overlapping patterns are rejected before lowering.

### Immutable Collections

Lists and maps are bounded immutable source values:

```strata
List<Phase,2>[Ready, Done]
Map<Phase,Phase,2>[Ready => Done, Done => Ready]
```

List rest patterns bind immutable suffix lists:

```strata
fn tail_of(List<Phase,2>[_, ..tail]) -> List<Phase,1> ! [] ~ [] @det {
    return tail;
}
```

Map subset/rest patterns bind over static keys:

```strata
fn selected(Map<Phase,Phase,2>[Ready => phase, ..rest]) -> Phase ! [] ~ [] @det {
    return phase;
}
```

Collection rest bindings are whole values, not mutable views. This slice does
not expose collection mutation, iteration APIs, dynamic-key map dispatch, or
source-visible in-place update. Nested collection patterns stay within typed
projection paths and static map-key semantics.

### Mantle Artifacts And Runtime Evidence

Lowering converts checked Strata IR into a typed `.mta` artifact. Mantle admits
the artifact, executes admitted transitions, and writes line-delimited runtime
evidence such as process spawn, message delivery, state update, output, stop,
and failure events.

## Try It

Run the smallest source-to-runtime gate:

```sh
cargo +stable run -p strata --bin strata -- check examples/hello.str
cargo +stable run -p strata --bin strata -- build examples/hello.str
cargo +stable run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
```

The program emits:

```text
hello from Strata
```

Mantle writes the trace to:

```text
target/strata/hello.observability.jsonl
```

More runnable examples are listed in [docs/src/examples.md](docs/src/examples.md).

## File Types

- `.str` files are Strata source files.
- `.mta` files are Mantle Target Artifacts.

Mantle artifacts identify their format, schema version, and source language
internally. The file extension is not the trust boundary.

## Repository Layout

```text
examples/                 runnable Strata examples
crates/strata/             Strata source checker, builder, and CLI
crates/mantle-artifact/    Mantle Target Artifact encode/decode/validation
crates/mantle-runtime/     local Mantle runtime and CLI
crates/strata-mantle-acceptance/
                          Strata/Mantle source-to-runtime acceptance tests
docs/                     mdBook documentation
tools/                    editor and MIME metadata
```

## Development

The detailed development workflow lives in the docs:

- [Getting started](docs/src/getting-started.md)
- [Development gates](docs/src/development-gates.md)

The main local verification bundle is:

```sh
just quality
```
