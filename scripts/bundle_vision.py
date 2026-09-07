"""Bundle the pinned official Apple Silicon llama.cpp server and its dependencies.

Usage: python3 scripts/bundle_vision.py [previously downloaded archive]
Models are downloaded by the app, never included in the application bundle.
"""
from pathlib import Path
import hashlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

VERSION = 'b10830'
SHA256 = '1a9359b3fe59a1345bac65323c43dfcf1c70ec0bd746afa09b29020ba0c33151'
URL = f'https://github.com/ggml-org/llama.cpp/releases/download/{VERSION}/llama-{VERSION}-bin-macos-arm64.tar.gz'
out = Path(__file__).resolve().parents[1] / 'src-tauri/resources/vision'
with tempfile.TemporaryDirectory(prefix='agentsmith-vision-build-') as temp:
    temp = Path(temp)
    archive = Path(sys.argv[1]) if len(sys.argv) > 1 else temp / 'engine.tar.gz'
    if len(sys.argv) == 1:
        urllib.request.urlretrieve(URL, archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError('Official engine archive hash mismatch')
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            if member.name.startswith('/') or '..' in Path(member.name).parts:
                raise RuntimeError('Invalid archive path')
        tar.extractall(temp, filter='data')
    src = temp / f'llama-{VERSION}'
    out.mkdir(parents=True, exist_ok=True)
    seen = set()
    def bundle(name):
        if name in seen:
            return
        seen.add(name)
        shutil.copy2(src / name, out / name)
        deps = subprocess.check_output(['otool', '-L', str(src / name)], text=True)
        for line in deps.splitlines()[1:]:
            dep = line.strip().split(' (')[0]
            if dep.startswith('@rpath/'):
                bundle(Path(dep).name)
    bundle('llama-server')
    shutil.copy2(src / 'LICENSE', out / 'llama.cpp-LICENSE')
    for name in seen:
        subprocess.run(['codesign', '--force', '--sign', '-', str(out / name)], check=True, capture_output=True)
    print(f'llama.cpp {VERSION}: {sum((out / n).stat().st_size for n in seen) / 1e6:.1f} MB')
