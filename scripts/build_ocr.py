"""Compile Apple Vision OCR. PNG input/output stays in memory over pipes."""
from pathlib import Path
import subprocess,tempfile
root=Path(__file__).resolve().parents[1]
out=root/'src-tauri/resources/ocr/agentsmith-ocr'
out.parent.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='agentsmith-swift-cache-') as cache:
 subprocess.run(['xcrun','swiftc','-O','-module-cache-path',cache,'-target','arm64-apple-macos26.0',str(root/'native/ocr_worker.swift'),'-o',str(out),'-framework','Vision','-framework','ImageIO'],check=True)
subprocess.run(['codesign','--force','--sign','-',str(out)],check=True)
print('Apple Vision OCR bundled')
