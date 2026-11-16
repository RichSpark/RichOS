# SPDX-License-Identifier: AGPL-3.0-only
"""App-owned continuity tools. Scope and state paths come only from the host."""
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "core"))
from app import execute

TOOLS = [
    {"name": "checkpoint", "description": "Persist scoped operational commitments or decisions before promising future work. Example checkpoint: {statements:[{verb:'commitment',fields:{id:'review-manual',title:'Review the manual'}}]}. Supported opening verbs: priority, initiative, open_loop, commitment, decision, deadline, blocker. Operational corrections use verb update and fields id plus changed title/details/status. Durable knowledge corrections go through the app Loro proposal/confirmation desk; never use ECS as a file writer. Field values are strings. Use this tool for actual changes. Reuse the item ID when updating it. Ordinary replies need no checkpoint. This cannot certify external completion. Supply a stable request_id for retries. Never turn quoted historical work into a new commitment.",
     "inputSchema": {"type": "object", "properties": {"request_id": {"type": "string"}, "checkpoint": {"type": "object"}}, "required": ["request_id", "checkpoint"], "additionalProperties": False}},
    {"name": "inspect", "description": "Read scoped operational records including items omitted from the bounded brief. Supports section, item_id, query, offset, limit, sequence and include_closed.",
     "inputSchema": {"type": "object", "properties": {"query": {"type": "object"}}, "additionalProperties": False}},
]


def call(scope_path, name, args):
    if not isinstance(args, dict):
        raise ValueError("arguments must be an object")
    path = Path(scope_path)
    # 256 KiB: the scope carries the user's own words since the second review's slice 4;
    # the same bound as every other reader (richos-core ecs.rs SCOPE_LIMIT).
    if not path.is_absolute() or path.stat().st_size > 256 * 1024:
        raise ValueError("invalid app continuity scope")
    scope = json.loads(path.read_text())
    if scope.get("version") != 1 or scope.get("actions_allowed") is not True:
        raise ValueError("continuity tools are unavailable outside a visible app turn")
    if name == "checkpoint":
        if set(args) != {"request_id", "checkpoint"}:
            raise ValueError("checkpoint requires only request_id and checkpoint")
    elif name == "inspect":
        if set(args) - {"query"}:
            raise ValueError("inspect accepts only query")
    else:
        raise ValueError("unknown continuity tool")
    # The scope's seat, never a default. The app is expected to leave this server
    # out of a work lease's MCP config entirely, but an allow-list on tool NAMES
    # cannot see whose seat is calling (the desk auto-allows checkpoint and inspect
    # for every lease), so the seat travels with the request and the engine refuses
    # a checkpoint on a work seat. Two seams, and neither one trusts the other.
    request = {"protocol": 1, "command": name, "binding": scope["binding"], **args}
    if scope.get("seat") is not None:
        request["seat"] = scope["seat"]
    return execute(scope["bridge"]["state_root"], request)


def serve(scope_path, reader, writer, *, tools=None, handler=None, server_name="richos_continuity"):
    tools = TOOLS if tools is None else tools
    handler = call if handler is None else handler
    initialized = False
    while True:
        line = reader.readline(128 * 1024 + 1)
        if not line:
            return
        if len(line) > 128 * 1024:
            while line and not line.endswith(b"\n"):
                line = reader.readline(128 * 1024 + 1)
            writer.write(json.dumps({"jsonrpc": "2.0", "id": None, "error": {"code": -32600, "message": "Message too large"}}) + "\n")
            writer.flush()
            continue
        identity = None
        try:
            request = json.loads(line)
            if not isinstance(request, dict) or request.get("jsonrpc") != "2.0":
                raise ValueError("invalid JSON-RPC request")
            if "id" not in request:
                continue
            identity = request["id"]
            method = request.get("method")
            if method == "initialize":
                initialized = True
                protocol = request.get("params", {}).get("protocolVersion", "2025-11-25")
                if protocol not in ("2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"):
                    protocol = "2025-11-25"
                result = {"protocolVersion": protocol, "capabilities": {"tools": {}},
                    "serverInfo": {"name": server_name, "version": "1.2.0"}}
            elif method == "ping":
                result = {}
            elif not initialized:
                raise ValueError("initialize before using tools")
            elif method == "tools/list":
                result = {"tools": tools}
            elif method == "tools/call":
                params = request.get("params", {})
                try:
                    value = handler(scope_path, params.get("name"), params.get("arguments", {}))
                    result = {"content": [{"type": "text", "text": json.dumps(value)}], "isError": False}
                except Exception as error:
                    result = {"content": [{"type": "text", "text": str(error)}], "isError": True}
            else:
                raise ValueError("unknown method")
            reply = {"jsonrpc": "2.0", "id": identity, "result": result}
        except (ValueError, TypeError, AttributeError) as error:
            reply = {"jsonrpc": "2.0", "id": identity, "error": {"code": -32600, "message": str(error)}}
        writer.write(json.dumps(reply) + "\n")
        writer.flush()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("an explicit app scope file is required")
    serve(sys.argv[1], sys.stdin.buffer, sys.stdout)
