# Orchestra Rust MVP

A runnable local orchestration experiment inspired by [wayyoungboy/Orchestra](https://github.com/wayyoungboy/Orchestra), using a Rust execution core, a Python scheduling service, and a TypeScript frontend.

**This is a new, deliberately small implementation of one orchestration slice. It is not a full rewrite, a compatible replacement, or a measured performance improvement over Orchestra.** The Python service implements transparent heuristics; there is no trained AI/RL scheduler. Default agents are demo subprocesses, not LLMs.

## Run locally / 本地启动

Requirements: macOS or Linux, Rust **1.99+**, Python **3.11+**, Node **22.12+**, npm, a C compiler/linker, and `make`. SQLite is bundled with the Rust dependency. Python requires no third-party packages. Tested here on macOS ARM64, Rust 1.99.0, Python 3.14.7, Node 25.8.2. Linux has not been tested in this workspace.

From this project directory:

```bash
make setup       # Install locked Rust and frontend dependencies
make dev         # Start the core, scheduler and UI; Ctrl+C stops all three
```

Open **[http://127.0.0.1:5173](http://127.0.0.1:5173)**. Create a task, observe its status, and expand **Details & output** after it finishes. From another terminal in the same directory:

```bash
python3 scripts/demo.py  # Six demo tasks, including a dependency workflow
```

The current workspace already has dependencies installed. A Rust toolchain was installed only under the enclosing workspace's `work/toolchain/`; `scripts/env.sh` automatically finds it when `cargo` is absent. No shell profile was changed. If you move this project, install Rust normally or set `ORCHESTRA_TOOLCHAIN_DIR` to that directory. `make dev` rebuilds Rust; restart it after Rust/Python or proxy changes. TypeScript uses Vite hot reload.

`make dev` reserves ports **5173 / 8080 / 8091** and reports startup failures. Task data lives in ignored `data/orchestra.db`, its WAL files, and its advisory lock file. One core owns each database. Queued tasks resume on restart; previously running tasks become `interrupted` when their final outcome was not persisted. They are never retried automatically.

## Implemented scope

- Rust/Axum API and a single state-owning engine; SQLite snapshots and an atomic transition journal.
- Bounded worker concurrency (default 2), actual child processes, bounded stdout/stderr capture, nonzero-exit failures, execution timeouts, queued/running cancellation, and graceful shutdown.
- Immutable dependency edges to existing tasks. Execution starts after all dependencies succeed; failure/cancellation propagates as `blocked`.
- Python FIFO, shortest-job-first (SJF), priority, and priority-weighted aging policies. Rust validates the returned permutation and falls back to FIFO on timeout/error/invalid output.
- Typed TypeScript task creation, provider selection, policy switching, status counters, cancellation, dependency selection, output inspection, and a persistent event feed. Updates use one-second polling.
- Operator-configured external executable providers. A working text-analysis example demonstrates the JSON-over-stdin contract.
- Unit tests, real-service integration checks, and a seeded discrete-event benchmark using the exact same Python policy functions as the service.

There is no tmux/PTY terminal, interactive session recovery, ACP, model billing, credential store, chat routing, durable message outbox, multi-user authentication, distributed execution, learned scheduling model, or upstream data/API migration. Dependencies gate execution only; their outputs are not automatically injected into later prompts. Output is shown on completion, not streamed live. The frontend is new vanilla TypeScript; the original Vue UI has not been ported.

## Project map

```text
crates/orchestra-core/src/
  domain.rs       Task specification, validation and state transitions
  engine.rs       State ownership, dependency readiness and slot assignment
  runtime.rs      Provider registry, subprocess execution and cleanup
  scheduler.rs    Advisory HTTP client, response validation and FIFO fallback
  store.rs        SQLite snapshots + transition journal
  api.rs          Local REST API, input and browser-origin checks
  main.rs         Configuration, database ownership and service shutdown
scheduler/orchestra_scheduler/
  policies.py     Pure, deterministic policy functions
  server.py       Loopback HTTP adapter; standard-library only
frontend/src/     Typed API client, UI and styles
examples/         JSON-stdin provider example and registry template
benchmarks/       Simulator, workload generator, tests and recorded sample results
scripts/          Development launcher, demo workflow, integration checks
docs/             Source inspection, architecture, contracts and validation report
licenses/         Exact upstream MIT license copy
```

## Verify and benchmark

```bash
make test         # Rust + Python unit tests and TypeScript production build
make integration  # Temporary real services and DB; cleans up on exit
make bench        # 200 jobs, 4 workers, seed 42, 3 workload scenarios

# Alternative benchmark configuration; outputs JSON and CSV
python3 benchmarks/compare.py --seed 123 --jobs 500 --workers 8 \
  --output benchmarks/results/local-seed123.json

# Optional Rust style/lint checks (install rustfmt/clippy first if needed)
source scripts/env.sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

The benchmark reports simulated makespan, mean/p95/max wait, turnaround, priority-weighted turnaround, utilization, and measured in-process policy decision time. It does **not** compare Go versus Rust or measure whole-system performance. See [benchmark methodology](benchmarks/README.md) and [recorded sample](benchmarks/results/sample-seed42.json).

The completed checks and their limits are recorded in [docs/VALIDATION.md](docs/VALIDATION.md).

## Configuration and external agents

See [.env.example](.env.example) for configuration keys. It is a reference, not an automatically loaded file. `ORCHESTRA_WORKERS` is 1..32; policy selection is in memory and defaults to `ORCHESTRA_STRATEGY` on restart. The local development launcher fixes the three ports to match the frontend proxy. For separate terminals:

```bash
# Terminal A, project root
PYTHONPATH=scheduler python3 -m orchestra_scheduler.server --port 8091

# Terminal B, project root
source scripts/env.sh
ORCHESTRA_WORKERS=4 ORCHESTRA_STRATEGY=sjf cargo run --locked -p orchestra-core

# Terminal C, project root
cd frontend
npm run dev
```

To try a custom provider, create `.local/`, copy [examples/providers.example.json](examples/providers.example.json) to `.local/providers.json`, replace its three absolute paths with your Python executable and project paths, then start with:

```bash
ORCHESTRA_PROVIDERS="$PWD/.local/providers.json" make dev
```

Select `text` in the UI. It reports word counts from the prompt. The server always adds the built-in `demo` provider. A registry executable receives one versioned JSON task envelope on stdin, writes output to stdout/stderr, and uses its exit status to report success. No shell interpolation is used. Model CLIs need a wrapper that implements this contract; direct Claude/Codex/Gemini integration has not been implemented or tested. See [protocol and provider contract](docs/PROTOCOL.md).

## Operating limits

This is a loopback-only, single-user development application. Registered executables inherit the core's environment and user permissions; the registry is an operator trust boundary, **not an OS sandbox**. Do not register untrusted programs or expose the API publicly. Browser-origin/Host checks are basic local defenses, not authentication.

The MVP retains at most 1,000 tasks per database, 32 dependencies per task, 16 KB of prompt text, and the first 8 KiB per stdout/stderr stream (decoded text may be larger for invalid UTF-8). The journal has no pruning policy. Use a different `ORCHESTRA_DB` for a fresh experiment; there is no destructive reset endpoint. SQLite work is synchronous inside the single engine, so this design targets small local queues.

On Unix, timeouts/cancellation kill the task's process group and reap the direct child. Programs that deliberately detach into another session are outside this guarantee. SIGKILL, host crashes or power loss cannot run cleanup; descendants may continue and need operator inspection. Restart does not provide exactly-once execution or tmux-style recovery.

## Attribution

Inspected upstream commit: [`2c9347858d2285ee128cdf8335420e1b4c52555b`](https://github.com/wayyoungboy/Orchestra/tree/2c9347858d2285ee128cdf8335420e1b4c52555b), on 2026-10-10.

MIT licensed. [LICENSE](LICENSE) retains the original copyright notice; [licenses/Orchestra-MIT.txt](licenses/Orchestra-MIT.txt) is the unmodified upstream license. [NOTICE](NOTICE) distinguishes upstream authorship from this experimental implementation. The inspection and migration boundaries are recorded in [docs/UPSTREAM.md](docs/UPSTREAM.md).
