import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / '.github/workflows/release-windows.yml'
NAME = 'OpenTouchDeck-windows-x64-setup.exe'


def workflow_shell(step):
    source = WORKFLOW.read_text(encoding='utf-8').split(f'      - name: {step}\n', 1)[1]
    lines = source.split('        run: |\n', 1)[1].splitlines()
    body = []
    for line in lines:
        if line.strip() and not line.startswith('          '):
            break
        body.append(line[10:])
    return '\n'.join(body)


@unittest.skipUnless(shutil.which('bash') and shutil.which('jq'), 'bash and jq required')
class ReleaseWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assets = self.root / 'release-assets'
        self.assets.mkdir()
        (self.root / 'signed-assets').mkdir()
        self.data = b'synthetic installer'
        self.digest = hashlib.sha256(self.data).hexdigest()
        self.manifest = dict(version='1.0.0', test_signing=False, source_revision='a' * 40,
                             installer=NAME, installer_sha256=self.digest, updater_public_key='public')
        self.env = dict(os.environ, INSTALLER=NAME, EXPECTED_SHA256=self.digest,
                        SOURCE_REVISION='a' * 40, TAGGED_SHA='a' * 40, RELEASE_TAG='v1.0.0',
                        GITHUB_EVENT_NAME='push', GITHUB_REF_TYPE='tag', GITHUB_REF_NAME='v1.0.0',
                        GITHUB_ENV='github-env', RUNNER_TEMP=self.root.as_posix(),
                        UPDATER_PUBLIC_KEY='public', GH_REPO='example/project')

    def prepare(self):
        (self.assets / NAME).write_bytes(self.data)
        (self.assets / 'release-manifest.json').write_text(json.dumps(self.manifest))
        (self.assets / 'latest.json').write_text('{"notes":"untrusted build feed"}')
        (self.assets / (NAME + '.sig')).write_text('unsigned placeholder')
        (self.root / 'signed-assets' / (NAME + '.sig')).write_text('sign job signature')
        (self.assets / 'SHA256SUMS').write_text(''.join(
            f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n'
            for p in self.assets.iterdir() if p.name != 'SHA256SUMS'), newline='\n')

    def run_shell(self, name, prelude):
        script = self.root / 'check.sh'
        script.write_text(prelude + '\n' + workflow_shell(name), newline='\n')
        return subprocess.run(['bash', script.as_posix()], cwd=self.root, env=self.env,
                              capture_output=True, text=True)

    def test_signer_rejects_invalid_identity_before_signing(self):
        prelude = '''git() {
          if [ "$1" = fetch ]; then return 0; fi
          if [ "$1" = show ]; then printf '%s' '{"version":"1.0.0"}'; else return 1; fi
        }'''
        for field, value in [(None, None), ('version', '9.9.9'), ('test_signing', True),
                             ('installer_sha256', 'b' * 64), ('source_revision', 'b' * 40)]:
            with self.subTest(field=field):
                original = dict(self.manifest)
                if field:
                    self.manifest[field] = value
                self.prepare()
                result = self.run_shell('Check build hash and version', prelude)
                self.assertEqual(result.returncode == 0, field is None, result.stderr)
                self.manifest = original
        self.prepare()
        self.env['GITHUB_REF_NAME'] = 'v2.0.0'
        self.assertNotEqual(self.run_shell('Check build hash and version', prelude).returncode, 0)

    def test_publish_assembles_from_original_artifacts(self):
        self.prepare()
        annotation = json.dumps({'object': {'type': 'commit', 'sha': 'a' * 40},
                                 'message': 'Trusted tag notes', 'tagger': {'date': '2026-01-01T00:00:00Z'}})
        prelude = '''gh() {
          case "$2" in
            */git/ref/tags/v1.0.0) printf '%s' '{"object":{"type":"tag","sha":"annotation"}}' ;;
            */git/tags/annotation) printf '%s' 'ANNOTATION' ;;
            *) return 1 ;;
          esac
        }'''.replace('ANNOTATION', annotation)
        result = self.run_shell('Check the downloaded assets belong to this tag', prelude)
        self.assertEqual(result.returncode, 0, result.stderr)
        latest = json.loads((self.assets / 'latest.json').read_text())
        self.assertEqual(latest['notes'].strip(), 'Trusted tag notes')
        self.assertEqual(latest['platforms']['windows-x86_64']['signature'], 'sign job signature')
        self.assertEqual(latest['platforms']['windows-x86_64']['size'], len(self.data))
        self.prepare()
        (self.assets / NAME).write_bytes(b'tampered')
        self.assertNotEqual(self.run_shell('Check the downloaded assets belong to this tag', prelude).returncode, 0)
        self.data = b'tampered with consistent checksums'
        self.prepare()
        self.assertNotEqual(self.run_shell('Check the downloaded assets belong to this tag', prelude).returncode, 0)


if __name__ == '__main__':
    unittest.main()