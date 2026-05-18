# File Types

This repository uses two first-class file extensions:

- `.str` for Strata source.
- `.mta` for Mantle Target Artifacts.

## `.str`

`.str` files are Strata source files. They are the user-authored program
surface and should be UTF-8 text with LF line endings.

Expected MIME type:

```text
text/x-strata
```

## `.mta`

`.mta` files are Mantle Target Artifacts. They are executable inputs to Mantle,
not Strata source and not proof or evidence artifacts.

The extension is intentionally language-neutral. Strata can emit `.mta`; other
frontends can emit `.mta` too. Mantle must decide whether an artifact is
admissible from its internal header and validation data, not from the filename.

Minimum artifact identity fields:

```text
format=mantle-target-artifact
schema_version=1
source_language=strata
```

The schema version identifies the admitted `.mta` encoding shape. It is not a
Strata language release, a migration counter, or a stability guarantee. In the
current greenfield implementation, `1` is the single admitted artifact schema
baseline. Unsupported schema versions are rejected; artifact producers must emit
the admitted schema.

Executable references, type identity, and state transitions inside `.mta` use
validated table IDs and typed transition forms. Process transition records are
encoded by transition index and carry a `message` ID field, an optional
`current_state` ID guard, an optional exact typed payload guard, and exact
effect authority for their actions.
Validation requires each message to have either a transition base with no
current-state guard or one transition base for every admitted current-state ID.
Within each message/current-state base, validation admits either one
payload-unguarded transition or a payload-specific set where every transition
carries an exact typed payload guard. Payload-specific sets enumerate admitted
concrete payload cases; they do not encode source-payload algebra. Runtime
selection indexes the admitted transition table by typed message ID, typed
current state ID when a state guard is present, and exact typed payload identity
when a payload guard is present.

Artifact type identity is carried by a Mantle type table. Process
`state_type_id`, `message_type_id`, message `payload_type_id`, payload template
`type_id`, state value `type_id`, and received-payload send-target
`target_payload_type_id` fields refer to that table by numeric `TypeId`. Type
labels are metadata only; Mantle does not parse source type strings such as
process-reference spellings to decide runtime behavior.

State value tables carry a type ID, typed value identity, ordered value label,
and optional typed payload metadata for each admitted state. The label must match
the rendering of the typed value, preserving record field and map entry order
from the admitted value. Mantle admission and runtime next-state resolution use
the type ID and value identity. State-match payload templates use the admitted
current state's typed payload metadata. Preserved order is visible in labels and
traces, but runtime projection is key-based and dispatch uses admitted typed IDs.
Labels remain trace and diagnostic metadata, not runtime dispatch keys.

Process references are encoded as per-process reference tables. A spawn action
binds a process-reference ID to a runtime process instance for the transition.
A send action targets either a process-reference ID or a received
typed process-reference payload plus a message ID. Reference debug names remain
metadata; runtime delivery uses admitted IDs and runtime process instance IDs.

Message variants may carry an optional payload type ID. Send actions may carry
an immutable payload value template, and Mantle delivers the evaluated value in
a runtime message envelope. Process-reference payloads carry the admitted target
process ID and runtime process ID. Message dispatch uses admitted typed IDs and,
for payload-specific transitions, admitted typed payload identity, not payload
text, source strings, or debug labels.

Each transition's `action_count` is bounded during decode before allocation.
Validation also caps the aggregate action count across all transitions for a
process as an admitted process resource budget.

Names are retained for diagnostics, traces, and metadata. Mantle execution must
load and run resolved IDs rather than dispatching by source text.

Generated `.mta` files should normally remain under `target/`. Checked-in
`.mta` files are allowed only as explicitly labeled fixtures or specimens and
must not be used as a substitute for a successful `strata build` and
`mantle run`.

Expected MIME type:

```text
application/vnd.mantle.target-artifact
```
