#!/usr/bin/env python3
"""github-stand-in.py - a local stand-in for GitHub's create-an-issue endpoint, for the test VM.

    github-stand-in.py <port> <log> [<seconds to hold each answer>] [<first issue number>]

Answers `POST /repos/<owner>/<repo>/issues` with `201 Created` and the issue GitHub would return
(numbers start at 412, as round 21's do, or at the fourth argument), and writes every request it
receives, its path, its headers and its body, one JSON line each, to <log> THE MOMENT IT ARRIVES,
before answering. With a hold (the third argument) the answer comes that many seconds later, so
the walk can see a send in flight and press Cancel while it is (bug-report-walk.sh step 5); keep
it under the app's 25-second request timeout. The Authorization header is logged as it arrives:
the token in the walk is a throwaway put in the guest's own keychain for this run. Anything else
is 404. It never contacts GitHub; bug-report-walk.sh points the app at it with
RICHOS_BUG_REPORT_API.

It also answers `POST /graphql` the way GitHub does for Request a feature (round 24): the lookup
of the repository's and its `ideas` category's ids, and `createDiscussion` with them, numbered from
the same counter. feature-request-walk.sh reads the log to prove an idea went there and never to
an issues path.
"""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = int(sys.argv[1])
LOG = sys.argv[2]
HOLD = float(sys.argv[3]) if len(sys.argv) > 3 else 0.0
NEXT = [int(sys.argv[4]) if len(sys.argv) > 4 else 412]


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length).decode("utf-8", "replace")
        with open(LOG, "a") as log:
            log.write(json.dumps({"path": self.path, "headers": dict(self.headers), "body": body, "held_s": HOLD}) + "\n")
        parts = self.path.strip("/").split("/")
        if self.path == "/graphql":
            # Request a feature (round 24): an idea is a discussion in the Ideas category, through
            # GitHub's GraphQL API (`richos_core::bug_report::post_discussion`): the lookup of the
            # repository's and the category's ids, then `createDiscussion` with them.
            request = json.loads(body or "{}")
            query, variables = request.get("query", ""), request.get("variables", {})
            if "createDiscussion" in query:
                number = NEXT[0]
                NEXT[0] += 1
                time.sleep(HOLD)
                ok = variables.get("repositoryId") == "R_stand_in" and variables.get("categoryId") == "DIC_stand_in_ideas"
                url = f"https://github.com/WebDevBooster/richos/discussions/{number}"
                self._answer(200, {"data": {"createDiscussion": {"discussion": {"number": number, "url": url}}}} if ok
                             else {"data": {"createDiscussion": None}, "errors": [{"type": "NOT_FOUND", "message": "Could not resolve to a node"}]})
            elif "discussionCategory" in query:
                category = {"id": "DIC_stand_in_ideas"} if variables.get("slug") == "ideas" else None
                self._answer(200, {"data": {"repository": {"id": "R_stand_in", "discussionCategory": category}}})
            else:
                self._answer(200, {"errors": [{"message": "unknown query"}]})
        elif len(parts) == 4 and parts[0] == "repos" and parts[3] == "issues":
            number = NEXT[0]
            NEXT[0] += 1
            time.sleep(HOLD)
            reply = {"number": number, "html_url": f"https://github.com/{parts[1]}/{parts[2]}/issues/{number}"}
            self._answer(201, reply)
        else:
            self._answer(404, {"message": "Not Found"})

    def _answer(self, code, value):
        data = json.dumps(value).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
