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
- which state transitions are valid;
- which values can carry process authority;
- which operations are deterministic;
- which runtime events prove what happened.

## Status

This repository contains a buildable source-to-runtime implementation. A real
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

Mantle owns the operational contract for validated artifacts: process identity
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
  capability and policy authorization;
- post-quantum cryptographic trust as a core semantic and admission
  requirement: ML-DSA signatures, ML-KEM or policy-approved hybrid key
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
artifact structures, not as source strings. Runtime dispatch uses loaded typed
tables and runtime identities, not source text.

The end state is not proof artifacts in place of behavior. The closure bar is a
checked `.str` program lowered to a language-neutral `.mta` artifact, validated
and executed by Mantle, with observability and evidence that explain both
accepted and rejected outcomes.

## Language Tour

### Source Units

A Strata source program starts from a root `.str` file. Each source unit starts
with a module declaration, may import sibling source units with
`import module_name;`, and defines protocols, ports, components, records,
enums, source functions, and processes:

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

Imports are resolved by Strata before lowering. The dependency graph must be
acyclic and deterministic, each source unit may use only its own declarations
and direct imports, unqualified callable names must remain deterministic across
reachable units, and Mantle receives only the lowered `.mta` artifact with typed
IDs. It does not resolve Strata imports at runtime.

### Protocol And Port Boundaries

Typed communication boundaries are declared before process execution:

```strata
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;
```

`send worker via WorkerPort Work;` also requires the sending process to declare
`authority name: Cap<PortConnect<WorkerPort>>;`. Checking proves that
`WorkerPort` targets `Worker` and uses the `WorkerMsg` protocol message enum.
Lowering emits Mantle boundary table IDs and typed required-authority
descriptors; Mantle validates those tables and traces accepted boundary sends
without resolving protocol, port, component, or import names at runtime.

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

Dynamic local spawning requires a process-local typed spawn capability. The
effect list says the step uses `spawn`; it is not the authority proof for which
process may be created. Unused authority descriptors are rejected as overbroad:

```strata
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }
}
```

Spawning returns a typed process reference. Sends use that typed reference, not a
string process name.

Process references can also travel as typed immutable message payloads when the
message declares a direct `ProcessRef<T>` payload.

### Local Supervision

Static local children are declared with lexical supervisor authority, not with
dynamic `Cap<Spawn<Target>>` authority:

```strata
supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
    child worker: Worker = spawn Worker as permanent;
}
```

Mantle starts supervised children in declaration order, restarts eligible
children with new runtime process IDs, reruns `init`, and does not replay
accepted messages or effects after a crash. The current surface supports local
`one_for_one` with `permanent`, `transient`, and `temporary` children. If a
restart is denied by intensity, capacity, or the default same-tick throttle,
Mantle fails the supervisor scope instead of silently retrying.

### Typed Effect Outcomes

Local send and spawn effects can bind immutable `Result` values in `step`
bodies:

```strata
let spawn_result: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
let send_result: Result<Unit,SendError<WorkerMsg>> = send worker Work;
```

Send outcomes commit accepted messages as `Ok(Unit)` and return typed
pre-acceptance failures such as `Full(message)`, `Stopped(message)`,
`Crashed(message)`, or `MailboxClosed(message)` while preserving the original
message value. `Stopped` is produced for a receiver whose normal stop remains
observable; `MailboxClosed` is reserved for explicit mailbox closure,
supervisor-driven shutdown, policy closure, or indistinguishable closed-state
rejection. Local spawn outcomes return
`Result<ProcessRef<Target>,SpawnError<Unit>>`: accepted spawns commit the new
process and return its typed process reference. If Mantle denies admitted spawn
authority before acceptance, the outcome is `Err(Denied(Unit))`; process
capacity exhaustion returns `Err(Exhausted(Unit))`; an unavailable local spawn
backend returns `Err(BackendUnavailable(Unit))` before a child is admitted. Outcome values are immutable
and step-local; process references remain authority values and are not storable
source state.

### Optional Authority Inspection

`strata authority-summary` checks source and prints process-local spawn and port
authorities, required `Cap<Spawn<Target>>` / `Cap<PortConnect<Port>>`
descriptors, spawn-site IDs, and lexical supervisor child plans without
building an artifact. `mantle inspect-authority` reads an admitted `.mta` and
prints the loaded authority, spawn-site, and supervisor tables without executing
the program. Both commands default to text and support `--format json` for CI,
audit, and review tooling.
They are optional inspection commands, not reports generated by normal build or
run.
`strata composition-report` is the matching Strata-owned inspection surface for
checked component compositions. It emits the diagnostic FNV-1a source fingerprint,
typed component-instance IDs, typed port-binding IDs, admitted binding results,
empty unsatisfied imports for admitted compositions, endpoint port authority
requirements, component export authority surfaces, and cross-component authority
edges. Mantle does not read this report; it executes only admitted `.mta`
artifacts.

`strata composition build` is the durable artifact surface for the same checked
composition graph. By default it writes
`target/strata/<stem>.component-composition.json` with the checked-subset
`schema_id: strata.checked_component_composition`, schema version `1.0`,
`hash_alg: fnv1a64-diagnostic`, canonical 16-character lowercase hexadecimal
source fingerprint provenance, component instances with their import/export port
obligations, implemented port bindings, empty fail-closed arrays for
not-yet-expressible binding classes, unsatisfied imports, cross-component
authority-flow edges, nullable policy/diagnostic hash slots, and a global
admission result. `strata composition admit` validates that checked subset
fail-closed: every declared component import must be either bound exactly once or
listed once as unsatisfied in a rejected artifact, and the diagnostic source
fingerprint must match the declared fingerprint algorithm's canonical shape.
This JSON artifact is Strata-owned checked-subset validation evidence, not the canonical
`strata.component_composition` deployment artifact and not `.mta`; Mantle does
not execute it in this slice. Source names in the artifact are metadata only;
typed component-instance, port-binding, port, protocol, and authority descriptor
IDs carry admission meaning.

In this checkout, use source-side recipes such as
`just strata-authority-summary <path.str>`,
`just strata-composition-report <path.str>`,
`just strata-composition-build <path.str>`, and
`just strata-composition-admit <path.json>` for reports and validation artifacts.
Use `just mantle-inspect-authority <path.mta>` to run artifact-side inspection
through the pinned toolchain recipe.

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

### Pure Source Computation

Pure source functions can name sequential immutable intermediate values before
their terminal return:

```strata
enum Bool { False, True }
enum Phase { Idle, Active }
record Work { phase: Phase }

fn status(Work { phase }) -> Phase ! [] ~ [] @det {
    return phase;
}

fn route(work: Work) -> Phase ! [] ~ [] @det {
    let current: Phase = status(work);
    let selected: Phase = if (current == Active) { Active } else { Idle };
    return selected;
}
```

Source-local bindings are source-time names. The checker resolves them before
lowering, so Mantle receives typed values, typed templates, and typed IDs rather
than source-local variable names. Types that carry `ProcessRef<T>` authority are
not source-local computation values.

### Scalar Values

The buildable source surface includes fixed-width integer value types:
`U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, and `I64`. Numeric value
literals require explicit suffixes, such as `10_u32` or `-1_i8`.

```strata
fn high_priority(weight: U32) -> Bool ! [] ~ [] @det {
    let adjusted: U32 = weight + 2_u32;
    return adjusted >= 10_u32;
}
```

Scalar arithmetic uses matching integer types only. Overflow, underflow,
division by zero, modulo by zero, signed/unsigned mixing, and cross-width
mixing are rejected. Runtime-bound scalar predicates and value-level scalar
conditionals lower as typed Mantle templates; source binding names and function
names are not runtime dispatch keys.

### Pattern Matching

Strata supports checked pattern dispatch in source functions, step signatures,
whole-body message matches, state matches, and function return-match expressions.

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

Function matches may repeat a top-level constructor only when nested typed enum
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

Collection rest bindings are whole values, not mutable views. The buildable
language surface does not expose collection mutation, source-level collection
iteration APIs, dynamic-key map dispatch, or source-visible in-place update.
Nested collection patterns stay within typed projection paths and static
map-key semantics.

### Mantle Artifacts And Runtime Evidence

Lowering converts checked Strata IR into a typed `.mta` artifact. Mantle
validates the artifact, executes typed transitions, and writes line-delimited
runtime evidence such as process spawn, message delivery, state update, output,
stop, and failure events.

## Try It

Run the smallest source-to-runtime gate:

```sh
just run-example hello
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
