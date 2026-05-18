# Diagnostics

Strata diagnostics are intended to reject invalid source close to the layer
that can explain it. Parser errors describe source shape. Checker errors
describe semantic rules. Runtime errors describe admitted execution failures.

## Reading A Diagnostic

Run:

```sh
cargo run -p strata --bin strata -- check examples/hello.str
```

If checking fails, fix the first reported error first. Later errors may be a
result of the first invalid shape.

## Common Source Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `expected record, enum, function, or proc declaration` | A top-level item is not accepted. | Use `record`, `enum`, `fn`, or `proc` after `module`. |
| `entry process Main is not declared` | The program has no `Main` process. | Add `proc Main ...`. |
| `uses reserved prefix __strata_checked_` | A source type name collides with internal checked type metadata. | Rename the source type without the reserved prefix. |
| `checked type_count exceeds Mantle artifact limit` | The checked program needs more distinct artifact types than Mantle admits. | Reduce the number of distinct state, message, payload, and process-reference types. |
| `process ... must declare type State` | A process is missing its state alias. | Add `type State = StateType;`. |
| `process ... must declare type Msg` | A process is missing its message alias. | Add `type Msg = MessageEnum;`. |
| `init must declare no parameters` | `init` has parameters. | Use `fn init() -> StateType ...`. |
| `init body must not perform statements` | `init` uses `emit`, `spawn`, or `send`. | Return only the initial state. |
| `match scrutinee ... fieldless enum variant` | An `init` whole-body match or `init return match` tries to match a non-constructor name or a payload-bearing constructor. | Match one fieldless enum constructor in this init slice. |
| `init return match ...` | An `init return match` is non-exhaustive, overlaps an earlier arm, has an unreachable wildcard, or nests another return match in an arm. | Cover each variant once or use one reachable `_`, and return one whole state value from each arm. |
| `init return match arm cannot use payload binding ... in returned state` | An `init return match` arm tries to materialize an arm payload binding into the initial state. | Return a whole state value that does not depend on match-arm payload bindings in this init slice. |
| `step must declare state parameter and message pattern` | `step` has the wrong parameter count. | Use `state: StateType, MessageConstructor` or `state: StateType, _`. |
| `step second parameter must be a message constructor pattern or wildcard pattern` | The second `step` parameter is a typed binding instead of a message pattern. | Replace `msg: MsgType` with a message constructor or `_`, or use a whole-body `match msg`. |
| `step returns ..., expected ProcResult<...>` | `step` return type is wrong. | Return `ProcResult<StateType>`. |
| `step return match scrutinee ... must be a concrete enum source value binding` | A `step return match` tries to match `state`, a non-enum value, or a value that is not an immutable source binding. | Match a transition-local enum payload or state-payload binding whose concrete value is already proven by the checker, or use whole-body `match msg` / `match state`. |
| `step return match scrutinee ... requires a discovered concrete message payload case` | A `step return match` tries to match a message payload binding from an unguarded transition. | Use a payload-sensitive pattern that lowers to exact discovered payload guards, or move the dispatch to whole-body `match msg`. |
| `step return match arms must not perform statements` | A `step return match` arm performs `emit`, `spawn`, or `send`, which would make effects branch-local. | Move effects that are uniform across every selected arm before `return match`, or split the behavior into explicit step clauses / whole-body dispatch. |
| `step return match arm must return Stop..., Continue..., or Panic...` | A `step return match` arm returns a bare state value or another unsupported result form. | Return one whole state value inside `Continue(...)`, `Stop(...)`, or `Panic(...)` from every arm. |
| `step body must return Stop..., Continue..., or Panic...` | A `step` returns a bare state value or an unsupported result form. | Return one whole state value inside `Continue(...)`, `Stop(...)`, or `Panic(...)`. |
| `step may-behaviors must be empty` | The `~ [...]` list is not empty. | Use `~ []`. |
| `step must be deterministic` | `step` uses `@nondet`. | Use `@det`. |
| `uses effect ... but does not declare it` | The body performs `emit`, `spawn`, or `send` without matching effect authority. | Add the exact used effect to `! [...]` or remove the statement. |
| `declares effect ... but does not use it` | The effect list is wider than the body. | Remove the unused effect. |
| `declares duplicate effect` | The effect list repeats one authority. | Keep each effect at most once. |
| `for loop collection must be an identifier binding` | A runtime `for` loop tries to iterate a literal or computed value. | Iterate an immutable runtime collection binding such as a typed message payload binding. |
| `for loop collection ... must have type List<T,N>` | A runtime `for` loop source is not a typed list collection. | Use a binding whose type is `List<Element,N>`. |
| `for loop collection must be a runtime list binding` | A `for` loop source is static source data rather than runtime data. | Pass the list as a typed runtime payload or state-derived runtime binding. |
| `for loop body cannot bind process reference` | A loop body tries to create new authority with `spawn`. | Bind process references before the loop and use only admitted linear loop body effects. |
| `nested for loops are not supported` | A loop body contains another loop. | Flatten the runtime payload shape or split the behavior into separate admitted steps for this slice. |
| `assignment statements are not supported` | Source code uses assignment-style mutation. | Bind immutable values through declarations or return a whole replacement state value. |
| `statement-level if branches must not return` | An effect-only runtime branch tries to terminate the step. | Put the final `return` after the statement-level branch, or use final-position runtime `if` when each branch must return. |
| `statement-level if branches cannot bind process references` | A branch tries to introduce branch-local authority with `spawn`. | Bind process references before the branch and use only admitted branch effects. |
| `statement-level if branches cannot contain for loops` | A branch tries to introduce nested loop control flow. | Keep bounded loops outside the branch for this slice. |
| `nested statement-level if branches are not supported` | A statement-level runtime branch contains another branch. | Split the behavior into separate steps or keep one branch level for this slice. |
| `statement-level if branches must contain at least one effect statement` | A branch is empty. | Add an `emit` or `send` branch effect, or remove the no-op branch until no-op branches are admitted. |
| `runtime if branch cannot bind process references` | A checked or admitted runtime branch tries to introduce branch-local authority. | Bind process references before the branch and keep branch bodies to declared effects. |
| `runtime if branch cannot contain nested runtime if actions` | A checked runtime branch contains another runtime branch. | Keep runtime branch actions single-level for this slice. |
| `runtime if branch cannot contain for loop actions` | A checked or admitted runtime branch contains loop control flow. | Move the loop outside the branch for this slice. |
| `for loop branch actions must not be empty` | A decoded or constructed artifact tries to admit a no-op branch inside a runtime loop. | Emit a real branch effect or keep the no-op branch out of the artifact until no-op loop branches are admitted. |

## Source Function Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `function ... conflicts with a declared type or value constructor` | A source function name collides with a type or enum constructor. | Choose a distinct function name. |
| `function ... must declare exactly one parameter` | A normal source function uses an arity outside the current buildable call form. | Use one typed binding parameter or one pattern parameter clause. |
| `function ... must use a declared record, enum, list, or map type` | A source function parameter or return type names something outside the source value type set. | Use a declared `record` or `enum` type, or `List<T,N>` / `Map<K,V,N>` over source value types. |
| `function ... must not declare effects` / `function ... must not perform statements` | A normal source function tries to perform runtime behavior. | Keep normal functions pure; perform `emit`, `spawn`, and `send` only in `step`. |
| `function ... may-behaviors must be empty` / `function ... must be deterministic` | A normal source function is not in the deterministic buildable subset. | Use `~ [] @det`. |
| `function ... is not declared` | A value expression calls an unknown function. | Declare a module function or process-local helper with that name. |
| `function ... returns ..., expected ...` | The function return type does not match the value position where it is called. | Call a function returning the expected type or change the annotation. |
| `source function call cycle ... is not supported` | Source helper calls are recursive, but helpers are expanded before lowering and have no recursion model. | Remove the cycle; pass whole values through non-recursive helpers. |
| `if condition requires enum Bool { False, True }` | A source conditional is used without the explicit fieldless Bool contract. | Declare `enum Bool { False, True }`. |
| `if condition must have type Bool` | A source conditional condition resolves to a non-Bool source value. | Return or pass `True` or `False` from the declared `Bool` enum. |
| `if then branch must produce ...` / `if else branch must produce ...` | A conditional branch does not match the expected source value type. | Return the same source value type from both branches. |
| `if condition requires a concrete Bool value` | A pure source conditional condition remains runtime-bound after helper expansion. | Use a concrete Bool value for source-level `if`, or use an admitted runtime `if` form in a `step` body. |
| `if branches are pure value expressions and must not perform statements` | A conditional branch contains `emit`, `let`, `send`, or `return`. | Keep branch bodies to one source value expression and move effects to admitted `step` forms. |
| `equality operands must have the same type` | A `==` or `!=` expression compares values of different checked types. | Compare two `Bool` values or two fieldless values from the same enum. |
| `equality operands must be Bool or fieldless enum values` | A `==` or `!=` operand is outside the admitted equality surface. | Use `Bool` or a payload-free enum value; records, lists, maps, strings, and nested expressions are not equality operands in this slice. |
| `process-reference equality is not supported` | A `==` or `!=` expression compares process-reference authority. | Keep process references as explicit authority handles; do not branch on reference identity in this slice. |
| `list and map equality are not supported in this source slice` | A `==` or `!=` expression tries to compare a collection type. | Compare an explicit `Bool` or payload-free enum predicate instead. |
| `record equality is not supported in this source slice` | A `==` or `!=` expression tries to compare a record value. | Compare an explicit `Bool` or payload-free enum predicate instead. |
| `equality type ... must not declare payload-bearing enum variants` | A `==` or `!=` expression targets an enum that can carry payload data. | Use a payload-free enum for equality, or add an explicit helper/match shape rather than payload equality. |
| `boolean ! operand must produce Bool` | A `!` predicate operand resolves to a non-Bool value. | Apply `!` only to `Bool`, typed equality, or nested Boolean predicate expressions. |
| `left operand of && must produce Bool` / `right operand of || must produce Bool` | A `&&` or `||` operand resolves to a non-Bool value; the diagnostic names the failing operator. | Compose only `Bool`, typed equality, or nested Boolean predicate expressions. |
| `boolean predicate expression produces Bool, expected ...` | A composed predicate is used where a non-Bool value is required. | Use predicate composition only in Bool positions such as conditions or Bool fields. |
| `parenthesized predicate grouping produces Bool, expected ...` | Parentheses are used as value grouping around a non-Bool expression. | Use parenthesized grouping only for Bool predicates. |
| `function ... declares duplicate pattern for variant ...` | More than one source function clause handles the same constructor. | Keep one clause per constructor. |
| `function ... must handle variant ...` | A source function signature pattern group or match body is non-exhaustive. | Add the missing constructor clause/arm or one `_` fallback. |
| `function ... wildcard pattern is unreachable` | Explicit source function clauses already cover every variant. | Remove the wildcard clause or remove the explicit clauses it should cover. |
| `pattern ... overlaps an earlier pattern for the same typed payload shape` | A helper match or return-match repeats a constructor with an identical, unguarded, or not-provably-disjoint nested predicate. | Keep one unguarded constructor arm, or split the constructor only by disjoint nested enum predicates. |
| `match has no matching pattern for ...` / `return match has no matching pattern for ...` | A helper call reached a concrete nested payload shape not covered by the helper match arms. | Add a disjoint nested predicate arm for that shape or add one `_` fallback where fallback behavior is intended. |
| `record pattern ... has no field ...` | A source helper record pattern names a field outside the matched record. | Bind a declared field from the record. |
| `record pattern ... binds field ... more than once` | A source helper record pattern repeats one field. | Bind each record field at most once. |
| `record pattern binding ... is declared more than once` | A source helper record pattern binds two fields to the same local name. | Use one distinct immutable binding name per field. |
| `record pattern binding ... conflicts ...` | A source helper record pattern binding reuses a reserved, process, type, or constructor name. | Choose a distinct immutable binding name. |
| `requires a concrete record value argument` | A record destructuring helper or helper match is trying to destructure a value that is not concrete after source helper expansion. | Pass a concrete record value into the helper or match a source binding that resolves to one. |
| `requires a concrete list value argument` / `requires a concrete map value argument` | A collection destructuring helper or helper match is trying to destructure a value that is not concrete after source helper expansion. | Pass a concrete `List[...]` or `Map[...]` value into the helper, or match a source binding that resolves to one. |
| `map pattern duplicates key ...` / `map value ... duplicates key ...` | A map pattern or map value repeats the same canonical key. | Keep each map key once. |
| `declares overlapping collection patterns ...` | Exact and rest/subset collection patterns could match the same concrete value. | Make list rest or map subset patterns disjoint, use a single exact pattern, or add a wildcard fallback for the non-overlapping remainder. |
| `list rest pattern must declare at least one prefix element` | A list rest pattern used `List[..tail]`, which would bind the original list without proving any element is present. | List at least one fixed-position element before `..tail`. |
| `list rest binding cannot be a wildcard` | A list rest pattern used `.._`, which would look like a binding while intentionally discarding the suffix. | Bind the suffix with `..tail`, or use an exact list pattern when no suffix value is needed. |
| `subset map pattern must declare at least one key` | A subset map pattern used `Map[..]`, which is equivalent to a map-specific catchall and binds nothing. | Use `_` for catchall behavior or list at least one static key before `..`. |
| `map rest pattern must declare at least one key` | A rest-binding map pattern used `Map[..rest]`, which would bind the original map without proving any key is present. | List at least one static key before `..rest`. |
| `map rest binding cannot be a wildcard` | A map rest pattern used `.._`, which would look like a binding while intentionally discarding the remainder. | Use `..` to ignore the remainder or `..rest` to bind it. |
| `map payload pattern keys must be static source values` / `map pattern keys must be static source values` | A map payload or helper pattern tries to derive a key from a runtime binding such as current `state` or a payload value. | Use static source keys in this slice; model dynamic-key dictionaries separately once key-set IFC semantics exist. |
| `map value type ... keys must be static source values` | A runtime-bound map value tries to derive a map key from a payload or state binding. | Use static source keys in this slice; model dynamic-key dictionaries separately once key-set IFC semantics exist. |
| `collection pattern binding ... conflicts ...` | A list or map helper pattern binding reuses an existing source value binding or declared value name. | Choose a distinct immutable binding name. |
| `list payload pattern must bind at least one value` / `map payload pattern must bind at least one value` | A constructor payload pattern tries to use a collection shape test without binding any projected value. | Bind at least one immutable element/value, or use the message constructor without payload destructuring when the payload can be ignored. |
| `match record pattern ... must declare exactly one arm` | A helper whole-body match over a record tries to use enum-style multi-arm dispatch. | Use one record destructuring arm for the matched record type. |
| `match over record ... cannot use a wildcard pattern` | A helper whole-body match over a record tries to use `_`. | Use the record destructuring pattern for the matched record type. |
| `match record pattern binding ... conflicts ...` | A helper whole-body record match binding reuses an existing source value binding. | Choose a distinct immutable binding name. |
| `return match scrutinee ... must be a source value binding` | A helper return-match tries to match a name that is not an in-scope immutable source value. | Match the helper parameter or a payload binding introduced by an enclosing source match. |
| `return match must handle variant ...` | A helper return-match is non-exhaustive. | Add the missing constructor arm or one `_` fallback. |
| `return match record pattern ... must declare exactly one arm` | A helper return-match over a record tries to use enum-style multi-arm dispatch. | Use one record destructuring arm for the matched record type. |
| `return match record pattern binding ... conflicts ...` | A helper return-match record binding reuses an existing source value binding. | Choose a distinct immutable binding name. |
| `payload ... has type ..., expected ...` | A source helper or step payload binding annotation does not match the constructor payload type. | Use the declared payload type. |
| `match payload binding ... conflicts with an existing source value binding` | A source helper match arm reuses the helper parameter name for a payload binding. | Use a distinct immutable payload binding name. |
| `value ... is not a variant of enum ...` | A payload-constructor expression names a constructor outside the expected enum. | Use a constructor from the expected enum or call a declared helper. |
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
| `match expressions are only admitted ...` | A general `match` is used inside a value expression such as a result constructor argument. | Use an admitted whole-body `match`, `return match`, step parameter pattern, or helper return-match form. |
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
| `match scrutinee ... is not a fieldless enum variant` | An `init` match uses a scrutinee that is not a fieldless enum constructor in this source slice. | Match a declared fieldless enum constructor. |
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
| `current state payload template requires a payload-bearing state` | An artifact or checked transition uses a state-payload template without a payload-bearing current state guard. | Ensure the transition is keyed by an admitted payload-bearing state value. |
| `current_state id ... is not a valid state value` / `is not a loaded state value` | An artifact transition references a current state outside the admitted state table. | Emit only admitted state IDs from lowering; reject or regenerate invalid artifacts. |

## Process And Mailbox Errors

| Diagnostic Contains | Likely Cause | Fix |
| --- | --- | --- |
| `spawns itself` | A process tries to spawn itself. | Spawn another declared process. |
| `conflicts with a process declaration` | A process reference uses the same name as a process definition. | Use a distinct reference name. |
| `undeclared process reference` | A send references a name that is never spawned in the process. | Add a matching `let worker: ProcessRef<Worker> = spawn Worker;` statement. |
| `send target ... is not a process reference payload` | A send target names a payload binding whose type is not `ProcessRef<T>`. | Send through a process reference binding or a received `ProcessRef<T>` payload. |
| `unbound process reference` | A transition sends through a reference before it is bound. | Spawn the reference before sending through it. |
| `duplicates process reference id` | A transition binds the same reference twice. | Use two distinct references or bind once. |
| `mailbox would exceed bound` | A send would overflow the target mailbox. | Increase the mailbox bound or send fewer messages before the target runs. |
| `would retain ... unhandled message` | A process can stop while messages remain in its mailbox. | Continue until queued messages are handled or avoid queuing them. |
| `mailbox_bound must be no greater than` | The mailbox bound exceeds the admitted limit. | Lower the bound. |

## Runtime Errors

Runtime errors are emitted by Mantle after artifact admission starts. Common
causes include invalid artifacts, blocked trace paths, mailbox exhaustion,
explicit `Panic(...)` transition results, trace size exhaustion, and dispatch
budget exhaustion.

Use the source gate first:

```sh
cargo run -p strata --bin strata -- check path/to/program.str
cargo run -p strata --bin strata -- build path/to/program.str
```

Then run Mantle:

```sh
cargo run -p mantle-runtime --bin mantle -- run target/strata/program.mta
```

If source checking passes but Mantle rejects an artifact, inspect the artifact
and runtime boundary docs before changing runtime behavior.
