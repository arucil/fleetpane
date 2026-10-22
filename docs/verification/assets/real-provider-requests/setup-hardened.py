from pathlib import Path
import json, os, shutil, socket, sqlite3, subprocess, time

ROOT = Path('/tmp/fp-real-requests-hardened')
ROOT.mkdir(mode=0o700)
REPO = Path('/home/amlo/Projects/Rust/fleetpane')
BINARY = REPO / 'target/debug/fleetpane-host'

def raw(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(json.dumps(request).encode() + b'\n')
        result = json.loads(stream.makefile('rb').readline())
        assert result['response'] != 'error', result
        return result['data']

def snap(endpoint):
    return raw(endpoint, {'command': 'snapshot'})

def cli(*args):
    result = subprocess.run([str(BINARY), *map(str, args)], text=True, capture_output=True, timeout=40)
    assert result.returncode == 0, result.stderr[-1200:]
    return json.loads(result.stdout)

hosts, records = [], []
for name in ['OpenCodex', 'DeepSeek']:
    home = ROOT / name
    home.mkdir(mode=0o700)
    state = home / 'provider-home'
    state.mkdir(mode=0o700)
    directory = home / 'workspace'
    directory.mkdir()
    (directory / 'notes.md').write_text('# Synthetic request QA 中文\n\nNo existing user files.\n')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('CODEX_', 'OPENAI_', 'DSH_'))}
    if name == 'OpenCodex':
        env['CODEX_HOME'] = str(state)
        (state / 'config.toml').write_text('model = "gpt-6.1-sol"\nmodel_provider = "opencodex"\napproval_policy = "on-request"\nsandbox_mode = "read-only"\n[model_providers.opencodex]\nname = "Existing OpenCodex loopback proxy"\nbase_url = "http://127.0.0.1:10100/v1"\nwire_api = "responses"\nrequires_openai_auth = false\nrequest_max_retries = 0\nstream_max_retries = 0\n')
    else:
        env['DSH_HOME'] = str(state)
        credential = state / '.credentials.yaml'
        shutil.copyfile(Path.home() / '.dsh/.credentials.yaml', credential)
        credential.chmod(0o600)
    endpoint = home / 'host/control.sock'
    with (home / 'host.log').open('w') as log:
        process = subprocess.Popen([str(BINARY), 'serve' if name == 'OpenCodex' else 'serve-acp', str(endpoint), str(home)], stdout=subprocess.DEVNULL, stderr=log, env=env, start_new_session=True)
    record = {'name': name, 'socket': str(endpoint), 'host_pid': process.pid, 'directory': str(directory)}
    records.append(record)
    (ROOT / 'setup.json').write_text(json.dumps(records, indent=2))
    deadline = time.monotonic() + 30
    while not (endpoint.exists() and 'ready:' in (home / 'host.log').read_text()):
        assert process.poll() is None, (home / 'host.log').read_text()[-1000:]
        assert time.monotonic() < deadline, 'provider readiness deadline'
        time.sleep(.1)
    hosts.append({'name': name, 'socket': str(endpoint)})
    (ROOT / 'hosts.json').write_text(json.dumps({'version': 1, 'hosts': hosts}))
    workspace = cli('workspace', ROOT / 'hosts.creations', 'add', ROOT / 'hosts.json', name, 'folder', directory)
    thread = cli('session', endpoint, '--take-control', 'create', directory)['result']['thread']
    record.update({'workspace': workspace, 'thread': thread, 'snapshot_created': snap(endpoint)})
    prompt = 'Synthetic local QA. Use the shell tool to run exactly this command in the current workspace: printf "REQUEST-QA-20261022\\n" > request-qa.txt . Do not use an alternative write tool. If approval is requested, wait for the human decision. Do not retry a denied action. After a decision, only summarize its outcome.'
    if name == 'DeepSeek':
        prompt = 'Synthetic local permission QA: use the bash tool once to run printf "DEEPSEEK-APPROVAL-QA\\n" with sandbox_permissions set to danger-full-access and justification "Verify the one-shot permission dialog in this isolated synthetic workspace". Wait for the human permission decision. Do not retry if declined. Do not modify files or access the network.'
    record['prompt'] = prompt
    record['sent'] = cli('session', endpoint, '--take-control', 'send', thread['id'], prompt)
    (ROOT / 'setup.json').write_text(json.dumps(records, indent=2))
    print(name, 'started', flush=True)

layout = {'tabs': [], 'selected': records[0]['workspace']['id'], 'visible': True}
for record in records:
    w = record['workspace']; d = w['directory']
    layout['tabs'].append({'workspace': w['id'], 'socket': w['endpoint']['socket'], 'directory': d['path'], 'device': d['device'], 'inode': d['inode'], 'session': {'host_id': record['snapshot_created']['host_id'], 'thread_id': record['thread']['id'], 'cwd': d['path'], 'archived': False}, 'panel': 'conversation', 'files': {'directory': '', 'file': None}, 'terminal': None, 'sidebar_width': 280, 'sidebar_visible': True})
with sqlite3.connect(ROOT / 'hosts.creations/creations.sqlite3') as db:
    db.execute('INSERT INTO workspace_layout VALUES(1,1,?)', (json.dumps(layout),))
(ROOT / 'baseline.json').write_text(json.dumps({r['name']: snap(r['socket']) for r in records}, indent=2))
print('ready', flush=True)
