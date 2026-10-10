#!/usr/bin/env python3
"""Start three local services; Ctrl+C stops and reaps them."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    if not (ROOT / "frontend/node_modules").is_dir():
        raise SystemExit("Run make setup first to install frontend dependencies.")
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "scheduler")
    # The frontend proxy uses this fixed pair of ports.
    env["ORCHESTRA_PORT"] = "8080"
    env["ORCHESTRA_SCHEDULER_URL"] = "http://127.0.0.1:8091"
    commands = [([sys.executable, "-m", "orchestra_scheduler.server"], ROOT),
                ([str(ROOT / "target/debug/orchestra-core")], ROOT),
                (["npm", "run", "dev"], ROOT / "frontend")]
    processes = []
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        for command, cwd in commands:
            processes.append(subprocess.Popen(command, cwd=cwd, env=env, start_new_session=True))
        print("\nOpen http://127.0.0.1:5173 — Ctrl+C stops all services.\n", flush=True)
        while all(p.poll() is None for p in processes):
            time.sleep(.25)
        raise SystemExit("A service stopped. See its output above (check for occupied ports).")
    except KeyboardInterrupt:
        print("\nStopping local services…", flush=True)
    finally:
        for p in reversed(processes):
            if p.poll() is None:
                os.killpg(p.pid, signal.SIGTERM)
        for p in processes:
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                p.wait()


if __name__ == "__main__":
    main()
