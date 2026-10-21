#!/usr/bin/env python3
"""Run owned cross-Host activity data until SIGINT/SIGTERM, for native QA."""
import argparse
import json
import runpy
import signal
import threading
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path, help='New absolute private directory')
    args = parser.parse_args()
    if not args.root.is_absolute():
        parser.error('root must be absolute')
    args.root.mkdir(mode=0o700)
    definitions = runpy.run_path(str(Path(__file__).with_name('fleet-directory-smoke.py')))
    fixture = definitions['Fixture'](args.root, activity=True)
    stopped = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopped.set())
    signal.signal(signal.SIGINT, lambda *_: stopped.set())
    try:
        fixture.start()
        (args.root / 'before.json').write_text(json.dumps([fixture.snapshot(i) for i in range(4)], indent=2) + '\n')
        (args.root / 'pids.json').write_text(json.dumps([p.pid for p in fixture.processes]))
        print('READY ' + str(fixture.config), flush=True)
        stopped.wait()
    finally:
        fixture.close()


if __name__ == '__main__':
    main()
