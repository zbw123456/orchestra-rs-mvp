"""Loopback-only advisory service; stdlib HTTP server for local development."""

import argparse
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

from .policies import STRATEGIES, rank, validate_request


class Handler(BaseHTTPRequestHandler):
    def reply(self, status, payload):
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/health":
            self.reply(200, {"status": "ok", "schema_version": 1, "strategies": STRATEGIES})
        else:
            self.reply(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/v1/rank":
            self.reply(404, {"error": "not found"})
            return
        self.connection.settimeout(2)
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 256_000:
                raise ValueError("body must be 1..256000 bytes")
            tasks, strategy, now = validate_request(json.loads(self.rfile.read(size)))
            self.reply(200, {"schema_version": 1, "ordered_ids": rank(tasks, strategy, now)})
        except (ValueError, TypeError, TimeoutError) as error:
            self.reply(400, {"error": str(error)})

    def log_message(self, format, *args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8091)
    args = parser.parse_args()
    server = HTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Scheduler listening on http://127.0.0.1:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
