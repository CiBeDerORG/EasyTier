"""Loopback-only HTTPS LuCI protocol fixture. Never contacts OpenWrt or ADB."""
import http.server,json,ssl,sys
from pathlib import Path
root=Path(sys.argv[1])
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_POST(self):
        request=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        with (root/'requests.jsonl').open('a') as f: f.write(json.dumps(request)+'\n')
        mode=json.loads((root/'mode.json').read_text())
        session,obj,method,args=request['params']
        result={'jsonrpc':'2.0','id':request['id'],'result':[0,{}]}
        if obj=='session' and method=='login':
            result['result']=[0,{'ubus_rpc_session':'a'*32}]
            if args['password']!=mode['password'] or args['username']!='easyadb':result['result']=[6]
        elif obj=='session' and method=='access': result['result']=[0,{'access':True}]
        elif method=='scan': result['result']=[0,mode.get(obj,mode.get('snapshot'))]
        else: result['result']=[3]
        data=json.dumps(result).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=http.server.HTTPServer(('127.0.0.1',0),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(root/'cert.pem',root/'key.pem');server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True);server.serve_forever()
