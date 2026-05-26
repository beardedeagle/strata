# Language Surface Proof Substrate

The current language surface proof substrate is an executable, machine-readable
model of the implemented Strata and Mantle surface. It maps the agreed proof
domains for the current language surface to declared feature entries, maps those
entries to typed proof obligations, and maps those obligations to the evidence
classes required for that feature: parser coverage, checker/static validation,
checked IR and lowering coverage, Strata/Mantle boundary preservation, Mantle
artifact admission, runtime execution, diagnostics, examples, positive and
negative tests, source-to-runtime gates, fuzz seeds, and bounded or property
coverage where that evidence applies.
Typed scalar values and value operators are part of the runtime-bearing
surface: their source syntax, checked folding, typed lowering, Mantle artifact
admission, typed value-if templates, runtime evaluation, and fail-closed
diagnostics are all recorded in the inventory.
Typed effect outcomes are also runtime-bearing: source-visible send/spawn
`Result` bindings, checked outcome templates, Mantle artifact admission, runtime
commit-or-return behavior, spawn success process-reference evidence,
source-to-runtime success plus `Full`/`Stopped` pre-acceptance failure examples,
direct runtime `Crashed` failure evidence, typed `MailboxClosed` admission,
parse/check/lower, artifact-decode, and runtime-from-source fuzz seeds, bounded
state-admission evidence, and diagnostics are recorded together.

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
surface feature is not included in a proof domain, when a domain omits a proof
obligation implied by its features, when an obligation has no supporting
evidence class, when a feature is missing one of its required evidence classes,
when an evidence path disappears, or when the recorded marker no longer exists
in the cited file. Source-to-runtime evidence must point at the active
`Justfile` check/build/run commands for the cited source example. The inventory
also fails if an evidence class is incompatible with the feature's declared
surface layer, so source-only, checker/lowering, artifact-admission, runtime,
docs/example, and future/non-admitted classifications remain distinct.

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
