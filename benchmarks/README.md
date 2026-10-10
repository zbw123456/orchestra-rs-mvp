# Scheduling benchmark

Run `make bench` from the root, or `python3 benchmarks/compare.py --seed 42 --jobs 200 --workers 4`. Python standard library only. Results are JSON and CSV, with seed and host/Python metadata. A recorded run is included as `results/sample-seed42.{json,csv}`; default local outputs are ignored by Git.

This is a **discrete-event simulation comparing scheduling policies**, not a Rust/Go performance comparison or an LLM-quality benchmark. It imports the same `rank` function that the live Python service uses. It measures both simulated scheduling outcomes and wall-clock CPU time spent calling the in-process policy. The policy timer excludes HTTP, serialization, process startup, DB writes, UI and LLM calls.

## Model and reproducibility

- Jobs are independent; identical workers each execute one job at a time; no preemption or dependencies.
- Job durations follow a seeded, capped log-normal distribution (10..10,000 simulated ms). Priority is sampled from 0, 5, 10, biased toward normal work.
- `burst_exact`: all jobs arrive at time zero; estimates equal actual duration.
- `stream_exact`: interarrival gaps 0..159 ms; exact estimates.
- `stream_noisy`: streaming arrivals; estimates scaled by random factors 0.2..2.5.
- A scenario is generated once, then reused unchanged across all four policies. Due to RNG consumption, the two streaming scenarios need not contain identical jobs; compare policies **within** a scenario.
- At each event, admit arrivals and process completions, then fill free slots using the selected policy. Ties are deterministic.
- Same seed/count/capacity/scenario reproduces simulated timing metrics. Policy execution microseconds depend on hardware, interpreter, load, and warmup; do not treat this single run as a statistically robust speed comparison. Try multiple seeds and repeat timing runs for any empirical claim.

## Metrics

| Metric | Definition |
| --- | --- |
| Makespan | Last completion minus first arrival |
| Wait | Start minus arrival; report mean, nearest-rank p95 and maximum |
| Turnaround | Completion minus arrival |
| Priority-weighted turnaround | Average turnaround weighted by `1 + priority` |
| Utilization | Total actual work / (workers × makespan), including idle intervals |
| Policy mean/p95 µs | Measured wall time of one in-process ranking call; timer noise applies |

## Recorded example: burst, seed 42, 200 jobs, 4 workers

| Policy | Mean wait (simulated ms) | p95 wait | Makespan |
| --- | ---: | ---: | ---: |
| FIFO | 17,824.6 | 33,815 | 37,579 |
| SJF | 6,824.1 | 22,760 | 40,433 |
| Priority | 16,746.2 | 34,237 | 37,521 |
| Aging | 7,093.0 | 22,833 | 40,971 |

SJF and aging reduced average wait for this finite synthetic workload while increasing makespan relative to FIFO. This illustrates a tradeoff, not a general ranking of policies. No result supports saying “Rust is faster than the original Orchestra.” Real agent workloads need measured duration estimates, model latency/cost, heterogeneous resources, dependency graphs and failure behavior.

Simulator tests cover a hand-calculated single-worker schedule, simultaneous arrival/completion, reproducible workloads, every job completing exactly once, no starts before arrival, and the concurrency bound.
