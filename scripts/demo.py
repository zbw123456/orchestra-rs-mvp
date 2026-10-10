#!/usr/bin/env python3
"""Submit a small dependency workflow to an already running local core."""
import argparse
import json
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    args = parser.parse_args()
    def submit(title, duration, priority=0, depends_on=None):
        payload = {"title": title, "prompt": f"Demonstrate: {title}", "estimate_ms": duration,
                   "demo_duration_ms": duration, "priority": priority, "depends_on": depends_on or []}
        request = urllib.request.Request(args.url + "/api/tasks", data=json.dumps(payload).encode(),
                                         headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=5) as response:
            task = json.load(response)
        print(task["id"], title)
        return task["id"]
    plan = submit("Plan the change", 2500, 5)
    api = submit("Implement API contract", 6000, 5, [plan])
    ui = submit("Build task interface", 4500, 0, [plan])
    submit("Review both results", 1800, 10, [api, ui])
    submit("Write development notes", 3200)
    submit("Check a quick regression", 900, 10)


if __name__ == "__main__":
    main()
