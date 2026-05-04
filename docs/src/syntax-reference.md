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

The aliases and functions may appear in any order, but each must appear exactly
once. Other process members are rejected.

## Functions

```text
function =
    "fn" ident "(" params? ")" "->" type_ref
    "!" effect_list
    "~" ident_list
    determinism
    function_body

params =
    param ("," param)* ","?

param =
    ident ":" type_ref

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
  | "{" message_match_body "}"

block_body =
    statement* return_statement

message_match_body =
    "match" "msg" "{" message_match_arm+ "}"

message_match_arm =
    ident "=>" "{" block_body "}" ","?
```

Buildable source requires bodies. `init` uses a block body. A multi-message
`step` uses `match msg`.

## Statements

```text
statement =
    emit_statement
  | spawn_statement
  | send_statement

emit_statement =
    "emit" string_literal ";"

spawn_statement =
    "spawn" ident ";"

send_statement =
    "send" ident ident ";"

return_statement =
    "return" return_expr ";"
```

The first identifier in `send` is the target process name. The second
identifier is the message variant to send.

## Types

```text
type_ref =
    ident
  | ident "<" type_ref ("," type_ref)* ","? ">"
```

The only built-in generic type currently accepted by checking is
`ProcResult<StateType>` as a `step` return type.

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

`mut` and `var` are reserved everywhere identifiers are accepted.
`ProcResult` is reserved as a type name because it names the built-in process
transition result type.
