# 0004: GPUI Kit for the desktop

Status: selected by the user, 2026-10-19. Supersedes the direct GPUI dependency
in [ADR 0003](0003-gpui-desktop-and-first-host-slice.md); its Host boundary remains
unchanged. This decision is not acceptance of the complete product or all
platform input/accessibility behavior.

## Decision

Use Apache-2.0 `gpui-kit = "=0.7.3"` as the desktop's UI dependency, including
the matching `gpui-pre` 0.3.9 engine/platform family in its separate lockfile.
Import engine APIs through `gpui_kit`; do not combine direct GPUI 0.2.2 entities
with Kit entities. Keep the default upstream backend; GPUI Fast is not enabled.
Protocol, directory client and Session Host retain no dependency on the toolkit.

Kit provides components and shared focus/theme/window infrastructure that would
otherwise have to be built in Fleetpane. Initialize it once, install its assets,
and open each window through its `open_window` entry point, which mounts the
window Root. Use Kit Buttons for Refresh, Load next page and Copy session ID,
with loading/disabled state tied to the real request. Read colors from the
active theme rather than embedding a second palette.

Keep the existing engine `uniform_list` for this fixed-height directory: it
already computes only visible rows and shares the keyboard scroll handle. Kit
re-exports that engine API. Use stable session IDs for row identity and rem-based
layout helpers. A native window minimum and initial size remain physical-pixel
platform constraints. Detail/conversation and terminal components can be chosen
when those workflows are implemented; this change introduces no dummy composer.

The original GPUI 0.2.2 `xattr`/`ENOATTR` dependency constraint no longer applies:
the Kit dependency graph does not contain that `zed-async-tar`/`xattr` chain.
The lockfile retains its existing libc resolution as an ordinary tested
dependency, not a new source-level compatibility patch.

## Evidence and limits

API and bootstrap choices were checked against the published 0.7.3 source and
the [official release tag](https://github.com/longbridge/gpui-kit/tree/v0.7.3),
revision `67265a36e260ffe03c5aaebdcd8d654dbf892eab`. Its GPUI Kit skill,
Coding Guides and Design Guides informed component ownership, theme use and
stable identity. No upstream implementation or assets were copied into this
repository; Cargo supplies the licensed library and its default assets.

See the [Kit migration verification record](../verification/2026-10-19-gpui-kit.md)
for build and real-window evidence. Native text composition, screen-reader
behavior, terminal emulation and macOS/Windows still need independent acceptance.
The dependency change alone establishes no responsiveness or memory improvement.
