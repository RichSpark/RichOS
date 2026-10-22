#!/usr/bin/env python3
"""git-http-stand-in.py - a password-protected Git remote over HTTP, for the test VM.

    git-http-stand-in.py <port> <project root> <user> <password> <log> [<git http-backend>]

Serves every bare repository under <project root> at http://127.0.0.1:<port>/<name>.git through
Git's own `git http-backend` (CGI), and answers 401 with a Basic challenge to any request without
<user>:<password>. So a push to it succeeds only when the pushing Git asked a credential helper
for that user and password: lease-setup-walk.py uses it to prove a lease pushes with the user's
own helper. Every request is logged, one JSON line, with the user it carried (never the password).
It never contacts anything else.
"""
import base64
import json
import os
import subprocess
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

PORT, ROOT, USER, PASSWORD, LOG = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
BACKEND = sys.argv[6] if len(sys.argv) > 6 else 'git-http-backend'


class Handler(BaseHTTPRequestHandler):
    def _log(self, row):
        with open(LOG, 'a') as log:
            log.write(json.dumps(row) + '\n')

    def _serve(self):
        header = self.headers.get('Authorization', '')
        user = None
        if header.startswith('Basic '):
            try:
                name, _, secret = base64.b64decode(header[6:]).decode().partition(':')
                user = name if secret == PASSWORD and name == USER else None
            except ValueError:
                user = None
        self._log({'method': self.command, 'path': self.path, 'authenticated_as': user,
                   'carried_credentials': bool(header)})
        if user is None:
            self.send_response(401)
            self.send_header('WWW-Authenticate', 'Basic realm="stand-in"')
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        url = urlsplit(self.path)
        env = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'GIT_PROJECT_ROOT': ROOT, 'GIT_HTTP_EXPORT_ALL': '1',
               'REQUEST_METHOD': self.command, 'PATH_INFO': url.path, 'QUERY_STRING': url.query,
               'CONTENT_TYPE': self.headers.get('Content-Type', ''), 'CONTENT_LENGTH': str(len(body)),
               'REMOTE_USER': user, 'REMOTE_ADDR': '127.0.0.1', 'SERVER_PROTOCOL': 'HTTP/1.1'}
        if self.headers.get('Git-Protocol'):
            env['GIT_PROTOCOL'] = self.headers['Git-Protocol']
        done = subprocess.run([BACKEND], input=body, env=env, capture_output=True)
        head, _, payload = done.stdout.partition(b'\r\n\r\n')
        if not _:
            head, _, payload = done.stdout.partition(b'\n\n')
        status, headers = 200, []
        for line in head.decode('latin-1').splitlines():
            name, _, value = line.partition(':')
            if name.lower() == 'status':
                status = int(value.strip().split()[0])
            elif name:
                headers.append((name, value.strip()))
        self.send_response(status)
        for name, value in headers:
            self.send_header(name, value)
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    do_GET = _serve
    do_POST = _serve

    def log_message(self, *_):
        pass


ThreadingHTTPServer(('127.0.0.1', PORT), Handler).serve_forever()
