# Boundary Contracts

Boundary declarations give source-visible communication an explicit typed
contract before it reaches Mantle.

```strata
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;
```

`WorkerMsg` must be a named enum. `WorkerPort` must target a declared process
whose `type Msg` is `WorkerMsg`. `WorkerComponent` exports that port. The
required capability descriptors must point back to the declaration being defined;
mismatched descriptors are rejected.

Components can also declare imported ports and a local composition can bind
component instances through typed port edges:

```strata
component MainComponent exports MainPort imports WorkerPort requires Cap<ComponentExport<MainComponent>>;

composition AppComposition {
    instance main component MainComponent;
    instance worker component WorkerComponent;
    bind main imports WorkerPort -> worker exports WorkerPort;
}
```

The checker builds typed component-instance IDs and typed port-binding edges
before lowering. A composition is admitted only when every component import is
bound exactly once, both instances are declared, the importing component actually
imports the port, the exporting component actually exports the port, and the two
ports share one protocol. Direct source-unit imports still control which
components and ports are visible; transitive-only component or port access is
rejected.

A typed boundary send uses the optional `via` clause:

```strata
authority connect_worker: Cap<PortConnect<WorkerPort>>;

fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
    let worker: ProcessRef<Worker> = spawn Worker;
    send worker via WorkerPort Work;
    return Stop(state);
}
```

Checking proves all of these facts before lowering:

- the send target is a typed `ProcessRef<Worker>`;
- `WorkerPort` targets `Worker`;
- `WorkerProtocol` uses `WorkerMsg`;
- `Work` is a variant of `WorkerMsg`;
- the sending process declares and uses exact
  `Cap<PortConnect<WorkerPort>>` authority.

After checking, boundary references are typed IDs. Lowering emits Mantle
protocol, port, component, composition, authority, and action tables with those
IDs. Mantle admits the tables before `ArtifactLoaded` and runtime dispatch uses
admitted process, message, and port IDs. Protocol, port, component, component
instance, process, and message names are metadata for diagnostics and traces
only.

Invalid boundary shapes fail closed at the earliest layer that can see them. A
source program with an undeclared port, mismatched protocol message type, missing
port authority, duplicate boundary name, ambiguous direct import, unbound
component import, duplicate component import binding, unimported-port binding,
protocol-mismatched or port-authority-mismatched composition edge, or reserved
descriptor type name fails
checking. A hand-authored artifact with mismatched boundary table IDs,
unimported-port composition binding, missing port authority, malformed
composition edge, port-authority mismatch, or unbound component import fails
admission before runtime
events begin.

Typed boundary sends emit `boundary_send_checked` runtime trace events before
mailbox acceptance. Accepted sends continue to message acceptance; admitted
authority-policy denials record `boundary_result=denied` and stop before target
message side effects. Invalid boundary shapes are still admission diagnostics,
not runtime trace events, because they do not enter runtime dispatch or produce
`ArtifactLoaded`.

Run the source-to-runtime example with:

```sh
just run-example boundary_contracts_main
just run-example component_composition_main
```

Current implementation limits: the source `composition` form is a local typed
graph admission input for component instances and port bindings. `strata
composition build` checks that graph and emits the Strata-owned checked-subset
`strata.checked_component_composition` validation artifact; it is not `.mta`, is
not Mantle runtime input, and it does not add remote send, distributed
transport, capability binding syntax, deployment manifests, package resolution,
hot upgrade, or generated port stubs. Runtime composition correlation requires
the separate explicit `strata composition bind-runtime` bridge, which validates
admitted checked composition evidence against a matching `.mta` and emits
`mantle.runtime_composition_binding` for `mantle run --composition-binding`.
Binding classes outside the current source subset are emitted as empty arrays
and fail closed if forged non-empty.
Use `just strata-composition-report examples/component_composition_main.str json`
for review evidence, and `just strata-build examples/component_composition_main.str`
plus `just strata-composition-build examples/component_composition_main.str` plus
`just strata-composition-admit
target/strata/component_composition_main.component-composition.json json` plus `just
strata-composition-bind-runtime
target/strata/component_composition_main.component-composition.json
target/strata/component_composition_main.mta
target/strata/component_composition_main.deployment-composition.json` when CI
needs fail-closed Strata-owned checked-subset validation evidence and the
explicit runtime-correlation binding. Use `just strata-target-requirements
examples/component_composition_main.str json` to inspect the typed Mantle runtime
features Strata requires for the checked program. `strata authority-summary ...
--format json` also reports the checked
component-boundary authority edge summary for the same checked IR. Mantle admits
the lowered `.mta` by comparing embedded typed requirements against `mantle
feature-declaration`; it does not consume source-side reports or composition
validation artifacts.
