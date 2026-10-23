# Backend verification handoff

Scope: Rust backend for node-instance bindings, 2026-10-23. Independent in-memory and temporary file databases only. No original port 11221 or user database touched. No production code changes were required by these tests.

Changes:
- `db/assets/tests.rs`: old creation fixtures now seed persistent NetworkConfig records and use v2 previews. Legacy IPv4/IPv6 fixtures use explicit pre-upgrade SQL records; existing CRUD/foreign-key/LAN-uniqueness/service-port/delete-cascade/idempotency coverage retained.
- `db/assets/binding_helper_tests.rs`: 15 focused address, rule, lifecycle, preview-version tests, registered in assets/mod.rs.
- `restful/assets/tests.rs`: v2 fixture upgrade and four additional actual-router authorization tests. Scope spans candidate, binding/address preview, creation, binding/address writes, target-owner admin and immediate revocation, cross-node isolation, webhook unavailability.
- Nine additional DB workflow tests: persistent catalog and Core implicit DHCP/pool fingerprint; central allocated IP scope/precedence; stale previews and gateway occupancy scope; missing/recreated/role-changed sources with assets/services retained and parent/child confirmations; explicit legacy attachment and IPv6-LAN conversion refusal; corrupt partial snapshots and parent-review isolation; injected transaction failure rollback; schema down/up/repeated up preserving IPv6/IDs/services/FKs; persisted attach/refresh/address operation replay after close/reopen and configuration deletion.

Commands/results:
1. `cargo test --locked -p easytier-web assets:: --no-run` passed (initial production compile).
2. `cargo test --locked -p easytier-web db::assets:: -- --nocapture`: 27 passed.
3. Expanded compile initially failed E0282 in new fixture; explicit Uuid annotation fixed it.
4. Expanded assets run: 41 passed, two new assertions used wrong contract names. Corrected exact assertions to `changed` + `binding_role_mismatch` and existing `request_payload_changed` (per v2 contract), without changing implementation or reducing coverage.
5. First full package run: 338 passed, 1 failed, 2 already-ignored stress tests. The new persistence fixture used VACUUM INTO from a memory pool; reopened copy contained only initialization data. Replaced this fixture with a database created on disk from the start and explicit pool close/reopen, matching production restart behavior and the existing durable replay test.
6. Corrected durable replay targeted test passed.
7. Final `cargo test --locked -p easytier-web`: **339 passed, 0 failed, 2 existing ignored**, 48.84 seconds; evidence in `backend-full-final.log`.

Existing ignored tests: `client_manager::listener_tests::pending_handshakes_are_bounded_and_release_capacity` (4097 TCP connections) and `client_manager::tests::receiver_takeover_wave_allows_an_independent_mount` (1000 real management connections). No test was newly ignored.

T13 coverage from Rust: V02-V17 largely covered for catalog, pure derivation, repository transactions and actual HTTP authorization; V09/V22 schema down/up verified on isolated data. V01/V18-V21 page behavior belongs to frontend/API/E2E agents. Existing whole-package tests cover config-server registration and core existing operations. External OIDC providers, real user hardware routes, public forwarding, and a malicious remote RPC IPv4-prefix response were not exercised by these new Rust tests. Runtime observation failure handling is inspected, not equivalent to adversarial remote injection.
