import json, socket, sys
from pathlib import Path
ROOT = Path('/tmp/fp-real-requests-final')

def raw(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(json.dumps(request).encode() + b'\n')
        result = json.loads(stream.makefile('rb').readline())
        assert result['response'] != 'error', result
        return result['data']

records = {}
for host in json.loads((ROOT / 'setup.json').read_text()):
    state = raw(host['socket'], {'command':'snapshot'})
    records[host['name']] = state
(ROOT / (sys.argv[1] + '.json')).write_text(json.dumps(records, indent=2))
print(json.dumps({name: {'requests': [p['id'] for p in state['pending_requests']],
    'activity':state['activity'], 'owner_generation':state['owner']['generation']} for name,state in records.items()}))
