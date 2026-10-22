#!/usr/bin/env python3
"""A loopback stand-in for RichDoctor's reporting relay, for a test guest that has no Node.

The contract is the relay's (`richos/mobile/service/RICHDOCTOR-RELAY.md`, protocol 2, agreed
2026-10-22); the real Worker, run in workerd with a fake GitHub, is
`richos/mobile/dev/report-relay-local.mjs` and is what a host-side check uses. This stand-in keeps
only what a walk in the guest needs: register with the manifest, per-image upload with the
receipt secret in a header, complete (which creates one issue only when every image is stored),
status, retire, the same-id replay rule and three injectable faults.

    report-relay-stand-in.py <port> <state.json>

    POST /v1/reports                               register (the contract's body)
    PUT  /v1/reports/<id>/attachments/<n>          one image, X-Receipt-Secret, X-Content-SHA256
    POST /v1/reports/<id>/complete                 {"receipt_secret": ...}
    POST /v1/reports/<id>/status                   {"receipt_secret": ...}
    POST /v1/reports/<id>/retire                   {"receipt_secret": ...}
    GET  /_issues                                  what the fake GitHub holds
    GET  /_attachments                             the stored images: report, n, sha256, bytes
    POST /_fault  {"next": "lost"|"reject"|"limited"}   the next complete fails that way once

Binds 127.0.0.1 only. Prints `listening <port>` when ready.
"""
import hashlib
import json
import re
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = int(sys.argv[1])
STATE = sys.argv[2]
REPO = "richos-test/report-relay-local"

try:
    S = json.load(open(STATE))
except (OSError, ValueError):
    S = {"reports": {}, "issues": [], "fault": None, "files": {}}


def save():
    with open(STATE, "w") as f:
        json.dump(S, f)


def sha(b):
    return hashlib.sha256(b if isinstance(b, bytes) else b.encode()).hexdigest()


def view(rid):
    r = S["reports"][rid]
    stored = [str(e["attachment_id"]) for e in r["manifest"] if f"{rid}/{e['attachment_id']}" in S["files"]]
    missing = [str(e["attachment_id"]) for e in r["manifest"] if str(e["attachment_id"]) not in stored]
    out = dict(r["state"])
    out.setdefault("stage", out["state"])
    out["stored"], out["missing"] = stored, missing
    if out["state"] in ("receiving", "ready"):
        out["state"] = "ready" if not missing else "receiving"
        out["stage"] = out["state"]
    return out


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def reply(self, code, body, headers=()):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def raw(self):
        return self.rfile.read(int(self.headers.get("content-length", "0")))

    def body(self):
        try:
            return json.loads(self.raw() or b"{}")
        except ValueError:
            return None

    def do_GET(self):
        if self.path == "/_issues":
            return self.reply(200, S["issues"])
        if self.path == "/_attachments":
            return self.reply(200, [{"key": k, **v} for k, v in S["files"].items()])
        if self.path == "/v1/health":
            return self.reply(200, {"ok": True, "service": "richos-report", "protocol": 2, "accepting": True})
        self.reply(404, {"state": "unknown_id"})

    def report(self, rid, secret):
        r = S["reports"].get(rid)
        if r is None:
            return None, (404, {"state": "unknown_id"})
        if r["secret"] != sha(secret or ""):
            return None, (403, {"state": "forbidden"})
        return r, None

    def do_PUT(self):
        parts = self.path.split("/")
        if len(parts) != 6 or parts[1:3] != ["v1", "reports"] or parts[4] != "attachments":
            return self.reply(404, {"state": "unknown_id"})
        rid, n = parts[3], parts[5]
        data = self.raw()
        r, err = self.report(rid, self.headers.get("X-Receipt-Secret"))
        if err:
            return self.reply(*err)
        if r["state"]["state"] not in ("receiving", "ready"):
            return self.reply(200, view(rid))
        entry = next((e for e in r["manifest"] if str(e["attachment_id"]) == n), None)
        if entry is None or sha(data) != entry["sha256"] or self.headers.get("X-Content-SHA256") != entry["sha256"] or len(data) != entry["bytes"]:
            return self.reply(422, {"state": "rejected", "reason": "attachment", "item": f"screenshot {n}", "detail": f"screenshot {n} is not the approved image."})
        S["files"][f"{rid}/{n}"] = {"sha256": sha(data), "bytes": len(data)}
        save()
        self.reply(200, view(rid))

    def do_POST(self):
        if self.path == "/_fault":
            b = self.body() or {}
            S["fault"] = b.get("next")
            save()
            return self.reply(200, {"ok": True})
        if self.path == "/v1/reports":
            b = self.body()
            if b is None or b.get("schema") != "richdoctor-report/2":
                return self.reply(422, {"state": "rejected", "reason": "schema", "detail": "not a protocol 2 report"})
            # The relay's own rule (checks.mjs UUID): the installation ID is a random UUID.
            if not re.fullmatch(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}", str(b.get("installation_id", ""))):
                return self.reply(400, {"state": "rejected", "reason": "schema", "detail": "The installation ID is not valid."})
            rid = b.get("report_id", "")
            r = S["reports"].get(rid)
            if r:
                if r["secret"] != sha(b.get("receipt_secret", "")):
                    return self.reply(403, {"state": "forbidden"})
                if r["hash"] != b.get("content_sha256"):
                    return self.reply(409, {"state": "conflict"})
                return self.reply(200, view(rid))
            S["reports"][rid] = {"secret": sha(b.get("receipt_secret", "")), "hash": b.get("content_sha256"), "manifest": b.get("manifest", []),
                                 "title": b.get("title"), "body": b.get("body"), "state": {"state": "receiving"}}
            save()
            return self.reply(200, view(rid))
        parts = self.path.split("/")
        if len(parts) != 5 or parts[1:3] != ["v1", "reports"]:
            return self.reply(404, {"state": "unknown_id"})
        rid, verb = parts[3], parts[4]
        b = self.body() or {}
        if verb == "retire":
            r = S["reports"].get(rid)
            if r is None:
                S["reports"][rid] = {"secret": sha(b.get("receipt_secret", "")), "hash": None, "manifest": [], "state": {"state": "retired"}}
                save()
                return self.reply(200, {"state": "retired", "stage": "retired"})
            if r["secret"] != sha(b.get("receipt_secret", "")):
                return self.reply(403, {"state": "forbidden"})
            if r["state"]["state"] in ("receiving", "ready", "rejected", "closed", "retired"):
                r["state"] = {"state": "retired"}
                save()
                return self.reply(200, {"state": "retired", "stage": "retired"})
            return self.reply(200, view(rid))
        r, err = self.report(rid, b.get("receipt_secret"))
        if err:
            return self.reply(*err)
        if verb == "status":
            return self.reply(200, view(rid))
        if verb != "complete":
            return self.reply(404, {"state": "unknown_id"})
        v = view(rid)
        if r["state"]["state"] not in ("receiving", "ready", "rejected", "closed"):
            return self.reply(200, v)
        if v["missing"]:
            return self.reply(200, v)
        fault, S["fault"] = S.get("fault"), None
        if fault == "limited":
            save()
            return self.reply(429, {"state": "limited", "retry_after_seconds": 60}, [("retry-after", "60")])
        if fault == "reject":
            r["state"] = {"state": "rejected", "reason": "content", "detail": "in the startup log excerpt, one line looks like it contains a password.", "item": "line 12"}
            save()
            return self.reply(422, view(rid))
        n = len(S["issues"]) + 1
        url = f"https://github.com/{REPO}/issues/{n}"
        S["issues"].append({"number": n, "title": r["title"], "body": r["body"], "report_id": rid})
        k = len(r["manifest"])
        r["state"] = {"state": "delivered", "issue_url": url, "issue_number": n, "images_confirmed": k, "images_expected": k}
        save()
        if fault == "lost":
            # Created, and the answer is lost: the connection closes with nothing sent.
            self.close_connection = True
            return
        self.reply(201, view(rid))


server = HTTPServer(("127.0.0.1", PORT), H)
print(f"listening {PORT}", flush=True)
server.serve_forever()
