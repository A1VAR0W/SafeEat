"""Exercise the upload bridge in a temporary directory, without production access."""
import hashlib
import http.client
import json
import os
import pathlib
import tempfile
import threading
import unittest

os.environ.setdefault('SAFEEAT_RELEASE_PUBLISH_TOKEN', 'isolated-test-token')
import importlib.util

spec = importlib.util.spec_from_file_location('publisher', pathlib.Path(__file__).with_name('release-publisher.py'))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class PublisherTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='safeeat-publisher-test-')
        publisher.ROOT = pathlib.Path(self.folder.name)
        publisher.TOKEN = 'isolated-test-token'
        self.server = publisher.http.server.ThreadingHTTPServer(('127.0.0.1', 0), publisher.Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.apk = b'A' * 100001
        self.name = 'SafeEat-1.0.0.apk'
        self.manifest = {
            'schemaVersion': 1, 'repository': 'A1VAR0W/SafeEat',
            'release': {'id': 1, 'tag_name': 'v1.0.0', 'draft': False, 'prerelease': False,
                        'html_url': 'https://github.com/A1VAR0W/SafeEat/releases/tag/v1.0.0'},
            'android': {'name': self.name, 'url': publisher.ORIGIN + '/safeeat-downloads/releases/' + self.name,
                        'size': len(self.apk), 'sha256': hashlib.sha256(self.apk).hexdigest(), 'versionCode': 76},
        }

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.folder.cleanup()

    def request(self, method, filename, body, authorized=True):
        connection = http.client.HTTPConnection(*self.server.server_address, timeout=5)
        headers = {'Authorization': 'Bearer isolated-test-token'} if authorized else {}
        connection.request(method, '/safeeat-release-publish/' + filename, body, headers)
        response = connection.getresponse()
        status = response.status
        response.read()
        connection.close()
        return status

    def publish(self):
        return self.request('POST', 'latest.json', json.dumps(self.manifest).encode())

    def test_authentication_and_path_restrictions(self):
        self.assertEqual(self.request('PUT', self.name, self.apk, False), 401)
        self.assertEqual(self.request('PUT', '../outside.apk', self.apk), 400)
        self.assertEqual(list(publisher.ROOT.iterdir()), [])

    def test_upload_publish_and_idempotent_retry(self):
        self.assertEqual(self.request('PUT', self.name, self.apk), 201)
        self.assertEqual(self.request('PUT', self.name, self.apk), 201)
        self.assertEqual(self.publish(), 200)
        self.assertEqual(json.loads((publisher.ROOT / 'latest.json').read_text()), self.manifest)
        self.assertEqual((publisher.ROOT / 'latest.apk').read_bytes(), self.apk)
        self.assertEqual(self.publish(), 200)

    def test_reject_changed_apk_and_invalid_hash(self):
        self.assertEqual(self.request('PUT', self.name, self.apk), 201)
        self.assertEqual(self.request('PUT', self.name, b'B' * len(self.apk)), 409)
        self.manifest['android']['sha256'] = '0' * 64
        self.assertEqual(self.publish(), 400)
        self.assertFalse((publisher.ROOT / 'latest.json').exists())

    def test_reject_foreign_url_and_unstable_release(self):
        self.assertEqual(self.request('PUT', self.name, self.apk), 201)
        self.manifest['release']['draft'] = True
        self.assertEqual(self.publish(), 400)
        self.manifest['release']['draft'] = False
        self.manifest['android']['url'] = 'https://example.com/foreign.apk'
        self.assertEqual(self.publish(), 400)

    def test_reject_version_code_rollback(self):
        self.assertEqual(self.request('PUT', self.name, self.apk), 201)
        self.assertEqual(self.publish(), 200)
        self.manifest['android']['versionCode'] = 75
        self.assertEqual(self.publish(), 400)
        self.assertEqual(json.loads((publisher.ROOT / 'latest.json').read_text())['android']['versionCode'], 76)


if __name__ == '__main__':
    unittest.main()
