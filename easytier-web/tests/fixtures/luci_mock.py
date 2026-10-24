"""Loopback-only HTTP(S) LuCI protocol fixture. Never contacts OpenWrt or ADB."""
import fnmatch
import http.server
import json
import ssl
import sys
from pathlib import Path

root = Path(sys.argv[1])
SESSION = "a" * 32
# Match the companion application's limited account: discovery grants do not
# implicitly grant session.access. A successful login returns effective ACLs.
DEFAULT_ACLS = {"ubus": {
    "easy-discovery.devices": ["list", "scan"],
    "easy-discovery.adb": ["list", "scan"],
}}


def allowed(acls, obj, method):
    ubus = acls.get("ubus", {}) if isinstance(acls, dict) else {}
    if not isinstance(ubus, dict):
        return False
    return any(
        isinstance(methods, list)
        and fnmatch.fnmatchcase(obj, object_pattern)
        and any(isinstance(pattern, str) and fnmatch.fnmatchcase(method, pattern)
                for pattern in methods)
        for object_pattern, methods in ubus.items()
    )


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with (root / "requests.jsonl").open("a") as f:
            f.write(json.dumps(request) + "\n")
        mode = json.loads((root / "mode.json").read_text())
        if mode.get("redirect"):
            self.send_response(307)
            self.send_header("Location", mode["redirect"])
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        session, obj, method, args = request["params"]
        acls = mode.get("acls", DEFAULT_ACLS)
        result = {"jsonrpc": "2.0", "id": request["id"]}
        responses = mode.get("responses", {})
        if f"{obj}.{method}" in responses:
            result = responses[f"{obj}.{method}"]
        elif obj == "session" and method == "login":
            result["result"] = [0, mode.get("login_result", {
                "ubus_rpc_session": SESSION, "acls": acls,
            })]
            if args["password"] != mode["password"] or args["username"] != "easyadb":
                result["result"] = [6]
        elif session != SESSION or not allowed(acls, obj, method):
            # uhttpd checks permission for the outer RPC before dispatching it.
            # In particular, default session.access requests fail here.
            result["error"] = {"code": -32002, "message": "Access denied"}
        elif obj == "session" and method == "access":
            result["result"] = [0, {"access": allowed(
                acls, args.get("object", ""), args.get("function", ""),
            )}]
        elif method == "scan":
            result["result"] = [0, mode.get(obj, mode.get("snapshot"))]
        else:
            result["result"] = [3]
        data = json.dumps(result).encode()
        self.send_response(mode.get("http_status", 200))
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
if len(sys.argv) < 3 or sys.argv[2] == "https":
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(root / "cert.pem", root / "key.pem")
    server.socket = context.wrap_socket(server.socket, server_side=True)
print(server.server_port, flush=True)
server.serve_forever()

