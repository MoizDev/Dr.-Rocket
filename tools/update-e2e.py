#!/usr/bin/env python3
"""Scratch-only signed HTTPS updater assurance. No production keys, archives or manifests.

Builds and installs two real app binaries via automation, asserting relaunch and version
advancement. macOS uses scratch app bundles and a local unsigned DMG, Windows a zip, Linux
a tarball. --staging-only exercises transport/signatures/receipts without a native install.
"""
import argparse
import functools
import http.server
import json
import os
import plistlib
from pathlib import Path
import shutil
import ssl
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import zipfile

REPO = Path(__file__).resolve().parents[1]


def run(args, **kwargs):
    shown = [str(a) if '\n' not in str(a) else '<inline fixture script>' for a in args]
    print('+', ' '.join(shown), flush=True)
    try:
        return subprocess.run(list(map(str, args)), cwd=REPO, check=True, **kwargs)
    except subprocess.CalledProcessError as error:
        if error.stdout:
            print(error.stdout, file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        raise


def wait_for(predicate, message, seconds=90):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.5)
    raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--staging-only', action='store_true')
    args = parser.parse_args()
    staging_only = args.staging_only
    platform = 'windows' if os.name == 'nt' else 'mac' if sys.platform == 'darwin' else 'linux'
    with tempfile.TemporaryDirectory(prefix='drrocket-update-e2e-') as temporary:
        root = Path(temporary).resolve()
        config = root / 'config'
        config.mkdir()
        openssl = shutil.which('openssl')
        if not openssl and os.name == 'nt':
            openssl = r'C:\Program Files\Git\usr\bin\openssl.exe'
        if not openssl:
            raise RuntimeError('OpenSSL is required for the ephemeral localhost TLS certificate')
        cert = root / 'localhost.pem'
        tls_key = root / 'localhost.key'
        run([openssl, 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
             '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost',
             '-keyout', tls_key, '-out', cert], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        keygen = """
          const fs = require('fs'), crypto = require('crypto');
          const {privateKey, publicKey} = crypto.generateKeyPairSync('ed25519');
          fs.writeFileSync(process.argv[1], privateKey.export({format:'pem',type:'pkcs8'}), {mode:0o600});
          console.log(Buffer.from(publicKey.export({format:'jwk'}).x, 'base64url').toString('base64'));
        """
        signing_key = root / 'ephemeral.pem'
        public = run(['node', '-e', keygen, signing_key], capture_output=True, text=True).stdout.strip()
        served = root / 'served'
        served.mkdir()
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(served)))
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, tls_key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        origin = f'https://localhost:{server.server_port}'
        env = {**os.environ, 'CARGO_BUILD_JOBS': '2', 'DRROCKET_CONFIG_DIR': str(config),
               'DRROCKET_UPDATE_FEED': f'{origin}/feed.json', 'DRROCKET_RENDERER': 'software',
               'DRROCKET_E2E_ROOT': str(root), 'DRROCKET_E2E_ORIGIN': origin,
               'DRROCKET_E2E_PUBLIC_KEY': public, 'DRROCKET_E2E_CA': str(cert), 'DRROCKET_E2E_VERSION': '98.0.0'}
        processes = []
        log = None
        try:
            # Explicit feature, debug profile, compile-time ephemeral key/CA/origin/scratch root.
            build = ['cargo', 'build', '--locked', '--features', 'updater-e2e']
            target = Path(env.get('CARGO_TARGET_DIR', str(REPO / 'target'))).resolve() / 'debug'
            exe_name = 'drrocket.exe' if os.name == 'nt' else 'drrocket'
            relative_exe = Path('Dr. Rocket.app/Contents/MacOS/drrocket') if platform == 'mac' else Path('Dr. Rocket') / ('Dr. Rocket.exe' if os.name == 'nt' else 'drrocket')
            installed = root / 'installed' / relative_exe

            def bundle_info(executable, version):
                if platform == 'mac':
                    info = {'CFBundleExecutable': 'drrocket', 'CFBundleIdentifier': 'com.drrocket.update-e2e',
                            'CFBundleName': 'Dr. Rocket E2E', 'CFBundlePackageType': 'APPL',
                            'CFBundleVersion': version, 'CFBundleShortVersionString': version,
                            'NSHighResolutionCapable': True}
                    (executable.parent.parent / 'Info.plist').write_bytes(plistlib.dumps(info))
            installed.parent.mkdir(parents=True)
            if not staging_only:
                run(build, env=env)
                shutil.copy2(target / exe_name, installed)
                bundle_info(installed, '98.0.0')
                run(build, env={**env, 'DRROCKET_E2E_VERSION': '99.0.0'})
                payload = root / 'new' / relative_exe
                payload.parent.mkdir(parents=True)
                shutil.copy2(target / exe_name, payload)
                bundle_info(payload, '99.0.0')
                if platform == 'mac':
                    archive = served / 'update.dmg'
                    run(['hdiutil', 'create', '-volname', 'DrRocketE2E', '-srcfolder', root / 'new',
                         '-ov', '-format', 'UDZO', archive])
                elif os.name == 'nt':
                    archive = served / 'update.zip'
                    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
                        output.write(payload, f'Dr. Rocket/{payload.name}')
                else:
                    archive = served / 'update.tar.gz'
                    with tarfile.open(archive, 'w:gz') as output:
                        output.add(payload.parent, arcname='Dr. Rocket')
            else:
                archive = served / 'fixture.archive'
                archive.write_bytes(b'staging fixture only; not a native installation archive')
            # Sign the exact same protocol as production, without invoking the production CLI.
            signer = """
              const fs = require('fs'), crypto = require('crypto');
              const [key, file, origin, os, out, mode] = process.argv.slice(1);
              const a = {url: origin+'/'+require('path').basename(file), sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};
              if (mode === 'mismatch') a.sha256 = '0'.repeat(64);
              const k = mode === 'wrong-key' ? crypto.generateKeyPairSync('ed25519').privateKey : fs.readFileSync(key);
              a.signature = crypto.sign(null, Buffer.from(`drrocket-update-v1\\n99.0.0\\n${os}\\n${a.sha256}\\n${a.url}\\n`), k).toString('base64');
              fs.writeFileSync(out, JSON.stringify({version:'99.0.0',downloads:{[os]:a}}));
            """
            for mode in ['feed', 'mismatch', 'wrong-key']:
                run(['node', '-e', signer, signing_key, archive, origin, platform, served / f'{mode}.json', mode], env=env)
                if mode != 'feed':
                    shutil.copy2(served / f'{mode}.json', root / f'{mode}.json')
            # Run the test executable inside its compiled scratch sandbox (Cargo itself remains
            # in the feature worktree). Reuses dependencies; never edits Cargo.toml or versions.
            built = run(['cargo', 'test', '--locked', '--features', 'updater-e2e', '--no-run', '--message-format=json'], env=env, capture_output=True, text=True)
            run(['cargo', 'clippy', '--locked', '--features', 'updater-e2e', '--all-targets', '--', '-D', 'warnings'], env=env)
            messages = [json.loads(line) for line in built.stdout.splitlines() if line.startswith('{')]
            # Exercise the actual compiled build-script guard without compiling a release app.
            build_script = next(Path(m['filenames'][0]) for m in messages
                                if m.get('reason') == 'compiler-artifact'
                                and Path(m.get('target', {}).get('src_path', '')).resolve() == REPO / 'build.rs')
            rejected = subprocess.run([build_script], cwd=REPO,
                                      env={**env, 'CARGO_FEATURE_UPDATER_E2E': '1', 'PROFILE': 'release', 'DEBUG': 'true'},
                                      capture_output=True, text=True)
            assert rejected.returncode != 0 and 'forbidden in release profiles' in rejected.stderr
            test_exe = next(Path(m['executable']) for m in messages if m.get('reason') == 'compiler-artifact' and m.get('executable') and m.get('profile', {}).get('test'))
            sandbox_test = root / test_exe.name
            shutil.copy2(test_exe, sandbox_test)
            run([sandbox_test, 'update::tests', '--include-ignored', '--nocapture'],
                env={**env, 'DRROCKET_E2E_PUBLIC_KEY': 'runtime values cannot replace the compiled key'})
            with tempfile.TemporaryDirectory(prefix='drrocket-outside-sandbox-') as outside:
                rejected = subprocess.run([sandbox_test, 'update::tests::signed_https_staging', '--include-ignored'],
                                          cwd=REPO, env={**env, 'DRROCKET_CONFIG_DIR': outside}, capture_output=True, text=True)
                assert rejected.returncode != 0, 'test build escaped its compiled config sandbox'
                assert not list(Path(outside).iterdir()), 'test build wrote outside its compiled sandbox'
            # Remove the fixture's staging state so the real app must download for itself.
            shutil.rmtree(config / 'update', ignore_errors=True)
            if staging_only:
                print('PASS: signed HTTPS staging and negative regressions (no native install attempted)', flush=True)
                return
            assert run([installed, '--version'], capture_output=True, text=True, env=env).stdout.strip() == '98.0.0'
            receipt = config / 'update' / '99.0.0' / 'receipt.json'
            first = root / 'stage.json'
            first.write_text(json.dumps({'steps': [{'wait_ms': 120000}, {'quit': True}]}))
            log = (root / 'app.log').open('w')
            proc = subprocess.Popen([installed], cwd=root, env={**env, 'DRROCKET_AUTOMATION': str(first)}, stdout=log, stderr=log)
            processes.append(proc)
            wait_for(lambda: receipt.exists() or proc.poll() is not None, 'app did not stage signed update')
            if not receipt.exists():
                raise RuntimeError((root / 'app.log').read_text())
            # Terminate before quit-to-install; next launch must recover/reverify the receipt.
            proc.terminate()
            proc.wait(timeout=20)
            marker = root / 'relaunched.png'
            second = root / 'restart.json'
            second.write_text(json.dumps({'steps': [{'wait_ms': 3000}, {'command': 'RestartToUpdate'},
                                                    {'wait_ms': 1000}, {'screenshot': str(marker)}, {'quit': True}]}))
            proc = subprocess.Popen([installed], cwd=root, env={**env, 'DRROCKET_AUTOMATION': str(second)}, stdout=log, stderr=log)
            processes.append(proc)
            proc.wait(timeout=90)
            wait_for(marker.exists, 'new app did not relaunch and render')
            wait_for(lambda: not Path(str(installed) + '.old').exists(), 'old executable not cleaned up')
            actual = run([installed, '--version'], capture_output=True, text=True, env=env).stdout.strip()
            assert actual == '99.0.0', f'installed version did not advance: {actual}'
            # A successful child startup also clears obsolete staged updates.
            wait_for(lambda: not receipt.exists(), 'staged receipt was not cleaned up')
            print('PASS: native signed HTTPS download, restart, install, relaunch and 98 -> 99 advancement', flush=True)
        except BaseException:
            if (root / 'app.log').exists():
                print((root / 'app.log').read_text(errors='replace'), file=sys.stderr)
            raise
        finally:
            for proc in processes:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait(timeout=20)
            child_pid = root / 'child.pid'
            if child_pid.exists():
                pid = int(child_pid.read_text())
                if os.name == 'nt':
                    subprocess.run(['taskkill', '/PID', str(pid), '/T', '/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                else:
                    try:
                        os.kill(pid, 15)
                    except ProcessLookupError:
                        pass
            if log:
                log.close()
            server.shutdown()


if __name__ == '__main__':
    main()
