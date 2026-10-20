# GPUI Component 0.7.3 patch

`gpui-component/` contains the published 0.7.3 crate source from
[GPUI Kit](https://github.com/longbridge/gpui-kit/tree/v0.7.3/crates/component),
under its included Apache-2.0 license. The desktop patches this crate locally;
the Kit and GPUI engine versions remain 0.7.3 and 0.3.9.

Published crate archive SHA-256:
`465a363235cd4062ef5f0953fc096fc764ed6f53dfbd9f409f669f93ea3b1ff9`.

The sole upstream source change is in `src/diff/state.rs`: initialize the
virtualized diff list with 600 px of overdraw instead of zero. In GPUI 0.3.9,
`List::prepaint` invalidates cached heights and initial uniform hints on first
layout and width changes. Zero overdraw then leaves offscreen rows at zero height,
preventing physical scrolling to the patch's end. A nearby measurement region
lets the list discover rows as it scrolls. It does not measure the full patch.

This follows the same configuration fix as the application's conversation list.
Native regression evidence covers the first open, bottom deleted line/no-newline
notice, scrolling back up, and resize; see
[rendering verification](../../../docs/verification/2026-10-19-rich-rendering.md).

Upstream metadata/build lockfiles are omitted; source, localization, build script,
test fixtures, published manifest and license are retained. Replace this vendored
crate when an upstream release contains and verifies the fix. Do not edit Cargo's
registry cache or silently drop the patch during an update.
