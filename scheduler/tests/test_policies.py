import unittest

from orchestra_scheduler.policies import Candidate, rank, validate_request


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.tasks = [Candidate("long", 0, 1000, 1), Candidate("short", 10, 50, 0),
                      Candidate("urgent", 20, 500, 10)]

    def test_distinct_policy_decisions(self):
        self.assertEqual(rank(self.tasks, "fifo", 100)[0], "long")
        self.assertEqual(rank(self.tasks, "sjf", 100)[0], "short")
        self.assertEqual(rank(self.tasks, "priority", 100)[0], "urgent")

    def test_aging_can_promote_old_low_priority_work(self):
        tasks = [Candidate("old", 0, 100, 0), Candidate("new", 10_000, 100, 10)]
        self.assertEqual(rank(tasks, "aging", 10_000), ["old", "new"])

    def test_ties_are_deterministic(self):
        tasks = [Candidate("b", 0, 100, 1), Candidate("a", 0, 100, 1)]
        for strategy in ("fifo", "sjf", "priority", "aging"):
            self.assertEqual(rank(tasks, strategy, 0), ["a", "b"])

    def test_invalid_data(self):
        with self.assertRaises(ValueError):
            rank(self.tasks, "magic-ai", 100)
        with self.assertRaises(ValueError):
            rank([self.tasks[0], self.tasks[0]], "fifo", 100)
        for now in (True, -1, "1"):
            with self.assertRaises(ValueError):
                validate_request({"schema_version": 1, "now_ms": now,
                                  "strategy": "fifo", "tasks": []})
        with self.assertRaises(ValueError):
            validate_request({"schema_version": True, "now_ms": 10,
                              "strategy": "fifo", "tasks": []})


if __name__ == "__main__":
    unittest.main()
