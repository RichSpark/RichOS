"""Read Codex's documented App Server protocol using the installed CLI."""
import contextlib
import json
import os
import re
import selectors
import subprocess
import time

FRAME_LIMIT = 1024 * 1024
REPLY_SECONDS = 4
RESET_OUTCOMES = ("reset", "alreadyRedeemed", "nothingToReset", "noCredit")


class Unavailable(ValueError):
    pass


@contextlib.contextmanager
def client(binary):
    with subprocess.Popen([binary, "app-server", "--listen", "stdio://"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0) as child:
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdout, selectors.EVENT_READ)
                rpc = AppServer(child, ready)
                rpc.request("initialize", {"clientInfo": {"name": "richos", "version": "1.0.0"}})
                rpc.send({"method": "initialized"})
                yield rpc
        finally:
            if child.poll() is None:
                child.kill()


class AppServer:
    def __init__(self, child, ready):
        self.child = child
        self.ready = ready
        self.buffer = bytearray()
        self.request_id = 0

    def send(self, frame):
        # Requests are small fixed messages, below the atomic pipe write size.
        self.child.stdin.write(json.dumps(frame).encode() + b"\n")

    def request(self, method, params=None):
        self.request_id += 1
        request = {"id": self.request_id, "method": method}
        if params is not None:
            request["params"] = params
        self.send(request)
        deadline = time.monotonic() + REPLY_SECONDS
        while time.monotonic() < deadline:
            end = self.buffer.find(b"\n")
            if end >= 0:
                frame = obj(json.loads(self.buffer[:end]))
                del self.buffer[:end + 1]
                if type(frame.get("id")) is int and frame["id"] == self.request_id:
                    result = frame.get("result")
                    if "error" in frame or not isinstance(result, dict):
                        raise Unavailable(f"{method} unavailable")
                    return result
            elif len(self.buffer) >= FRAME_LIMIT:
                raise Unavailable("Codex App Server frame exceeded the limit")
            elif self.ready.select(max(0, deadline - time.monotonic())):
                chunk = os.read(self.child.stdout.fileno(), min(65536, FRAME_LIMIT - len(self.buffer)))
                if not chunk:
                    raise Unavailable("Codex App Server stopped")
                self.buffer.extend(chunk)
        raise Unavailable(f"{method} timed out")


def obj(value):
    return value if isinstance(value, dict) else {}


def number(value):
    return value if type(value) is int and 0 <= value < 10**12 else None


def flag(value):
    return value if type(value) is bool else None


def bucket(value):
    windows = []
    for name in ("primary", "secondary"):
        window = obj(value.get(name))
        if window:
            windows.append(dict(kind=name, used_percent=number(window.get("usedPercent")),
                                duration_minutes=number(window.get("windowDurationMins")),
                                resets_at=number(window.get("resetsAt"))))
    credits = obj(value.get("credits"))
    balance = credits.get("balance")
    limit_id = value.get("limitId")
    reached = value.get("rateLimitReachedType")
    return dict(id=limit_id if isinstance(limit_id, str) and re.fullmatch(r"[\w.-]{1,96}", limit_id) else None,
                windows=windows,
                credits=dict(has_credits=flag(credits.get("hasCredits")),
                             unlimited=flag(credits.get("unlimited")),
                             balance=balance if isinstance(balance, str) and
                             re.fullmatch(r"\d{1,24}(?:\.\d{1,24})?", balance) else None) if credits else None,
                reached=reached if isinstance(reached, str) and re.fullmatch(r"[a-z_]{1,64}", reached) else None,
                spend_control_reached=flag(value.get("spendControlReached")))


class Reading:
    def __init__(self, value):
        primary = value.get("rateLimits")
        if not isinstance(primary, dict):
            raise Unavailable("Codex did not return rate limits")
        buckets = obj(value.get("rateLimitsByLimitId"))
        main = obj(buckets.get("codex")) or primary
        self.main = bucket(main)
        self.account = value.get("accountId")
        self.allowed = flag(value.get("ordinaryUsageAllowed"))
        self.resets = number(obj(value.get("rateLimitResetCredits")).get("availableCount"))
        self.limits = [self.main]
        self.limits.extend(bucket(row) for row in buckets.values() if isinstance(row, dict) and row != main)

    def status(self):
        return dict(status="available", limits=self.limits, available_resets=self.resets,
                    ordinary_usage_allowed=self.allowed)

    def exhausted_windows(self):
        """Only live windows on the main Codex allowance can trigger redemption."""
        if (self.main["id"] not in (None, "codex") or self.allowed is True or
                self.main["spend_control_reached"] or self.main["reached"] not in (None, "rate_limit_reached")):
            return []
        now = time.time()
        return sorted(w["resets_at"] for w in self.main["windows"]
                      if (w["used_percent"] or 0) >= 100 and (w["resets_at"] or 0) > now)

    def summary(self):
        windows = ", ".join(f'{w["duration_minutes"]}-minute window {w["used_percent"]}% used'
                            for w in self.main["windows"]
                            if w["used_percent"] is not None and w["duration_minutes"] is not None)
        resets = self.resets if self.resets is not None else "unknown"
        credits = self.main["credits"] or {}
        balance = "unlimited" if credits.get("unlimited") else credits.get("balance")
        return f'{windows or "usage windows unavailable"}; banked resets {resets}; credit balance {balance or "unknown"}'


def read(rpc):
    return Reading(rpc.request("account/rateLimits/read"))


def usage_failed(event):
    # exec --json currently serializes only a message in turn.failed. The
    # built-in English quota message is also corroborated by a fresh quota read
    # before a reset or replay. Model and workspace errors get reported too.
    error = obj(event.get("error"))
    return (event.get("type") == "turn.failed" and
            (error.get("codexErrorInfo") == "usageLimitExceeded" or
             str(error.get("message", "")).startswith(("You’ve hit your usage limit", "Your workspace is out of credits"))))
