"""Deterministic non-preemptive ranking. Larger priority means more urgent."""

from dataclasses import dataclass

STRATEGIES = ("fifo", "sjf", "priority", "aging")


@dataclass(frozen=True)
class Candidate:
    id: str
    created_at_ms: int
    estimate_ms: int
    priority: int


def rank(tasks: list[Candidate], strategy: str, now_ms: int) -> list[str]:
    if strategy not in STRATEGIES:
        raise ValueError(f"unknown strategy: {strategy}")
    if len({t.id for t in tasks}) != len(tasks):
        raise ValueError("duplicate task IDs")
    if any(t.estimate_ms <= 0 or not 0 <= t.priority <= 10 for t in tasks):
        raise ValueError("invalid estimate or priority")

    def key(t: Candidate):
        tie = (t.created_at_ms, t.id)
        if strategy == "fifo":
            return tie
        if strategy == "sjf":
            return (t.estimate_ms, *tie)
        if strategy == "priority":
            return (-t.priority, *tie)
        # Priority-weighted highest response ratio next (HRRN).
        # Recompute waiting time on every decision. No learned model.
        wait = max(0, now_ms - t.created_at_ms)
        score = (1 + t.priority / 10) * (1 + wait / t.estimate_ms)
        return (-score, *tie)

    return [t.id for t in sorted(tasks, key=key)]


def validate_request(payload: object) -> tuple[list[Candidate], str, int]:
    if not isinstance(payload, dict) or type(payload.get("schema_version")) is not int or payload.get("schema_version") != 1:
        raise ValueError("schema_version must be 1")
    raw = payload.get("tasks")
    now = payload.get("now_ms")
    strategy = payload.get("strategy")
    if not isinstance(raw, list) or len(raw) > 1000:
        raise ValueError("tasks must contain at most 1000 candidates")
    if type(now) is not int or now < 0 or strategy not in STRATEGIES:
        raise ValueError("invalid now_ms or strategy")
    tasks = []
    for item in raw:
        if not isinstance(item, dict):
            raise ValueError("invalid candidate")
        ident = item.get("id")
        created, estimate, priority = (item.get(k) for k in
                                       ("created_at_ms", "estimate_ms", "priority"))
        if not isinstance(ident, str) or not 1 <= len(ident) <= 100:
            raise ValueError("invalid candidate ID")
        if any(type(v) is not int for v in (created, estimate, priority)):
            raise ValueError("candidate numeric fields must be integers")
        if not 0 <= created <= now or not 1 <= estimate <= 3_600_000 or not 0 <= priority <= 10:
            raise ValueError("candidate values out of range")
        tasks.append(Candidate(ident, created, estimate, priority))
    return tasks, strategy, now
