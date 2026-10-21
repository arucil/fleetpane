# Explicit Paseo scratch collection

Accepted 2026-10-21. Extends [durable batch receipts](0064-durable-paseo-batch-receipts.md).

Killing a copy process can leave its temporary SQLite database and rollback
journal. Retrying publishes an independent complete history but does not reclaim
the interrupted copy. The old temporary files live beside complete archives or
fork exports, and older binaries have no common lifetime lock. Their age, PID or
an interrupted batch receipt cannot prove that deleting them is safe.

New history copies and Codex context exports use a private `paseo-scratch-v1`
directory under the controller store. Each blocking copy/export holds a shared
`flock` on a stable private `paseo-scratch-v1.lock` inode from before scratch
creation through publication and cleanup. Concurrent writers remain allowed.
The guard belongs to the blocking operation, so cancelling its async awaiter
does not release the lock while work is still running. Actual process death
releases the lock through the OS. The lock file is never collected or unlinked.

`Store::paseo_scratch` implements both explicit inventory and collection. It runs
off the UI thread, acquires an exclusive nonblocking lock, and refuses an active
writer or collector. A new writer likewise refuses an active collection; there
is no automatic retry. A preview grants no deletion authority over a later
worker: collection acquires the lock and validates the files again.

The entire collection is checked before the first unlink: a private non-symlink
directory, at most 40,000 entries, canonical UUID `.tmp` or `.tmp-journal` names,
and private regular files. Unknown names, symlinks, nested directories, public
files and limit violations are explicit errors with no preflight deletion.
Before each unlink the engine checks the observed device/inode and file type
and permissions again. I/O failures stop the operation and report how many files
were already removed; deletion is not transactional. Successful nonempty cleanup
syncs the scratch directory. No contents are parsed and no recursive deletion is
used.

Reports contain file count, logical byte size, whether collection was requested,
and the count of older temporary filenames retained. Bytes are logical file
lengths, not guaranteed recovered disk blocks: hardlinks or sparse files can
differ. The engine inventories at most 40,000 entries in each old history/fork
directory and never deletes those files. New-version lock availability proves
nothing about an old binary writing an old-format path.

Only versioned scratch is eligible. Source transcripts, controller records,
published `.sqlite` histories, published `.jsonl` forks and lock inodes remain
outside deletion. A crash after hardlink publication can leave a scratch link;
removing that link preserves the published archive. Complete archive publication
and read paths keep their existing semantics. The controller schema is unchanged.

CLI `paseo-import scratch-status STORE` and `clean-scratch STORE` use the same
engine as the native import page. The native Clear button requires a successful
nonempty preview, displays counts/errors and becomes disabled after cleanup.
There is no startup scan, automatic deletion, age-based heuristic, archive-wide
quota or legacy-file collection. Those remain separate work.

[Local verification](../verification/2026-10-21-paseo-scratch.md) covers shared
writers, cancellation, paused/killed real copy processes, unchanged source and
published archives, legacy preservation, explicit error paths and Linux native
actions. Cross-platform native behavior and performance are not established.
