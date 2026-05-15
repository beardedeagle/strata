# Syntax Reference

This page gives a compact grammar-style view of the accepted source syntax. The
Language Reference explains the same surface in prose.

The notation is informal:

- quoted text is literal syntax;
- `?` means optional;
- `*` means zero or more;
- `+` means one or more;
- `|` means choice.

## Source File

```text
source_file =
    module_decl top_level_decl*

module_decl =
    "module" ident ";"

top_level_decl =
    record_decl
  | enum_decl
  | function
  | process_decl
```

## Records

```text
record_decl =
    "record" ident ";"
  | "record" ident "{" record_field ("," record_field)* ","? "}"

record_field =
    ident ":" type_ref
```

Fieldless records use the semicolon form. Braced records must declare at least
one field.

## Enums

```text
enum_decl =
    "enum" ident "{" enum_variant_list? "}"

enum_variant_list =
    enum_variant ("," enum_variant)* ","?

enum_variant =
    ident
  | ident "(" type_ref ")"
```

Enums used as process state or message types must have at least one variant.
Payload variants are accepted for process state and message enums. State enum
payload constructors create immutable whole-state values.

## Processes

```text
process_decl =
    "proc" ident "mailbox" "bounded" "(" number ")" "{"
        process_member*
    "}"

process_member =
    state_alias
  | message_alias
  | init_function
  | step_function
  | source_function

state_alias =
    "type" "State" "=" type_ref ";"

message_alias =
    "type" "Msg" "=" type_ref ";"
```

The aliases and functions may appear in any order. `State`, `Msg`, and `init`
must each appear exactly once. Non-`init`/`step` functions are process-local
source helpers. Each concrete message case must resolve to exactly one generated
transition, either through an explicit constructor pattern, through one wildcard
pattern, through one `match msg` step body, or through a state-match step for a
specific message pattern. In parameter-pattern, state-match, and whole-body
`match msg` dispatch, same top-level message constructor clauses may split by
exact typed payload guard when their nested predicates are provably disjoint
over discovered concrete payload cases. Payload-sensitive state-match splitting
requires explicit payload cases; payload-sensitive state-match wildcard fallback
is rejected in this slice. A process cannot mix parameter-pattern/state-match
step forms with a `match msg` step body in this slice. Other process members are
rejected.

## Functions

```text
function =
    "fn" ident "(" params? ")" "->" type_ref
    "!" effect_list
    "~" ident_list
    determinism
    function_body

params =
    function_param ("," function_param)* ","?

function_param =
    param_binding
  | pattern

param_binding =
    ident ":" type_ref

pattern =
    ident
  | ident "(" constructor_payload_pattern ")"
  | ident "{" record_pattern_fields "}"
  | "List" list_type_args? "[" list_pattern_items? "]"
  | "Map" map_type_args? "[" map_pattern_entries? "]"
  | "_"

constructor_payload_pattern =
    ident ":" type_ref
  | ident
  | ident "(" constructor_payload_pattern ")"
  | ident "{" record_pattern_fields "}"
  | "List" list_type_args? "[" list_pattern_items? "]"
  | "Map" map_type_args? "[" map_pattern_entries? "]"
  | "_"

record_pattern_fields =
    record_pattern_field ("," record_pattern_field)* ","?

record_pattern_field =
    ident
  | ident ":" ident

list_type_args =
    "<" type_ref "," number ">"

map_type_args =
    "<" type_ref "," type_ref "," number ">"

list_pattern_items =
    collection_pattern_binding ("," collection_pattern_binding)* ("," ".." ident ","? | ","?)

map_pattern_entries =
    ".." ident? ","?
  | value_expr "=>" collection_pattern_binding
    ("," value_expr "=>" collection_pattern_binding)*
    ("," ".." ident? ","? | ","?)

collection_pattern_binding =
    nested_collection_pattern
  | ident
  | "_"

nested_collection_pattern =
    ident "(" constructor_payload_pattern ")"
  | ident "{" record_pattern_fields "}"
  | "List" list_type_args? "[" list_pattern_items? "]"
  | "Map" map_type_args? "[" map_pattern_entries? "]"

effect_list =
    "[" (effect ("," effect)* ","?)? "]"

effect =
    "emit" | "spawn" | "send"

ident_list =
    "[" (ident ("," ident)* ","?)? "]"

determinism =
    "@det" | "@nondet"
```

Collection pattern bindings that begin with an identifier are nested patterns
only when the identifier is followed by `(` or `{`, or when `List`/`Map` is
followed by optional type arguments and `[`. Otherwise the identifier is an
immutable binding name.

Buildable source accepts bodies for `init`, `step`, module helpers, and
process-local helpers. It requires deterministic functions and empty
may-behavior lists. Normal source helpers are pure: they use `! []`, perform no
statements, and are expanded before lowering.

## Function Bodies

```text
function_body =
    ";"
  | "{" block_body "}"
  | "{" match_body "}"

block_body =
    statement* return_statement

match_body =
    "match" ident "{" match_arm+ "}"

match_arm =
    pattern "=>" "{" block_body "}"
```

Patterns are source-level binding and decomposition syntax. This source slice
admits constructor patterns, constructor payload bindings, constructor payload
destructuring, record destructuring patterns, list/map collection patterns, and
`_` wildcards. Buildable semantic consumers are normal source function
signatures and match bodies, helper return-match expressions, fieldless enum
`init` matches, actor `step` message dispatch, and message-specific
`match state` step bodies. Record/list/map destructuring patterns are accepted
in normal source helper signatures, helper match bodies, helper return-match
expressions, message constructor payloads, and current-state enum payloads when
the payload has the matching type. Helper signatures, helper match bodies,
helper return-match expressions, whole-body `match msg` arms, step parameter
patterns, and state-match step clauses may split a top-level constructor by
disjoint exact nested typed payload predicates. Source helper calls still expand
before lowering; enum pattern dispatch requires a concrete enum constructor
value and record/list/map destructuring requires a concrete value.

Buildable source requires bodies. `init` uses no parameters. Each
parameter-pattern `step` uses `state: StateType` followed by one message
constructor or wildcard pattern:

```text
parameter_pattern_step_function =
    "fn" "step" "(" "state" ":" type_ref ","
        (ident | ident "(" constructor_payload_pattern ")" | "_") ")"
    "->" "ProcResult" "<" type_ref ">"
    "!" effect_list "~" "[]" "@det"
    "{" block_body "}"
```

The first `type_ref` must name the process state type. An `ident` after the
comma is a message constructor accepted by the process message type. A payload
pattern such as `Assign(job: Job)` binds the received payload as an immutable
transition-local value. A constructor payload pattern such as
`Assign(Job { phase })`, `Envelope(Assign(Job { phase }))`,
`Assign(Ready)`, `Assign(List[Job { phase }, ..tail])`, or
`Assign(Map[Ready => Job { phase }])` destructures an immutable concrete payload
and binds only the selected values. A fieldless nested enum constructor such as
`Ready` is a typed shape predicate and does not introduce a binding. List rest
patterns are suffix-only in this slice:
`..tail` must follow at least one fixed-position element and binds an immutable
whole list containing the unmatched suffix. Map payload patterns are exact unless
they end with `..`, as in `Assign(Map[Ready => selected, ..])`; the subset marker
permits additional static keys while requiring the listed keys. `..rest`
additionally binds an immutable map containing entries except the listed static
keys. `_` is a wildcard pattern that covers accepted variants without explicit
clauses.

Multiple parameter-pattern `step` clauses may share one top-level message
constructor only when exact nested typed payload predicates are provably
disjoint. A wildcard fallback may cover only discovered concrete payload cases
not matched by explicit payload-sensitive clauses. The fallback lowers to exact
typed payload-guarded transitions for those discovered cases; it is not a
runtime catch-all for future payload values.

A match `step` uses a typed message parameter and a whole-body
`match` over that parameter:

```text
match_step_function =
    "fn" "step" "(" "state" ":" type_ref "," ident ":" type_ref ")"
    "->" "ProcResult" "<" type_ref ">"
    "!" effect_list "~" "[]" "@det"
    "{" match_body "}"
```

Each match arm uses constructor or wildcard pattern syntax. Constructor payload
patterns may bind or destructure nested constructor, record, list, and map
payloads. Arms that share one top-level message constructor may split by exact
typed payload guard when their nested predicates are provably disjoint. The
match scrutinee must be the typed message parameter in the current buildable
step subset. Match arms are block-delimited and do not use comma separators.
The step effect list applies to every generated transition, so each arm must use
exactly the declared effects.

A state-match `step` uses the normal state parameter plus a message constructor
or wildcard pattern, then uses a whole-body `match state`:

```text
state_match_step_function =
    "fn" "step" "(" "state" ":" type_ref ","
        (ident | ident "(" constructor_payload_pattern ")" | "_") ")"
    "->" "ProcResult" "<" type_ref ">"
    "!" effect_list "~" "[]" "@det"
    "{" "match" "state" "{" match_arm+ "}" "}"
```

State-match arms resolve against the declared process state enum. Payload
variants may bind their whole payload with an explicit type, such as
`Working(job: Job)`, or destructure a concrete record/list/map payload, such as
`Working(Job { phase })`. Nested constructor payloads use the same structural
pattern rules. Fieldless variants must not bind or destructure a
payload. Bindings are immutable and transition-local. Each generated transition
is keyed by the message ID and the admitted current state ID. State-match clauses
may share one top-level message constructor by exact disjoint nested typed
payload predicates; each generated transition is additionally keyed by the exact
typed payload guard. Payload-sensitive state-match wildcard fallback is rejected
in this slice. State changes still occur only by returning a whole state value
through `Continue(...)`, `Stop(...)`, or `Panic(...)`.

A normal source helper is a module-level function or a process-local function
whose name is not `init` or `step`:

```text
source_function =
    "fn" ident "(" (param_binding | pattern) ")"
    "->" type_ref
    "!" "[]" "~" "[]" "@det"
    ("{" block_body "}" | "{" match_body "}")
```

Helper block bodies must not contain statements. Helper match bodies match the
function's typed binding parameter. A helper block may also return a `match`
over an in-scope source value binding; the match arms are exhaustive,
duplicate-free, immutable, and still expand before lowering. Helper calls and
payload-bearing enum values share the same surface syntax:

```text
call_or_payload_constructor =
    ident "(" value_expr ")"
```

The checker resolves that form against the expected type. A declared enum
constructor becomes an immutable enum value; a declared helper is expanded in
`init`, `step` result values, and send payload values. Recursive helper call
cycles are rejected.

## Statements

```text
statement =
    emit_statement
  | process_ref_statement
  | send_statement

emit_statement =
    "emit" string_literal ";"

process_ref_statement =
    "let" ident ":" process_ref_type "=" "spawn" ident ";"

process_ref_type =
    "ProcessRef" "<" ident ">"

send_statement =
    "send" ident ident payload_arg? ";"

payload_arg =
    "(" value_expr ")"

return_statement =
    "return" return_expr ";"
```

The identifier after `let` names an immutable process reference value. The
identifier after `spawn` is the process definition name. The `ProcessRef<T>`
annotation must name the same process definition.

The first identifier in `send` is a local process reference or a received
payload binding whose type is `ProcessRef<T>`. The second identifier is the
message variant to send. Payload variants require one payload value. Unit
variants reject payload values.

## Types

```text
type_ref =
    ident
  | ident "<" type_arg ("," type_arg)* ","? ">"

type_arg =
    type_ref
  | number
```

The built-in generic types accepted by checking are
`ProcResult<StateType>` as a `step` return type and
`ProcessRef<ProcessName>` in spawn bindings, message payload declarations, and
payload-binding step patterns. `List<T,N>` and `Map<K,V,N>` are accepted source
value types when their element, key, and value arguments are source value types
and `N` is a numeric capacity.

## Values

```text
return_expr =
    value_expr
  | match_body

value_expr =
    ident
  | ident "(" value_expr ")"
  | ident "{" record_value_field ("," record_value_field)* ","? "}"
  | "List" list_type_args? "[" value_expr_list? "]"
  | "Map" map_type_args? "[" map_value_entries? "]"

record_value_field =
    ident ":" value_expr

value_expr_list =
    value_expr ("," value_expr)* ","?

map_value_entries =
    value_expr "=>" value_expr
    ("," value_expr "=>" value_expr)* ","?
```

The parenthesized value expression is typed during checking. It is a helper call
when `ident` names a visible source helper and a payload-bearing enum value when
`ident` names a constructor of the expected enum type.

List and map constructors are explicit. Optional type and capacity arguments are
admitted for readability; the checker still validates each value against the
expected bounded source value type.

`init` returns a state value. `step` returns `Continue(value)`, `Stop(value)`,
or `Panic(value)`.

## Literals

The literal surface is intentionally narrow:

- decimal numbers are accepted for mailbox bounds;
- string literals are accepted for `emit`;
- string escapes are not supported;
- newline and carriage return characters are not allowed inside string
  literals.

## Identifiers

```text
ident =
    (ASCII letter | "_") (ASCII letter | ASCII digit | "_")*
```

`as`, `let`, `mut`, and `var` are reserved everywhere identifiers are accepted.
The single `_` token is reserved for wildcard patterns. `ProcResult`,
`ProcessRef`, `List`, and `Map` are reserved type names because they name
built-in transition, process-reference, and collection types.
