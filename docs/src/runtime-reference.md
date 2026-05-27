# Runtime Reference

This page documents Strata source forms that lower to typed Mantle runtime behavior. It continues the authoring reference from [Language Reference](language-reference.md).

## Runtime Branching

Step bodies can use a final-position runtime `if` whose condition is a checked
`Bool` value template, or a statement-level runtime `if` whose branches run
effects and then continue to the enclosing final return:

```strata
fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag == True) {
        emit "worker took warm branch";
        return Stop(WarmReady);
    } else {
        emit "worker took cold branch";
        return Stop(ColdReady);
    }
}
```

```strata
fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag != False) {
        emit "worker handled true";
    } else {
        emit "worker handled false";
    }
    return Continue(state);
}
```

Statement-level runtime branches also support guard/no-op shapes:

```strata
fn step(state: WorkerState, Check(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    if (flag == True) {
        emit "guard saw true";
    }
    if (!(flag == False)) {
        emit "guard enabled";
    } else {
    }
    if (flag == True) {
    } else {
        emit "guard saw false";
    }
    return Continue(state);
}
```

This is ordinary runtime branching. If the condition depends on the received
payload or current state payload, Strata lowers the checked condition, branch
actions, and any branch next states into the Mantle artifact. Mantle validates
the typed condition, validates both branches, executes only the selected branch,
and records `branch_selected` trace events with a stable artifact
`branch_path`. Branch effects must be declared by the step effect list.
Statement-level runtime branches may contain one direct nested statement-level
runtime branch action; deeper direct branch nesting is rejected at source
checking, artifact admission, and loaded-runtime admission. Runtime branch
statement prefixes cannot bind source-local values or process references.
Branches may contain bounded
runtime `for` statements, including effect-only loop prefixes before the
terminal return in final-position runtime branches. Each loop body remains the
same runtime-loop body surface. Final-position branch prefixes may also
contain one direct statement-level runtime `if` action; deeper direct
branch-action nesting remains rejected. A final-position runtime branch may end
in one direct nested final-position runtime `if`, which lowers as a bounded
nested `NextState::IfElse`; third-level terminal runtime branches remain
rejected at source checking, artifact admission, and loaded-runtime admission.
An omitted statement-level `else` lowers as an explicit empty branch. Empty
statement-level branches perform no effects, no state change, no authority
acquisition, and no hidden work; branch selection is still observable through
`branch_selected`. One branch may be empty when the sibling branch contains at
least one action. Both branches empty are rejected at source checking,
artifact admission, and loaded-runtime admission. Final-position runtime
branches must return the same step result from both branches. Statement-level
runtime branches cannot return; state changes still occur only through the
enclosing immutable whole-value `Continue`, `Stop`, or `Panic` return.

Scalar ordering comparisons, scalar equality, and checked scalar arithmetic are
supported over matching fixed-width integer types. Runtime-bound scalar
predicates and runtime-bound pure value conditionals lower as typed Mantle
templates. Mantle admission and runtime evaluation also validate checked scalar
arithmetic templates and typed value-if templates in admitted value-template
positions. Unbounded loops, imports, floats, string equality, and a standard
library remain outside the buildable language surface.

## Runtime Iteration

Step bodies can use a bounded runtime `for` loop over an immutable typed list
binding:

```strata
authority spawn_worker: Cap<Spawn<Worker>>;

fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn, send] ~ [] @det {
    let worker: ProcessRef<Worker> = spawn Worker;
    for item in items {
        send worker Branch(item);
    }
    return Stop(state);
}
```

This is ordinary runtime iteration. The collection source must be an identifier
binding that checks as `List<Element,N>` and remains runtime-bound, such as a
message payload binding. Strata lowers the checked collection template, typed
loop element ID, immutable element binding, maximum item count, and body actions
into the Mantle artifact. Mantle validates the loop structure and body,
executes the body once per runtime element in collection order, enforces the
collection length and runtime fuel limits, and records `loop_started`,
`loop_iteration`, and `loop_completed` trace events.

The loop element is immutable and may be used only as a typed value template in
the loop body. A loop item may also use a record pattern over the element type,
for example `for Job { phase: routed_phase } in jobs { ... }`. That binds an
immutable projected field value for the loop body; lowering emits a typed record
field projection from the active loop element, not an executable source binding
name. Loop bodies may use statement-level runtime `if` over the active loop
element, a projected loop-element field, or another checked `Bool` template,
including the same one-sided no-op branch shapes supported outside loops. Mantle
selects the branch during execution and records `branch_selected` inside the loop
trace with the active loop element ID and iteration index. Bounded loops may also
appear inside statement-level runtime branches, allowing a guard to select or
skip a whole loop. If the selected branch is empty, Mantle records only the outer
`branch_selected` event and emits no loop events. If the selected branch contains
the loop, trace order is `branch_selected`, `loop_started`, ordered
`loop_iteration` body effects, and `loop_completed`.

The loop body is intentionally narrow: no nested loops, no
`spawn`, no `return`, no assignment, and no process-reference element type. A
loop body may contain the statement-level runtime `if` / no-op shape,
including one direct nested statement-level runtime branch and including when
the loop is itself inside a selected guard branch. Deeper direct branch nesting
remains rejected. Send targets may be process references bound before the loop
or direct `ProcessRef<T>` payloads received by the current step.
Declare every body effect in the enclosing step effect list.

## Statements

The accepted statements are:

```strata
emit "text";
let worker: ProcessRef<Worker> = spawn Worker;
send worker Ping;
if (flag == True) { emit "true"; } else { emit "false"; }
if (flag == True) { emit "true"; }
if (flag == True) {} else { emit "false"; }
for item in items { send worker Branch(item); }
for item in items { if ((item != False) && !(item == False)) { send worker Branch(item); } }
for Job { phase: routed_phase } in jobs { send worker AssignPhase(routed_phase); }
return Stop(state);
return Continue(next_state);
return Panic(failed_state);
```

`emit` records and prints an output literal. Output literals must be non-empty,
must not contain control characters, and do not support string escapes.

`spawn` starts another declared process and returns an immutable typed process
reference. The reference binding is local to the transition and must be
typed as `ProcessRef<TargetProcess>`.

`send` queues a message through a process reference spawned in the same
transition or through a received `ProcessRef<T>` payload binding. The message
must be accepted by the reference target's process message enum. Static
validation rejects
self-spawn, spawning the already-started entry process, duplicate
process-reference binding in one transition, sends before the reference is
bound, mailbox overflow, and messages left unhandled after a target stops
normally.

Payload messages use the variant constructor at the send site:

```strata
send worker Assign(Job { phase: Ready });
```

The payload value is checked against the target message variant's payload type.
Unit message variants reject payload arguments, and payload variants require
one payload argument.

Process references can be payloads when the message variant declares a typed
reference:

```strata
enum WorkerMsg { Work(ProcessRef<Sink>) }
send worker Work(sink);
```

The received reference is immutable and can be used as a send target:

```strata
fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
    send reply_to Done;
    return Stop(state);
}
```

Runtime dispatch uses the transported runtime process ID and checked target
process ID. Source names remain diagnostics and trace metadata.

Received references can also be used as send targets inside
statement-level branches and bounded loop bodies. This does not make process
references general source values: they remain direct message payload authority,
and lowering emits typed received-payload send targets rather than source
binding names.

Current process-reference boundaries:

| Surface | Current status |
| --- | --- |
| `let worker: ProcessRef<Worker> = spawn Worker;` | Supported as an immutable transition-local binding when the process has exact `Cap<Spawn<Worker>>` authority. The checker resolves the target process before lowering. |
| `send worker Ping;` | Supported for a process reference spawned earlier in the same transition. Lowering emits a process-reference table ID. |
| `enum WorkerMsg { Work(ProcessRef<Sink>) }` | Supported only as a direct message payload type. |
| `send worker Work(sink);` and `send reply_to Done;` | Supported for direct process-reference payload forwarding. Mantle routes by checked target process ID and runtime process ID. |
| Multiple `ProcessRef<Worker>` bindings to one process definition | Supported. Each spawn creates a separate runtime process instance. |
| Process references directly in record fields or collection element/key/value types | Rejected. Process references are runtime authority, not general immutable data values. |
| Process references nested inside record, enum, list, map, or next-state payload templates | Rejected. A process reference must be the direct payload of a message that declares `ProcessRef<T>`. |
| Message enums that carry direct `ProcessRef<T>` payloads as pure source values | Rejected. They are message authority surfaces, not source-local computation values. |
| Process references in process state values or next-state templates | Rejected. Process states remain immutable source values without embedded runtime authority. |
| Sending by process definition names, source strings, registries, dynamic worker pools, supervisor child sets, stale-reference semantics, restart semantics | Future actor-topology semantics. They require separate authority, lifetime, failure, and observability rules. |

Patterns are source-level syntax for typed value decomposition. The current
runnable surface supports constructor patterns, constructor payload bindings,
nested constructor and record/list/map payload destructuring, function return-match
expressions, and wildcards. Normal source functions may match concrete enum values
or destructure concrete record/list/map values, `init` may use one whole-body
match over a fieldless enum constructor to select the initial state, and actor
message dispatch may use one whole-body match over the typed message parameter:

```strata
fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    match msg {
        First => {
            emit "worker matched First";
            return Continue(SawFirst);
        }
        Second => {
            emit "worker matched Second";
            return Stop(Done);
        }
    }
}
```

Step `match` is an authoring form for the same semantics as step parameter
patterns, including typed payload bindings and nested record/list/map payload
destructuring. Whole-body `match msg` arms and step parameter patterns may split
one top-level message constructor by exact nested typed payload predicates when
those predicates are provably disjoint over discovered concrete payload cases.
Checking resolves each arm into typed message-keyed transitions, typed payload
guards when needed, and typed projection templates before lowering. Mantle still
dispatches by typed message IDs, current state IDs when a transition is
state-specific, payload type IDs, exact typed payload identity, and loaded
template structure, not by source strings. In this buildable step subset the
match scrutinee must be the typed message parameter, and match arms are
block-delimited without comma separators.

A message-specific `step` may instead match the current state parameter when
the process state type is an enum:

```strata
fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    match state {
        Idle => {
            emit "worker had no job";
            return Stop(Idle);
        }
        Working(job: Job) => {
            emit "worker completed job";
            return Stop(Done(job));
        }
        Done(job: Job) => {
            emit "worker already done";
            return Stop(Done(job));
        }
    }
}
```

State-match arms resolve against the declared process state enum and are
exhaustive over its variants. Payload-bearing state variants may bind the whole
payload with the declared payload type or destructure a concrete record/list/map
payload. Each binding is immutable and local to that transition arm. Lowering
emits typed Mantle transitions keyed by typed message ID plus checked current
state ID, and runtime selection fails closed if the current state is not in the
loaded state table.

An `init` match is checked against the enum that owns the scrutinee constructor.
It must be exhaustive, duplicate-free, and statement-free; each arm returns an
immutable whole state value. Payload-bearing enum variants can be covered by
explicit patterns or `_`, but `init` arms cannot materialize payload bindings in
the returned state because the initial state lowers to one static state ID.

## Effects

The `! [...]` effect list is exact source-level effect usage for each `step`
clause. It must exactly match the clause actions. For a
`match msg`, `match state`, or `step return match` step, the one effect list
applies to every generated transition. Match-body arms must use exactly those
effects. A return-match uniform prefix lowers the same actions onto each
selected transition, and selected-arm prefixes must still leave every generated
transition with exactly the declared effects. Missing, duplicate, and unused
declared effects are rejected before lowering.

Dynamic local process creation also requires a process-local typed authority
declaration such as `authority spawn_worker: Cap<Spawn<Worker>>;`. The
`! [spawn]` effect proves that the transition uses spawning; it does not prove
which process may be spawned.

| Effect | Statement |
| --- | --- |
| `emit` | `emit "text";` |
| `spawn` | `let worker: ProcessRef<Worker> = spawn Worker;` |
| `send` | `send worker Ping;` or `send reply_to Done;` |

`init` cannot perform statements in the buildable surface and therefore uses an
empty effect list.

## Authority Inspection

Authority inspection is an optional tooling surface. It does not run during
normal check, build, or run commands, and it does not write a report file.

The source-side command checks Strata source and prints the checked process
authorities and spawn-site classifications before lowering:

```sh
just strata-authority-summary examples/actor_emit_spawn_send.str
just strata-authority-summary examples/actor_emit_spawn_send.str json
```

The artifact-side command admits a Mantle Target Artifact and prints the
artifact authority and spawn-site tables without executing the program:

```sh
just strata-build examples/actor_emit_spawn_send.str
just mantle-inspect-authority target/strata/actor_emit_spawn_send.mta
just mantle-inspect-authority target/strata/actor_emit_spawn_send.mta json
```

Text output is intended for local review. JSON output is deterministic and uses
typed process IDs, authority IDs, capability descriptors, and spawn-site IDs so
CI and audit tooling can compare the source-side and artifact-side authority
surfaces. Process and authority labels are included only as metadata.

## Step Patterns

A `step` parameter pattern handles one message constructor:

```strata
fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
    emit "hello from Strata";
    return Stop(state);
}
```

Payload constructors can bind the received payload in a `step` parameter pattern
or a whole-body `match msg` arm:

```strata
fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Stop(WorkerState { job: job });
}

fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
    match msg {
        Assign(job: Job) => {
            return Stop(WorkerState { job: job });
        }
    }
}
```

The binding is immutable and local to that transition. It can be used where a
value of the bound payload type is expected, including whole-value state
returns, record fields, downstream message payload sends, and send targets when
the payload type is `ProcessRef<T>`. Payload bindings cannot shadow the `state`
parameter, process declarations, type names, or value constructors.
Process-reference bindings in the same transition cannot shadow a payload
binding.

Record, list, and map payloads can also be destructured directly in step
parameter patterns, `match msg` arms, and `match state` arms:

```strata
fn step(state: WorkerState, Assign(Job { phase })) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Items(List[phase, ..tail])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Lookup(Map[Ready => phase, ..rest])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Envelope(Assign(Job { phase }))) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { seen: phase });
}

fn step(state: WorkerState, Holding(List[Job { phase }, ..tail])) -> ProcResult<WorkerState> ! [] ~ [] @det {
    return Continue(WorkerState { tail: tail });
}
```

These bindings are immutable projections of the concrete payload value. A
constructor payload, record field, list element, list rest, map value, or map
rest can be used in whole-value state returns and downstream payloads, but
process references still remain valid only as direct message payload bindings.
Fieldless nested enum constructors such as `Envelope(Assign(Ready))` are
accepted as typed shape predicates; they do not introduce bindings.
Shape-only collection payload patterns such as `Items(List[_])`,
`Lookup(Map[Ready => _])`, or `Lookup(Map[..])` are rejected;
use the constructor pattern without destructuring when the payload is ignored.
Multiple parameter-pattern or state-match `step` clauses may share one
top-level message constructor only when exact nested typed payload predicates
are provably disjoint. For state-match clauses, each accepted payload case
expands across the checked current-state cases from the `match state` body, and
lowering emits typed transitions keyed by message ID, current state ID, and exact
typed payload guard. A wildcard step pattern may cover discovered concrete
payload cases not matched by explicit payload-sensitive step-signature or
state-match clauses. The fallback lowers to exact typed payload-guarded
transitions for those discovered cases; it is not an open-ended runtime
catch-all for future payload values. State-match fallback cases additionally
expand across the checked current-state cases from the fallback `match state`
body before lowering.

If a process accepts more than one message, it can declare explicit clauses for
specific constructors and one wildcard clause for the remaining variants:

```strata
fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "worker handled First";
    return Continue(SawFirst);
}

fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "worker handled Second";
    return Stop(Done);
}
```

Every accepted message variant, or every discovered concrete payload case inside
a payload-sensitive split, must resolve to exactly one generated transition.
Explicit constructor clauses handle their named variants. One wildcard clause
may cover variants that do not have explicit clauses. When payload-sensitive
step-signature or state-match splitting is active for a variant, the same
wildcard may cover only discovered concrete payload cases not matched by
explicit payload predicates. Duplicate explicit clauses, overlapping payload
predicates, missing coverage, duplicate wildcard clauses, missing variant
coverage, and unreachable wildcard clauses are rejected.
Parameter patterns are compile-time dispatch only: Mantle dequeues one message
at a time and dispatches by typed message ID, current state ID when a transition
is state-specific, and exact typed payload identity when a payload guard exists.
Payload-bearing variants keep one stable typed message case, and their
immutable values travel in runtime message envelopes.

## State Transitions

`step` returns `ProcResult<StateType>`:

```strata
return Continue(next_state);
return Stop(final_state);
return Panic(failed_state);
```

`Continue(value)` replaces the process state and keeps the process running.
`Stop(value)` replaces the process state and terminates the process normally.
`Panic(value)` replaces the process state, marks the process failed, records
failure trace evidence, and fails the run without replaying the consumed
message.

Passing the `state` parameter preserves the supplied state:

```strata
return Stop(state);
```

Passing a record value, enum variant, list, or map creates an explicit
whole-value state replacement:

```strata
return Continue(WorkerState { phase: Idle });
return Continue(Working(Job { phase: Ready }));
return Continue(List<Phase,2>[Ready, Done]);
return Continue(Map<Phase,Phase,1>[Ready => Done]);
return Stop(Handled);
return Panic(Failed);
```

A `step` body may also use `return match` over a concrete enum source value
binding when the checker can reduce the match to one typed transition before
lowering. Statements before the `return match` are a uniform action prefix; the
checker lowers that same typed action list onto every generated transition.
The selected arm may then perform its own typed action-block prefix,
including `emit`, direct in-scope `send`, statement-level runtime `if`, and
bounded runtime `for` actions, before the terminal whole-value result:

```strata
fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
    emit "return-match prefix";
    return match phase {
        Ready => {
            emit "ready arm prefix";
            return Continue(SawReady);
        }
        Done => {
            emit "done arm prefix";
            return Stop(Done);
        }
    };
}
```

This form is not runtime dispatch. The checker requires an immutable enum source
binding with a concrete value proven during step clause or state-match expansion,
checks every arm as a `ProcResult<StateType>`, selects the matching arm, and
lowers the selected arm to the existing typed transition shape. Uniform prefix
effects occur before the selected arm prefix in source program order and remain
committed runtime actions. Selected arm prefixes use the same
statement-level action sequencing as a `step` body: local `emit`, in-scope direct
`send`, statement-level runtime `if`, and bounded runtime `for` over checked
runtime `List<T,N>` bindings. The checker still validates every arm template
fail-closed before selected actions lower as typed Mantle artifact actions.
Arm-local `spawn`, process-reference binding, nested runtime `for` statements,
final-position runtime `if`, nested return matches, matching `state`, matching
non-enum values, and dynamic payload catch-all dispatch are rejected.

## Typed Effect Outcomes

Local send and spawn operations can bind typed immutable outcomes in `step`
bodies:

```strata
let spawn_result: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
let send_result: Result<Unit,SendError<WorkerMsg>> = send worker Work;
return Stop(MainState { sent: send_result });
```

These bindings lower to typed Mantle effect-outcome IDs and typed value
templates. Source binding names are not runtime dispatch keys. When an outcome
template is used as a next-state value, the checker admits the finite outcome
states required by that transition before artifact execution. Spawn outcome
successes carry process references, so they are kept as step-local authority
values and are not admitted as ordinary source state.

The source checker enforces declaration order for outcome bindings. An outcome
is visible only after its binding statement, and outcome bindings must precede
ordinary effect statements. Process-reference spawns in that prefix are executed
before state resolution so outcome sends can target them. Process-reference
spawns after ordinary effects execute in ordinary source order and cannot be
used by later outcome bindings.

For local send outcomes, Mantle checks the target process, status, and mailbox
capacity before accepting the message. Accepted sends are committed and return
`Ok(Unit)`. Pre-acceptance local failures return `Err(Full(message))`,
`Err(Stopped(message))`, `Err(Crashed(message))`, or
`Err(MailboxClosed(message))` with the original message value preserved. The
current local runtime can produce full, stopped, and already-failed crashed
outcomes; it does not yet expose a distinct source-created closed-mailbox
lifecycle. If the message carries a direct `ProcessRef<T>` payload, the failure
outcome preserves the runtime process-reference metadata with the message value;
it does not turn that authority into storable source state.

For local spawn outcomes, accepted spawns commit the new process and return
`Ok(process_ref)` with a typed `ProcessRef<TargetProcess>` payload. If process
authority is denied by the admitted runtime policy before acceptance, Mantle
returns `Err(Denied(Unit))`. If process capacity is exhausted before the spawn is
accepted, Mantle returns `Err(Exhausted(Unit))`. Bare statement `send` and
`spawn` still fail closed: pre-acceptance failure is reported as a runtime error
instead of being silently dropped.

Outcome values can also drive follow-up effects through ordinary immutable
branching:

```strata
if (spawn_result != Err(Exhausted(Unit))) {
    emit "spawn accepted";
} else {
    emit "spawn rejected";
}

if (send_result == Ok(Unit)) {
    emit "send accepted";
} else {
    emit "send not accepted";
}
```

These conditions branch on a typed built-in variant pattern. They do not add
structural equality for preserved message payloads, and they do not compare
process-reference identities from `Ok(process_ref)` spawn outcomes.

`examples/effect_outcome_mailbox_full.str` and
`examples/effect_outcome_stopped_target.str` exercise source-to-runtime
pre-acceptance send failure outcomes. `examples/effect_outcome_spawn_denied.str`
checks and builds an authorized local spawn site, then runs Mantle with denied
spawn admission to observe `Err(Denied(Unit))` before process acceptance. Direct
Mantle runtime tests exercise `Crashed(M)` for targets already failed before
acceptance, and admission tests cover the required `MailboxClosed(M)` send-error
shape. A source-created `Panic(...)` currently records failure evidence and
fails the run before a later source sender can observe that crashed target.

Current pattern-matching closure boundaries:

| Surface | Current status |
| --- | --- |
| `step return match` after uniform pre-return effects | Supported. Uniform prefix actions lower identically onto each selected typed transition. |
| `step return match` arm-local `emit` / in-scope direct `send` prefixes | Supported only after source-time concrete arm selection; selected arm actions lower as typed transition actions before the terminal result. |
| `step return match` arm-local statement-level runtime `if` prefix | Supported after source-time concrete arm selection. Branches use typed action templates and obey the same statement-level runtime branch nesting bound as ordinary `step` actions. |
| `step return match` arm-local bounded runtime `for` prefix | Supported over checked runtime `List<T,N>` bindings. Loop bodies use typed action templates, may include statement-level runtime branches, and still reject nested runtime `for`. |
| `step return match` arm-local bounded runtime `for` with a loop-body runtime `if` | Supported under the ordinary statement-level branch nesting bound. Branch conditions and branch actions lower through typed loop-element templates inside the selected arm. |
| `step return match` arm-local statement-level runtime `if` containing bounded runtime `for` | Supported for branch-local loops over checked runtime `List<T,N>` bindings. Branch-local loops stay selected inside the concrete arm and lower as typed branch actions, not source-label dispatch. |
| `step return match state` | Rejected. Use whole-body `match state`, then match a concrete state-payload binding when one is proven. |
| `init match` over payload-bearing arm constructors | Supported only when every arm returns a state value that does not materialize payload bindings. |
| `init return match` over payload-bearing constructors | Rejected. `init return match` selects a static initial state from a fieldless enum scrutinee. |
| Init arms materializing payload bindings in the initial state | Rejected. The initial state lowers to one static typed state ID. |
| Shape-only collection predicates such as `Items(List[_])`, `Lookup(Map[Ready => _])`, or `Lookup(Map[..])` | Rejected. Bind at least one immutable projected value, or match only the enclosing constructor when the payload is ignored. |
| Dynamic-key map matching | Rejected. Map pattern keys are static source values. |
| Arbitrary/general match expressions outside supported function, `init`, `step`, `match msg`, and `match state` forms | Future language semantics, not part of the current buildable surface. |

State changes are immutable whole-value transitions. There is no assignment
statement and no source-visible field mutation.

Payload-bearing process states can be observed only through checked immutable
state patterns such as `Working(job: Job)`. Returning `Done(job)` creates a new
whole state value; it does not rewrite the payload inside the existing state.

## Limits

The buildable language surface enforces bounded sizes:

| Limit | Value |
| --- | --- |
| Source bytes | 1 MiB |
| Identifier bytes | 128 |
| Distinct checked artifact types | 4096 |
| Output literal bytes | 16 KiB |
| Processes | 256 |
| State values per process | 1024 |
| Message variants per process | 1024 |
| Static process-reference bindings per process definition | 4096 |
| Distinct output literals | 4096 |
| Actions per process | 4096 |
| Mailbox bound | 65,536 |
| Type nesting | 32 |
| Value nesting | 32 |

These limits are part of the artifact and runtime boundary.
