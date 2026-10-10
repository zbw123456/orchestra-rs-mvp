#!/usr/bin/env python3
"""Discrete-event policy benchmark. Simulated ms are not runtime throughput."""
import argparse
import csv
from dataclasses import dataclass
import heapq
import json
import math
from pathlib import Path
import platform
import random
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scheduler"))
from orchestra_scheduler.policies import Candidate, STRATEGIES, rank


@dataclass(frozen=True)
class Job:
    id: str
    arrival: int
    duration: int
    estimate: int
    priority: int


def workload(seed: int, count: int, scenario: str) -> list[Job]:
    rng = random.Random(seed)
    arrival = 0
    jobs = []
    for index in range(count):
        duration = max(10, min(10_000, round(rng.lognormvariate(6, 1.2))))
        if scenario != "burst_exact":
            arrival += rng.randrange(0, 160)
        estimate = duration if scenario != "stream_noisy" else max(1, round(duration * rng.uniform(.2, 2.5)))
        jobs.append(Job(f"job-{index:05d}", arrival, duration, estimate, rng.choice([0, 0, 0, 5, 10])))
    return jobs


def percentile(values: list[float], p: float) -> float:
    return sorted(values)[max(0, math.ceil(len(values) * p) - 1)]


def simulate(jobs: list[Job], strategy: str, workers: int) -> tuple[dict, dict]:
    if workers < 1 or not jobs or len({j.id for j in jobs}) != len(jobs):
        raise ValueError("positive workers and non-empty unique jobs required")
    if any(j.duration <= 0 or j.estimate <= 0 or j.arrival < 0 for j in jobs):
        raise ValueError("invalid job times")
    pending = sorted(jobs, key=lambda j: (j.arrival, j.id))
    lookup = {j.id: j for j in jobs}
    waiting = {}
    running = []
    trace = {}
    index = 0
    now = pending[0].arrival
    timings = []
    while index < len(pending) or running or waiting:
        while index < len(pending) and pending[index].arrival <= now:
            j = pending[index]
            waiting[j.id] = j
            index += 1
        while running and running[0][0] <= now:
            heapq.heappop(running)
        if waiting and len(running) < workers:
            candidates = [Candidate(j.id, j.arrival, j.estimate, j.priority) for j in waiting.values()]
            start = time.perf_counter_ns()
            ordered = rank(candidates, strategy, now)
            timings.append((time.perf_counter_ns() - start) / 1000)
            for ident in ordered[:workers - len(running)]:
                j = waiting.pop(ident)
                trace[ident] = {"start_ms": now, "end_ms": now + j.duration}
                heapq.heappush(running, (now + j.duration, ident))
        future = [running[0][0]] if running else []
        if index < len(pending):
            future.append(pending[index].arrival)
        if future:
            now = min(future)
    waits = [trace[j.id]["start_ms"] - j.arrival for j in jobs]
    turns = [trace[j.id]["end_ms"] - j.arrival for j in jobs]
    makespan = max(t["end_ms"] for t in trace.values()) - min(j.arrival for j in jobs)
    weighted = sum((1 + lookup[i].priority) * (t["end_ms"] - lookup[i].arrival) for i, t in trace.items())
    metrics = {
        "strategy": strategy, "jobs": len(jobs), "workers": workers,
        "makespan_ms": makespan, "mean_wait_ms": statistics.mean(waits),
        "p95_wait_ms": percentile(waits, .95), "max_wait_ms": max(waits),
        "mean_turnaround_ms": statistics.mean(turns),
        "priority_weighted_turnaround_ms": weighted / sum(1 + j.priority for j in jobs),
        "worker_utilization": sum(j.duration for j in jobs) / (workers * makespan),
        "policy_mean_us": statistics.mean(timings), "policy_p95_us": percentile(timings, .95),
        "decisions": len(timings),
    }
    return metrics, trace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--jobs", type=int, default=200)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--output", type=Path, default=Path("benchmarks/results/local-comparison.json"))
    args = parser.parse_args()
    if not 1 <= args.jobs <= 1000 or not 1 <= args.workers <= 32:
        parser.error("jobs must be 1..1000 and workers 1..32")
    rows = []
    for scenario in ("burst_exact", "stream_exact", "stream_noisy"):
        jobs = workload(args.seed, args.jobs, scenario)
        for strategy in STRATEGIES:
            metrics, _ = simulate(jobs, strategy, args.workers)
            rows.append({"scenario": scenario, **metrics})
    result = {"schema_version": 1, "seed": args.seed, "python": platform.python_version(),
              "platform": platform.platform(), "scope": "non-preemptive simulation; independent jobs; identical workers; no IPC, LLM, process, DB or dependency costs", "results": rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    with args.output.with_suffix(".csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    print(f"{'scenario':16} {'policy':9} {'mean wait':>11} {'p95 wait':>11} {'makespan':>11} {'policy us':>10}")
    for row in rows:
        print(f"{row['scenario']:16} {row['strategy']:9} {row['mean_wait_ms']:10.1f} {row['p95_wait_ms']:10.1f} {row['makespan_ms']:10.1f} {row['policy_mean_us']:10.1f}")
    print(f"Saved {args.output} and {args.output.with_suffix('.csv')}")


if __name__ == "__main__":
    main()
