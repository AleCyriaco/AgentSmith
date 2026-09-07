"""Build and bundle FreeRDP's native dependencies for a relocatable macOS app."""
from pathlib import Path
import subprocess, shutil, os
root=Path(__file__).resolve().parents[1]
prefix=Path(subprocess.check_output(['brew','--prefix','freerdp'],text=True).strip())
out=root/'src-tauri/resources/rdp'
out.mkdir(parents=True,exist_ok=True)
exe=out/'agentsmith-rdp'
subprocess.run(['clang','-O2','-Wno-unused-result',str(root/'native/rdp_worker.c'),'-I'+str(prefix/'include/freerdp3'),'-I'+str(prefix/'include/winpr3'),'-L'+str(prefix/'lib'),'-lfreerdp3','-lwinpr3','-o',str(exe)],check=True)
seen=set()
def bundle(binary):
    if str(binary) in seen:return
    seen.add(str(binary))
    deps=subprocess.check_output(['otool','-L',str(binary)],text=True).splitlines()[1:]
    for item in deps:
        dep=item.strip().split(' (')[0]
        if not dep.startswith(('/opt/homebrew/','/usr/local/')):continue
        target=out/Path(dep).name
        if not target.exists():
            shutil.copy2(dep,target)
            os.chmod(target,0o755)
            subprocess.run(['install_name_tool','-id','@loader_path/'+target.name,str(target)],check=True)
        subprocess.run(['install_name_tool','-change',dep,'@loader_path/'+target.name,str(binary)],check=True)
        bundle(target)
bundle(exe)
for file in out.iterdir():
    if file.suffix=='.dylib' or file==exe:subprocess.run(['codesign','--force','--sign','-',str(file)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
shutil.copy2(prefix/'LICENSE',out/'FreeRDP-LICENSE')
print('FreeRDP helper and native dependencies:',out)
