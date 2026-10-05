"""Authenticated CI upload bridge for releases from the private GitHub repository.

Only binds localhost, behind nginx HTTPS. No GitHub credentials on this server.
"""
import hashlib
import hmac
import http.server
import json
import os
import pathlib
import re
import tempfile
import threading

ROOT = pathlib.Path(os.environ.get('SAFEEAT_RELEASE_DIR', '/var/www/safeeat-downloads/releases'))
TOKEN = os.environ['SAFEEAT_RELEASE_PUBLISH_TOKEN']
ORIGIN = 'https://chat-ink.tail552c89.ts.net:10000'
LOCK = threading.Lock()
NAME = re.compile(r'SafeEat-(0|[1-9]\d{0,5})\.(0|[1-9]\d{0,5})\.(0|[1-9]\d{0,5})\.apk')

def require(condition):
    if not condition:
        raise ValueError('Invalid release metadata')

def file_hash(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

class Handler(http.server.BaseHTTPRequestHandler):
    def reply(self, code):
        self.send_response(code)
        self.send_header('Content-Length', '0')
        self.end_headers()

    def allowed(self):
        if not hmac.compare_digest(self.headers.get('Authorization', ''), 'Bearer ' + TOKEN):
            self.reply(401)
            return False
        return True

    def do_PUT(self):
        if not self.allowed(): return
        name = self.path.removeprefix('/safeeat-release-publish/')
        if not NAME.fullmatch(name): return self.reply(400)
        try: size = int(self.headers.get('Content-Length', '0'))
        except ValueError: return self.reply(400)
        if not 100000 < size <= 262144000: return self.reply(413)
        self.connection.settimeout(120)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=ROOT, prefix='.upload-', delete=False) as output:
                temporary = pathlib.Path(output.name)
                remaining = size
                while remaining:
                    chunk = self.rfile.read(min(65536, remaining))
                    if not chunk: raise ValueError('Incomplete upload')
                    remaining -= len(chunk)
                    output.write(chunk)
            with LOCK:
                destination = ROOT / name
                if destination.exists() and file_hash(destination) != file_hash(temporary):
                    return self.reply(409)
                temporary.chmod(0o644)
                temporary.replace(destination)
            self.reply(201)
        except (OSError, ValueError): self.reply(400)
        finally:
            if temporary: temporary.unlink(missing_ok=True)

    def do_POST(self):
        if not self.allowed(): return
        if self.path != '/safeeat-release-publish/latest.json': return self.reply(404)
        self.connection.settimeout(30)
        try:
            size = int(self.headers.get('Content-Length', '0'))
            if not 0 < size <= 40000: return self.reply(413)
            data = json.loads(self.rfile.read(size))
            a, release = data['android'], data['release']
            match = NAME.fullmatch(a['name'])
            if not match: raise ValueError()
            version = '.'.join(match.groups())
            require(data['schemaVersion'] == 1 and data['repository'] == 'A1VAR0W/SafeEat')
            require(release['tag_name'] == 'v' + version and release['draft'] is False and release['prerelease'] is False)
            require(type(release['id']) is int and release['id'] > 0)
            require(release['html_url'] == 'https://github.com/A1VAR0W/SafeEat/releases/tag/v' + version)
            require(a['url'] == ORIGIN + '/safeeat-downloads/releases/' + a['name'])
            require(type(a['versionCode']) is int and 0 < a['versionCode'] <= 2100000000)
            with LOCK:
                apk = ROOT / a['name']
                require(apk.stat().st_size == a['size'])
                require(file_hash(apk) == a['sha256'])
                current = ROOT / 'latest.json'
                if current.exists():
                    previous = json.loads(current.read_text())['android']
                    require(a['versionCode'] >= previous['versionCode'])
                    if a['versionCode'] == previous['versionCode']: require(a['sha256'] == previous['sha256'])
                tmp = ROOT / '.latest.json.tmp'
                tmp.write_text(json.dumps(data, ensure_ascii=False), encoding='utf8')
                tmp.chmod(0o644)
                link = ROOT / '.latest.apk.tmp'
                link.unlink(missing_ok=True)
                link.symlink_to(a['name'])
                link.replace(ROOT / 'latest.apk')
                tmp.replace(current)
            self.reply(200)
        except (KeyError, ValueError, TypeError, OSError): self.reply(400)

if __name__ == '__main__':
    ROOT.mkdir(parents=True, exist_ok=True)
    http.server.ThreadingHTTPServer(('127.0.0.1', 8099), Handler).serve_forever()
