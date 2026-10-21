import pathlib,sqlite3,time
root=pathlib.Path('/tmp/fleetpane-recent-qa')
with sqlite3.connect(root/'hosts.creations/creations.sqlite3') as db:
    db.execute('BEGIN EXCLUSIVE')
    (root/'lock-ready').touch()
    print('exclusive local-state lock held',flush=True)
    while not (root/'lock-release').exists():time.sleep(.025)
    db.commit()
print('lock released',flush=True)
