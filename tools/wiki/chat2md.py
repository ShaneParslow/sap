#!/usr/bin/env python3
"""Render a chat transcript to markdown for docs/raw/.

Each turn gets a heading `## mN` so wiki pages can cite it as
[[<transcript-file>#mN]] (Obsidian heading link).

  chat2md.py claude-ai   <conversations.json> <conversation-uuid> <out.md>
  chat2md.py claude-code <session.jsonl> <out.md>

Tool calls are reduced to one line each and tool results are dropped; the
claude.ai mode also writes the conversation's original JSON next to <out.md>
so nothing is lost.
"""
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

TOOL_INPUT_CHARS = 160


def one_line(s, n):
    s = " ".join(str(s).split())
    return s if len(s) <= n else s[: n - 1] + "…"


def stamp(ts):
    if not ts:
        return "?"
    dt = datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone(timezone.utc)
    return dt.strftime("%Y-%m-%d %H:%M UTC")


def demote(text):
    """Push headings inside a message below the `## mN` turn headings."""
    out, fence = [], False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            fence = not fence
        out.append("##" + line if not fence and re.match(r"#{1,4} ", line) else line)
    return "\n".join(out)


def render(turns, front):
    out = ["---"]
    for k, v in front.items():
        out.append(f"{k}: {json.dumps(v) if not isinstance(v, (int, float)) else v}")
    out.append("---\n")
    out.append(f"# {front['title']}\n")
    out.append("Raw transcript. Immutable: do not edit; cite turns as `[[this-file#mN]]`.\n")
    for i, (who, ts, parts) in enumerate(turns):
        out.append(f"## m{i}\n")
        out.append(f"**{who}** · {stamp(ts)}\n")
        out.append(demote("\n\n".join(p.rstrip() for p in parts if p.strip())) + "\n")
    return "\n".join(out)


def claude_ai(export, uuid, out):
    convs = json.load(open(export))
    conv = next(c for c in convs if c["uuid"] == uuid)
    turns = []
    for m in conv["chat_messages"]:
        parts = []
        for a in m.get("attachments", []):
            name = a.get("file_name") or "pasted text"
            parts.append(f"> [!quote] Attachment: {name}\n" + "\n".join(
                "> " + line for line in a.get("extracted_content", "").splitlines()))
        for x in m.get("content", []):
            t = x.get("type")
            if t == "text":
                parts.append(x["text"])
            elif t == "thinking" and x.get("thinking", "").strip():
                parts.append("> [!note]- Thinking\n" + "\n".join(
                    "> " + line for line in x["thinking"].splitlines()))
            elif t == "tool_use":
                parts.append(f"`[tool: {x.get('name')}]` "
                             + one_line(json.dumps(x.get("input")), TOOL_INPUT_CHARS))
        turns.append((m["sender"], m["created_at"], parts))
    out = Path(out)
    out.write_text(render(turns, {
        "title": conv.get("name") or uuid,
        "source": "claude.ai export",
        "conversation": uuid,
        "created": conv["created_at"],
        "updated": conv["updated_at"],
        "turns": len(turns),
        "full_json": out.with_suffix(".json").name,
    }))
    out.with_suffix(".json").write_text(json.dumps(conv, indent=1, ensure_ascii=False))


def claude_code_turns(jsonl):
    turns = []
    for line in open(jsonl):
        d = json.loads(line)
        if d.get("type") not in ("user", "assistant") or d.get("isSidechain"):
            continue
        content = d["message"]["content"]
        if isinstance(content, str):
            content = [{"type": "text", "text": content}]
        parts = []
        for x in content:
            t = x.get("type")
            if t == "text" and x["text"].strip():
                parts.append(x["text"])
            elif t == "tool_use":
                inp = x.get("input", {})
                arg = inp.get("description") or inp.get("file_path") or inp.get("command") or inp
                parts.append(f"`[tool: {x.get('name')}]` " + one_line(arg, TOOL_INPUT_CHARS))
        if not parts:
            continue  # tool results, attachments, empty turns
        who = d["type"]
        if turns and turns[-1][0] == who:
            turns[-1][2].extend(parts)
        else:
            turns.append((who, d.get("timestamp"), parts))
    return turns


def claude_code(jsonl, out):
    session = Path(jsonl).stem
    turns = claude_code_turns(jsonl)
    first = turns[0][1] if turns else None
    Path(out).write_text(render(turns, {
        "title": f"Claude Code session {session[:8]}",
        "source": "claude code",
        "session": session,
        "created": first,
        "turns": len(turns),
    }))


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "claude-ai":
        claude_ai(*sys.argv[2:])
    elif len(sys.argv) == 4 and sys.argv[1] == "claude-code":
        claude_code(*sys.argv[2:])
    else:
        sys.exit(__doc__)
