# Language Surface Assurance

The current language surface assurance gate keeps an executable inventory of the
implemented Strata and Mantle surface. It maps declared current features to the
evidence classes that are expected for that feature: parser coverage,
checker/static validation, checked IR and lowering coverage, Mantle artifact
admission, runtime execution, diagnostics, examples, positive tests, negative
tests, source-to-runtime gates, fuzz seeds, and bounded or property coverage
where that evidence applies.

Run it with:

```sh
just language-surface-assurance
```

The machine-readable inventory is rooted in:

```text
crates/strata-mantle-acceptance/tests/language_surface_assurance.rs
```

The feature declarations are split by surface under:

```text
crates/strata-mantle-acceptance/tests/language_surface_assurance/
```

The gate fails when a declared current feature is missing one of its required
evidence classes, when an evidence path disappears, or when the recorded marker
no longer exists in the cited file. Source-to-runtime evidence must point at the
active `Justfile` check/build/run commands for the cited source example. The
inventory also fails if an evidence class is incompatible with the feature's
declared surface layer, so source-only, checker/lowering, artifact-admission,
runtime, docs/example, and future/non-admitted classifications remain distinct.

This gate supports implementation claims. It is not a theorem-prover proof, and
it does not replace runnable source-to-runtime behavior:

```text
.str source -> strata check -> strata build -> mantle run -> trace
```

Runtime-bearing features still need source-to-runtime evidence. Trace labels,
source names, and documentation text remain metadata and diagnostics surfaces;
executable behavior must cross the Strata/Mantle boundary as checked IR, typed
IDs, typed value templates, and admitted Mantle artifacts.
