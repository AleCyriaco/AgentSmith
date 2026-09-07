"""Read-only protocol handshake. Never authenticates or sends a model prompt."""
import json, os, pathlib, selectors, subprocess, tempfile, time

ROOT = pathlib.Path(__file__).resolve().parents[1]
CLIENTS = {
    'codex': ['/Applications/ChatGPT.app/Contents/Resources/codex', 'app-server', '--listen', 'stdio://'],
    'gemini': [str(ROOT / 'test-runtime/node_modules/.bin/gemini'), '--experimental-acp', '--extensions', 'none'],
    'grok': [str(pathlib.Path.home() / '.grok/bin/grok'), '--tools', '', '--deny', '*', '--permission-mode', 'dontAsk', '--no-subagents', '--no-memory', '--disable-web-search', 'agent', '--no-leader', 'stdio'],
}
for name, args in CLIENTS.items():
    with tempfile.TemporaryDirectory(prefix='agentsmith-probe-') as directory:
        env = os.environ.copy()
        for key in ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'GEMINI_API_KEY', 'GOOGLE_API_KEY', 'XAI_API_KEY']:
            env.pop(key, None)
        settings = pathlib.Path(directory) / 'settings.json'
        settings.write_text(json.dumps({'tools': {'core': ['agentsmith_no_local_tools']}, 'mcp': {'allowed': ['agentsmith_no_mcp']}, 'hooksConfig': {'enabled': False}, 'security': {'auth': {'selectedType': None, 'enforcedType': 'oauth-personal', 'useExternal': True}}, 'advanced': {'ignoreLocalEnv': True}}))
        env['GEMINI_CLI_SYSTEM_SETTINGS_PATH'] = str(settings)
        params = {'clientInfo': {'name': 'agentsmith_probe', 'version': '0.2.0'}} if name == 'codex' else {'protocolVersion': 1, 'clientCapabilities': {'fs': {'readTextFile': False, 'writeTextFile': False}, 'terminal': False}}
        with subprocess.Popen(args, cwd=directory, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as process:
            try:
                process.stdin.write((json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': params}) + '\n').encode())
                process.stdin.flush()
                sel = selectors.DefaultSelector()
                sel.register(process.stdout, selectors.EVENT_READ)
                end = time.monotonic() + 25
                buf = b''
                done = False
                while time.monotonic() < end and not done:
                    if not sel.select(1):
                        continue
                    chunk = os.read(process.stdout.fileno(), 65536)
                    if not chunk:
                        raise RuntimeError('client exited before initialization')
                    buf += chunk
                    while b'\n' in buf:
                        line, buf = buf.split(b'\n', 1)
                        try:
                            reply = json.loads(line)
                        except ValueError:
                            continue
                        if reply.get('id') == 1:
                            result = reply.get('result', {})
                            print(name, json.dumps({'initialized': 'result' in reply, 'version': result.get('agentInfo', {}).get('version'), 'image': result.get('agentCapabilities', {}).get('promptCapabilities', {}).get('image'), 'authMethods': [m.get('id') for m in result.get('authMethods', [])]}), flush=True)
                            done = True
                if not done:
                    raise RuntimeError('initialization timeout')
            except Exception as error:
                print(name, type(error).__name__, str(error), flush=True)
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
