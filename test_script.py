import sys, json

def read_message():
    while True:
        line = sys.stdin.readline()
        if not line:
            return None
        if line.startswith("Content-Length:"):
            length = int(line.split(":")[1].strip())
            sys.stdin.readline()  # blank line
            body = sys.stdin.read(length)
            return json.loads(body)

def write_message(obj):
    body = json.dumps(obj)
    sys.stdout.write(f"Content-Length: {len(body)}\r\n\r\n{body}")
    sys.stdout.flush()

while True:
    msg = read_message()
    if msg is None:
        break
    if "id" not in msg:
        continue  # notification, no response
    method = msg.get("method", "")
    if method == "initialize":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mock-server", "version": "0.1.0"}
            }
        })
    elif method == "tools/list":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "tools": [
                    {
                        "name": "greet",
                        "description": "Say hello",
                        "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}}
                    }
                ]
            }
        })
    elif method == "tools/call":
        name = msg.get("params", {}).get("arguments", {}).get("name", "world")
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "content": [{"type": "text", "text": f"Hello, {name}!"}],
                "isError": False
            }
        })
    elif method == "ping":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {}
        })
    else:
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "error": {"code": -32601, "message": "Method not found"}
        })
