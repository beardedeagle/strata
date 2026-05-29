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
    module_decl import_decl* top_level_decl*

module_decl =
    "module" ident ";"

import_decl =
    "import" ident ";"

top_level_decl =
    record_decl
  | enum_decl
  | protocol_decl
  | port_decl
  | component_decl
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

Imports must appear immediately after the module declaration and before any
top-level declaration. The current import form admits one source-unit module
identifier and resolves it from the importing source unit's directory; aliases,
wildcards, re-exports, packages, and path strings are not accepted.

## Protocols, Ports, And Components

```text
protocol_decl = "protocol" ident "message" ident "requires" type_ref ";"
port_decl = "port" ident "protocol" ident "target" ident "requires" type_ref ";"
component_decl = "component" ident "exports" ident "requires" type_ref ";"
```

A protocol names one enum message type, a port binds that protocol to one
target process, and a component exports one port. The required authority
annotations must be exact: `Cap<ProtocolBoundary<ProtocolName>>`,
`Cap<PortConnect<PortName>>`, and `Cap<ComponentExport<ComponentName>>`.
See [Boundary Contracts](boundary-contracts.md) for the checked and lowered
contract.

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
  | authority_decl
  | supervisor_decl
  | init_function
  | step_function
  | source_function

state_alias =
    "type" "State" "=" type_ref ";"

message_alias =
    "type" "Msg" "=" type_ref ";"

authority_decl =
    "authority" ident ":" process_authority_type ";"

process_authority_type =
    "Cap" "<" "Spawn" "<" ident ">" ">"
  | "Cap" "<" "PortConnect" "<" ident ">" ">"

supervisor_decl =
    "supervise" "local" "one_for_one" "(" "max_restarts" ":" number "_u32" "," "within_ms" ":" number "_u64" ")" "{" supervisor_child+ "}"

supervisor_child =
    "child" ident ":" ident "=" "spawn" ident "as" ("permanent" | "transient" | "temporary") ";"
```

The aliases, authority declarations, supervisor declarations, and functions may
appear in any order. `State`, `Msg`, and `init` must each appear exactly once.
`Cap<Spawn<ProcessName>>` and `Cap<PortConnect<PortName>>` authorities must
target declared objects and be used by matching local actions. Non-`init`/`step`
functions are process-local source functions. A local supervisor declares static
lexical children and does not grant dynamic spawn authority.
Each concrete message case must resolve to one generated transition through an
explicit constructor pattern, one wildcard pattern, one `match msg` body, or a
state-match step for a constructor or wildcard message pattern. Parameter-pattern,
state-match, and whole-body `match msg` dispatch may split same-constructor
clauses by exact typed payload guard when nested predicates are disjoint over
discovered payload cases. A process cannot mix parameter-pattern/state-match
forms with a `match msg` step body. Other process members are rejected.

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

Buildable source accepts bodies for `init`, `step`, module functions, and
process-local functions. It requires deterministic functions and empty
may-behavior lists. Normal source functions are pure: they use `! []`, allow only
source value expressions, immutable source-local bindings, and pure braced
return branches, perform no runtime statements, and are expanded before
lowering.

## Function Bodies

```text
function_body =
    ";"
  | "{" block_body "}"
  | "{" match_body "}"

block_body =
    block_statement* (return_statement | return_if_else)

block_statement =
    source_function_statement
  | runtime_statement

match_body =
    "match" ident "{" match_arm+ "}"

match_arm =
    pattern "=>" "{" block_body "}"
```

Patterns are source-level binding and decomposition syntax. Strata supports
constructor patterns, constructor payload bindings/destructuring, record
destructuring patterns, list/map collection patterns, and `_` wildcards.
Buildable semantic consumers are normal source function signatures and match
bodies, function return-match expressions, fieldless enum `init` matches,
actor `step` dispatch, message-specific `match state` step bodies, and step
return-match expressions with optional uniform action prefixes. Record/list/map
destructuring patterns are accepted in those payload-capable positions when the
payload has the matching type. These forms may split a top-level constructor by
disjoint exact nested typed payload predicates.
Source function calls still expand before lowering; enum pattern dispatch
requires a concrete enum constructor value and record/list/map destructuring
requires a concrete value.

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

The first `type_ref` names the process state type. The message pattern is a
constructor, payload constructor, or `_`. Patterns bind immutable transition
locals, may destructure concrete records/lists/maps and nested constructors, and
use fieldless nested enum constructors as typed shape predicates. List rest is
suffix-only after at least one fixed element. Map patterns are exact unless they
end in `..` or `..rest`; `..rest` binds the remaining immutable map.

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
variants may bind the whole payload or destructure concrete record/list/map and
nested constructor payloads. Bindings are immutable and transition-local. Each
generated transition is keyed by message ID and checked current state ID, with an
exact typed payload guard when payload-sensitive clauses split. State changes
still occur only by returning a whole value through `Continue(...)`, `Stop(...)`,
or `Panic(...)`.

In a block-bodied `step`, the return expression may be a match over a
transition-local enum source binding whose concrete value is already proven by
step clause or state-match expansion. Any statements before the return match are
a uniform action prefix for every selected lowered transition:

```text
step_return_match =
    "return" "match" ident "{" match_arm+ "}" ";"
```

Every arm body may use local `emit`, in-scope direct `send`, statement-level
runtime `if`, and bounded runtime `for` before the terminal result. The checker
validates each arm fail-closed, selects the concrete arm before lowering, and
emits the same typed transition metadata as a direct step return. Arm-local
`spawn`, process-reference binding, nested runtime `for`, final-position runtime
`if`, nested return matches, catch-all dispatch, source-string selectors, and
source-function or `init` arm statements remain rejected.

In a pure block-bodied `init`, the return expression may be a match over one
fieldless enum constructor:

```text
init_return_match =
    "return" "match" ident "{" match_arm+ "}" ";"
```

Every arm body must be statement-free and must return one whole state value. The
checker selects the concrete arm before lowering and emits the selected initial
state as the existing typed state ID. This syntax does not allow effect
statements before the return match, nested return matches in arms, dynamic
dispatch, or source-string selectors.

A normal source function is a module-level function or a process-local function
whose name is not `init` or `step`:

```text
source_function =
    "fn" ident "(" (param_binding | pattern) ")"
    "->" type_ref
    "!" "[]" "~" "[]" "@det"
    ("{" block_body "}" | "{" match_body "}")
```

Function block bodies must not contain effect statements, process-reference
bindings, sends, or runtime loops. They may contain zero or more immutable
source-local value bindings before the terminal return:

```text
source_local_binding =
    "let" ident ":" type_ref "=" value_expr ";"
```

The binding type must be a source value type without process-reference
authority, and the value expression must be pure. Message enums that carry
direct `ProcessRef<T>` payloads are message authority surfaces, not
source-local value types. Source-local bindings are distinct from
`ProcessRef<T>` spawn bindings and are rejected in `init`, `step`,
statement-level runtime branches, or runtime loop bodies.

Function match bodies match the function's typed binding parameter. A function
block may also return a `match` over an in-scope source value binding, or use
braced pure return branches:

```strata
fn readiness(flag: Bool) -> Readiness ! [] ~ [] @det {
    if (flag) {
        return WarmReady;
    } else {
        return ColdReady;
    }
}
```

The braced form is pure control flow for returned values. Each branch may
contain immutable source-local bindings before its terminal `return`. Concrete
conditions select one branch during source function expansion. Function
return-match arms are exhaustive, duplicate-free, immutable, and still expand
before lowering.
Function calls and payload-bearing enum values share the same surface syntax:

```text
call_or_payload_constructor =
    ident "(" value_expr ")"
```

The checker resolves that form against the expected type. A declared enum
constructor becomes an immutable enum value; a declared function is expanded in
`init`, `step` result values, and send payload values. Recursive function call
cycles are rejected.

## Statements

```text
source_function_statement =
    source_local_binding

runtime_statement =
    emit_statement
  | process_ref_statement
  | spawn_outcome_statement
  | send_statement
  | send_outcome_statement
  | if_statement
  | for_statement

source_local_binding =
    "let" ident ":" type_ref "=" value_expr ";"

emit_statement =
    "emit" string_literal ";"

process_ref_statement =
    "let" ident ":" process_ref_type "=" "spawn" ident ";"

process_ref_type =
    "ProcessRef" "<" ident ">"

spawn_outcome_statement =
    "let" ident ":" "Result" "<" process_ref_type "," "SpawnError" "<" "Unit" ">" ">"
    "=" "spawn" ident ";"

send_statement =
    "send" ident ("via" ident)? ident payload_arg? ";"

send_outcome_statement =
    "let" ident ":" "Result" "<" "Unit" "," "SendError" "<" type_ref ">" ">"
    "=" "send" ident ("via" ident)? ident payload_arg? ";"

payload_arg =
    "(" value_expr ")"

if_statement =
    "if" "(" value_expr ")" "{" branch_statement* "}"
    ("else" "{" branch_statement* "}")?

branch_statement =
    emit_statement
  | send_statement
  | nested_if_statement
  | for_statement

nested_if_statement =
    "if" "(" value_expr ")" "{" nested_branch_statement* "}"
    ("else" "{" nested_branch_statement* "}")?

nested_branch_statement =
    emit_statement
  | send_statement
  | for_statement

for_statement =
    "for" for_item "in" ident "{" loop_statement* "}"

loop_statement =
    emit_statement
  | send_statement
  | if_statement

for_item =
    ident
  | ident "{" record_pattern_fields "}"

return_statement =
    "return" return_expr ";"

return_if_else =
    "if" "(" value_expr ")" "{" block_body "}"
    "else" "{" block_body "}"
```

Statement admission is contextual. Pure source function blocks admit only
`source_function_statement`; `init` and `step` runtime blocks admit
`runtime_statement`. Runtime branches and loop bodies use the narrower
`branch_statement`, `nested_branch_statement`, and `loop_statement` sets above.

The identifier in `process_ref_statement` names an immutable process reference
value. The identifier after `spawn` is the process definition name. The
`ProcessRef<T>` annotation must name the same process definition.

`spawn_outcome_statement` binds
`Result<ProcessRef<TargetProcess>,SpawnError<Unit>>` as an immutable step-local
value. The `Ok` branch carries the committed process reference; it is authority
data and is not valid source state. A top-level outcome binding is visible only
to later statements and return values.

The first `send` identifier is a local process reference or direct
`ProcessRef<T>` payload binding. Optional `via PortName` must name a declared
port matching the target process and message type, with a used local
`Cap<PortConnect<PortName>>` authority. Payload variants require one payload
value; unit variants reject payload values.

`send_outcome_statement` binds a typed local send result. The annotation must be
`Result<Unit,SendError<TargetMessageType>>` so pre-acceptance failure variants
can preserve the original message value. Top-level send and spawn outcome
bindings must precede ordinary non-prefix effect statements in the same step
body. A top-level process-reference `spawn` can remain in that pre-state prefix
so later outcome sends can target the spawned process reference.

The `for` collection source is an identifier binding with runtime-bound
`List<T,N>` type. The element item is an immutable element binding or record
pattern over the element type. Loop bodies support statement-level runtime `if`,
but reject nested loops, `return`, `spawn`, branch-local source or process
reference bindings, and branch nesting beyond one direct nested layer.

## Types

```text
type_ref =
    ident
  | ident "<" type_arg ("," type_arg)* ","? ">"

type_arg =
    type_ref
  | number
```

Checking accepts `ProcResult<StateType>` for `step`, `ProcessRef<ProcessName>`
for direct process authority surfaces, `Cap<Spawn<ProcessName>>` and
`Cap<PortConnect<PortName>>` in process authorities, and `List<T,N>` /
`Map<K,V,N>` over source value types. `Unit`, `Option<T>`, `Result<T,E>`,
`SendError<M>`, and `SpawnError<A>` are built-in value shapes for explicit
domain failure and typed effect outcomes.

## Values

```text
return_expr =
    value_expr
  | match_body

value_expr =
    value_or_expr

value_or_expr =
    value_and_expr
  | value_or_expr "||" value_and_expr

value_and_expr =
    value_equality_expr
  | value_and_expr "&&" value_equality_expr

value_equality_expr =
    value_ordering_expr
  | value_ordering_expr equality_op value_ordering_expr

equality_op =
    "=="
  | "!="

value_ordering_expr =
    value_additive_expr
  | value_additive_expr ordering_op value_additive_expr

ordering_op =
    "<"
  | "<="
  | ">"
  | ">="

value_additive_expr =
    value_multiplicative_expr
  | value_additive_expr additive_op value_multiplicative_expr

additive_op =
    "+"
  | "-"

value_multiplicative_expr =
    value_unary_expr
  | value_multiplicative_expr multiplicative_op value_unary_expr

multiplicative_op =
    "*"
  | "/"
  | "%"

value_unary_expr =
    value_primary_expr
  | "!" value_unary_expr
  | "-" suffixed_integer_literal

value_primary_expr =
    ident
  | suffixed_integer_literal
  | ident "(" value_expr ")"
  | ident "{" record_value_field ("," record_value_field)* ","? "}"
  | "List" list_type_args? "[" value_expr_list? "]"
  | "Map" map_type_args? "[" map_value_entries? "]"
  | "(" value_expr ")"
  | "if" "(" value_expr ")" "{" value_expr "}" "else" "{" value_expr "}"

record_value_field =
    ident ":" value_expr

value_expr_list =
    value_expr ("," value_expr)* ","?

map_value_entries =
    value_expr "=>" value_expr
    ("," value_expr "=>" value_expr)* ","?

suffixed_integer_literal =
    number scalar_suffix

scalar_suffix =
    "_u8" | "_u16" | "_u32" | "_u64"
  | "_i8" | "_i16" | "_i32" | "_i64"
```

Parenthesized value expressions group any value expression without changing its
type.
`ident(value)` is a function call when `ident` names a visible source function and
a payload-bearing enum value when it names a constructor of the expected enum type.

List and map constructors are explicit. Optional type and capacity arguments are
accepted for readability; the checker still validates each value against the
expected bounded source value type.

Typed equality predicates are deliberately narrow. `left == right`
and `left != right` are supported only when both operands have the same checked
type and that type is `Bool`, a scalar integer type, or a payload-free enum.
Fully concrete source equality folds during checking. Runtime-bound equality
lowers as a typed Mantle value template; operands are not runtime dispatch
strings. String equality, record/list/map structural equality,
process-reference equality, and payload enum equality remain unsupported.

Scalar literals require explicit suffixes in value positions. Arithmetic uses
`+`, `-`, `*`, `/`, and `%`; ordering uses `<`, `<=`, `>`, and `>=`. Scalar
operators require matching integer types and perform checked arithmetic. Fully
concrete overflow, underflow, division by zero, and modulo by zero fail during
checking. Runtime-bound scalar operators lower as typed Mantle value templates
and fail closed during runtime evaluation.

Boolean predicate composition is also narrow. `!`, `&&`, and `||` are supported
only over `Bool` values, typed equality or scalar-ordering predicates, or nested composed
predicates. `!` binds tighter than `&&`, and `&&` binds tighter than `||`; use
parentheses for explicit grouping. Fully concrete predicates fold during
checking. Runtime-bound predicates lower as typed Mantle value templates over
typed Bool-producing operands, not as source strings or function names.

Pure conditionals require the exact fieldless source contract
`enum Bool { False, True }`. Both branches are value expressions checked against
the same expected type. Concrete conditions select one branch before lowering;
runtime-bound expression-form conditionals lower as typed Mantle value
templates. Expression branch bodies cannot contain statements or effects.

Final-position `return_if_else` is runtime control flow in `step` bodies. The
condition must have the same `Bool` contract, but it may depend on received
payload or current-state payload bindings. Each branch is a block body with its
own statements and terminal return. Branch statement prefixes are limited to
`emit`, `send`, bounded `for` actions, and one direct statement-level
`if_statement` action. A bounded `for` prefix keeps the ordinary
loop-body surface, including the loop-body branch action. Strata lowers
the checked condition, branch action prefixes, and branch next states to Mantle
control flow; Mantle executes only the selected branch and traces the branch
choice. Deeper direct branch-action nesting remains rejected at source checking,
artifact admission, and loaded-runtime admission.

Statement-level `if_statement` is runtime control flow for effects before the
enclosing return. Branches may contain `emit`, `send`, and bounded `for`
statements, plus one direct nested statement-level `if_statement`. The `else`
branch may be omitted, and one branch body may be empty when the sibling branch
has at least one effect statement, nested branch action, or bounded-loop action;
both branches empty are rejected. An omitted
`else` lowers as an explicit empty branch in the typed Mantle artifact. Branches
cannot bind source-local values or process references, return, contain nested
loops, or exceed the one direct nested branch-action layer. Inside `for`, the
condition may use the immutable loop element binding or an immutable field
projected from a loop-element record pattern; lowering emits typed Mantle
templates over the loop element ID, not the source binding name. Loop-body
branch bodies follow the same direct nested branch-action bound.

`init` returns a state value or a pure `return match` that the checker reduces
to one state value before lowering. `step` returns `Continue(value)`,
`Stop(value)`, `Panic(value)`, a final-position runtime `if`, or a
`return match` that the checker reduces to one of those result forms before
lowering while preserving any uniform action prefix as typed transition actions.
A final-position runtime `if` branch may end in one direct nested final-position
runtime `if`; third-level terminal runtime branches remain rejected.

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

`_`, `as`, `authority`, `bounded`, `child`, `component`, `else`, `emit`, `enum`,
`exports`, `fn`, `for`, `if`, `in`, `let`, `import`, `local`, `mailbox`, `match`,
`module`, `mut`, `one_for_one`, `permanent`, `port`, `proc`, `protocol`, `record`,
`requires`, `return`, `security`, `send`, `spawn`, `supervise`, `target`,
`temporary`, `transient`, `type`, `var`, and `via` are reserved everywhere
identifiers are accepted. The single `_` token is reserved for wildcard
patterns.
`ProcResult`, `ProcessRef`, `Cap`, `Spawn`, `ProtocolBoundary`, `PortConnect`,
`ComponentExport`, `List`, `Map`, `Unit`, `Option`, `Result`, `SendError`,
`SpawnError`, `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, and `I64` are
reserved type names because they name built-in transition, process-reference,
capability descriptor, collection, effect outcome, and scalar value types.
