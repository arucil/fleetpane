"""Shared synthetic ACP/MCP content; never dereference these resource URIs."""


def blocks(case):
    if case == "text":
        return [{"type": "resource", "resource": {
            "uri": "test:///exact.txt", "mimeType": "text/plain",
            "text": "  原文 exact\r\n\ntrailing  \n",
            "_meta": {"synthetic": False},
        }, "annotations": {"priority": 0}}]
    if case == "long":
        return [{"type": "resource", "resource": {
            "uri": "test:///long.txt", "mimeType": "text/plain",
            "text": "  BEGIN 中文\r\n" + "字" * 4000 + "\n"
            + "\n".join(f"line {i:03d}  " for i in range(150)) + "\n\n  END  \n",
        }}]
    if case == "markdown":
        return [{"type": "resource", "resource": {
            "uri": "test:///notes.md", "mimeType": "text/markdown; charset=utf-8",
            "text": "# Resource notes 中文\n\n**Bold text** and `inline code`.\n\n"
            "| Key | Value |\n| --- | --- |\n| status | supplied |\n\n"
            "```rust\nfn main() {}\n```\n",
        }}]
    if case == "empty":
        return [{"type": "resource", "resource": {"uri": "test:///empty.txt", "text": ""}}]
    if case == "binary":
        return [{"type": "resource", "resource": {
            "uri": "test:///binary.bin", "mimeType": "application/octet-stream", "blob": "AAEC/w==",
        }}]
    if case == "link":
        return [{"type": "resource_link", "uri": "test:///zero report.pdf?version=1#part",
                 "name": "zero report.pdf", "title": "Report 中文", "mimeType": "application/pdf",
                 "description": "Synthetic resource link · content not supplied", "size": 0,
                 "_meta": {"synthetic": 42}}]
    if case == "invalid":
        return [{"type": "resource", "resource": {"uri": "test:///missing-body"}}]
    if case == "future":
        return [{"type": "future_resource", "extra": {"enabled": False, "count": 0}}]
    raise ValueError(f"Unknown synthetic resource case: {case}")
