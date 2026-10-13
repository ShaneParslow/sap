---
status: meta
summary: Writing rules for the wiki, aimed at the usual failures of LLM-written docs - depth that tracks the conversation instead of the subject, uneven siblings, and arbitrary ordering.
updated: 2026-10-08
---

# Style

These rules exist because LLM-written documentation fails in predictable ways. It is very specific about whatever was discussed most recently and thin on everything else, even when the thin parts are just as affected. It puts a one-line item and a two-page item under the same heading level. It orders things in the order they came up. Each rule below targets one of those failures. Rules 1–4 matter most.

## 1. Depth follows the subject, not the session

How much a page says about something should depend on how important and how complex that thing is, never on how long a chat spent on it. A chat that spent two hours on ring buffers and two minutes on revocation does not justify a wiki where ring buffers get twenty paragraphs and revocation gets one, when revocation is the harder problem.

Before writing, ask of each item: *if a new contributor read only this wiki, would the space given to this item match how much they need to know about it?* Something discussed only briefly but central to the design gets filled out from what is known, with the gaps marked as open questions. Something discussed at great length but peripheral gets summarized and linked to the transcript for the full discussion.

## 2. Siblings are peers

Sections at the same heading level, and pages listed together in a hub, should be comparable in scope and in how much detail they carry. Check by reading only the headings: they should read as a list of things of the same kind. "Ring layout / Backpressure / Message size / Why Xen's double-fetch bug happened" fails; the last item is a detail of a different section, not a peer.

When one sibling needs much more space than the others, it usually means one of two things: it should be its own page (move the detail down a level and leave a summary and a link), or the other siblings are underwritten (fill them in). Do not fix it by trimming the long sibling until it is as shallow as the others.

## 3. Detail lives at the leaves

Each level of the wiki has a job:

| Level | Job | Typical length |
|---|---|---|
| [[overview]] | What sap is and how the parts fit. A reader stops here or picks an area. | ≤ 800 words |
| Area hub | What the area covers, how its pages relate, each page in one or two lines with its status. | 150–500 words |
| Topic page | One function in full: mechanism, rationale, alternatives, numbers, edge cases. | 400–1500 words |
| Section within a topic | One aspect of the function. | a few paragraphs, a table, or a list |

Detail is welcome; it just belongs on the topic page, not the hub or overview. When a topic page passes about 1500 words, it is usually covering two functions; split it along the functional seam. Higher levels summarize and link, and a summary must stay true when the detail below it changes; if a detail changes the summary, update both.

## 4. Order by a stated principle

Every list of sections, pages or items has an order, and that order should come from the subject, not from the order things were discussed. Use one of:
- **Dependency:** things are defined before they are used. The default for [[index]] and hubs, and for sections that build on each other.
- **Lifecycle:** the order things happen (create, use, transfer, revoke, destroy; boot, spawn, run, exit).
- **Data path:** the order data flows (sender, ring, receiver).
- **Importance:** most consequential first. Use it for lists of options, risks or open questions.

Within a topic page, the default section order is:
1. what it is, in one or two sentences, and its status;
2. the design or current behavior;
3. how it works, in detail;
4. alternatives and why they lost;
5. open questions;
6. where it connects to other pages (only if not already linked inline).

Leave out sections that would be empty. Never add a section just to complete the pattern.

## 5. State the current conclusion, not the history

Chats wander, reverse and revisit. The wiki states where things ended up. "Cap'n Proto is the interface format" — not "We first considered WIT, then realized…". History goes in [[decisions]] (one line per reversal, with the reason) and in session pages. A topic page mentions a rejected alternative only in its alternatives section, and only when knowing why it lost prevents someone from proposing it again.

## 6. Be honest about status and confidence

The page status and callouts say how settled something is ([[schema#Status]]); the prose must match. Don't write a `proposed` design in the voice of a settled one ("the kernel does X") without the page status making that clear, and don't mark a suggestion as decided because it sounded confident in the chat. Numbers carry their source and their caveats: "about 100 µs per round trip on Flow-IPC's own benchmark", not "IPC costs 100 µs". Say "unknown" where it is unknown.

## 7. Write for a reader who wasn't there

No "as we discussed", "you asked", "great question", or references to a chat's internal flow. No second person addressed to the human. A page must make sense to someone who never saw the chat; anything it depends on is linked. Use one name per concept (the one in [[glossary]]) everywhere; define a term on first use or link to its definition.

## 8. Plain, compact prose

- Prose for reasoning; bullets for enumerations of parallel items; tables for comparisons across the same attributes. Don't break an argument into bullets.
- Bold sparingly: for the term being defined, or the one sentence a skimmer must not miss. Not for decoration.
- No filler: "it's worth noting", "importantly", "in summary", "robust", "seamless", "leverage". Cut throat-clearing openings and recap closings.
- Prefer concrete examples to abstract restatement. One good example beats three paragraphs of generalities.
- Short paragraphs, each with one point.

## 9. Check before finishing

After writing or substantially editing a page, reread it as a whole and check:
- Do the headings alone read as a sensible, consistently ordered outline of peers? (rules 2, 4)
- Is any section far deeper or shallower than its siblings without a reason in the subject? (rules 1, 2)
- Does anything here belong on a leaf page instead? (rule 3)
- Do the status and the voice agree? (rule 6)
- Would someone who never saw the chat follow it? (rule 7)
