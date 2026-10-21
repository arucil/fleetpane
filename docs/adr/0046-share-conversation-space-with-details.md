# ADR 0046: Share remaining conversation space with scrollable details

Status: accepted for the covered Linux native workflow, 2026-10-21.

## Problem and evidence

Paseo root-history review 132 changes ScrollView to inverted FlatList. Fleetpane
already uses a virtual GPUI list with stable retained row entities, bounded
history pages and explicit tail intent. Replacing the list is not the missing
behavior: at 900 × 560 the conversation can have zero height. A before capture
reproduces this with a 20-line draft, one image, agent tasks and another session's
pending approval. The old layout gives configuration/images fixed editor space
and puts an unconstrained details sibling ahead of the list's remaining space.
ADR 0041 kept Send/Stop reachable but explicitly left this reading gap open.

## Decision

Keep the same virtual list, textarea, draft, question and permission entities.
After the session header/navigation and fixed editor/button strip, a flexible
conversation body owns the actual remaining height. Its list takes free space;
its independently scrollable details region uses at most half that body. The
percentage is relative to the body's computed size, not the whole window. Short
details retain their natural height. No arbitrary pixel minimum is added that
could push Send/Stop out of the window.

Move the existing draft status, model options, attachment controls/previews and
ACP follow-up/Stop messages into the end of the scrollable details content. Keep
their handlers and enabled conditions. They can now yield height along with
tasks, history notices, approvals and questions. At small sizes, users scroll
this region to reach options or answers while still reading messages. Model
menus remain Kit overlays; the editor and Send/Stop stay outside this scroller.
The textarea still has its existing one-to-six / small-window three-row limit.

No query, send, answer, Stop, provider lifetime, draft revision or timeline
identity changes are part of layout. Manual history scrolling and details
scrolling remain independent; no timer, retry or new scroll observer is added.

## Acceptance and limits

[Before/after native evidence](../verification/2026-10-21-conversation-space.md)
covers the same 900 × 560 fixture, history paging/scrolling, exact copied and
sent text/image, model menu, native approval/Stop, resize, and a two-question
Codex protocol exchange while a separate ACP Host remains unchanged. All 11
desktop tests, strict Clippy, format and build pass. No synthetic test merely
repeats style declarations; rendered behavior is the acceptance.

This allocates the existing conversation body's space. Arbitrarily large outer
error/drain/route banners can still consume that body's height; general pane
sizing, accessibility/keyboard-only navigation, IME, other-platform behavior and
performance budgets are separate acceptance. A small details viewport requires
scrolling and does not promise every control is simultaneously visible.
