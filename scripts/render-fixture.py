#!/usr/bin/env python3
"""Write synthetic histories/assets for native rendering checks.

Use provider-fixture.py --history OUTPUT/history.json with a throwaway Host.
No model, account, real workspace or external network is used.
"""

import argparse
import base64
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
root = args.output.resolve()
root.mkdir(parents=True, exist_ok=True)
svg = '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="160"><rect width="480" height="160" fill="#e0f2fe"/><rect x="24" y="24" width="160" height="112" rx="12" fill="#0284c7"/><circle cx="300" cy="80" r="52" fill="#16a34a"/><circle cx="410" cy="80" r="32" fill="#f59e0b"/></svg>'
image = root / "image fixture.svg"
image.write_text(svg)
encoded = base64.b64encode(svg.encode()).decode()
markdown = """# Markdown 渲染

**Bold text** · *italic* · ~~deleted~~ · `inline_code()`

- First list item
- 第二项中文

> A block quote with [a link](https://example.com).

| Component | Result |
| --- | --- |
| Markdown | rendered |
| 中文 | 正常 |

```rust
fn main() {
    println!("你好 Fleetpane");
}
```
"""


def item(kind, identity, **fields):
    return {"id": identity, "type": kind, **fields}


def command(identity, exit_code, output):
    return item(
        "commandExecution",
        identity,
        command="cargo test --workspace",
        cwd=str(root),
        status="completed",
        exitCode=exit_code,
        durationMs=1234,
        aggregatedOutput=output,
    )


samples = [
    ("01 Markdown", [item("agentMessage", "markdown", text=markdown)]),
    (
        "02 Commands",
        [
            command("success", 0, "running 12 tests\n12 passed\n中文输出正常"),
            command("failure", 1, "error: synthetic failure\nexpected 2, got 1"),
            command("bounded", 0, "large output 字\n" * 2000),
        ],
    ),
    (
        "03 MCP result",
        [
            item(
                "mcpToolCall",
                "capture",
                server="fixture",
                tool="capture",
                status="completed",
                arguments={"id": "example", "quality": "high"},
                durationMs=42,
                result={
                    "content": [
                        {"type": "text", "text": "Captured image\nsecond line"},
                        {"type": "image", "mimeType": "image/svg+xml", "data": encoded},
                    ],
                    "structuredContent": {"ok": True},
                },
            )
        ],
    ),
    (
        "04 File changes",
        [
            item(
                "fileChange",
                "changes",
                status="completed",
                changes=[
                    {
                        "path": "/example/src/main.rs",
                        "kind": {"type": "update", "move_path": None},
                        "diff": '@@ -1,3 +1,3 @@\n fn main() {\n-    println!("old");\n+    println!("你好");\n }\n',
                    },
                    {
                        "path": "/example/new file.txt",
                        "kind": {"type": "add"},
                        "diff": "new file\nsecond line\n",
                    },
                    {
                        "path": "/example/deleted.txt",
                        "kind": {"type": "delete"},
                        "diff": "deleted line",
                    },
                ],
            )
        ],
    ),
    (
        "05 Images",
        [
            item("imageView", "local", path=str(image)),
            item(
                "userMessage",
                "upload",
                content=[
                    {"type": "text", "text": "An uploaded image"},
                    {"type": "image", "url": f"data:image/svg+xml;base64,{encoded}"},
                ],
            ),
        ],
    ),
    (
        "06 Image failures",
        [
            item("imageView", "missing", path=str(root / "missing.png")),
            item(
                "userMessage",
                "invalid",
                content=[{"type": "image", "url": "data:image/png;base64,invalid*"}],
            ),
        ],
    ),
    (
        "07 Reasoning and unknown",
        [
            item("reasoning", "empty", content=[], summary=[]),
            item(
                "reasoning",
                "visible",
                content=[],
                summary=["A nonempty reasoning summary"],
            ),
            item("futureTool", "unknown", payload={"version": 2, "extra": "preserved"}),
            item(
                "agentMessage",
                "ending",
                text="Empty reasoning is hidden; other data stays available.",
            ),
        ],
    ),
    (
        "08 Long history",
        [
            item(
                "agentMessage",
                "oldest",
                text="# Oldest item\n\nThe beginning is reachable.",
            )
        ]
        + [command(f"command-{i}", 0, f"output {i}\n" * 20) for i in range(100)]
        + [
            item("imageView", "tail-image", path=str(image)),
            item("agentMessage", "tail", text="**Newest item**"),
        ],
    ),
]
threads = []
for index, (name, items) in enumerate(samples):
    threads.append(
        {
            "id": f"render-{index}",
            "cwd": str(root),
            "preview": name,
            "name": name,
            "status": {"type": "idle"},
            "turns": [
                {
                    "id": f"turn-{index}",
                    "status": "completed",
                    "error": None,
                    "items": items,
                }
            ],
        }
    )
(root / "history.json").write_text(json.dumps(threads, ensure_ascii=False))
print(f"Synthetic rendering histories written: {len(threads)}")
