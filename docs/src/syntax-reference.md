# Syntax Reference

This page gives a compact grammar-style view of the current accepted source
syntax. The Language Reference explains the same surface in prose.

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
    ident ("," ident)* ","?
```

Enums used as process state or message types must have at least one variant.

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

state_alias =
    "type" "State" "=" type_ref ";"

message_alias =
    "type" "Msg" "=" type_ref ";"
```

The aliases and functions may appear in any order. `State`, `Msg`, and `init`
must each appear exactly once. `step` must appear once for each message variant.
Other process members are rejected.

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
  | signature_pattern

param_binding =
    ident ":" type_ref

signature_pattern =
    ident

effect_list =
    "[" (effect ("," effect)* ","?)? "]"

effect =
    "emit" | "spawn" | "send"

ident_list =
    "[" (ident ("," ident)* ","?)? "]"

determinism =
    "@det" | "@nondet"
```

Buildable source currently accepts bodies for `init` and `step` only. It
requires deterministic functions and empty may-behavior lists.

## Function Bodies

```text
function_body =
    ";"
  | "{" block_body "}"

block_body =
    statement* return_statement
```

Buildable source requires bodies. `init` uses no parameters. Each `step` uses
`state: StateType` followed by one message-variant signature pattern:

```text
step_function =
    "fn" "step" "(" "state" ":" state_type "," message_variant ")"
    "->" "ProcResult" "<" state_type ">"
    "!" effect_list "~" "[]" "@det"
    "{" block_body "}"
```

Signature patterns are accepted only for actor `step` message dispatch in this
slice.

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
    "send" ident ident ";"

return_statement =
    "return" return_expr ";"
```

The identifier after `let` names an immutable process reference value. The
identifier after `spawn` is the process definition name. The `ProcessRef<T>`
annotation must name the same process definition.

The first identifier in `send` is a process reference. The second identifier is
the message variant to send.

## Types

```text
type_ref =
    ident
  | ident "<" type_ref ("," type_ref)* ","? ">"
```

The built-in generic types currently accepted by checking are
`ProcResult<StateType>` as a `step` return type and
`ProcessRef<ProcessName>` in spawn bindings.

## Values

```text
return_expr =
    value_expr
  | ident "(" value_expr ")"

value_expr =
    ident
  | ident "{" record_value_field ("," record_value_field)* ","? "}"

record_value_field =
    ident ":" value_expr
```

`init` returns a state value. `step` returns `Stop(value)` or
`Continue(value)`.

## Literals

The current literal surface is intentionally narrow:

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
`ProcResult` and `ProcessRef` are reserved type names because they name built-in
transition and process-reference types.
