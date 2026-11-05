"""The Ticket Tagger's service: a reference for an app that runs on your own servers (Ferrum's
`runtime: service`). Python's standard library only.

    TAGGER_PORT=8090 python3 service/tagger.py

Ferrum posts three things to it, under the address in app.yaml (`service.url`):

* `install`: the workspace's API, a key for the app's own account and the secret its events are
  signed with. Kept in `TAGGER_STATE` (a JSON file), one entry per workspace.
* `events`: `Ticket.created`, naming the ticket without its contents. The signature is checked, the
  ticket read with the key (the account reaches tickets and nothing else) and its topic set.
* `uninstall`: signed; the workspace is forgotten.

The topic comes from `classify`: words in the ticket by default, or a model when `ANTHROPIC_API_KEY`
is set. What a model is sent is the ticket's subject and description, nothing else.
"""
import hashlib
import hmac
import json
import os
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STATE = os.environ.get("TAGGER_STATE", "tagger-state.json")
TOPICS = ["Billing", "Account", "Bug", "Question", "Other"]
WORDS = {"Billing": ["invoice", "refund", "charge", "payment", "bill"], "Account": ["password", "login", "sign in", "account"], "Bug": ["error", "crash", "broken", "bug", "fails"]}


def load():
    try:
        with open(STATE) as f:
            return json.load(f)
    except (FileNotFoundError, ValueError):
        return {}


def save(state):
    with open(STATE, "w") as f:
        json.dump(state, f)


def signed(secret, header, body, now=None, window=300):
    """Whether `X-Ferrum-Signature: t=<ts>,v1=<hex>` is the HMAC-SHA256 of `<ts>.<body>`, and fresh."""
    try:
        parts = dict(p.split("=", 1) for p in header.split(","))
        ts = int(parts["t"])
    except (KeyError, ValueError):
        return False
    if abs((now or time.time()) - ts) > window:
        return False
    expected = hmac.new(secret.encode(), f"{ts}.".encode() + body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, parts.get("v1", ""))


def classify(subject, description):
    """A topic of TOPICS for a ticket."""
    key = os.environ.get("ANTHROPIC_API_KEY")
    if key:
        prompt = f"Answer with one word of {', '.join(TOPICS)}: the topic of this support ticket.\n\nSubject: {subject}\n\n{description}"
        req = urllib.request.Request(
            "https://api.anthropic.com/v1/messages",
            data=json.dumps({"model": os.environ.get("TAGGER_MODEL", "claude-haiku-5-5"), "max_tokens": 5, "messages": [{"role": "user", "content": prompt}]}).encode(),
            headers={"x-api-key": key, "anthropic-version": "2023-06-01", "content-type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=20) as r:
            said = json.load(r)["content"][0]["text"].strip().strip(".")
        return said if said in TOPICS else "Other"
    text = f"{subject} {description}".lower()
    return next((topic for topic, words in WORDS.items() if any(w in text for w in words)), "Question" if "?" in text else "Other")


def call(api, key, method, path, body=None, version=None):
    headers = {"authorization": f"Bearer {key}", "content-type": "application/json"}
    if version is not None:
        headers["if-match"] = f'"v{version}"'
    req = urllib.request.Request(f"{api}{path}", method=method, data=json.dumps(body).encode() if body is not None else None, headers=headers)
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.load(r)


def tag(workspace, ticket_id):
    """Reads the ticket with the workspace's key and sets its topic; returns it."""
    api, key = workspace["api"], workspace["key"]
    ticket = call(api, key, "GET", f"/v1/Ticket/{ticket_id}")
    topic = classify(ticket.get("subject") or "", ticket.get("description") or "")
    call(api, key, "PATCH", f"/v1/Ticket/{ticket_id}", {"tg_topic": topic}, ticket.get("version"))
    return topic


class Handler(BaseHTTPRequestHandler):
    def answer(self, status, body=b""):
        self.send_response(status)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        path = self.path.rstrip("/").rsplit("/", 1)[-1]
        state = load()
        if path == "install":
            v = json.loads(body)
            state[v["workspace"]] = {"api": v["api"], "key": v["key"], "secret": v["secret"]}
            save(state)
            return self.answer(200)
        event = json.loads(body or b"{}")
        workspace = state.get(str(event.get("workspace") or self.headers.get("x-ferrum-workspace") or ""))
        # Events and uninstalls say which workspace only through the secret that signed them.
        if workspace is None:
            workspace = next((w for w in state.values() if signed(w["secret"], self.headers.get("x-ferrum-signature", ""), body)), None)
            if workspace is None:
                return self.answer(401)
        elif not signed(workspace["secret"], self.headers.get("x-ferrum-signature", ""), body):
            return self.answer(401)
        if path == "uninstall":
            save({k: w for k, w in state.items() if w is not workspace and w != workspace})
            return self.answer(200)
        if path == "events" and event.get("entity") == "Ticket" and event.get("event") == "created":
            tag(workspace, event["id"])
        return self.answer(200)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    port = int(os.environ.get("TAGGER_PORT", "8090"))
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
