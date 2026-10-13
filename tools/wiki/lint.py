#!/usr/bin/env python3
"""Mechanical checks for the docs/ wiki. Judgement checks (contradictions,
uneven depth, missing implications) are in docs/meta/workflows.md, 'Lint'.

  lint.py          # all checks
  lint.py --stale  # only pages whose cited code changed since the page did

Checks: frontmatter fields and status values, duplicate file names, broken
[[wikilinks]] and #mN anchors, pages missing from index.md, `code:` paths
that don't exist, and stale `implemented` pages.
"""
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DOCS = REPO / "docs"
STATUSES = {"implemented", "decided", "proposed", "exploring", "reference", "hub", "meta", "log",
            "in-progress", "complete"}  # last two: session pages
REQUIRED = ("status", "summary", "updated")
LINK = re.compile(r"\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|[^\]]*)?\]\]")


def strip_code(text):
    """Drop fenced blocks and inline code so example links aren't checked."""
    text = re.sub(r"^```.*?^```", "", text, flags=re.M | re.S)
    return re.sub(r"`[^`\n]*`", "", text)


def frontmatter(text):
    if not text.startswith("---\n"):
        return {}
    end = text.find("\n---", 4)
    fm = {}
    for line in text[4:end].splitlines():
        m = re.match(r"(\w+):\s*(.*)", line)
        if not m:
            continue
        k, v = m.groups()
        if v.startswith("["):
            v = [x.strip().strip("\"'") for x in v.strip("[]").split(",") if x.strip()]
        fm[k] = v
    return fm


def git(*args):
    r = subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def stale(page, paths, verified=None):
    last = git("log", "-1", "--format=%H", "--", str(page))
    # A `verified:` commit newer than the page's own last commit records a
    # re-check that needed no edit (or stands in for an untracked page).
    if verified and (not last or git("merge-base", "--is-ancestor", last, verified) is not None):
        last = git("rev-parse", verified) or last
    if not last:
        return "no commit to check against (page untracked and no `verified:` field)"
    newer = git("rev-list", f"{last}..HEAD", "--", *paths)
    if newer:
        return f"code changed in {len(newer.split())} commit(s) since the page's last commit {last[:10]}"
    if git("status", "--porcelain", "--", *paths):
        return "code has uncommitted changes; update the page in the same commit"
    return None


def main():
    pages = sorted(p for p in DOCS.rglob("*.md") if ".obsidian" not in p.parts)
    by_name, problems = {}, []
    for p in pages:
        by_name.setdefault(p.stem, []).append(p)
    for name, ps in by_name.items():
        if len(ps) > 1:
            problems.append(f"duplicate file name '{name}': {', '.join(str(x.relative_to(DOCS)) for x in ps)}")

    index = (DOCS / "index.md").read_text() if (DOCS / "index.md").exists() else ""
    indexed = {m.group(1).strip() for m in LINK.finditer(index)}
    only_stale = "--stale" in sys.argv

    for p in pages:
        rel = p.relative_to(DOCS)
        text = p.read_text()
        fm = frontmatter(text)
        is_raw = rel.parts[0] == "raw"
        if fm.get("code"):
            paths = fm["code"] if isinstance(fm["code"], list) else [fm["code"]]
            for c in paths:
                if not (REPO / c).exists():
                    problems.append(f"{rel}: code path '{c}' does not exist")
            if fm.get("status") == "implemented":
                why = stale(p, paths, fm.get("verified"))
                if why:
                    problems.append(f"{rel}: STALE: {why}")
        if only_stale or is_raw:
            continue
        for k in REQUIRED:
            if k not in fm:
                problems.append(f"{rel}: missing frontmatter '{k}'")
        if fm.get("status") and fm["status"] not in STATUSES:
            problems.append(f"{rel}: unknown status '{fm['status']}'")
        if rel.parts[0] != "sessions" and rel.name != "index.md" and p.stem not in indexed:
            problems.append(f"{rel}: not listed in index.md")
        for m in LINK.finditer(strip_code(text)):
            target, anchor = m.group(1).strip(), m.group(2)
            anchor = anchor.rstrip("\\") if anchor else anchor  # `\|` alias escape in tables
            hits = by_name.get(Path(target).name.removesuffix(".md"))
            if not hits:
                problems.append(f"{rel}: broken link [[{target}]]")
            elif anchor and not re.search(rf"^#+ {re.escape(anchor)}\s*$", hits[0].read_text(), re.M):
                problems.append(f"{rel}: [[{target}#{anchor}]] has no such heading")

    print("\n".join(problems) if problems else "wiki lint: clean")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
