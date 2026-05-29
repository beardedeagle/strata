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
protocol, port, component, authority, and action tables with those IDs. Mantle
admits the tables before `ArtifactLoaded` and runtime dispatch uses admitted
process, message, and port IDs. Protocol, port, component, process, and message
names are metadata for diagnostics and traces only.

Invalid boundary shapes fail closed at the earliest layer that can see them. A
source program with an undeclared port, mismatched protocol message type, missing
port authority, duplicate boundary name, ambiguous direct import, or reserved
descriptor type name fails checking. A hand-authored artifact with mismatched
boundary table IDs or missing port authority fails admission before runtime
events begin.

Accepted typed boundary sends emit `boundary_send_checked` runtime trace events.
Denied boundary shapes are admission diagnostics, not runtime trace events,
because they do not enter runtime dispatch or produce `ArtifactLoaded`.

Run the source-to-runtime example with:

```sh
just run-example boundary_contracts_main
```
