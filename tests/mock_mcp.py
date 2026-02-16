#!/usr/bin/env python3
import sys
import json

def main():
    for line in sys.stdin:
        try:
            req = json.loads(line)
            method = req.get("method")
            id = req.get("id")

            if method == "initialize":
                resp = {
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "serverInfo": {"name": "MockServer", "version": "1.0.0"}
                    }
                }
            elif method == "tools/list":
                resp = {
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echo back the input",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "text": {"type": "string"}
                                    },
                                    "required": ["text"]
                                }
                            }
                        ]
                    }
                }
            elif method == "tools/call":
                params = req.get("params", {})
                tool_name = params.get("name")
                args = params.get("arguments", {})
                
                if tool_name == "echo":
                    text = args.get("text", "nothing")
                    resp = {
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{"type": "text", "text": f"MOCK_ECHO: {text}"}],
                            "isError": False
                        }
                    }
                else:
                    resp = {
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": "Method not found"}
                    }
            elif method == "notifications/initialized":
                continue # No response needed for notifications
            else:
                resp = {
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "Method not found"}
                }
            
            print(json.dumps(resp), flush=True)

        except Exception as e:
            # sys.stderr.write(f"Error: {e}\n")
            pass

if __name__ == "__main__":
    main()
