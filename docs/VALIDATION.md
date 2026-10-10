# Validation record

Performed on **2026-10-10** on macOS ARM64. Rust 1.99.0, Python 3.14.7, Node 25.8.2; Vite 7.3.7. Dependency versions are recorded in Cargo.lock and frontend/package-lock.json.

| Check | Result |
| --- | --- |
| `cargo test --locked` | 11 tests passed: transitions/input, exact ranking permutation, outage fallback, restart classification, dependency blocking, SQLite durability, process exits, output limits, timeout and cancellation |
| Python scheduling tests | 4 tests passed, including policy differences, deterministic ties, aging and invalid input |
| Python simulation tests | 3 tests passed: hand-computed timings, simultaneous events, seeded workloads and capacity/job conservation |
| `npm run build` | TypeScript strict type check and Vite production build passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Real Rust + Python integration | Passed all seven reported groups below |
| Browser smoke test | Passed creation, success/output display, SJF selection and running cancellation |
| Upstream MIT copy | SHA-256 matched the checked-out upstream LICENSE byte-for-byte |

Integration starts temporary services on dynamically selected loopback ports, uses a temporary database and provider registry, and stops processes on exit. It covers:

1. Actual demo subprocess execution, Python ranking, dependency ordering and captured output.
2. Registered external Python provider reading the versioned JSON stdin envelope.
3. Failure/blocked propagation, timeout and cancellation of queued/running work.
4. Rejection of unknown providers/dependencies, invalid estimates, command injection fields, foreign browser origins and non-loopback Host headers.
5. Worker concurrency ≤2 reconstructed from journal transitions, and event cursor pagination boundaries.
6. Python service termination followed by successful Rust FIFO fallback.
7. Graceful core restart preserving task snapshots and the journal.

The browser test used the actual three-service development stack. The development database contains two explicitly named verification tasks: one succeeded and one was cancelled. No LLM API or user credentials were used. The supplied [screenshot](browser-preview.jpg) captures the local UI after verification.

The benchmark ran seed 42 / 200 jobs / 4 workers over three scenarios and all four policies. JSON and CSV are saved under `benchmarks/results/sample-seed42.*`. Only the simulated scheduling outcomes and in-process Python ranking timing were measured. No original-Go-versus-Rust speed comparison was performed.

Not validated: Linux/Windows execution, model CLI integrations, production security, distributed behavior, process cleanup after SIGKILL/power loss, intentional provider session detachment, upstream data migration, multi-user use, or large-scale queues. The runtime group cleanup tests cover ordinary Unix descendants; restart classification is unit-tested without claiming exactly-once external side effects.

The local workspace sandbox required explicit execution approval for dependency downloads and loopback listeners. Those restrictions belong to the development environment; the project itself does not require privileged ports or administrator permissions.
