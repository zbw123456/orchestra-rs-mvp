# Upstream inspection and scope

Inspected on **2026-10-10** at commit **2c9347858d2285ee128cdf8335420e1b4c52555b**. The public repository was cloned into the enclosing workspace's `work/upstream-orchestra` for read-only inspection before implementation. No upstream setup scripts or application code were executed.

## Findings from the checked-out source

| Evidence at the pinned commit | Observed responsibility | MVP treatment |
| --- | --- | --- |
| [LICENSE](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/LICENSE) | MIT; copyright 2026 wayyoungboy | Exact copy in `licenses/`; attribution retained in root license and NOTICE |
| [backend/go.mod](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/go.mod) | Go 1.25.0, toolchain 1.25.12; Gin, SQLite, WebSocket dependencies | New Rust core; no claimed speedup from a language change |
| [models/task.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/models/task.go) and [storage/repository/task.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/storage/repository/task.go) | Workspace/member-linked task states, transition rules, timestamps, version field, SQL persistence | New task model with one owner, explicit transitions and independent schema |
| [provider/provider.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/provider/provider.go) | Provider/session interfaces, message channels, send and abort | One-shot executable registry; versioned JSON stdin; no interactive sessions |
| [supervisor/supervisor.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/supervisor/supervisor.go) | Session lifecycle, registry, worker limits, bounded message history, event publication | Rust owns process lifetimes and bounded concurrency |
| [tmux/manager.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/tmux/manager.go) | Detached session creation, listing, termination and input | Not ported; ordinary Unix process groups only |
| [eventbus/eventbus.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/eventbus/eventbus.go) | In-memory typed pub/sub for process, queue, member and message events | Durable task transition journal plus browser polling |
| [outbox/worker.go](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/backend/internal/outbox/worker.go) | Persistent async message delivery with attempts, backoff and dead state | Deferred; a journal is not an outbox |
| [frontend/package.json](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/frontend/package.json) | Vue, TypeScript, Pinia, xterm and Vite | Small new TypeScript/Vite UI; no reused Vue screens |
| [docs/ARCHITECTURE.md](https://github.com/wayyoungboy/Orchestra/blob/2c9347858d2285ee128cdf8335420e1b4c52555b/docs/ARCHITECTURE.md) and README | Workspaces, members, tasks, chat, terminals, persistence, security | Used to identify boundaries and explicitly limit the port |

The README's “Go 1.21+” prerequisite differs from the actual `go.mod` version. Likewise, this inspection treats repository source as evidence rather than assuming the previous conversation's summaries are current.

## Why these boundaries

Rust owns authoritative lifecycle state, capacity, dependency readiness and process handles. Python returns only an ordering over eligible task IDs. A slow or failed policy cannot assign extra slots, resurrect terminal tasks, or directly execute a program. A 300 ms deadline and deterministic Rust FIFO fallback keep local work moving.

The Python service uses one set of pure policy functions shared with the benchmark. A future learned policy can implement the same contract without moving process ownership into Python. No RL code or training claims are included without a dataset and evaluation plan.

SQLite was retained conceptually because durable local state is useful here. The MVP writes a full bounded task snapshot and journal event in one transaction. It does not copy the upstream database schema, claim compatibility, or migrate user data.

## What would be needed for a real migration

1. Define compatibility contracts for upstream workspaces, members, tasks and authentication; add import/export fixtures against a pinned schema.
2. Implement and test one actual model CLI adapter with its permission flow and structured output before adding more providers.
3. Decide whether to retain tmux recovery or build another process supervisor; resolve crash outcomes and side effects before considering retries.
4. Add message outbox, terminal streaming and workspace authorization as separate reviewed capabilities.
5. Profile representative workloads and compare equivalent implementations and hardware. The present policy simulation cannot establish Go-versus-Rust or whole-system performance claims.
