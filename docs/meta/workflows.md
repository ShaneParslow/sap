---
status: meta
summary: Procedures for working sessions, running summaries, ingesting chats, recovering cut-off sessions, keeping code-backed pages current, answering questions, and linting.
updated: 2026-10-08
---

# Workflows

Procedures the agent follows. Structure is in [[schema]]; writing rules are in [[style]]. The procedures are listed in the order they occur in a session.

## 1. Starting a session

1. A `SessionStart` hook runs `tools/wiki/pending.py --hook`. If it prints un-ingested sessions, tell the human in one line and offer to recover them (section 5) before or after the requested work. Don't recover silently in the middle of an unrelated task.
2. Read [[overview]] and [[index]], then the hub and topic pages for the area the work touches. Read pages before answering from memory: the wiki is the record, not the agent's context.

## 2. Keeping a running session summary

Every session that discusses design or changes code gets a summary page, written *during* the session so that a cut-off session still leaves a record.

1. At the first substantive point (a decision, a design discussion, a code change), create `docs/sessions/YYYY-MM-DD-<id8>-<slug>.md`, where `<id8>` is the first 8 characters of the session id (the transcript file name in `~/.claude/projects/<repo>/`), with `status: in-progress`.
2. Append a terse bullet to `## Running notes` at each milestone: a decision made, a direction rejected, a commit, a page created. One line each, with the reason when there is one. This is cheap; do it as you go, not at the end.
3. Running notes are chronological and may be messy. They are not the wiki. Nothing in them counts as diffused until section 4 runs.

Template:

```markdown
---
status: in-progress
summary: One line on what the session was about.
updated: 2026-10-08
session: <full session id or conversation uuid>
transcript: "[[<transcript file name>]]"
---

# 2026-10-08 · <title>

## Goal
## Outcome            (written at close-out)
## Decisions          (written at close-out: decision, reason, source turn)
## Pages changed      (written at close-out)
## Commits            (written at close-out)
## Open threads       (written at close-out)
## Running notes
- ...
```

## 3. Closing out a session

When the work is done, or the human says to wrap up:

1. Export the transcript: `python3 -I tools/wiki/chat2md.py claude-code ~/.claude/projects/<repo>/<session>.jsonl docs/raw/sessions/YYYY-MM-DD-<id8>.md`. The export will be missing the last few turns, which is expected: `pending.py` notices the transcript is behind and the next session re-exports it (re-exporting a Claude Code transcript is the one allowed overwrite in `raw/`, because it is regenerated from the same source).
2. Write the structured sections of the summary page from the running notes and the transcript.
3. Diffuse (section 4).
4. Set the summary's `status: complete`, append to [[log]], run `tools/wiki/lint.py`.
5. Commit the wiki changes together with any code they describe (section 6).

## 4. Diffusing a chat into the wiki

This is ingestion. The input is a transcript; the output is updated topic pages. It applies to Claude Code sessions, claude.ai chats, and any other conversation.

**a. Extract.** Read the whole transcript. List every claim that should outlive the chat, each tagged with its turn (`mN`) and its kind:
- *decision*: the human chose something (status `decided`);
- *proposal*: something was put forward and not rejected (`proposed`). An agent recommending something does not make it a decision; look for the human adopting it, building on it, or saying so;
- *rejection or reversal*: something previously held was dropped; record why;
- *fact*: about external systems, hardware, numbers (`reference`);
- *open question*: raised and not settled (`exploring`).

Later turns override earlier ones. A chat often recommends A, then B; the wiki states B and [[decisions]] records that A was dropped and why.

**b. Place.** Assign each claim to the page for the function it is about, not the page for the topic that was being discussed when it came up. A remark about revocation made during a discussion of IPC belongs on [[cap-revocation]]. Create a page only when no existing page covers the function.

**c. Sweep for impact.** This step is the one most often skipped, and skipping it is what makes LLM wikis lopsided. For each decision, reversal and proposal, walk every area hub in [[index]] order and ask: *does this change anything here, even though nobody discussed it?* A change to the IPC wire format affects the object store's on-disk values, the process bootstrap message, versioning, and the roadmap. Write the implication on each affected page, in proportion to its effect there. An implication that was not discussed is at most `proposed`, or goes in a `> [!question]` callout. A page that is affected and says nothing is a defect.

**d. Write.** Update each page to state the current conclusion in its proper place. Rewrite; don't append "Update:" paragraphs. Follow [[style]]: depth in proportion to importance, not to how long the chat dwelt on it.

**e. Rebalance.** Look at every page you touched as a whole. If the new material made one section much deeper than its siblings, move the detail to a leaf page and leave a summary and a link. If it made siblings look thin by comparison, flag them in [[open-questions]] or fill them.

**f. Update the root pages.** [[index]] (new pages, changed summaries), the area hubs, [[roadmap]] (new or reordered work), [[open-questions]] (opened and closed), [[decisions]] (decisions and reversals with dates and source turns), [[glossary]] (new terms).

**g. Record.** Fill in the session page's *Pages changed* list, append to [[log]], run `lint.py`.

For a long chat, do steps a–c for the whole chat before writing anything. Writing page by page as you read produces exactly the session-shaped wiki this procedure exists to prevent.

## 5. Recovering a cut-off session

A session can end without close-out: the terminal closed, the context ran out, the process was killed. Claude Code keeps the transcript in `~/.claude/projects/<repo>/<session>.jsonl` regardless, so nothing is lost; it just hasn't been processed.

`pending.py` lists every session whose transcript isn't exported or is behind, or whose summary is missing or still `in-progress`. For each one:

1. Export or re-export the transcript (section 3, step 1).
2. Open the summary page if it exists. The running notes show how far the session got.
3. Read the transcript from where the running notes stop, or from the start if there is no summary, and complete the summary.
4. Check `git log` and `git status` for commits or uncommitted changes the session made; record them.
5. Diffuse (section 4), mark the summary `complete`, log it.

The same procedure handles chats from other tools: export them to `raw/chats/` (`chat2md.py claude-ai <export.json> <uuid> <out.md>` for claude.ai exports) and run section 4.

## 6. Changing code

Code is not ingested; `implemented` pages point at it (see [[schema#Citations]]).

1. When a change alters behavior described by an `implemented` page, update that page in the same commit. Then line citations stay valid at the page's last commit and `lint.py --stale` stays quiet.
2. When code implements something that was `decided` or `proposed`, either change that page to `implemented` (if it now describes the code) or split it: an `implemented` page for what exists, and the design page keeping the parts not yet built. Update [[roadmap]].
3. When implementation forces a design change, that is a decision: record it in [[decisions]] and run the impact sweep (section 4c) for it.
4. Describe behavior in prose and cite the code. Don't paste code that will drift; quote at most a few lines when the exact form matters (a struct layout, a syscall signature).

## 7. Answering questions

Answer from the wiki first, citing pages; fall back to transcripts and code when the wiki is thin, and say so. If an answer produced new synthesis (a comparison, an analysis, a resolved question), file it: into an existing page if it fits there, else as a new topic page, and log it.

## 8. Linting

Run `python3 -I tools/wiki/lint.py` after every change to the wiki. It checks frontmatter, unique names, broken links and `#mN` anchors, index coverage, and stale `implemented` pages.

Two reports help when code or history moves: `lint.py --citations` prints the line each `path:line` citation in an implemented page points at, so a citation that moved with the code is easy to spot; `lint.py --commits` lists commit hashes the wiki mentions that the repository doesn't have (other repositories' hashes, such as seL4's, appear too).

**Rewriting history.** Line citations survive a rebase as long as each page stays in the same commit as its code. Commit hashes written into the wiki don't: the roadmap's Done list, decisions, session pages and `verified:` fields name commits, and a rebase or amend changes them. Avoid rewriting commits the wiki already names; if it happens, run `lint.py --commits` and update the hashes it lists.

Periodically (and when the human asks), also do the checks a script can't:
- contradictions between pages, or between a page and [[decisions]];
- statuses that no longer hold (a `proposed` page the code now implements; an `exploring` page that was settled);
- uneven depth: siblings in a hub or sections in a page with very different levels of detail ([[style]], rules 2 and 3);
- functional areas mentioned in several pages but with no page of their own;
- terms used without a [[glossary]] entry.

Log each lint pass in [[log]] with what was fixed.
