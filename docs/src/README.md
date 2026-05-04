# Overview

Strata is an experimental systems language for explicit authority, typed
concurrency, and runtime-visible execution. Mantle is the runtime target for
Strata programs.

The current product boundary is source-to-runtime execution:

```text
.str source -> strata check -> strata build -> .mta artifact -> mantle run
```

The repository is early, but the current slices are runnable. A real `.str`
program can be checked, built into a Mantle Target Artifact, admitted by Mantle,
and executed with runtime observability.

If you are new to Strata, start with Getting Started, then Language Concepts,
then the Hello tutorial. If you already know the shape of the project, use the
Language Reference and Syntax Reference as the current source-authoring
contract.

The documentation in this book tracks accepted behavior, file identities,
runtime boundaries, and the development gates that must stay green as the
language and runtime grow.

## Reading Paths

For a first working program:

1. Read Getting Started.
2. Build and run `examples/hello.str`.
3. Read Tutorial: Hello.
4. Read Tutorial: Actors And Messages.

For precise source behavior:

1. Read Language Reference.
2. Read Syntax Reference.
3. Check Diagnostics when a command rejects a program.

For runtime and contributor work:

1. Read Runtime Traces.
2. Read Artifact And Runtime Boundary.
3. Read Implementation Architecture.
4. Use Development Gates before closing changes.
