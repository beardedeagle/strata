# Examples

Runnable Strata examples live under `examples/`.

Read them in this order:

1. `hello.str` for the minimum source-to-runtime program.
2. `actor_ping.str` for spawning, sending, and a single worker transition.
3. `actor_sequence.str` for multiple messages and message-keyed transitions.
4. `actor_match.str` for whole-body match authoring that checks
   into typed message-keyed transitions.
5. `init_match.str` for whole-body match authoring in `init`.
6. `init_return_match.str` for pure init return-match expressions.
7. `function_match.str` for module functions, process-local functions, and
   pattern matching outside actor dispatch.
8. `function_payload_match.str` for payload-bearing enum construction and
   matching in normal source functions.
9. `function_if_else.str` for pure value-level conditionals selected before
   lowering.
   See also `function_local_bindings.str` for sequential immutable source-local
   computation in normal source functions.
10. `function_collection_match.str` for immutable list/map source values and
   collection patterns in normal source functions.
11. `function_return_match.str` for function return-match expressions.
12. `process_return_match.str` for process step return-match expressions with
    uniform effect prefixes.
13. `process_return_match_arm_prefix.str` for selected step return-match
    arm action prefixes.
14. `process_return_match_arm_runtime_if_prefix.str` for selected
    step return-match arm-local runtime branch prefixes.
15. `process_return_match_arm_for_prefix.str` for selected step return-match
    arm-local bounded runtime loop prefixes.
16. `process_return_match_arm_for_if_prefix.str` for selected step return-match
    arm-local bounded runtime loop prefixes with loop-body runtime branch
    actions.
17. `process_return_match_arm_if_for_prefix.str` for selected step return-match
    arm-local runtime branch prefixes with bounded runtime loop branch actions.
18. `process_return_match_arm_action_block.str` for selected step return-match
    arms using ordinary action-block sequencing with multiple runtime branches
    and bounded loops.
19. `function_record_pattern.str` for source function record destructuring
   patterns.
20. `function_record_return_match.str` for function return-match record
   destructuring.
21. `function_record_body_match.str` for whole-body function match record
   destructuring.
22. `state_payload_enum.str` for payload-bearing process state enum transitions.
23. `collection_state.str` for immutable collection state and payload-dependent
   collection next-state templates.
24. `state_payload_match.str` for matching immutable current process state
   payloads.
25. `actor_instances.str` for multiple runtime instances of one process
   definition.
26. `actor_payloads.str` for typed message payloads and immutable payload
   bindings in actor step parameter patterns.
27. `runtime_if_else.str` for Mantle-backed runtime branching over a message
   payload.
28. `runtime_scalar_priority.str` for typed scalar payload computation,
   runtime-bound value conditionals, and Mantle-backed scalar branch selection.
29. `runtime_payload_projection_if.str` for Mantle-backed runtime branching over
   a projected field from an immutable received record payload.
30. `runtime_payload_projection_next_state.str` for Mantle-backed runtime
   next-state branching over a projected field from an immutable received record
   payload.
31. `runtime_state_payload_projection_if.str` for Mantle-backed runtime
   branching over a projected field from an immutable current-state record
   payload.
32. `runtime_state_payload_projection_next_state.str` for Mantle-backed runtime
   next-state branching over a projected field from an immutable current-state
   record payload.
33. `runtime_nested_if_actions.str` for one bounded layer of nested
   statement-level runtime branch actions.
34. `runtime_final_if_guarded_loop.str` for bounded loop action prefixes inside
   final-position runtime branches.
35. `runtime_final_if_nested_if_actions.str` for one direct nested
   statement-level runtime branch action inside final-position runtime
   branches.
36. `runtime_final_if_nested_terminal_if.str` for one direct nested terminal
   final-position runtime branch inside final-position runtime branches.
37. `runtime_guard_noop.str` for omitted `else` and explicit no-op runtime
   branch behavior.
38. `runtime_for_each.str` for Mantle-backed bounded runtime iteration over a
   typed list payload.
39. `runtime_for_each_empty.str` for the zero-iteration runtime collection case.
40. `runtime_for_each_if.str` for Mantle-backed runtime branch selection inside
   bounded loop bodies.
41. `runtime_for_each_nested_if_actions.str` for one bounded nested runtime
   branch inside a bounded loop-body branch.
42. `runtime_guarded_for_each.str` for guarding a whole bounded runtime loop.
43. `runtime_guarded_ref_loop.str` for routing a guarded bounded loop through a
   received direct process reference.
44. `runtime_guarded_ref_loop_jobs.str` for routing ordinary immutable `Job`
   values through a guarded loop and received direct process reference.
45. `runtime_loop_element_projection.str` for projecting immutable record
   fields from guarded runtime loop elements.
46. `actor_payload_match.str` for the same payload binding through a whole-body
   `match msg`.
47. `actor_payload_split_match.str` for payload-sensitive same-message
   splitting inside a whole-body `match msg`.
48. `actor_payload_split_signature.str` for payload-sensitive same-message
   splitting across step parameter patterns.
49. `actor_payload_split_signature_wildcard.str` for payload-sensitive
   step-signature wildcard fallback over discovered concrete payload cases.
50. `actor_payload_state_match_split.str` for payload-sensitive same-message
   splitting across state-match step clauses.
51. `actor_payload_state_match_wildcard.str` for payload-sensitive state-match
   wildcard fallback over discovered concrete payload cases.
52. `nested_patterns.str` for nested immutable constructor, record, list, and
   map payload destructuring.
53. `actor_reply.str` for transporting typed process references through message
   payloads.
54. `actor_emit_spawn_send.str` for one transition with declared emit, spawn,
   and send authority.
55. `imports_main.str`, with `imports_types.str` and `imports_worker.str`, for
   source-unit imports checked and lowered from one root source path.
56. `boundary_contracts_main.str`, with `boundary_contracts_worker.str`, for
   typed protocol, port, and component boundary contracts.
57. `component_composition_main.str`, with `component_composition_worker.str`,
   for checked local component-instance and port-binding composition admission.
58. `effect_outcomes.str` for immutable typed local send/spawn outcomes,
   commit-or-return state evidence, and the `MailboxClosed` send-error contract
   shape.
59. `effect_outcome_mailbox_full.str` for a source-visible `Full` send outcome.
60. `effect_outcome_stopped_target.str` for a source-visible `Stopped` send
   outcome.
61. `effect_outcome_crashed_target.str` for the fail-closed boundary where a
   source-created `Panic(...)` prevents a later observer from recovering the
   crash as a source-visible send outcome.
62. `effect_outcome_spawn_denied.str` for source-visible local spawn authority
   denial before process acceptance.
63. `actor_panic_no_replay.str` for fail-closed actor failure and no replay
   after message dequeue.
64. `local_supervision_restart.str`, `local_supervision_permanent_stop.str`,
   `local_supervision_temporary.str`, `local_supervision_transient_restart.str`,
   `local_supervision_transient.str`, and
   `local_supervision_inactive_send_outcome.str` for local `one_for_one`
   supervision, lexical child sends, restart modes, stopped-child send outcomes,
   and restart observability.

Detailed notes are grouped by topic:

- [Basic examples](examples-basic.md)
- [Function and return-match examples](examples-functions.md)
- [Runtime and actor examples](examples-runtime.md)
- [Import, boundary, and failure examples](examples-boundary-failure.md)
