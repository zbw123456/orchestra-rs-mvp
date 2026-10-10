import unittest
from compare import Job, simulate, workload


class SimulationTests(unittest.TestCase):
    def test_known_single_worker_schedule(self):
        jobs = [Job("a", 0, 10, 10, 0), Job("b", 0, 1, 1, 0)]
        fifo, _ = simulate(jobs, "fifo", 1)
        sjf, _ = simulate(jobs, "sjf", 1)
        self.assertEqual(fifo["makespan_ms"], 11)
        self.assertEqual(fifo["mean_wait_ms"], 5)
        self.assertEqual(sjf["mean_wait_ms"], .5)

    def test_arrivals_and_completion_at_same_time(self):
        jobs = [Job("a", 0, 10, 10, 0), Job("b", 10, 2, 2, 10)]
        metrics, trace = simulate(jobs, "aging", 1)
        self.assertEqual(trace["b"]["start_ms"], 10)
        self.assertEqual(metrics["mean_wait_ms"], 0)
        self.assertEqual(metrics["worker_utilization"], 1)

    def test_seed_and_job_conservation(self):
        self.assertEqual(workload(7, 30, "stream_noisy"), workload(7, 30, "stream_noisy"))
        jobs = workload(7, 30, "stream_noisy")
        for strategy in ("fifo", "sjf", "priority", "aging"):
            metrics, trace = simulate(jobs, strategy, 3)
            self.assertEqual(len(trace), 30)
            self.assertLessEqual(metrics["worker_utilization"], 1)
            for j in jobs:
                self.assertGreaterEqual(trace[j.id]["start_ms"], j.arrival)
                self.assertEqual(trace[j.id]["end_ms"] - trace[j.id]["start_ms"], j.duration)
            boundaries = sorted({t[k] for t in trace.values() for k in ("start_ms", "end_ms")})
            for clock in boundaries:
                self.assertLessEqual(sum(t["start_ms"] <= clock < t["end_ms"] for t in trace.values()), 3)


if __name__ == "__main__":
    unittest.main()
