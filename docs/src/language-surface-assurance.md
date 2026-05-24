# Language Surface Proof Substrate

The current language surface proof substrate is an executable, machine-readable
inventory of the implemented Strata and Mantle surface. It maps the agreed
proof domains for the current language surface to declared feature entries, then
maps those entries to the evidence classes required for that feature: parser
coverage, checker/static validation, checked IR and lowering coverage, Mantle
artifact admission, runtime execution, diagnostics, examples, positive tests,
negative tests, source-to-runtime gates, fuzz seeds, and bounded or property
coverage where that evidence applies.

Run it with:

```sh
just language-surface-assurance
```

The machine-readable inventory is rooted in:

```text
crates/strata-mantle-acceptance/tests/language_surface_assurance.rs
```

The proof-domain declarations and feature declarations are split by surface
under:

```text
crates/strata-mantle-acceptance/tests/language_surface_assurance/
```

The gate fails when a declared proof domain is missing, when a declared language
surface feature is not included in a proof domain, when a feature is missing one
of its required evidence classes, when an evidence path disappears, or when the
recorded marker no longer exists in the cited file. Source-to-runtime evidence
must point at the active `Justfile` check/build/run commands for the cited
source example. The inventory also fails if an evidence class is incompatible
with the feature's declared surface layer, so source-only, checker/lowering,
artifact-admission, runtime, docs/example, and future/non-admitted
classifications remain distinct.

This gate is the current-language-surface proof substrate for implementation
claims. It is not a full theorem-prover proof of every semantic rule, and it
does not replace runnable source-to-runtime behavior:

```text
.str source -> strata check -> strata build -> mantle run -> trace
```

Runtime-bearing features still need source-to-runtime evidence. Trace labels,
source names, and documentation text remain metadata and diagnostics surfaces;
executable behavior must cross the Strata/Mantle boundary as checked IR, typed
IDs, typed value templates, and admitted Mantle artifacts.
