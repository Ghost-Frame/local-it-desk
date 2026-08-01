#!/usr/bin/env python3
"""Serve deterministic Docker Hub API fixtures for repository-helper tests."""

from __future__ import annotations

import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


SCENARIO = os.environ["MOCK_SCENARIO"]
PORT_FILE = Path(os.environ["MOCK_PORT_FILE"])
EXPECTED_PAT = os.environ["MOCK_EXPECTED_PAT"]
BEARER_TOKEN = os.environ["MOCK_BEARER_TOKEN"]
REPOSITORY_PATH = "/v2/namespaces/ghostframe/repositories/local-it-desk"


class DockerHubHandler(BaseHTTPRequestHandler):
    """Handle the small authenticated API surface exercised by the helper."""

    repository_created = False
    immutable_enabled = False

    def log_message(self, format: str, *args: object) -> None:
        """Suppress the standard access log so fixtures cannot leak request data."""

    def _body(self) -> dict[str, object]:
        """Decode a JSON request body."""
        length = int(self.headers.get("Content-Length", "0"))
        return json.loads(self.rfile.read(length) or b"{}")

    def _send(self, status: int, body: dict[str, object]) -> None:
        """Return one JSON response with an explicit content length."""
        payload = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _authorized(self) -> bool:
        """Report whether the request carries the fixture bearer token."""
        return self.headers.get("Authorization") == f"Bearer {BEARER_TOKEN}"

    def _repository(self, private: bool = False) -> dict[str, object]:
        """Build a repository response for the current fixture state."""
        return {
            "namespace": "someone-else" if SCENARIO == "wrong-owner" else "ghostframe",
            "name": "local-it-desk",
            "is_private": private,
            "immutable_tags_settings": {
                "enabled": self.immutable_enabled,
                "rules": [r"^[0-9]+\.[0-9]+\.[0-9]+$"] if self.immutable_enabled else [],
            },
        }

    def do_POST(self) -> None:
        """Exchange credentials or create the repository for the selected scenario."""
        if self.path == "/v2/auth/token":
            body = self._body()
            if SCENARIO == "leaky-auth":
                self._send(401, {"detail": f"rejected {body.get('secret', '')}"})
                return
            if body.get("identifier") != "ghostframe" or body.get("secret") != EXPECTED_PAT:
                self._send(401, {"detail": "bad credentials"})
                return
            self._send(200, {"access_token": BEARER_TOKEN})
            return

        if self.path == "/v2/namespaces/ghostframe/repositories" and self._authorized():
            body = self._body()
            if body.get("is_private") is not False:
                self._send(400, {"detail": "repository must be public"})
                return
            type(self).repository_created = True
            self._send(201, self._repository())
            return
        self._send(404, {"detail": "not found"})

    def do_GET(self) -> None:
        """Return the target repository or the selected absence fixture."""
        if self.path != REPOSITORY_PATH or not self._authorized():
            self._send(404, {"detail": "not found"})
            return
        if SCENARIO == "not-found":
            self._send(404, {"detail": "not found"})
            return
        if SCENARIO == "create" and not self.repository_created:
            self._send(404, {"detail": "not found"})
            return
        self._send(200, self._repository(private=SCENARIO == "private"))

    def do_PATCH(self) -> None:
        """Apply the immutable semantic-version fixture setting."""
        if self.path != f"{REPOSITORY_PATH}/immutabletags" or not self._authorized():
            self._send(404, {"detail": "not found"})
            return
        body = self._body()
        expected_rule = r"^[0-9]+\.[0-9]+\.[0-9]+$"
        if body != {"immutable_tags": True, "immutable_tags_rules": [expected_rule]}:
            self._send(400, {"detail": "wrong immutable rule"})
            return
        type(self).immutable_enabled = True
        self._send(200, self._repository())


def main() -> None:
    """Start the fixture server and publish its ephemeral port to the test."""
    server = ThreadingHTTPServer(("127.0.0.1", 0), DockerHubHandler)
    PORT_FILE.write_text(str(server.server_port), encoding="utf-8")
    server.serve_forever()


if __name__ == "__main__":
    main()
