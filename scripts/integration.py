#!/usr/bin/env python3
"""Exercise real Rust + Python processes using an isolated temporary database."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def main():
    core_port, scheduler_port = free_port(), free_port()
    while scheduler_port == core_port:
        scheduler_port = free_port()
    base = f"http://127.0.0.1:{core_port}"
    processes = []
    with tempfile.TemporaryDirectory(prefix="orchestra-test-") as temp:
        env = {**os.environ, "PYTHONPATH": str(ROOT / "scheduler"), "ORCHESTRA_PORT": str(core_port),
               "ORCHESTRA_DB": str(Path(temp) / "test.db"), "ORCHESTRA_WORKERS": "2",
               "ORCHESTRA_SCHEDULER_URL": f"http://127.0.0.1:{scheduler_port}", "ORCHESTRA_STRATEGY": "aging"}
        registry = Path(temp) / "providers.json"
        registry.write_text(json.dumps([{"name": "text", "program": sys.executable,
            "args": [str(ROOT / "examples/text_agent.py")], "cwd": str(ROOT)}]))
        env["ORCHESTRA_PROVIDERS"] = str(registry)
        log = open(Path(temp) / "services.log", "w+")
        def start(command):
            p = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=log)
            processes.append(p)
            return p
        def request(path, body=None, method=None, origin=None):
            headers = {"Content-Type": "application/json"}
            if origin:
                headers["Origin"] = origin
            req = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers=headers, method=method)
            with urllib.request.urlopen(req, timeout=5) as response:
                return json.load(response)
        def wait(predicate, seconds=8):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                try:
                    value = predicate()
                    if value:
                        return value
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.03)
            raise AssertionError("condition timed out")
        def task(ident):
            return next(t for t in request("/api/state")["tasks"] if t["id"] == ident)
        def submit(title, **kwargs):
            return request("/api/tasks", {"title": title, "demo_duration_ms": 100, **kwargs})["id"]
        def done(ident, status):
            result = wait(lambda: (t if t["status"] not in ("queued", "running") else None) if (t := task(ident)) else None)
            assert result["status"] == status, result
            return result
        try:
            scheduler = start([sys.executable, "-m", "orchestra_scheduler.server", "--port", str(scheduler_port)])
            wait(lambda: urllib.request.urlopen(f"http://127.0.0.1:{scheduler_port}/health", timeout=1).status == 200)
            core = start([str(ROOT / "target/debug/orchestra-core")])
            wait(lambda: request("/health"))
            parent = submit("parent", demo_duration_ms=300)
            child = submit("dependent", depends_on=[parent])
            p, c = done(parent, "succeeded"), done(child, "succeeded")
            assert c["started_at_ms"] >= p["finished_at_ms"]
            assert "Demo result" in c["output"]
            assert request("/api/state")["scheduling_source"].startswith("python:")
            print("PASS real subprocess execution, Python ranking, dependency ordering, captured output")

            custom = submit("external adapter", provider="text", prompt="agents coordinate agents")
            assert '"word_count": 3' in done(custom, "succeeded")["output"]
            print("PASS registered external provider receives the JSON task protocol")

            failed = submit("failure", demo_fail=True)
            blocked = submit("blocked", depends_on=[failed])
            done(failed, "failed")
            done(blocked, "blocked")
            timeout = submit("timeout", demo_duration_ms=3000, timeout_ms=50)
            done(timeout, "timed_out")
            running = submit("cancel running", demo_duration_ms=5000)
            wait(lambda: task(running)["status"] == "running")
            queued = submit("cancel queued", depends_on=[running])
            request(f"/api/tasks/{queued}/cancel", {}, "POST")
            request(f"/api/tasks/{running}/cancel", {}, "POST")
            done(queued, "cancelled")
            done(running, "cancelled")
            print("PASS failure propagation, timeout, queued/running cancellation")

            for body in ({"title": "bad", "provider": "shell"}, {"title": "bad", "depends_on": ["missing"]},
                         {"title": "bad", "estimate_ms": 0}, {"title": "bad", "command": "whoami"}):
                try:
                    request("/api/tasks", body)
                    raise AssertionError("invalid input accepted")
                except urllib.error.HTTPError as error:
                    assert error.code in (400, 422), error.code
            try:
                request("/api/tasks", {"title": "forged"}, origin="https://untrusted.example")
                raise AssertionError("invalid origin accepted")
            except urllib.error.HTTPError as error:
                assert error.code == 403
            try:
                req = urllib.request.Request(base + "/api/state", headers={"Host": "untrusted.example"})
                urllib.request.urlopen(req, timeout=3)
                raise AssertionError("invalid host accepted")
            except urllib.error.HTTPError as error:
                assert error.code == 403
            print("PASS API input and browser-origin checks")

            batch = [submit(f"capacity-{i}", demo_duration_ms=180) for i in range(6)]
            for ident in batch:
                done(ident, "succeeded")
            journal = request("/api/events?after=0")
            active = set()
            for event in journal:
                if event["status"] == "running":
                    active.add(event["task_id"])
                else:
                    active.discard(event["task_id"])
                assert len(active) <= 2
            assert not active
            assert request(f"/api/events?after={journal[-1]['seq']}") == []
            print("PASS worker concurrency bound and event cursor")

            scheduler.terminate()
            scheduler.wait(timeout=5)
            request("/api/strategy", {"strategy": "sjf"}, "PUT")
            fallback = submit("fallback")
            done(fallback, "succeeded")
            assert request("/api/state")["scheduling_source"].startswith("fallback:fifo")
            print("PASS scheduler outage falls back to FIFO")

            before = request("/api/state")["tasks"]
            core.terminate()
            core.wait(timeout=5)
            core = start([str(ROOT / "target/debug/orchestra-core")])
            wait(lambda: request("/health"))
            assert request("/api/state")["tasks"] == before
            assert request("/api/events?after=0") == journal + request(f"/api/events?after={journal[-1]['seq']}")
            print("PASS graceful restart preserves task snapshots and journal")
        except BaseException:
            log.flush()
            log.seek(0)
            print(log.read(), file=sys.stderr)
            raise
        finally:
            for p in reversed(processes):
                if p.poll() is None:
                    p.terminate()
                try:
                    p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait()
            log.close()
    print("All integration checks passed; temporary services stopped.")


if __name__ == "__main__":
    main()
