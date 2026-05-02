# Strata And Mantle File Types

This repository uses two first-class file extensions:

- `.str` for Strata source.
- `.mta` for Mantle Target Artifacts.

## `.str`

`.str` files are Strata source files. They are the user-authored program surface and should be UTF-8 text with LF line endings.

Expected MIME type:

```text
text/x-strata
```

## `.mta`

`.mta` files are Mantle Target Artifacts. They are executable inputs to Mantle, not Strata source and not proof/evidence artifacts.

The extension is intentionally language-neutral. Strata can emit `.mta`; a future Lattice frontend can emit `.mta` too. Mantle should decide whether an artifact is admissible from its internal header and validation data, not from the filename.

Minimum artifact identity fields:

```text
format=mantle-target-artifact
format_version=3
source_language=strata
```

Executable references and state transitions inside `.mta` use validated table IDs and typed transition forms. Names are retained for diagnostics, traces, and metadata, but Mantle execution must load and run resolved IDs rather than dispatching by source text.

The first product target path is:

```text
target/strata/hello.mta
```

Generated `.mta` files should normally remain under `target/`. Checked-in `.mta` files are allowed only as named test fixtures or specimens and must not be used as a substitute for a successful `strata build` and `mantle run`.

Expected MIME type:

```text
application/vnd.mantle.target-artifact
```

## First Gate

The initial source-to-runtime product gate is:

```text
strata check examples/hello.str
strata build examples/hello.str
mantle run target/strata/hello.mta
```
