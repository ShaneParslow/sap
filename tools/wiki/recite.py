#!/usr/bin/env python3
"""Move line citations in implemented wiki pages to follow the code.

For each implemented page, every cited file is compared between the page's
last commit and the working tree, and each citation's line number is moved
to where that line went. Two forms are handled:

  `path/to/file.rs:42` or `file.rs:42-60`  (a file in the page's code: list)
  (line 42), (lines 42-60), (lines 42, 57)  (the last file cited before it)

Run it once per change, before committing: it maps numbers from the page's
last commit, so a second run would move already-moved numbers again. Prose
like "(`signal`, line 34)" isn't recognized; write such citations with the
file (`notification.rs:34`) so they are.

Citations whose line was changed or deleted can't be moved automatically;
they are listed for a human or agent to fix. Run with --dry-run to see the
changes without writing them.

With --show, nothing is moved: every citation, relative ones included, is
printed with the line it points at in the working tree.

Named citations are resolved by finding the definition, which is safe to
repeat; --named-only does just that, leaving numbered ones alone.

Usage: python3 -I tools/wiki/recite.py [--dry-run | --show | --named-only] [page.md ...]
"""

import difflib
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DOCS = REPO / "docs"

FULL = re.compile(r"`([\w/.-]+\.\w+):(\d+)(?:([-–])(\d+))?`")
BARE = re.compile(r"`([\w/.-]+\.(?:rs|ld|S|toml))`")
RELATIVE = re.compile(r"\((lines?) (\d+)((?:[-–]\d+)?(?:, \d+)*)\)")
# "(`name`, line 42)" or "(`a`, `b`, lines 42, 57)": each number is the
# definition of the matching name, found directly rather than by diffing.
NAMED = re.compile(r"\(((?:`[\w:]+`, )+)(lines?) (\d+(?:, \d+)*)\)")


def definition_line(path, name):
    """The line defining `name` in `path` (function, type, constant, field,
    label), if exactly one does."""
    target = REPO / path
    if not target.exists():
        return None
    n = re.escape(name.split("::")[-1])
    patterns = [
        rf"^\s*(pub(\([\w:]+\))?\s+)?(unsafe\s+)?(extern\s+\"C\"\s+)?(fn|struct|enum|const|static|mod|type|trait)\s+{n}\b",
        rf"^\s*(pub(\([\w:]+\))?\s+)?{n}\s*:",
        rf"^\s*{n}\s*[:=]",
    ]
    lines = target.read_text().split("\n")
    for pattern in patterns:
        hits = [i + 1 for i, line in enumerate(lines) if re.match(pattern, line)]
        if len(hits) == 1:
            return hits[0]
        if hits:
            return None
    return None


def git(*args):
    r = subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


def locate(name, current, code):
    """The file and line defining `name`: in the module a qualified name
    (`thread::finalise`) points at, else the file cited just before, else
    whichever file of the page's code list has exactly one definition."""
    qualifier = name.split("::")[0] if "::" in name else None
    candidates = []
    if qualifier:
        candidates.append([c for c in code if Path(c).stem == qualifier])
    if current:
        candidates.append([current])
    candidates.append(list(code))
    for files in candidates:
        hits = [(f, line) for f in files if (line := definition_line(f, name))]
        if len(hits) == 1:
            return hits[0]
    return None, None


def frontmatter(text):
    if not text.startswith("---\n"):
        return {}
    end = text.find("\n---", 4)
    fm = {}
    for line in text[4:end].splitlines():
        m = re.match(r"(\w+):\s*(.*)", line)
        if m:
            k, v = m.groups()
            if v.startswith("["):
                v = [x.strip().strip("\"'") for x in v.strip("[]").split(",") if x.strip()]
            fm[k] = v
    return fm


class LineMap:
    """Maps 1-based line numbers of a file at `commit` to the working tree."""

    def __init__(self, path, commit):
        old = git("show", f"{commit}:{path}")
        new_path = REPO / path
        self.ok = old is not None and new_path.exists()
        if not self.ok:
            return
        a, b = old.split("\n"), new_path.read_text().split("\n")
        self.map = {}
        for tag, i1, i2, j1, _ in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
            if tag == "equal":
                for k in range(i2 - i1):
                    self.map[i1 + k + 1] = j1 + k + 1

    def __call__(self, n):
        return self.map.get(n) if self.ok else None


SHOW = "--show" in sys.argv
# Only resolve named citations (safe to repeat), leaving the rest alone.
NAMED_ONLY = "--named-only" in sys.argv


def recite(page, dry_run):
    text = page.read_text()
    fm = frontmatter(text)
    if fm.get("status") != "implemented":
        return 0, []
    commit = (git("log", "-1", "--format=%H", "--", str(page.relative_to(REPO))) or "").strip()
    if not commit and not SHOW:
        return 0, [f"{page.name}: never committed, nothing to compare against"]
    code = fm.get("code") or []

    def resolve(name):
        matches = [c for c in code if c == name or c.endswith("/" + name)]
        return matches[0] if len(matches) == 1 else name

    maps, problems, changes = {}, [], 0

    def move(path, n, where):
        nonlocal changes
        if NAMED_ONLY:
            return n
        if SHOW:
            target = REPO / path
            lines = target.read_text().split("\n") if target.exists() else []
            line = lines[n - 1].strip() if 0 < n <= len(lines) else "<no such line>"
            print(f"{page.name}: {path}:{n}: {line[:80]}")
            return n
        if path not in maps:
            maps[path] = LineMap(path, commit)
        new = maps[path](n)
        if new is None:
            problems.append(f"{page.name}: {path}:{n} ({where}): line changed or removed; fix by hand")
            return n
        if new != n:
            changes += 1
        return new

    # Relative citations use the file named most recently before them,
    # with or without a line number, within the same section.
    out = []
    current = None
    for para in re.split(r"(\n\s*\n)", text):
        if para.lstrip().startswith("#"):
            current = None
        pieces, pos = [], 0
        tokens = sorted([(m.start(), m, "full") for m in FULL.finditer(para)]
                        + [(m.start(), m, "bare") for m in BARE.finditer(para)]
                        + [(m.start(), m, "named") for m in NAMED.finditer(para)]
                        + [(m.start(), m, "rel") for m in RELATIVE.finditer(para)], key=lambda t: t[0])
        for _, m, kind in tokens:
            pieces.append(para[pos:m.start()])
            if m.start() < pos:
                continue  # inside a token already handled
            if kind == "bare":
                current = resolve(m.group(1))
                pieces.append(m.group(0))
            elif kind == "named":
                names = re.findall(r"`([\w:]+)`", m.group(1))
                numbers = [int(x) for x in re.findall(r"\d+", m.group(3))]
                if len(names) != len(numbers):
                    pieces.append(m.group(0))
                else:
                    found = []
                    for name, n in zip(names, numbers):
                        where, new = locate(name, current, code)
                        if where:
                            current = where
                        if SHOW:
                            print(f"{page.name}: {where}:{new or n}: (definition of {name})")
                        if new is None:
                            problems.append(f"{page.name}: no unique definition of `{name}` among the page's code files; fix by hand")
                            new = n
                        elif new != n and not SHOW:
                            changes += 1
                        found.append(str(new))
                    pieces.append(f"({m.group(1)}{m.group(2)} {', '.join(found)})")
            elif kind == "full":
                current = resolve(m.group(1))
                a = move(current, int(m.group(2)), m.group(0))
                s = f"`{m.group(1)}:{a}"
                if m.group(4):
                    s += f"{m.group(3)}{move(current, int(m.group(4)), m.group(0))}"
                pieces.append(s + "`")
            elif current is None:
                pieces.append(m.group(0))
            else:
                nums = re.findall(r"\d+|[-–]|, ", m.group(2) + m.group(3))
                moved = "".join(str(move(current, int(x), m.group(0))) if x.isdigit() else x for x in nums)
                pieces.append(f"({m.group(1)} {moved})")
            pos = m.end()
        pieces.append(para[pos:])
        out.append("".join(pieces))
    new_text = "".join(out)
    if new_text != text and not dry_run:
        page.write_text(new_text)
    return changes, problems


def main():
    dry_run = "--dry-run" in sys.argv or SHOW
    pages = [Path(a).resolve() for a in sys.argv[1:] if not a.startswith("--")]
    if not pages:
        pages = sorted(p for p in DOCS.rglob("*.md") if "raw" not in p.parts and ".obsidian" not in p.parts)
    total, problems = 0, []
    for p in pages:
        changes, probs = recite(p, dry_run)
        if changes:
            print(f"{p.relative_to(REPO)}: {changes} citation(s) {'would move' if dry_run else 'moved'}")
        total += changes
        problems += probs
    for p in problems:
        print(p)
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
