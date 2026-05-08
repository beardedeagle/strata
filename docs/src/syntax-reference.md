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
Payload variants are accepted for process message enums. State enum payload
variants are rejected in this slice.

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
source helpers. Each message variant must resolve to exactly one `step` clause,
either through an explicit constructor pattern, through one wildcard pattern,
or through one match step body. A process cannot mix step parameter patterns
with a match step body in this slice. Other process members are rejected.

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
  | ident "(" ident ":" type_ref ")"
  | "_"

effect_list =
    "[" (effect ("," effect)* ","?)? "]"

effect =
    "emit" | "spawn" | "send"

ident_list =
    "[" (ident ("," ident)* ","?)? "]"

determinism =
    "@det" | "@nondet"
```

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
admits constructor patterns, constructor payload bindings, and `_` wildcards.
Buildable semantic consumers are normal source function signatures and match
bodies, fieldless enum `init` matches, and actor `step` message dispatch. Actor
`step` dispatch accepts constructor payload bindings in parameter patterns and
whole-body `match msg` arms. Normal source function patterns currently cover
fieldless enum constructors.

Buildable source requires bodies. `init` uses no parameters. Each
parameter-pattern `step` uses `state: StateType` followed by one message
constructor or wildcard pattern:

```text
parameter_pattern_step_function =
    "fn" "step" "(" "state" ":" type_ref ","
        (ident | ident "(" ident ":" type_ref ")" | "_") ")"
    "->" "ProcResult" "<" type_ref ">"
    "!" effect_list "~" "[]" "@det"
    "{" block_body "}"
```

The first `type_ref` must name the process state type. An `ident` after the
comma is a message constructor accepted by the process message type. A payload
pattern such as `Assign(job: Job)` binds the received payload as an immutable
transition-local value. `_` is a wildcard pattern that covers accepted variants
without explicit clauses.

A match `step` uses a typed message parameter and a whole-body
`match` over that parameter:

```text
match_step_function =
    "fn" "step" "(" "state" ":" type_ref "," ident ":" type_ref ")"
    "->" "ProcResult" "<" type_ref ">"
    "!" effect_list "~" "[]" "@det"
    "{" match_body "}"
```

Each match arm uses the same pattern syntax as parameter-pattern dispatch. The
match scrutinee must be the typed message parameter in the current buildable
step subset. Match arms are block-delimited and do not use comma separators.
The step effect list applies to every generated transition, so each arm must
use exactly the declared effects.

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
function's typed binding parameter. Helper calls are value expressions:

```text
helper_call =
    ident "(" value_expr ")"
```

The checker validates helper signatures and bodies before lowering, then
expands calls in `init`, `step` result values, and send payload values.
Recursive helper call cycles are rejected.

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
  | ident "<" type_ref ("," type_ref)* ","? ">"
```

The built-in generic types accepted by checking are
`ProcResult<StateType>` as a `step` return type and
`ProcessRef<ProcessName>` in spawn bindings, message payload declarations, and
payload-binding step patterns.

## Values

```text
return_expr =
    value_expr
  | ident "(" value_expr ")"

value_expr =
    ident
  | ident "(" value_expr ")"
  | ident "{" record_value_field ("," record_value_field)* ","? "}"

record_value_field =
    ident ":" value_expr
```

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
The single `_` token is reserved for wildcard patterns. `ProcResult` and
`ProcessRef` are reserved type names because they name built-in transition and
process-reference types.
