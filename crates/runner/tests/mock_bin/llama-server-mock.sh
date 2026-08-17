#!/bin/sh
# Mock llama-server for sidecar tests: serves 200 on /health at the --port
# it is given, after a short startup delay so ready-polling has to retry.
# python3 is present on macOS and GitHub Actions runners.

# --version, answered the way llama-server answers it: two lines on
# stderr, the first of the form "version: <build> (<commit>)".
case "$1" in
--version)
    echo "version: 9999 (mock0000)" >&2
    echo "built with MockClang 1.0.0 for Darwin arm64" >&2
    exit 0
    ;;
esac

exec python3 -c '
import sys, time
from http.server import BaseHTTPRequestHandler, HTTPServer

port = int(sys.argv[sys.argv.index("--port") + 1])
print("mock stdout: starting", flush=True)
print("mock stderr: noise", file=sys.stderr, flush=True)
time.sleep(0.3)

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200 if self.path == "/health" else 404)
        self.end_headers()

    def log_message(self, *args):
        pass

HTTPServer(("127.0.0.1", port), Handler).serve_forever()
' -- "$@"
