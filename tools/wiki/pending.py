#!/usr/bin/env python3
"""List Claude Code sessions for this repo that have not been ingested.

A session is done when docs/raw/sessions/ holds its transcript and
docs/sessions/ holds a summary with `status: complete`. Anything else is
reported, including sessions that were cut off before finishing.

  pending.py                 # human-readable list
  pending.py --hook          # SessionStart hook: reads hook JSON on stdin,
                             # skips the current session, prints nothing if clean
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from chat2md import claude_code_turns  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
DOCS = REPO / "docs"
MIN_USER_CHARS = 200  # ignore sessions with no real conversation


def transcripts_dir():
    return Path.home() / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(REPO))


def user_chars(jsonl):
    n = 0
    for line in open(jsonl):
        d = json.loads(line)
        if d.get("type") != "user" or d.get("isSidechain"):
            continue
        c = d["message"]["content"]
        if isinstance(c, str):
            n += len(c)
        else:
            n += sum(len(x.get("text", "")) for x in c if x.get("type") == "text")
    return n


def summary_status(sid):
    for p in (DOCS / "sessions").glob(f"*{sid[:8]}*.md"):
        m = re.search(r"^status:\s*(\S+)", p.read_text(), re.M)
        return p.relative_to(REPO), (m.group(1) if m else "?")
    return None, None


def main():
    current = None
    if "--hook" in sys.argv:
        try:
            current = json.load(sys.stdin).get("session_id")
        except Exception:
            pass
    rows = []
    for jsonl in sorted(transcripts_dir().glob("*.jsonl"), key=lambda p: p.stat().st_mtime):
        sid = jsonl.stem
        if sid == current or user_chars(jsonl) < MIN_USER_CHARS:
            continue
        raw = list((DOCS / "raw" / "sessions").glob(f"*{sid[:8]}*.md"))
        summary, status = summary_status(sid)
        missing = []
        if not raw:
            missing.append("transcript")
        else:
            m = re.search(r"^turns:\s*(\d+)", raw[0].read_text(), re.M)
            if not m or int(m.group(1)) < len(claude_code_turns(jsonl)):
                missing.append(f"up-to-date transcript ({raw[0].relative_to(REPO)} is behind)")
        if summary is None:
            missing.append("summary")
        elif status != "complete":
            missing.append(f"summary is '{status}' ({summary})")
        if missing:
            rows.append(f"- {sid} ({jsonl}): missing {', '.join(missing)}")
    if rows:
        print("Un-ingested Claude Code sessions (see docs/meta/workflows.md, 'Recovering a cut-off session'):")
        print("\n".join(rows))
    elif "--hook" not in sys.argv:
        print("All sessions ingested.")


if __name__ == "__main__":
    main()
