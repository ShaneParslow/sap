---
status: meta
summary: How the wiki is laid out, what each page type is, the frontmatter, the status vocabulary, and how to cite chats and code.
updated: 2026-10-08
---

# Wiki schema

The wiki is an LLM-maintained knowledge base in the style of [[llm-wiki]]. The agent writes and maintains it; the human reads it, steers it, and occasionally edits it. `docs/` is an Obsidian vault, but nothing depends on Obsidian: it is plain markdown with `[[wikilinks]]`.

This page defines structure. [[workflows]] defines the procedures (ingesting, session logging, linting). [[style]] defines how pages are written.

## Sources of truth

There are two, and they work differently.

**Chats** (claude.ai conversations, Claude Code sessions) are events. They happen once, are exported to `docs/raw/` as transcripts, never edited, and then *ingested*: their content is diffused into wiki pages. A transcript is the evidence; the wiki page is the current conclusion.

**Code** is state. It changes constantly, so it is never ingested. Wiki pages *reference* it instead, and a reference is only valid at a particular commit. Code is the authority on what the system does; the wiki explains what it does, where, and why. When the two disagree about current behavior, the code wins and the page is stale.

This resolves the tension between ingestion and a moving codebase: chats flow into the wiki once, code is pointed at and re-checked.

## Layout

```
CLAUDE.md                 agent entry point; imports the meta pages
docs/                     Obsidian vault
  index.md                catalog of every page, in reading order
  overview.md             what sap is, in one page; start here
  roadmap.md              ordered work items (the to-do list)
  open-questions.md       unresolved questions, each linked to its page
  decisions.md            dated log of decisions and reversals
  glossary.md             terms and search terms
  log.md                  append-only log of wiki operations
  meta/                   schema (this page), workflows, style
  <area>/                 topic pages, one folder per functional area
  sessions/               one summary page per chat or coding session
  raw/                    immutable sources
    chats/                claude.ai transcripts (+ original JSON)
    sessions/             Claude Code session transcripts
    references/           external documents kept for reference
tools/wiki/               chat2md.py, pending.py, lint.py
```

Areas are split by function, not by when things were discussed. The current areas, in reading order: `vision`, `capabilities`, `hardware`, `kernel`, `interfaces`, `ipc`, `store`, `programs`, `system`, `prior-art`. Reading order is concept first (why, then the capability model that everything uses), then layers bottom-up. A new area is added only when a group of at least three pages doesn't fit an existing one; add it to this list and to [[index]] in the right position.

## Page types

| Type | Where | What it holds |
|---|---|---|
| Hub | `<area>/<area>.md` | What the area covers, how its pages relate, each child page with its status and a one-line summary. No detail of its own. |
| Topic | `<area>/<prefix>-<topic>.md` | One function or mechanism, in full detail. The bulk of the wiki. |
| Entity | `prior-art/<name>.md` | One external system: what it is, what sap takes from it, what it gets wrong. |
| Session | `sessions/YYYY-MM-DD-<id8>-<slug>.md` | What one chat or session covered, decided, and changed. Narrative and history live here, not in topic pages. |
| Root | `docs/*.md` | Index, overview, roadmap, questions, decisions, glossary, log. |
| Raw | `raw/**` | Transcripts and reference documents. Never edited after export. |

**File names are unique across the vault** and readable on their own, because links are `[[name]]` without a path and because a good name is most of the navigation. Topic pages carry an area prefix (`ipc-channels`, `kernel-scheduling`, `cap-ownership`, `store-sturdy-refs`, `hw-cheri`, `iface-capnp`, `prog-representation`). Pick the name for the function the page describes, not for the discussion that produced it.

## Frontmatter

```yaml
---
status: proposed          # see below; exactly one per page
summary: One line, used verbatim in index.md and the area hub.
updated: 2026-10-08       # last substantive edit
sources: ["[[2026-09-27-future-of-computing#m25|§m25]]"]   # chats this page draws on
code: [kernel/arch/riscv/trap.rs]   # implemented pages only: files the page describes
---
```

`lint.py` checks the required fields (`status`, `summary`, `updated`). `sources` and `code` are optional but expected where they apply.

## Status

Every topic page has exactly one status. It tells the reader how much to trust the page and what kind of statement it makes.

| Status | Meaning | Required evidence |
|---|---|---|
| `implemented` | Describes what the code does now. | `code:` paths and `path:line` citations. |
| `decided` | The human has committed to this design. Not (fully) built yet. | A chat or session where the human chose it. |
| `proposed` | A concrete design that has been put forward (often by an agent) and not rejected, but not explicitly adopted either. | The chat where it was proposed. |
| `exploring` | Open. Alternatives are live, or the idea is only partly thought through. | — |
| `reference` | External facts: prior art, hardware, numbers. Not a claim about sap's design. | Sources. |

Hub, root and meta pages use `hub`, `meta` or `log` as appropriate; [[roadmap]] holds the to-do items themselves.

The status line is what separates current behavior (`implemented`), work to be done (`decided`/`proposed`, sequenced in [[roadmap]]), and ideas that aren't settled (`exploring`, collected in [[open-questions]]).

**Mixed pages.** When a page is mostly one status but contains a small piece of another, mark that piece with an Obsidian callout rather than downgrading the page:

```markdown
> [!question] Open
> Whether moves should be a kernel primitive. See [[kernel-choice]].

> [!todo] Not built
> The broker currently copies; own-semantics enforcement is roadmap item M4.
```

If a callout grows past a short paragraph, the page is really two pages; split it. Callout types in use: `question` (exploring), `todo` (planned, not built), `warning` (known problem or caveat), `note` (aside).

## Citations

**Chats.** Cite a transcript turn as `[[<transcript>#mN|§mN]]`. Every turn in a transcript is a heading `## mN`, so the link resolves in Obsidian and `lint.py` checks it. Cite at the end of the paragraph or table the claim lives in. A page whose content comes from one stretch of a chat can cite the range once in `sources` and only cite inline where a specific claim needs it.

**Code.** In `implemented` pages, cite as `` `path/to/file.rs:42` `` or `` `path/to/file.rs:42-60` ``. Line numbers in a page are valid as of **the last commit that changed that page** (`git log -1 -- <page>`), so reading a citation at its commit is `git show <commit>:path/to/file.rs`. List every cited file in `code:`. `lint.py --stale` reports pages whose `code:` files have changed in later commits; such a page must be re-verified, and updating it in the same commit as the code change keeps it current. To cite code from a different point in history, pin it explicitly: `` `path:42@a1b2c3d` ``.

**Re-verifying without an edit.** When code a page cites changes but the page is still accurate, record the re-check as `verified: <commit>` in its frontmatter, naming the commit whose code you checked the citations against. `lint.py --stale` measures staleness from that commit when it is newer than the page's own last commit. (Until 2026-10-13 `docs/` wasn't tracked, and every implemented page carried `verified:` for that reason.) Before committing code, run `lint.py --stale`: the pages it lists as having uncommitted code changes are the ones to check in the same commit; line numbers must still be checked, since a page committed with its code is never flagged: `lint.py --citations` prints the line each `path:line` citation points at, which makes a moved citation obvious.

**External facts** (specs, papers, projects) get a plain markdown link to the source when one exists, and a date for anything time-sensitive ("as of April 2026").

## Links

Use `[[page]]` for internal links and `[[page#Heading]]` for sections. Link the first mention of another page's subject in each section, not every mention. Every page except sessions and raw files must be reachable from [[index]].
