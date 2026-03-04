#!/usr/bin/env python3
"""Minimal MCP stdio server for testing."""
import json
import sys

def respond(id, result):
    msg = {"jsonrpc": "2.0", "id": id, "result": result}
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

def error(id, code, message):
    msg = {"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        continue

    # Skip notifications (no id)
    if "id" not in msg:
        continue

    method = msg.get("method", "")
    id = msg["id"]

    if method == "initialize":
        respond(id, {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mock-server", "version": "0.1.0"}
        })
    elif method == "tools/list":
        respond(id, {
            "tools": [
                {
                    "name": "echo",
                    "description": "Echo the input back",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "message": {"type": "string", "description": "Message to echo"}
                        },
                        "required": ["message"]
                    }
                },
                {
                    "name": "add",
                    "description": "Add two numbers",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number", "description": "First number"},
                            "b": {"type": "number", "description": "Second number"}
                        },
                        "required": ["a", "b"]
                    }
                }
            ]
        })
    elif method == "tools/call":
        params = msg.get("params", {})
        tool_name = params.get("name", "")
        arguments = params.get("arguments", {})

        if tool_name == "echo":
            message = arguments.get("message", "")
            respond(id, {
                "content": [{"type": "text", "text": message}],
                "isError": False
            })
        elif tool_name == "add":
            a = arguments.get("a", 0)
            b = arguments.get("b", 0)
            respond(id, {
                "content": [{"type": "text", "text": str(a + b)}],
                "isError": False
            })
        else:
            respond(id, {
                "content": [{"type": "text", "text": f"Unknown tool: {tool_name}"}],
                "isError": True
            })
    else:
        error(id, -32601, f"Method not found: {method}")
