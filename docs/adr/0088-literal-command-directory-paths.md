# Literal command paths resolve against the command directory

Provider-reported command actions can describe relative file paths while the
command's cwd differs from its conversation's cwd. Resolving against the
conversation root can silently open a different same-name file.

Resolve each explicit read path against its supplied command directory before
emitting file navigation. Preserve literal filenames and absolute targets; do not
decode URL escapes, interpret line suffixes, scan the shell command or infer a
different file from its output. Invalid relative directories stay visible as card
errors. Original conversation Host/session/cwd validation still applies, and
workspace file opening enforces root identity and rejects parent traversal,
symlinks and special files. The path resolver performs no filesystem I/O.
