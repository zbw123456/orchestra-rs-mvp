# Local API and provider contracts

Base URL: `http://127.0.0.1:8080`. JSON field names use snake_case. APIs are new MVP contracts, not upstream-compatible endpoints.

| Method/path | Result |
| --- | --- |
| `GET /health` | Core availability and `schema_version: 1`; not a Python-service health check |
| `GET /api/state` | Full bounded task list, worker use/capacity, selected policy, provider names, source of last dispatch |
| `POST /api/tasks` | Validate, durably enqueue and return a task |
| `POST /api/tasks/{id}/cancel` | Cancel queued work or request process cleanup; repeated calls are idempotent |
| `PUT /api/strategy` | `{"strategy":"fifo|sjf|priority|aging"}` (one enum value); affects later dispatch |
| `GET /api/events?after=0` | Next ≤100 journal events in increasing `seq` order |

Normal successful responses return 200. Domain validation errors return JSON `{"error":"..."}` with 400, absent task IDs return 404, unavailable engine returns 503. Axum extractor errors (malformed JSON/invalid field types, unsupported content type, oversized bodies) use its own status/text responses. Mutation body limit: 64 KiB. Browser writes allow only the documented local UI/core origins; other origins return 403. Host must be localhost or 127.0.0.1. There is no authentication.

## Create a task

```bash
curl http://127.0.0.1:8080/api/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"Review API","prompt":"Check input validation","provider":"demo","estimate_ms":1200,"demo_duration_ms":1200,"timeout_ms":30000,"priority":5,"depends_on":[],"demo_fail":false}'
```

Only `title` is required. Defaults: prompt `""`, provider `"demo"`, estimate/demo duration 1,000 ms, timeout 30,000 ms, priority 0, empty dependencies, no simulated failure. Unknown fields are rejected.

Title: nonblank, at most 200 UTF-8 bytes. Prompt: at most 16,000 bytes. Duration estimates and demo durations: 1..3,600,000 ms. Timeout: 10..3,600,000 ms. Priority: 0..10, larger means more urgent. Dependencies: at most 32 unique, existing task IDs.

Task responses include the submitted spec plus `id`, `status`, `created_at_ms`, nullable `started_at_ms`/`finished_at_ms`, `cancel_requested`, `output`, `detail` and `revision`. Timestamps are Unix milliseconds. Client input cannot set lifecycle fields. UI HTML limits are convenience checks; the server enforces byte limits.

Dependency failure produces `blocked` without execution. Running cancellation is asynchronous: expect `cancel_requested: true` before `cancelled`. Terminal-task cancellation returns the unchanged task. The event cursor is independent of task revision and continues across restarts.

## Advisory scheduler

`POST http://127.0.0.1:8091/v1/rank`

```json
{
  "schema_version": 1,
  "strategy": "aging",
  "now_ms": 1000,
  "tasks": [
    {"id": "task-a", "created_at_ms": 100, "estimate_ms": 1000, "priority": 0},
    {"id": "task-b", "created_at_ms": 200, "estimate_ms": 300, "priority": 5}
  ]
}
```

```json
{"schema_version": 1, "ordered_ids": ["task-b", "task-a"]}
```

Empty candidate lists are valid. Python accepts at most 1,000 candidates and 256,000 request bytes. Rust bounds responses to 256,000 bytes and uses a 300 ms HTTP timeout. Rust does not follow redirects or proxies. Python never receives prompts, provider credentials or permission to execute. `GET /health` on 8091 lists supported strategies.

## External executable provider

Registry shape is illustrated in [examples/providers.example.json](../examples/providers.example.json). The registry is read only at startup. Names must be unique, nonempty, ≤64 bytes, and cannot replace `demo`. Both executable and cwd must be absolute existing paths (file/directory respectively). Arguments are an operator-controlled array; no shell interpolation is applied. Only registered names are accepted from clients.

Each execution receives one JSON envelope, newline and EOF on stdin:

```json
{"schema_version":1,"task":{"id":"...","title":"...","prompt":"...","provider":"text","status":"running","estimate_ms":1000,"demo_duration_ms":1000,"timeout_ms":30000,"priority":0,"depends_on":[],"demo_fail":false,"created_at_ms":1,"started_at_ms":2,"finished_at_ms":null,"cancel_requested":false,"output":"","detail":"dispatched via python:aging","revision":1}}
```

The task fields follow the API spec. Demo-only options are ignored by external providers. The provider writes UTF-8 stdout/stderr; binary/invalid UTF-8 is decoded lossily. Exit zero means success, nonzero means failure. Output is opaque text and isn't parsed into further agent actions. A task's timeout applies to execution, not time waiting in the queue. Dependencies are references only: the runtime does not retrieve or inject their outputs into this envelope.

Providers run with the core's inherited environment and user privileges. A model CLI wrapper should explicitly manage credentials, permission prompts, cwd and noninteractive termination. No automatic permission-bypass flags are supplied by this project. The text-analysis example exercises the interface without an LLM or API key.
