# Diagnostics

Strata diagnostics are intended to reject invalid source close to the layer
that can explain it. Parser errors describe source shape. Checker errors
describe semantic rules. Runtime errors describe validated execution failures.

## Reading A Diagnostic

Run:

```sh
just strata-check examples/hello.str
```

If checking fails, fix the first reported error first. Later errors may be a
result of the first invalid shape.

## Common Source Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `expected record, enum, function, or proc declaration` | A top-level item is not accepted. | Use `record`, `enum`, `fn`, or `proc` after `module`. |
| `entry process Main is not declared` | The program has no `Main` process. | Add `proc Main ...`. |
| `type name ... is reserved` | A source record or enum tries to use a built-in transition, capability, collection, outcome, or scalar type name. | Rename the source type. |
| `uses reserved prefix __strata_checked_` | A source type name collides with internal checked type metadata. | Rename the source type without the reserved prefix. |
| `checked type_count exceeds Mantle artifact limit` | The checked program needs more distinct artifact types than the Mantle artifact limit allows. | Reduce the number of distinct state, message, payload, and process-reference types. |
| `process ... must declare type State` | A process is missing its state alias. | Add `type State = StateType;`. |
| `process ... must declare type Msg` | A process is missing its message alias. | Add `type Msg = MessageEnum;`. |
| `init must declare no parameters` | `init` has parameters. | Use `fn init() -> StateType ...`. |
| `init body must not perform statements` | `init` uses `emit`, `spawn`, or `send`. | Return only the initial state. |
| `match scrutinee ... fieldless enum variant` | An `init` whole-body match or `init return match` tries to match a non-constructor name or a payload-bearing constructor. | Match one fieldless enum constructor in `init`. |
| `init return match ...` | An `init return match` is non-exhaustive, overlaps an earlier arm, has an unreachable wildcard, or nests another return match in an arm. | Cover each variant once or use one reachable `_`, and return one whole state value from each arm. |
| `init return match arm cannot use payload binding ... in returned state` | An `init return match` arm tries to materialize an arm payload binding into the initial state. | Return a whole state value that does not depend on match-arm payload bindings. |
| `step must declare state parameter and message pattern` | `step` has the wrong parameter count. | Use `state: StateType, MessageConstructor` or `state: StateType, _`. |
| `step second parameter must be a message constructor pattern or wildcard pattern` | The second `step` parameter is a typed binding instead of a message pattern. | Replace `msg: MsgType` with a message constructor or `_`, or use a whole-body `match msg`. |
| `step returns ..., expected ProcResult<...>` | `step` return type is wrong. | Return `ProcResult<StateType>`. |
| `step return match scrutinee ... must be a concrete enum source value binding` | A `step return match` tries to match `state`, a non-enum value, or a value that is not an immutable source binding. | Match a transition-local enum payload or state-payload binding whose concrete value is already proven by the checker, or use whole-body `match msg` / `match state`. |
| `step return match scrutinee ... requires a discovered concrete message payload case` | A `step return match` tries to match a message payload binding from an unguarded transition. | Use a payload-sensitive pattern that lowers to exact discovered payload guards, or move the dispatch to whole-body `match msg`. |
| `step return match arm cannot bind process reference ...` | A `step return match` arm tries to acquire arm-local authority with `spawn`. | Bind process references before `return match`; selected arm prefixes may use `emit`, in-scope direct `send`, statement-level runtime `if`, and bounded runtime `for` actions. |
| `step return match arm cannot perform final-position runtime if` | A `step return match` arm tries to use terminal runtime branching. | Move runtime branching outside the return-match arm, or split the behavior into explicit step clauses / whole-body dispatch. |
| `step return match arm nested return match is not supported` | A `step return match` arm tries to nest another `return match`. | Keep the selected arm as typed action statements followed by one terminal `Continue(...)`, `Stop(...)`, or `Panic(...)`. |
| `step return match arm must return Stop..., Continue..., or Panic...` | A `step return match` arm returns a bare state value or another unsupported result form. | Return one whole state value inside `Continue(...)`, `Stop(...)`, or `Panic(...)` from every arm. |
| `step body must return Stop..., Continue..., or Panic...` | A `step` returns a bare state value or an unsupported result form. | Return one whole state value inside `Continue(...)`, `Stop(...)`, or `Panic(...)`. |
| `step may-behaviors must be empty` | The `~ [...]` list is not empty. | Use `~ []`. |
| `step must be deterministic` | `step` uses `@nondet`. | Use `@det`. |
| `uses effect ... but does not declare it` | The body performs `emit`, `spawn`, or `send` without matching effect usage. | Add the exact used effect to `! [...]` or remove the statement. |
| `declares effect ... but does not use it` | The effect list is wider than the body. | Remove the unused effect. |
| `declares duplicate effect` | The effect list repeats one effect. | Keep each effect at most once. |
| `send outcome binding must have type Result<Unit,SendError<...>>` | A local send outcome annotation does not preserve the target process message type. | Annotate the binding as `Result<Unit,SendError<TargetMsg>>`. |
| `spawn outcome binding must have type Result<ProcessRef<...>,SpawnError<Unit>>` | A local spawn outcome annotation does not return the committed process reference on success. | Annotate the binding as `Result<ProcessRef<TargetProcess>,SpawnError<Unit>>`. |
| `effect outcome binding ... conflicts with ...` | An outcome binding reuses a step state parameter, process reference, process declaration, or source value name. | Give the immutable outcome a fresh step-local name. |
| `effect outcome binding ... is used before it is bound` | A statement references an outcome before the outcome `let` statement. | Move the outcome binding before the first use. |
| `effect outcome binding ... must appear before ordinary effect statements` | A later outcome binding would be executed before an earlier non-prefix effect at runtime. | Bind local effect outcomes before ordinary `emit`, `send`, `if`, or `for` statements. A top-level process-reference `spawn` may stay in the pre-state prefix so later outcome sends can target it. |
| `effect outcome binding ... cannot be used as a next-state value because type ... has non-finite payload values` | A next state tries to store an outcome whose possible payload values cannot be finitely admitted into the process state table. | Store only finite outcome value shapes in state, or handle the effect without placing that outcome in state. |
| `effect outcome binding ... would expand next-state candidates to ...` | Multiple finite outcome bindings would exceed the admitted process state-value limit. | Store fewer independent outcome fields, branch before storing, or keep the outcome outside process state. |
| `effect outcome id ... appears after ordinary effects` | A Mantle artifact tries to bind an effect outcome after an ordinary effect boundary. | Emit outcome actions in the pre-state prefix before ordinary effects. |
| `process reference outcome must remain step-local` | A Mantle artifact or loaded runtime template tries to use a process-reference-carrying outcome as ordinary state or payload data. | Branch on a typed built-in outcome variant, or keep the process reference as step-local authority. |
| `for loop collection must be an identifier binding` | A runtime `for` loop tries to iterate a literal or computed value. | Iterate an immutable runtime collection binding such as a typed message payload binding. |
| `for loop collection ... must have type List<T,N>` | A runtime `for` loop source is not a typed list collection. | Use a binding whose type is `List<Element,N>`. |
| `for loop collection must be a runtime list binding` | A `for` loop source is static source data rather than runtime data. | Pass the list as a typed runtime payload or state-derived runtime binding. |
| `for loop element binding ... cannot have process reference type` | A runtime `for` loop tries to iterate process-reference values as ordinary loop data. | Keep `ProcessRef<T>` as direct message authority and do not place it in loop collections. |
| `for loop record pattern ... cannot match ...` | A runtime loop record pattern names the wrong record type or tries to destructure a non-record element. | Match the actual record element type, or use a plain immutable loop element binding. |
| `record pattern ... must bind at least one field` | A source record pattern, including a loop-element record pattern, has no fields. | Bind at least one immutable field or use a plain immutable binding. |
| `for loop record pattern ... binds field ... more than once` | A runtime loop record pattern repeats one record field. | Bind each projected field at most once. |
| `for loop record pattern ... has no field ...` | A runtime loop record pattern names a field outside the loop element record. | Bind a declared field from the record element. |
| `loop element binding ... is declared more than once` | A loop-element record pattern maps multiple fields to the same local binding. | Use one distinct immutable binding name per projected field. |
| `loop element binding ... cannot have process reference type` | A loop-element record pattern projects a `ProcessRef<T>` field as ordinary data. | Keep process references out of records, lists, maps, state, and projected loop data. |
| `loop element binding ... conflicts ...` | A loop element or projected field binding reuses a reserved name, source binding, process reference, process declaration, type, or constructor. | Choose a distinct immutable binding name for the loop body. |
| `for loop body cannot bind process reference` | A loop body tries to create new authority with `spawn`. | Bind process references before the loop and keep the loop body to checked linear effects. |
| `nested for loops are not supported` | A loop body contains another loop. | Flatten the runtime payload shape or split the behavior into separate steps. |
| `assignment statements are not supported` | Source code uses assignment-style mutation. | Bind immutable values through declarations or return a whole replacement state value. |
| `statement-level if branches must not return` | An effect-only runtime branch tries to terminate the step. | Put the final `return` after the statement-level branch, or use final-position runtime `if` when each branch must return. |
| `statement-level if branches cannot bind local values or process references` | A runtime branch tries to introduce branch-local source computation or authority. | Use source-local bindings only in pure source functions, bind process references before the branch, and keep runtime branches to checked branch effects. |
| `statement-level if action nesting exceeds maximum depth` | Direct statement-level runtime branch nesting goes beyond the single supported nested layer. | Keep direct branch actions to an outer branch plus one nested branch. |
| `statement-level if branches cannot both be empty` | A statement-level runtime branch has no actions on either side. | Put an action, such as `emit`, `send`, or a bounded `for` loop, in one branch. Omitted `else` and explicit `else {}` are allowed only when the other branch has work. |
| `runtime if branch cannot bind process references` | A checked runtime branch tries to introduce branch-local authority. | Bind process references before the branch and keep branch bodies to declared effects. |
| `runtime if action nesting exceeds maximum depth` | A checked runtime branch action exceeds the direct nesting bound. | Keep runtime branch actions to an outer branch plus one nested branch. |
| `next_state runtime if nesting exceeds maximum depth` | Source, checked IR, artifact admission, or loaded-runtime admission sees a third terminal runtime branch. | Keep terminal next-state runtime branching to an outer final-position branch plus one direct nested final-position branch. |
| `runtime if action branches cannot both be empty` | A decoded or constructed artifact tries to validate a runtime branch action with no actions in either branch. | Keep no-op branches explicit in the typed artifact, but ensure the sibling branch has at least one action. |

## Source Function Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `function ... conflicts with a declared type or value constructor` | A source function name collides with a type or enum constructor. | Choose a distinct function name. |
| `function ... must declare exactly one parameter` | A normal source function uses an arity outside the current buildable call form. | Use one typed binding parameter or one pattern parameter clause. |
| `function ... must use a declared record, enum, scalar, list, or map type without process-reference authority` | A source function parameter or return type names something outside the source value type set, including an enum that carries `ProcessRef<T>` authority. | Use a declared `record` or `enum` type, a scalar integer type, or `List<T,N>` / `Map<K,V,N>` over source value types that do not contain process references. |
| `function ... must not declare effects` / `function ... must not perform statements` | A normal source function tries to perform runtime behavior. | Keep normal functions pure; perform `emit`, `spawn`, and `send` only in `step`. |
| `function ... may-behaviors must be empty` / `function ... must be deterministic` | A normal source function is not in the deterministic buildable subset. | Use `~ [] @det`. |
| `source function parameter ... conflicts ...` | A source function binding parameter reuses a process-reference binding, type, constructor, process, or source function name. | Choose a distinct immutable parameter name. |
| `source-local binding ... conflicts ...` | A pure source function binding reuses a parameter, pattern binding, prior local binding, process-reference binding, type, constructor, process, or source function name. | Choose a distinct immutable binding name. |
| `function ... source-local binding ... must use a declared record, enum, scalar, list, or map type without process-reference authority` | A source-local binding annotation is not a source value type, such as `ProcessRef<T>`, an enum that carries `ProcessRef<T>`, or an unknown type. | Bind only records, enums, scalar integer types, `List<T,N>`, or `Map<K,V,N>` over source value types that do not contain process references. |
| `source-local binding ... value must produce ...` | A source-local binding right-hand side is unknown, impure, or does not match the annotated type. | Use a pure source value expression with the exact annotated source value type. |
| `function ... is not declared` | A value expression calls an unknown function. | Declare a module function or process-local function with that name. |
| `function ... returns ..., expected ...` | The function return type does not match the value position where it is called. | Call a function returning the expected type or change the annotation. |
| `source function call cycle ... is not supported` | Source function calls are recursive, but pure functions are expanded before lowering and have no recursion model. | Remove the cycle; pass whole values through non-recursive functions. |
| `if condition requires enum Bool { False, True }` | A source conditional is used without the explicit fieldless Bool contract. | Declare `enum Bool { False, True }`. |
| `if condition must have type Bool` | A source conditional condition resolves to a non-Bool source value. | Return or pass `True` or `False` from the declared `Bool` enum. |
| `if then branch must produce ...` / `if else branch must produce ...` | A conditional branch does not match the expected source value type. | Return the same source value type from both branches. |
| `if branches are pure value expressions and must not perform statements` | A conditional branch contains `emit`, `let`, `send`, or `return`. | Keep branch bodies to one source value expression and move effects to supported `step` forms. |
| `source function ... return-if ... branch must not perform statements` | A braced source-function return branch tries to perform `emit`, `send`, `spawn`, a runtime loop, or another runtime statement before returning. | Keep source functions pure; use only immutable source-local bindings before the terminal pure return. |
| `equality operands must have the same type` | A `==` or `!=` expression compares values of different checked types. | Compare two `Bool` values, two scalar values of the same integer type, or two fieldless values from the same enum. |
| `equality operands must be Bool, scalar values, or fieldless enum values` | A `==` or `!=` operand is outside the equality surface. | Use `Bool`, matching scalar integer values, or a payload-free enum value; strings, records, lists, maps, process references, and payload-bearing enum values are not equality operands. |
| `numeric value literals require an explicit scalar suffix` / `unsupported scalar literal suffix ...` | A numeric value expression is unsuffixed or uses a suffix outside the admitted scalar set. | Use suffixes such as `_u32` or `_i64`; mailbox bounds and collection capacities remain unsuffixed syntax. |
| `scalar literal ... is outside ... range` | A scalar literal cannot fit in its declared fixed-width integer type. | Use a value in range or choose a wider explicit scalar type. |
| `scalar operands must have the same type` / `scalar literal ... has type ..., expected ...` | A scalar operator mixes widths or signedness. | Use matching scalar types; explicit casts and inference are not part of the buildable surface. |
| `scalar arithmetic result ... is outside ... range` | Concrete scalar arithmetic overflows or underflows the target type. | Keep the computation in range or choose a wider explicit scalar type. |
| `scalar division by zero` / `scalar modulo by zero` | A concrete or runtime scalar operation divides or takes modulo by zero. | Guard the zero case before the operation or use a non-zero divisor. |
| `process-reference equality is not supported` | A `==` or `!=` expression compares process-reference authority. | Keep process references as explicit authority handles; do not branch on reference identity. |
| `list and map equality are not supported` | A `==` or `!=` expression tries to compare a collection type. | Compare an explicit `Bool` or payload-free enum predicate instead. |
| `record equality is not supported` | A `==` or `!=` expression tries to compare a record value. | Compare an explicit `Bool` or payload-free enum predicate instead. |
| `equality type ... must not declare payload-bearing enum variants` | A `==` or `!=` expression targets payload data instead of a safe built-in outcome variant pattern. | Use a payload-free enum, compare a safe pattern such as `Ok(Unit)` or `Err(Exhausted(Unit))`, or use an explicit function/match shape rather than payload equality. |
| `requires one operand to be a safe built-in variant pattern` | A `==` or `!=` expression compares two runtime-dependent `Option`, `Result`, `SendError`, or `SpawnError` values directly. | Compare the runtime value against a safe variant pattern such as `Ok(Unit)`, `None`, or `Err(Full(Work))`; do not compare two payload-carrying runtime values structurally. |
| `boolean ! operand must produce Bool` | A `!` predicate operand resolves to a non-Bool value. | Apply `!` only to `Bool`, typed equality, scalar ordering, or nested Boolean predicate expressions. |
| `left operand of && must produce Bool` / `right operand of || must produce Bool` | A `&&` or `||` operand resolves to a non-Bool value; the diagnostic names the failing operator. | Compose only `Bool`, typed equality, scalar ordering, or nested Boolean predicate expressions. |
| `boolean predicate expression produces Bool, expected ...` | A composed predicate is used where a non-Bool value is required. | Use predicate composition only in Bool positions such as conditions or Bool fields. |
| `parenthesized value operand must produce ...` | A parenthesized expression does not match the expected source value type. | Keep the grouping expression typed to the surrounding value position. |
| `function ... declares duplicate pattern for variant ...` | More than one source function clause handles the same constructor. | Keep one clause per constructor. |
| `function ... must handle variant ...` | A source function signature pattern group or match body is non-exhaustive. | Add the missing constructor clause/arm or one `_` fallback. |
| `function ... wildcard pattern is unreachable` | Explicit source function clauses already cover every variant. | Remove the wildcard clause or remove the explicit clauses it should cover. |
| `pattern ... overlaps an earlier pattern for the same typed payload shape` | A function match or return-match repeats a constructor with an identical, unguarded, or not-provably-disjoint nested predicate. | Keep one unguarded constructor arm, or split the constructor only by disjoint nested enum predicates. |
| `match has no matching pattern for ...` / `return match has no matching pattern for ...` | A function call reached a concrete nested payload shape not covered by the function match arms. | Add a disjoint nested predicate arm for that shape or add one `_` fallback where fallback behavior is intended. |
| `record pattern ... has no field ...` | A source function record pattern names a field outside the matched record. | Bind a declared field from the record. |
| `record pattern ... binds field ... more than once` | A source function record pattern repeats one field. | Bind each record field at most once. |
| `record pattern binding ... is declared more than once` | A source function record pattern binds two fields to the same local name. | Use one distinct immutable binding name per field. |
| `record pattern binding ... conflicts ...` | A source function record pattern binding reuses a reserved, process, process-reference binding, source function, type, or constructor name. | Choose a distinct immutable binding name. |
| `requires a concrete record value argument` | A record destructuring function or function match is trying to destructure a value that is not concrete after source function expansion. | Pass a concrete record value into the function or match a source binding that resolves to one. |
| `requires a concrete list value argument` / `requires a concrete map value argument` | A collection destructuring function or function match is trying to destructure a value that is not concrete after source function expansion. | Pass a concrete `List[...]` or `Map[...]` value into the function, or match a source binding that resolves to one. |
| `map pattern duplicates key ...` / `map value ... duplicates key ...` | A map pattern or map value repeats the same canonical key. | Keep each map key once. |
| `declares overlapping collection patterns ...` | Exact and rest/subset collection patterns could match the same concrete value. | Make list rest or map subset patterns disjoint, use a single exact pattern, or add a wildcard fallback for the non-overlapping remainder. |
| `list rest pattern must declare at least one prefix element` | A list rest pattern used `List[..tail]`, which would bind the original list without proving any element is present. | List at least one fixed-position element before `..tail`. |
| `list rest binding cannot be a wildcard` | A list rest pattern used `.._`, which would look like a binding while intentionally discarding the suffix. | Bind the suffix with `..tail`, or use an exact list pattern when no suffix value is needed. |
| `subset map pattern must declare at least one key` | A subset map pattern used `Map[..]`, which is equivalent to a map-specific catchall and binds nothing. | Use `_` for catchall behavior or list at least one static key before `..`. |
| `map rest pattern must declare at least one key` | A rest-binding map pattern used `Map[..rest]`, which would bind the original map without proving any key is present. | List at least one static key before `..rest`. |
| `map rest binding cannot be a wildcard` | A map rest pattern used `.._`, which would look like a binding while intentionally discarding the remainder. | Use `..` to ignore the remainder or `..rest` to bind it. |
| `map payload pattern keys must be static source values` / `map pattern keys must be static source values` | A map payload or function pattern tries to derive a key from a runtime binding such as current `state` or a payload value. | Use static source keys; model dynamic-key dictionaries separately once key-set IFC semantics exist. |
| `map value type ... keys must be static source values` | A runtime-bound map value tries to derive a map key from a payload or state binding. | Use static source keys; model dynamic-key dictionaries separately once key-set IFC semantics exist. |
| `collection pattern binding ... conflicts ...` | A list or map function pattern binding reuses an existing source value binding, process-reference binding, source function, or declared value name. | Choose a distinct immutable binding name. |
| `list payload pattern must bind at least one value` / `map payload pattern must bind at least one value` | A constructor payload pattern tries to use a collection shape test without binding any projected value. | Bind at least one immutable element/value, or use the message constructor without payload destructuring when the payload can be ignored. |
| `match record pattern ... must declare exactly one arm` | A source function whole-body match over a record tries to use enum-style multi-arm dispatch. | Use one record destructuring arm for the matched record type. |
| `match over record ... cannot use a wildcard pattern` | A source function whole-body match over a record tries to use `_`. | Use the record destructuring pattern for the matched record type. |
| `match record pattern binding ... conflicts ...` | A source function whole-body record match binding reuses an existing source value binding, process-reference binding, or source function name. | Choose a distinct immutable binding name. |
| `return match scrutinee ... must be a source value binding` | A function return-match tries to match a name that is not an in-scope immutable source value. | Match the function parameter or a payload binding introduced by an enclosing source match. |
| `return match must handle variant ...` | A function return-match is non-exhaustive. | Add the missing constructor arm or one `_` fallback. |
| `return match record pattern ... must declare exactly one arm` | A function return-match over a record tries to use enum-style multi-arm dispatch. | Use one record destructuring arm for the matched record type. |
| `return match record pattern binding ... conflicts ...` | A function return-match record binding reuses an existing source value binding, process-reference binding, or source function name. | Choose a distinct immutable binding name. |
| `payload ... has type ..., expected ...` | A source function or step payload binding annotation does not match the constructor payload type. | Use the declared payload type. |
| `match payload binding ... conflicts ...` | A source function match arm reuses a parameter, process-reference binding, or source function name for a payload binding. | Use a distinct immutable payload binding name. |
| `value ... is not a variant of enum ...` | A payload-constructor expression names a constructor outside the expected enum. | Use a constructor from the expected enum or call a declared function. |
| `enum variant ... requires a payload` / `does not accept a payload` | A payload-bearing constructor was used as a fieldless value, or a fieldless constructor was called with a payload. | Match the constructor's declared payload shape. |

## Message Handling Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `step pattern message ... is not accepted` | A `step` pattern names a message constructor outside the process message enum. | Use a declared message constructor. |
| `duplicate step pattern for message` | A message variant has more than one explicit `step` clause. | Keep one explicit clause per variant. |
| `duplicate wildcard step pattern` | More than one `step` clause uses `_`. | Keep one wildcard clause. |
| `wildcard step pattern is unreachable` | Explicit clauses already cover every accepted message variant or every discovered concrete payload case the wildcard could cover. | Remove the wildcard clause or remove an explicit clause that it should cover. |
| `must declare step pattern for message` | A message variant is not covered by an explicit or wildcard `step` clause. | Add a `step` clause for the missing message or add one `_` clause. |
| `declares a wildcard step pattern with a payload-sensitive state match step pattern` / `declares payload-sensitive step pattern ... with a state match wildcard step pattern` | A payload-sensitive state-match split is mixed with a non-state-match wildcard fallback, or a state-match wildcard fallback is mixed with a non-state-match payload-sensitive clause. | Use state-match bodies for both the explicit payload-sensitive clauses and the wildcard fallback, use supported step-signature or `match msg` fallback where that surface is intended, or remove the wildcard or payload predicate. |
| `step pattern ... overlaps an earlier pattern` / `state match step pattern ... overlaps an earlier pattern` | Two step patterns can match the same message payload shape. | Keep one unguarded clause or make the nested payload predicates exact and disjoint. |
| `must declare step pattern for message ... payload ...` | A payload-sensitive step split omits a discovered concrete payload case. | Add an explicit step clause for the missing payload case; on step-signature, state-match, or `match msg` surfaces, one `_` fallback may cover discovered remainder cases when the fallback shape is supported. |
| `step pattern ... has no discovered payload case` | A step payload predicate does not correspond to a discovered concrete payload case. | Use a concrete payload case that the checker can discover from sends and constructors. |
| `payload-sensitive step pattern for message ... has no discovered payload case for wildcard fallback` / `payload-sensitive state match step pattern for message ... has no discovered payload case for wildcard fallback` | A wildcard fallback is paired with payload-sensitive step signatures or state-match clauses, but the checker did not discover any concrete payload case for the wildcard to lower. | Send or construct the concrete payload case before it is handled, remove the fallback, or use explicit discovered payload cases. |
| `match body must be the whole function body` | A `match msg` appears after another statement or has trailing body statements. | Use one whole-body `match msg` form or step parameter patterns. |
| `match expressions are only supported ...` | A general `match` is used inside a value expression such as a result constructor argument. | Use a supported whole-body `match`, `return match`, step parameter pattern, or function return-match form. |
| `match step must declare a typed message parameter` | A match `step` uses a parameter pattern instead of `msg: MsgType`. | Use `fn step(state: StateType, msg: MsgType)`. |
| `match scrutinee ... must be the step message parameter` | The `match` scrutinee is not the typed message parameter. | Match the declared message parameter, usually `match msg`. |
| `state match step must use a match body` | A `match state` step was parsed in a non-match body shape. | Make `match state { ... }` the whole step body. |
| `state match pattern ... requires a payload binding` | A payload-bearing state variant is matched without binding its payload. | Write the arm as `Variant(name: PayloadType)`. |
| `state match pattern ... does not carry a payload` | A fieldless state variant was matched with a payload binding. | Remove the binding from the fieldless variant arm. |
| `state match payload ... has type ..., expected ...` | A state payload binding annotation does not match the state variant payload type. | Use the declared state variant payload type. |
| `state payload binding ... conflicts with message payload binding` | A `match state` arm reuses the enclosing message payload binding name. | Give the state payload binding its own transition-local name. |
| `message parameter ... has type ..., expected ...` | The typed message parameter is not the process `Msg` type. | Use the process message type in the second parameter. |
| `cannot mix match step bodies with step parameter patterns` | One process mixes `match msg` dispatch with parameter-pattern or state-match dispatch. | Use either parameter-pattern/state-match clauses or one `match msg` body for the process. |
| `sends message ... not accepted by ...` | The target process message enum has no such variant. | Send a declared target message variant. |
| `message ... requires a payload` | A send omits the payload for a payload variant. | Pass one value with `send worker Variant(value);`. |
| `message ... does not accept a payload` | A send passes a payload to a unit variant. | Remove the payload argument or send a payload variant. |
| `payload type ... must be a named record, enum, list, map, or process reference type` | A payload variant uses an unsupported applied/generic type. | Declare a named record or enum type, use `List<T,N>` / `Map<K,V,N>` over source values, or use `ProcessRef<TargetProcess>`. |
| `payload type ... must declare exactly one target process` | A direct `ProcessRef` payload declaration has the wrong arity or a const argument. | Declare process-reference payloads as `ProcessRef<TargetProcess>`. |
| `step pattern payload ... has type ... expected ...` | A step payload binding annotation does not match the variant payload type. | Use the declared payload type in the parameter pattern. |
| `payload binding ... conflicts` / `process reference ... conflicts with payload binding` | A local immutable binding shadows `state`, a process, a type, a value constructor, or another local binding in the same transition. | Use distinct immutable binding names. |
| `payload has type ..., expected ...` | A runtime envelope or artifact payload template carries the wrong value type. | Match the payload value type to the target message variant. |
| `payload ... exceeds maximum length` | A payload value label is too large for the artifact or runtime trace boundary. | Use a smaller payload value or split the payload into smaller fields/messages. |
| `payload ... is not a bound process reference` | A `ProcessRef<T>` payload send uses a value that is not a process reference. | Pass an immutable process reference binding or received `ProcessRef<T>` payload. |
| `contains a process reference; process references must be direct message payloads` | A record field or collection type tries to make process references general source values. | Keep `ProcessRef<T>` as the direct payload type of a message variant, then forward that received reference directly. |
| `process references must be direct message payloads` / `process reference template must be a direct message payload` | A process reference payload is nested inside a record, enum, collection, or next-state template. | Send `ProcessRef<T>` only as the direct payload of a message that declares `ProcessRef<T>`. |

## Match Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `match scrutinee ... is not a fieldless enum variant` | An `init` match uses a scrutinee that is not a fieldless enum constructor. | Match a declared fieldless enum constructor. |
| `match pattern ... is not a variant of enum ...` | A match arm names a constructor outside the scrutinee enum. | Use a constructor from the matched enum. |
| `init match must handle variant` | An `init` match is non-exhaustive. | Add an arm for the missing variant or one `_` arm. |
| `init match declares duplicate pattern` | More than one arm handles the same constructor. | Keep one arm per constructor. |
| `init match wildcard pattern is unreachable` | Explicit arms already cover the matched enum. | Remove the wildcard arm or remove the explicit arms it should cover. |
| `init match pattern ... does not carry a payload` | A fieldless constructor pattern tries to bind a payload. | Remove the binding. |
| `init match arm cannot use payload binding ... in returned state` | An `init` match arm tries to materialize a payload binding even though `init` matches lower to a static initial state. | Return a concrete whole state value from each `init` match arm. |

## State Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `value ... is not a variant of enum ...` | A returned enum value does not belong to the expected enum. | Return a variant from the process state enum. |
| `record constructor ... does not match expected record ...` | A record value constructor does not match the expected state type. | Construct the expected record type. |
| `record value fields use ':'` | A record value used assignment syntax. | Use `field: value`, not `field = value`. |
| `process reference payloads are not valid state values` / `process reference templates are not valid next-state values` | A state value or next-state template tries to embed runtime process authority. | Keep process references in direct message payloads; process states must be immutable data values. |
| `state value state conflicts` | A state enum variant is named `state`. | Rename the variant. |
| `current state payload template requires a payload-bearing state` | An artifact or checked transition uses a state-payload template without a payload-bearing current state guard. | Ensure the transition is keyed by a checked payload-bearing state value. |
| `current_state id ... is not a valid state value` / `is not a loaded state value` | An artifact transition references a current state outside the loaded state table. | Emit only typed state IDs from lowering; reject or regenerate invalid artifacts. |

## Process And Mailbox Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `spawns itself` | A process tries to spawn itself. | Spawn another declared process. |
| `authority type must be Cap<Spawn<ProcessName>>` | A process authority declaration uses a malformed capability wrapper. | Declare local spawn authority as `authority name: Cap<Spawn<TargetProcess>>;`. |
| `authority descriptor must be Spawn<ProcessName>` | A `Cap<...>` authority declaration does not contain the supported spawn descriptor. | Use `Cap<Spawn<TargetProcess>>`; other capability algebra is not admitted in this slice. |
| `spawn authority target must be a process name` | The spawn descriptor target is not a bare process name. | Target a declared process directly, for example `Cap<Spawn<Worker>>`. |
| `spawn authority targets entry process` | A process authority declaration tries to create the already-started entry process. | Target a non-entry worker process. |
| `spawn target ... requires authority Cap<Spawn<...>>` | A step uses local dynamic `spawn` without exact authority for that target process. | Add a process-local `authority` declaration for the exact target, or remove the spawn. |
| `duplicates spawn authority descriptor` | A process declares the same spawn capability more than once. | Keep one declaration per exact target process. |
| `declares unused spawn authority` | A process declares spawn authority that no local spawn site uses. | Remove the unused declaration or add the corresponding local spawn. |
| `conflicts with a process declaration` | A process reference uses the same name as a process definition. | Use a distinct reference name. |
| `undeclared process reference` | A send references a name that is never spawned in the process. | Add a matching `let worker: ProcessRef<Worker> = spawn Worker;` statement. |
| `send target ... is not a process reference payload` | A send target names a payload binding whose type is not `ProcessRef<T>`. | Send through a process reference binding or a received `ProcessRef<T>` payload. |
| `unbound process reference` | A transition sends through a reference before it is bound. | Spawn the reference before sending through it. |
| `duplicates process reference id` | A transition binds the same reference twice. | Use two distinct references or bind once. |
| `mailbox would exceed bound` | A send would overflow the target mailbox. | Increase the mailbox bound or send fewer messages before the target runs. |
| `would retain ... unhandled message` | A process can stop while messages remain in its mailbox. | Continue until queued messages are handled or avoid queuing them. |
| `mailbox_bound must be no greater than` | The mailbox bound exceeds the validated limit. | Lower the bound. |

## Runtime Errors

Runtime errors are emitted by Mantle after artifact admission starts. Common
causes include invalid artifacts, blocked trace paths, mailbox exhaustion,
explicit `Panic(...)` transition results, trace size exhaustion, and dispatch
budget exhaustion.

Use the source gate first:

```sh
just strata-check path/to/program.str
just strata-build path/to/program.str
```

Then run Mantle:

```sh
just mantle-run target/strata/program.mta
```

If source checking passes but Mantle rejects an artifact, inspect the artifact
and runtime boundary docs before changing runtime behavior.
