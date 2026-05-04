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

The extension is intentionally language-neutral. Strata can emit `.mta`; a
future frontend can emit `.mta` too. Mantle must decide whether an artifact is
admissible from its internal header and validation data, not from the filename.

Minimum artifact identity fields:

```text
format=mantle-target-artifact
schema_version=1
source_language=strata
```

The schema version identifies the currently admitted `.mta` encoding shape. It
is not a Strata language release or a compatibility promise.

Executable references and state transitions inside `.mta` use validated table
IDs and typed transition forms. Process transition records are encoded by
transition index and carry a `message` ID field. Validation requires one unique
transition for each accepted message, and runtime selection indexes the admitted
transition table by typed message ID.

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
