---
status: reference
summary: Kenton Varda's self-hostable server built on Cap'n Proto, where each document is an isolated grain sharing access through sturdy refs and a powerbox. The closest existing system to sap's store layer.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m21|§m21]]"]
---

# Sandstorm

**What it is:** a self-hostable server where every document is an isolated "grain", and grains share access through sturdy refs and a powerbox. Built by Cap'n Proto's author on Cap'n Proto. The company shut down; the project is community-maintained ([[2026-09-27-future-of-computing#m21|§m21]]).

**Relation to sap:** a web-delivered version of the object store with capabilities: same author, same wire format, same model. Its design docs and code show how persistent capabilities, powerbox grants and sharing behave when real users touch them, which is exactly the part of sap's store that is least tested ([[store-sturdy-refs]], [[store-granting-access]]). Read before designing the powerbox.
