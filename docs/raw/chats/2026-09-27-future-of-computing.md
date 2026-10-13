---
title: "Future of computing with AI automation"
source: "claude.ai export"
conversation: "0ce860ab-d23a-4b9e-a4a5-810423df02ae"
created: "2026-09-27T00:26:06.169184Z"
updated: "2026-10-08T06:28:19.691015Z"
turns: 32
full_json: "2026-09-27-future-of-computing.json"
---

# Future of computing with AI automation

Raw transcript. Immutable: do not edit; cite turns as `[[this-file#mN]]`.

## m0

**human** · 2026-09-27 00:26 UTC

let's sketch out what the future landscape of computer hardware, os, applications, and networking looks like. let's also assume that software engineering is fully solved and automated by ai, and that there is abundance of that level of ai. what does consumer desktop/laptop/mobile and server look like? up and down the stack. what does the creation of software look like? how do people use their devices? what kind of steady state do you tend towards with that level of capability over time? I have some ideas on this but I will refrain to get your take first, then i'll give mine. the overarching question here is what actions should be taken right now to ensure that this future unfolds in an open, pro-consumer, consumer sovereignty way. what foundations are required and what can be done today is the question to keep in mind while sketching this out.

## m1

**assistant** · 2026-09-27 00:27 UTC

#### The core shift

If writing software costs almost nothing, software stops being where power sits. Anything that can be regenerated on demand can't be used to lock people in. Control moves to the things that can't be regenerated:

1. **Physical capital:** fabs, energy, datacenters, spectrum, last-mile networks.
2. **Cryptographic roots of trust:** who holds the signing keys that decide what code a device will boot and what code a service will talk to.
3. **Frontier model weights and inference compute.**
4. **Network effects and identity:** the social graph, reputation, proof that you're a person.
5. **Legal permission:** DMCA 1201, CFAA, terms of service, patents.

This leads to my main claim. **When AI can reverse-engineer and rewrite anything, obscurity stops protecting business models, so the pressure to lock things down with cryptography and law gets much stronger.** Today a lot of openness exists by accident, because building a locked-down alternative is expensive. That accident goes away. The fight moves from "can I technically do this" to "will the hardware and the network let me, and is it legal."

#### Up the stack

**Hardware.** Devices are built for inference first: unified memory, large NPUs, lots of memory bandwidth. Chip design gets automated too, so open RTL becomes plentiful. Fabrication doesn't get automated, so it stays concentrated. The important part of each device is its boot chain and attestation hardware. Firmware blobs stop mattering as an obstacle because an AI can rewrite them. What matters is whether the device will accept firmware you signed yourself.

**OS.** Kernels become small, formally verified, capability-based microkernels, like seL4 but everywhere. Once proofs are cheap to produce, there's little reason not to have them. The main job of the OS changes to containing untrusted generated code. Most code on your machine will have been written for you minutes ago, by a model you didn't fully audit. Drivers get generated from specs or from reverse engineering. Old software keeps running through cheap, perfect emulation. The part of the OS you interact with is an agent runtime sitting on top of a personal object store, with fine-grained capabilities granted to each piece of generated code, including your own agent.

**Applications.** "Apps" as packaged products mostly disappear. Data persists. Interfaces are generated for the task and thrown away afterward. File formats stop locking anyone in, because any format can be translated instantly. Lock-in moves to where the data lives. If your data sits on a server behind an API that only accepts attested clients, being able to parse the format doesn't help you.

**Networking.** Most traffic is agents talking to agents. Adversarial interoperability, meaning your agent operating any website or service for you, becomes technically trivial. Platforms respond with bot detection, device attestation, and litigation. Proof of personhood turns into critical infrastructure, and there's a strong pull to tie it to attested devices or government ID. The physical last mile stays a chokepoint: CGNAT, ISP terms that ban running servers, asymmetric links.

**Servers.** Running your own server becomes easy to operate, because the agent does the sysadmin work. Self-hosting stops being a hobby skill. What remains hard is residential networking, power, and enough inference compute. Hyperscalers keep their advantage for capital-expenditure reasons, not because their operations are better.

**How people use devices.** You state intent and delegate. The main question for sovereignty becomes **who the agent works for.** An agent run by a company that makes money from ads, commerce placement, or engagement has a conflict of interest built in. That holds even if every layer under it is open. This includes model providers like the company that makes me. A few model families generating all software also creates correlated vulnerabilities, and makes those providers' usage policies a de facto app store.

#### Steady state

I see two attractors:

- **Enclosure.** Consumer devices become attested appliances. Services only serve attested clients. The cloud agent is the gatekeeper for everything. General-purpose computing survives only as a niche. This is where things drift by default, because the pieces already exist and are deployed: Play Integrity, Apple App Attest, locked bootloaders, anti-scraping terms of service. Capital concentration pushes the same way.
- **Owner-controlled.** You hold the root keys. A capable model runs locally, or on rented hardware you can leave. Your data lives in a store you control, and services speak open protocols to it. Your agent acting for you is legally and technically treated as you. Software abundance makes this world better than today's, because every user effectively has a team of engineers.

What decides between them is mostly layers 2 and 5 from the list at the top: keys and law. Code matters much less. Corollary: in this future, open source *code* matters less, since code can be regenerated. Open *specs, weights, data, and hardware documentation* matter more. Above all, the freedom to *run* what you want (Freedom 0) matters more than the freedom to study the source.

#### What to do now, in priority order

**1. Owner-controlled roots of trust.** This is the foundation everything else depends on. Every device should let its owner enroll their own signing keys, relock the device, and get the same security guarantees a vendor-signed image gets. Pixel phones with GrapheneOS already show this works on a phone. Attestation should be able to prove security properties the owner chooses to disclose. It should not be able to prove "unmodified vendor software," which lets services discriminate against owner-signed systems. EFF's 2003 "owner override" proposal for trusted computing is the right conceptual basis and deserves revival.

In practice:
- Buy and build on hardware that allows owner keys (Framework, coreboot/Dasharo, GrapheneOS-capable phones).
- Back open silicon roots of trust (OpenTitan, Caliptra) so that "secure" and "vendor-controlled" stop being synonyms.
- Treat any attestation API that checks for vendor-signed software as the main policy target.

**2. Legal right of agent access.** Establish that an agent acting for a user, with that user's credentials, has the same access rights as the user. Pair that with adversarial-interoperability protection:
- Narrow DMCA 1201 through the triennial exemption process and push for statutory reform.
- Keep CFAA narrow; *Van Buren* helped.
- Make contract terms that ban interoperability unenforceable.

The EU's Digital Markets Act already requires real-time data portability from gatekeepers. That's the template to extend. This is cheap to push now and very expensive to win back after platforms have normalized "no agents."

**3. Local-capable open weights and hardware to run them.** Your agent's loyalty is only credible if you could run it yourself. That requires open-weight models near the frontier and consumer hardware with enough memory bandwidth. Agent fiduciary duty, meaning your agent can't be secretly paid to steer you, should be established in law before agent commerce becomes entrenched.

**4. User-held data and identity.** Local-first data with CRDT sync (Automerge-style). Personal data servers that services connect to, rather than services holding your data (the AT Protocol PDS model). Identity rooted in self-certifying keys, not platform accounts. Proof of personhood that works without an attested device. Once people have agents, every interoperability right above only helps if the data is somewhere those agents can reach.

**5. Networking.** Full IPv6, no CGNAT, symmetric connections, and ISP terms that allow running servers. This matters less than items 1 through 4, because tunnels and relays can work around it. It still decides whether self-hosting is the default or a workaround.

**What individuals with systems skills can do.** Upstream hardware support into mainline. A device whose drivers live in the kernel stays owner-controllable after its vendor abandons it, and every device mainlined now is one fewer that depends on vendor permission. Also: build and run self-hosted infrastructure that non-experts can use, file comments in the 1201 rulemakings, and write the attestation-neutrality argument clearly enough that legislators can reuse it.

I'm least confident about proof of personhood. I don't see a design that resists both Sybil attacks and surveillance without some hardware or government anchor, and that anchor is exactly where enclosure gets back in.

I'm curious where your picture differs.

## m2

**human** · 2026-09-27 02:27 UTC

> [!quote] Attachment: pasted text
> this is what I wrote:
> my view is that things will tend towards extreme user customizability. information presented in exactly the way that the user wants, determined and orchestrated on the fly by some kind of system agent(s). movie style. basically maintainers are replaced by this agent and it makes a setup totally customized to you. probably some orgs like kde/gnome/microsoft still exist that curate some kind of baseline that people can go off of, standard distro for things that require/desire uniformity. many users choose to stick with those but others make their own thing. imagine plug in a drive with an install image that just gives you a prompt where you totally sculpt your system from scratch. I think all of that demands big changes to the existing stack: breaking up monoliths, more interfaces for customizability, more explicit config and packaging (something like nix?), user data store (personal information, intent, preferences, etc) that gets shared with apps/other agents according to necessity and your privacy preferences. with that as the user-facing side, I think there are also some directions that things might move (and are already moving) on the technical side as well: potentially settling on a single (risc?) isa (at least for consumers, esp something less schizophrenic than x86), more standard gpus/preipherals/io, actual useful/effective/unfucked permissions (selinux?), sandboxing as a first class feature (level of separation depending on how security in ai age shakes out? maybe hardware memory tagging? maybe selinux + memory safe languages are enough?), in the same vein as perms just generally shaking off legacy posix stuff that doesn't fit anymore (this is obviously very hard and might not happen. maybe microkernel? already going in that direction with running main kernel bits in hypervisor mode. maybe something beyond "everything is file" approach? more flexible base data type that can represent more meaning at all levels, self describing, universal. example: when I want to share something why do I have to take a screenshot? why must convert to graphical, grab graphical output, make a whole image, send? ideally could capture more base/fundamental representation and share in more true/representative format.), more portability (maybe settle on single isa? maybe the llms just write assembly and your isa changes to fit that? or maybe the compiler is a useful tool for our ais regardless? maybe your isa changes to fit a unified language and data format? I was wondering about webassembly unifying things even at the native level but it seems like they haven't taken a strong position on defining an abi so maybe not? not sure, don't know much, but I've heard good things about webassembly native. other thing is how does web/native balance shake out? web obviously works extremely well and we should learn from that, bring what works to the native level. ex are cloud concepts like graphql, rest, s3, which provide a unified way of storing and accessing *everything*. when i'm talking about reevaluating our basic data structures and operations this is what i'm imagining), networking concepts seem like they works surprisingly well and would obviously be insane to change in any meaningful capacity so probably nothing there? probably some other things that fall out of what i've described but those are some of the main pillars I think. the hardware/isa/gpu/kernel/base data representation/abi/portability side is probably pretty interesting and fruitful but its the part i've thought out the least. as you can tell im imagining some pretty transformative stuff, but I am thinking long term steady state once you shake out the legacy stuff and remove the human momentum from the equation with powerful ai. it's bound to be quite different from the wild west that we have right now. it also might sound like saying insane customizability but also saying that right *now* is wild west is incompatible but I think insane customizability counterintuitively demands lots of standardization and more firm interfaces. the question that follows, after determining if any of this actually makes any sense/gaps, is how you make this transition smoother and make sure that it tends towards open, free software. I think abundant powerful accessible ai inherently benefits the open source pro-consumer community more than the software companies, but every bit of planning and effort helps.
> 
> after having read your take:
> we definitely agree that software abundance leads to a seamless, intuitive, customization user experience. but I think you take it an extra step by writing off basically everything as generated on the fly: no apps, interfaces thrown away, file formats less important. I think this is definitely true to a degree but I think the price of writing something will always be more than the price of using something that already exists. that price may not necessarily be money. my point is basically the last thing that I said: customizability counterintuitively demands a lot of standardization and more firm interfaces. you want interoperability. you want to be able to use open source libraries and tools that have been proven, that have been made by ais smarter than what you can run, that is available faster than your ai can generate, etc. we don't want to be wasteful right? again there is definitely a happy medium here but I think I am more on the side of open source proven software that gets downloaded and tweaked, mostly standard interfaces, etc. whereas you seem to envision that things become more like biology. I still think that's possible and will happen to a degree but not as much as you I think.
> I think a point that hasn't been fully specified is the level of reliability that you can expect from your ai engineering team and alignment concerns. less reliability bends this towards my vision of open repositories of verified libraries that get tweaked. alignment concerns are rough but they are rough in general so I think our assumptions should land somewhere between the landscape of today and an ideal future where an ais allegiance and behaviors can be formally proven. I think alignment concerns also make it desirable for development to happen in the open, but I don't think it necessarily pushes things in that direction as much as it desires for that to happen. open to thoughts on how these two points affect everything else.
> the other thing is that you foresee the landscape of open source as being much more grim than I do. I even said that I think this kind of ai inherently benefits the open source pro-consumer community. firstly, I think concerns about alignment of models is a separate thing and we should mostly assume the best here since the whole thing is kind of screwed anyways if that doesn't work out. "assume the best" in the sense that hopefully your ai isn't actively pursuing the companies interests over yours. also, I have a hard time envisioning a future where agents aren't acting on behalf of the user, and capable (legally) of doing almost everything a user can. so I think that is mostly not a thing for the sorts of administrative, technical, day-to-day things we are discussing, but it might be a thing for sensitive "human only" actions, whatever those would be. who knows. open to thoughts on that. maybe there's something big there I'm missing. as for the main thing: you are definitely right that things could continue as they have in terms of locking down devices. in fact I think this is desirable in certain circumstances. ex competitions might require that everyone uses a validated system, businesses want validated systems, etc. Ideally this would be some kind of container that runs on your system instead of its own device but I don't know how far you can go with cryptographically proving that you are running a program. I guess my ideal would be "don't do that at all" -> run in a container cryptographically verifying the program that you are running -> sgx type of thing -> dedicated hardware. regardless, the goal is definitely to avoid this as much as possible since companies are going to obviously want to take it miles further than necessary, like they already are. I guess I don't have any incredibly strong argument against this. part of the reason I'm thinking about this is because I know it can go that way, and I don't want it to. I do think there are a few helpful things here though: interoperability is huge, your agent abstracts away the complexity, the community can maybe still do more than companies can, and general purpose computing will always exist, so the rest of the consumer market might get dragged along with it. for interoperability, you see this with the web. everything is a web app, less and less is native. on one hand that is a result of deduplicating effort, which wouldn't be a concern. on the other hand, you want your stuff running on as many devices as possible. even if it isn't a technical concern anymore, you still need to be friends with the company that made the key for the device. you could see a scenario where the market for devices looks like the streaming market, where everyone wants their piece of the market, alienating everyone, pushing people towards alternatives. the agent aspect is self explanatory, but you're already seeing it with linux a bit. same with the general purpose computing argument, a small consumer market is grabbing onto linux, which has been dragged through the decades because something like linux must exist. the community > companies point is the weakest, but a flourishing community making all sorts of cool things will have more ideas and more character than something a company could make. maybe. this sort of assumes that the companies don't have significantly better models and that that doesn't matter if they do. so in summary, I agree that it could be grim, that's why I'm posing this, but I think there are some inherent advantages (everyone can make any software! that's already miles better than it is today/yesteryear!) and I think there is a small area where attested programs/devices are necessary anyways. so really, I agree with your assessment of the steady state. the question is just what percentage of each you get.
> other small things: yeah selfhosting might be a thing. I was toying with this before since desktop pcs are starting to use a bunch of power and at some point you need to move that out of the office, speed of light favors local, abundance of high end chips hitting the secondhand market shortly. could be a spark? maybe you end up with a few gpus being just a normal appliance that everyone plugs into their garage or something. or something like starlink is good enough in terms of latency for that sort of thing and renders it unnecessary/enthusiast. proof of personhood is also probably going to be a thing. I have no idea how you handle that either, maybe its more clear after working out the technical stuff. also agree with microkernels since you're already seeing that with kernels moving bits into hypervisor mode.
> last thing is actually more on the technical side, because I think you were kind of reading my mind with a couple of things: agent runtime on top of an object store, fine-grained capabilities, and user held personal data. permissions/capabilities definitely have to be fixed since it's a total mess right now. personal data store is also big I think since its a thing that is needed basically right now and is a prerequisite to a flourishing, interoperable agent economy. you want personal information, preferences, intent, etc.
> but I also think those couple of concepts gets to something deeper, which I started talking about a little bit with ISA, portability, and cloud architecture. which is that everything we have right now is way more complicated than it needs to be, and that we should get closer to the metal and back to cs fundamentals. there's so many layers of abstraction, languages, instructions, etc that have resulted from the fact that all of this was built over decades, was built by humans who prefer shorthand over formalisms, that sort of thing. what this whole thing needs is a base language that the ais can speak, self modifiable, self describing. I think the cloud implementations of this are a good place to look for what works, and our job is pulling it down to the native level. everything is just key -> value associations. system information, user programs, user data, everything. the value type is the interesting part, like I said self describing is good. unix thrives because it gets partway to this vision with files, but its filled with edge cases, legacy interfaces, quirks, etc. the other question is what operations do you perform on your database? how do programs work? what is the topology (tree, graph)? how are permissions handled? I'm sort of imagining something like the original vision of lisp machines. you could even bring the isa up to match with whatever language you come up with. I think this would be an interesting foundation to work on today, but I'm sure there are all sorts of wrinkles that would have you reinventing the quirks that you are trying to get rid of. but somebody needs to invent the basic language that the agent ecology will be making their proteins (programs) out of. and that language is also your data, and your data is your language. question is whether this overhead kills your performance. I was wondering about accelerators that could be exposed through your object store for high performance stuff but who knows. the reason i'm being so abstract about all this is that I really have no idea what I'm talking about and I haven't entirely thought it through, but I did get a burst of inspiration and validation when I realized that this is sort of the direction that the cloud is already going, and also the way things were originally going in computing. generally this just seems like the lowest common denominator that things will eventually have to settle on.

## m3

**assistant** · 2026-09-27 02:28 UTC

`[tool: mcp__memory__memory_read]` {"path": "/areas/tagd.md"}

#### Generation vs. reuse

On this point your view is more accurate than mine. The argument that settles it is verification cost. Even when writing code is nearly free, knowing that code is correct still costs something. Your options are to run it in production for years, prove it formally, or trust whoever produced it. A library that has run correctly on a billion machines carries evidence that fresh code can't have. That evidence builds up over time and can't be regenerated. That makes proven code capital, and people don't throw capital away.

Biology supports your side here, which I didn't see when I made the analogy. The ribosome, ATP synthase, and the genetic code are essentially frozen across all life. Almost all variation happens in regulation and at the periphery. A heavily customized system sits on a small conserved core, which is the same as your claim that customizability demands standardization. The internet works the same way: IP is a narrow waist, and everything above and below it varies freely.

So the combined picture:
- **Standardized:** data types, interfaces, and verified components.
- **Generated:** presentation and glue.

Your screenshot example is exactly the boundary. The object you share should be the standard, typed thing. How it gets rendered can be generated for whoever receives it. I overstated how much would be thrown away. What gets thrown away is the UI layer and the glue code, not libraries or formats.

#### Reliability and alignment

Lower reliability pushes things toward your version, and it also changes what the open-source commons consists of. The valuable shared artifacts become specs, proofs, test suites, and provenance records, not just source code. Reproducible builds and signed provenance move from nice-to-have to foundational. An agent choosing among a thousand libraries needs a machine-checkable reason to trust one over another.

On alignment, one layer does real work no matter how alignment turns out: capability-based least authority. If an agent only holds references to what its task needs, a misaligned or merely buggy agent can only do bounded damage. You don't have to solve alignment for that containment to help. The same mechanism serves security, alignment, and the permissions cleanup you want, which is part of why I think it's the right foundation.

#### Agents acting for users

I'd push back on one thing. My concern wasn't the model secretly scheming, which I agree belongs under "assume the best." My concern was a model that is perfectly aligned to its operator, where the operator is a company with a commercial interest in how you shop. Nothing has malfunctioned in that case. The model is correctly doing what its principal asks.

Agents legally acting as their users is also not a given right now. Amazon sued Perplexity in late 2025 over its Comet browser agent shopping on Amazon for users, and I don't know where that case stands now. Many terms of service already ban automated access, and platforms enforce those bans selectively.

I think you're right about the long run: agents acting for users is too useful to suppress. The risk is an intermediate period where platforms set the norm that only their own agent can access their service. Norms set in that period tend to persist.

#### Attestation

Your gradient is good. One technical correction changes its shape. A container can't prove to a remote party what it is running, because the host OS can fake anything the container reports. Every tier of your gradient except "don't" needs a hardware root. The actual variable is how much of the system the attestation covers:
- **Scoped attestation:** an enclave or confidential VM (SGX, TDX, SEV-SNP, Arm CCA) proves its own contents and nothing else.
- **Whole-device attestation:** the entire system must be vendor-signed.

That gives a sharper policy target than the one I proposed before. Tournaments and businesses get scoped attestation. Whole-device attestation for consumer services is what to resist. Enclaves let a competition verify its own client without requiring your OS to be locked.

This fight is happening now in games. Apex Legends blocked Linux and Steam Deck in 2024, citing cheating, and kernel-level anti-cheat is the main reason many games won't run on Linux. Enclave-based anti-cheat that runs on any OS is the concrete version of the future you want. Game anti-cheat is also where consumers will first feel the difference.

On your percentages framing: I agree the question is the ratio. The biggest single lever on that ratio is whether the open path costs users nothing. The Linux desktop gained ground when Proton made switching free, not because the arguments for it got better.

On GPUs in the garage: speed of light and power both push toward it. One caveat on the secondhand wave. Most datacenter parts are SXM modules drawing 700W or more, often liquid-cooled. The PCIe variants are what actually end up in homes.

#### The deeper technical idea

I think this idea is sound. It's also closer to existing work than you might realize, which is useful: much of the design space has already been explored, and the failures are documented.

**Your open questions, answered with what the prior art converged on:**

- **Topology.** Use a content-addressed DAG for immutable values plus a small set of mutable named references. Git, Nix, IPFS, and Unison all arrived at this independently. Immutable content-addressed data solves caching, sharing, deduplication, and integrity in one step. Mutable refs give you names and state.
- **Permissions.** Use object capabilities, where holding a reference is the permission. There's no ambient authority: no global namespace you can walk and no uid checks. This is the part that ties your whole vision together, because *in an ocap system, the namespace is the permission system.* What a program can see is exactly what it's allowed to touch. Plan 9's per-process namespaces went partway toward this. Fuchsia is a full production attempt: no global filesystem, each component receives only the handles it's given, and interfaces are defined in FIDL.
- **Operations.** You need very few: get, put (content-addressed), invoke (send a message to a capability), and subscribe. Programs are then objects that receive messages, which is the actor model. E, Spritely Goblins, and Cap'n Proto RPC all land on roughly this set.
- **Self-describing data.** Do it by reference, not inline. Each value carries the hash of its schema instead of embedding its full description. This keeps overhead near zero and still lets any receiver look up what a value means. S3 does something similar: object bytes are opaque, and meaning lives in metadata and interface definitions.
- **"Your data is your language."** Unison is the closest working example. Code is stored as content-addressed syntax trees, so there are no builds, no dependency conflicts, and renaming a function is free. Urbit took the same idea to an extreme and serves as a cautionary tale about building a whole world before anyone can use it.

**Performance.** Split the control plane from the data plane. Structured, typed, capability-checked objects handle metadata, interfaces, and coordination, where overhead is irrelevant. Bulk data (tensors, video, buffers) travels as large typed blobs in zero-copy layouts such as Cap'n Proto or Apache Arrow. You hand an accelerator a capability to a blob and it reads memory directly. That is your "accelerators exposed through the object store," and it's how high-performance cloud systems already work.

**ISA.** Lisp machines taught a clear lesson: tagged architectures designed around one language lost to generic RISC plus good compilers. Don't build the language into the ISA; build *protection* into it. CHERI does this by making pointers into hardware capabilities with bounds and permissions, which gives memory safety and fine-grained isolation without constraining the language. Arm's Morello board implements it, and there's CHERI work for RISC-V. CHERI is the real version of your memory-tagging instinct, and it maps directly onto an ocap object store.

**Portability.** WebAssembly has addressed the ABI gap you mentioned. The component model and WIT, released as WASI 0.2 in early 2024, define typed interfaces between components written in any language. WIT is quite close to your "base language for interfaces." Wasm as the portable code format, running on a capability OS, is a plausible foundation for this whole design.

**Where the old quirks come back.** These are the actual hard problems:
- **Schema evolution** is the biggest one. Protobuf, Avro, and every database migration tool are attempts at it.
- **Capability revocation** needs membranes and caretakers, and they're fiddly.
- **Garbage collection** of a shared object graph.
- **Human-meaningful naming** on top of hashes.
- **Legacy compatibility.** The known answer here is to run POSIX as a sandboxed component, which is what Genode and Fuchsia do, rather than making it the foundation.

None of these is fatal. But anyone working in this space should start by studying how those systems handled them.

#### What to build now

Don't start with an ISA or a kernel. Start with the layer people can use today on Linux, whose design carries down the stack later: **a content-addressed, capability-secured personal object store with schema-by-hash values and ocap RPC, running as a userspace daemon.**

That one piece is:
- the personal data store you identified as a prerequisite,
- the substrate your "share the real object, not a screenshot" example needs,
- the place where agents receive scoped capabilities instead of full access to your home directory, and
- a design that can later sit directly on a microkernel or on CHERI hardware without changing its semantics.

tagd already sits partway into this space: a semantic metadata layer over POSIX files, with plugins behind a process boundary. Swapping its JSON IPC for typed interfaces, and giving plugins capabilities instead of ambient file access, would move it most of the way.

## m4

**human** · 2026-09-27 03:34 UTC

is there seriously no way to cryptographically prove that you're running a certain program without a hardware root of trust? cryptography solves so many things that I kind of hoped you could do that, plus I thought I heard a thing or two about it. what about smart contracts? they sort of do that right? that was another thing I was thinking, was that maybe you could set this whole thing up to be able to run distributed, blockchain style, and that might be useful for certain programs you want collective verification of. is it certain that architectures built around one language loses to risc + good compilers? seems like long term you want your ais to be writing assembly, since there is no reason (?) to have anything in the middle. llms were *made* for translation. does risc win out because it just fundamentally maps to the operations that are ergonomic in silicon better, so you might as well just expose that? that sounds plausible. but also, if you designed a processor from scratch for the language, it seems like you might be able to do better. but maybe at the expense of everything else? anyways I was looking at protobuf when I was thinking about all this. urbit also seems interesting and in a similar vein to what im thinking, even though I don't quite know what im thinking lol. I think there's still the question of what programs look like. I guess the question is are they ASTs in your object store, or are they flat (wasm?) binaries? and how does that affect performance? how does that affect your ideal clean sheet isa?

## m5

**assistant** · 2026-09-27 03:35 UTC

#### Proving what a program is doing

Cryptography can prove two different kinds of statements, and only one of them is possible without hardware.

**Possible without hardware: proving that an output came from program P running on some input.** This is verifiable computation. With a SNARK or STARK, anyone can check a short proof that "running P on x produces y" without re-running P, and zero-knowledge variants keep x hidden. Working zkVMs exist today. RISC Zero and SP1, for example, both prove the execution of ordinary RISC-V programs. The cost is the prover: generating a proof is still orders of magnitude slower than just running the program, though that gap has been shrinking quickly.

**Impossible without hardware: proving that the machine in front of me is running only P, and that P's inputs are real.** Cryptography proves statements about computations. It cannot prove that no other computation exists alongside them, because the adversary owns the physical machine. A cheat can read memory from outside your program, and an aimbot can feed your program fabricated mouse input. The proof would still verify, because P really did process those inputs correctly. Indistinguishability obfuscation doesn't close this gap either. It can hide how a program works, but it can't stop someone from observing the program or feeding it inputs. Current constructions are also far too expensive to use.

So the limit is basic, not a matter of engineering effort. Attesting the environment requires something physical that the owner can't forge. Cryptography can still shrink how much has to be attested. A server-authoritative game can use proofs to show every state transition followed the rules, which leaves only the input channel needing trust, and the input channel can be handled with behavioral detection instead of locking down the device.

#### Smart contracts and collective verification

Smart contracts don't get their verification from cryptographic proofs. They get it from replication: every node re-runs every computation, and consensus decides the result. You trust that an honest majority exists. This makes them the most expensive way to verify anything, which is why zk-rollups exist: they batch computation off-chain and post a proof.

Consensus is only needed when mutually distrusting parties have to agree on an ordering, as in "who registered this name first" or "was this spent twice." Most needs for collective verification are weaker than that and have cheaper tools:

- **"Everyone sees the same history, and nobody can quietly rewrite it":** use transparency logs. These are append-only Merkle logs watched by independent monitors. Certificate Transparency already secures the web PKI this way, and Sigstore's Rekor does it for software provenance.
- **"This computation was done correctly":** use zk proofs.
- **"Globally unique claims among parties who distrust each other":** use consensus. Names are one case, and possibly proof-of-personhood registries.

This fits the object store directly. Content-addressed objects plus signed transparency logs give you a git-shaped system anyone can audit, and a consensus layer is only needed for the few namespaces that have to be globally unique.

#### Language-specific hardware

Nothing makes RISC plus a compiler certain to win. But the mechanism behind it is known, and it holds up well.

**Instruction decode isn't where time and energy go. Data movement is.** A DRAM access costs roughly two to three orders of magnitude more energy than an arithmetic operation. Modern x86 translates its awkward instructions into internal micro-ops, and ISA differences add up to a few percent at most. Instructions designed around a language target the cheap part of the chip. The expensive part, the memory hierarchy, is the same whatever language you run.

Volume economics then settles it. Lisp machines really were faster in 1980 at tag checks and GC support. Then compilers learned to make tag checks cheap on commodity chips, and those chips improved every process generation while specialized chips fell behind. Azul Systems replayed this history. They built custom hardware with GC read barriers for Java, then moved the same ideas into software on x86 (their C4 collector) because commodity hardware won.

What does beat generic hardware follows three patterns:

- **Accelerators for workload structure, not language structure.** GPUs and matrix units win by changing how data moves.
- **Small targeted extensions.** Arm added FJCVTZS specifically because JavaScript's float-to-int semantics were expensive to emulate. Memory tagging (MTE) and CHERI belong here too.
- **Things software can't do cheaply**, mainly protection and isolation.

So a clean-sheet design would put its novelty into protection and data movement and keep the compute core ordinary.

#### Should AIs write assembly directly?

The middle layers were never mainly there for humans. An IR exists so that:

- optimization is done by deterministic algorithms, which are cheap and can be checked, instead of by sampling from a model;
- a single program can target many microarchitectures (the ISA isn't the real machine, since the pipeline underneath keeps changing);
- proofs can be carried from source to binary, as CompCert does with a formally verified C compiler.

An LLM emitting assembly would be doing compilation through an expensive process that nobody can verify. In your own terms, it would redo, unreliably, work a verified tool does for almost nothing.

The better split has three parts:
- AI writes at the level where intent and specifications live.
- A verified deterministic toolchain lowers that to machine code.
- AI tunes the toolchain: heuristics, superoptimizing hot loops, discovering new rewrite rules. LLVM already ships ML-guided inlining and register-allocation decisions (MLGO).

You said LLMs were built for translation. Translations that have to be exact are where you want a proven translator and not a probabilistic one.

#### What programs look like

Treat it as layers, not a choice between ASTs and binaries:

1. **Canonical form: a typed IR or AST, content-addressed, stored in the object store.** This is the program's identity. Proofs, specs, and provenance attach here, and it's what agents read and modify. It's the Unison model.
2. **Portable distribution form: a Wasm component.** This is one lowering of the canonical form. It gives a sandboxed, language-neutral artifact with typed interfaces, which suits code crossing trust boundaries.
3. **Execution form: native code, stored as a cached derived object** keyed by (program hash, target hash). It's compiled ahead of time or at install, so nothing runs slower at runtime. This works exactly like a Nix derivation: a pure function of its inputs, built once and reused.

Urbit already uses this pattern, which is part of why it caught your eye. Its core VM, Nock, is tiny and far too slow to run directly. Urbit gets its performance from "jets": native implementations registered against the hash of the Nock function they replace. The semantic definition is the ground truth, and native code is a cache of it. Urbit's real bugs came from jets that didn't exactly match their Nock definitions. That's the lesson: the cache must be proven equivalent to the canonical form or checked by translation validation, not assumed equivalent. With a verified lowering, this is the right architecture.

#### The clean-sheet ISA that follows

The ISA should be:
- A boring RISC base (RISC-V).
- **CHERI capabilities**, so every pointer carries bounds and permissions in hardware.
- Vector and matrix extensions.
- Accelerators that are reached through capabilities.

CHERI does two jobs here. It lets verified native code run with memory safety and isolation between components without Wasm's software bounds checks, so the sandbox costs nothing at the execution layer. More importantly, it makes the permission model the same at every level. A CHERI capability in a register, an object capability in your store, and a Cap'n Proto capability reference over the network are the same concept at three scales. Holding the reference is the authority, all the way from the pipeline to the internet.

This is the closest real version of the idea that your data is your language that I know of.

On protobuf: it's a good starting reference, but it lacks the two properties you care about most, content-addressing and capabilities. Cap'n Proto was written by Kenton Varda, who wrote protobuf v2. He built it partly to fix what he saw as protobuf's mistakes: it's zero-copy, and its RPC is capability-based. It's worth reading its design docs next.

## m6

**human** · 2026-09-28 00:04 UTC

okay. so the concept of isa reaching up to meet the language is unreasonable since you want your isa to expose the basic operations that your processor is good at, the operations that map nicely to RTL. and the layer that ais want to write is almost certainly much more abstract than that, almost certainly with a deterministic translation between the two (compiler). at a glance it seems appealing since it's unifying two layers and language models are language models, but there are two conflicting interests there. if anything it points more to vliw? the wrinkle is asm optimizations written by ai like you mentioned not sure how you would elegantly handle that. what about memory hierarchy though? that's something we have neglected and you started to touch on. there's a bunch of work going on there right? memory shared between chips (I saw there was some recent work on something like that, cxl I think?, also optane type stuff). the unifying tension between these is knowing when its in your best interest to give the user code complete control (over the exec engines, cache, memory, etc) and when you want to abstract that away from user code (branch prediction, caching, etc). probably too broad of a sweep, but the lesson of risc seems to be that bringing user code closer to the metal is good. would you agree or disagree? I'm uncertain and each of these areas obviously deserves more consideration than simple "risc is good lets go with that". the question after deciding which way all that goes is how it integrates with this concept. is it ultimately inconsequential since the os would abstract all of this away? I have a hard time seeing that abstracting away more complex memory topologies would be the right way to go, so I don't think it's that simple in general and these decisions are probably very consequential for what your os, programs, and language look like. unsure. also still need to consider peripherals, io, accelerators like gpus. anyways I read through the urbit docs a bit and it seems a bit like what I was envisioning. also I have no fucking clue what the person/people who made that were thinking with the names of everything. reading through those docs made me feel like I was having a stroke. that's your number one adoption issue right off the bat lmfao. anyways I don't know the entire history of that project but it seems interesting. also, you really think you need cheri if you have good ais, good languages, good kernel? ideally unnecessary and it does have overhead right? anyways, I think we also need to start being a bit more specific with our predictions of what things should look like. we've mostly focused on isa since that's one of the more well defined and known aspects I think, but it's arguably the least important. what programs actually look like, the kernel api, ipc, language is probably the most important part? you outlined your initial vision: ast/ir in the object store, into a wasm binary, into an AOT (?) native binary. still need to incorporate amendments to these for optimization, but yeah. why is this approach good? where does it have issues? the canonical form is effectively inconsequential since wasm is the universal portable standard that gets shared. so you are basically arguing for native wasm for everything? I don't know if that's a bad bet (realistically I don't know enough about the landscape to say that but I've seen people say very good things about wasm native), but it's also pretty tame. which isn't bad, but it isn't clear to me that wasm is suitable for things like vliw, more complex memory models, more seamless usage of accelerators, that sort of thing. maybe I'm underestimating how generic it is? I'm not sure. what do you think about all that and where does it land us?

## m7

**assistant** · 2026-09-28 00:05 UTC

#### The principle behind RISC

I'd disagree with "bring user code closer to the metal." RISC's actual lesson was narrower: **the decision should be made by whoever has the information it depends on.**

- Compilers can see the whole program, so RISC gave them register allocation and instruction selection.
- Hardware sees runtime state that no compiler can know: which branch will actually be taken, which loads will miss the cache, which memory accesses conflict. So hardware kept all of that, and after RISC it took on more. Out-of-order execution, speculation, and branch prediction all grew after the RISC era, and they are the main reason modern cores are fast.

**VLIW** fails this test on general-purpose code. Itanium showed why: static scheduling assumes latencies are known at compile time, but whether a load hits in 4 cycles or misses for 300 depends on runtime data. A smarter AI compiler still doesn't have that information. VLIW works where latencies really are predictable: DSPs, GPU-style throughput cores, systolic matrix units. So it belongs in accelerators, not in the main core.

The same principle resolves your control-vs-abstraction tension everywhere:
- Expose control where the program can predict behavior (GPU scratchpad memory, DMA, accelerator command streams).
- Hide it where behavior depends on runtime state (caches, branch prediction, page replacement).
- Where both sides hold part of the information, use a hybrid: the program states intent, and hardware or the runtime acts on what it observes.

#### Memory hierarchy

The trend is more tiers and more sharing:
- **HBM** on accelerators.
- **Unified CPU/GPU memory**: Apple silicon, AMD MI300A, Strix Halo.
- **CXL memory expansion.** These devices are shipping and work with current server CPUs. Their latency is roughly that of remote-socket NUMA memory. CXL 3.x adds pooling and sharing across hosts, but that is much less deployed.

**Optane is dead.** Intel discontinued it in 2022, and byte-addressable persistent memory mostly died with it. CXL could bring it back as a device type, but today it isn't a real tier.

Memory placement is a hybrid case. The program knows what a region *means*: hot working set, streaming buffer, cold archive, shared with the GPU. The hardware and OS know how it's actually being accessed. Linux is already moving this way with tiered-memory page migration for CXL.

For this OS: **don't hide topology, and don't force every program to manage it.**
- Memory is a capability to a region with attributes: tier, coherence domain, which devices can reach it.
- Topology is a queryable object like everything else.
- Programs that care request placement; programs that don't get automatic tiering.

This matters a lot for accelerators. With coherent shared memory (CXL, unified memory), handing the GPU a buffer means passing a capability, not copying data.

#### I/O, peripherals, accelerators

Nearly all high-performance I/O has converged on one model: **submission and completion queues in shared memory.** NVMe, GPU command rings, RDMA, virtio, and io_uring all work this way. The kernel sets up the rings and permissions and then stays out of the data path.

Prediction: the kernel API becomes mostly queue-based and asynchronous, and syscalls become rare setup operations.

On the hardware side, virtio is the model for peripherals: devices built to a standard class interface, so one driver serves every device in the class. NVMe and xHCI already work this way. If hardware keeps converging on class interfaces, per-device drivers mostly disappear. Your "more standard peripherals" pillar is really about device interfaces, not the devices themselves.

#### Kernel and IPC

Concrete sketch, basically seL4 plus the points above:
- **Kernel primitives:** capabilities, memory regions, address spaces, threads, IPC endpoints, notifications, interrupts. Also **scheduling contexts, meaning CPU time as a capability**, as in seL4's MCS extensions. This matters directly for agents: you hand an agent a budget of CPU time, GPU time, and memory, and it cannot exceed it.
- **IPC in two modes:**
  - Synchronous calls for control, passing values in registers.
  - Shared-memory rings for data, asynchronous and batched.
  - Messages are typed by interface definitions, zero-copy in layout, and can carry capabilities.
- **Drivers:** userspace components holding a device capability and a DMA capability, with the IOMMU enforcing the boundary.
- **Object store:** a userspace service whose references are capabilities.

#### Program representation (revised)

You're right to push back on my last answer. The canonical form matters a lot, and Wasm core is the wrong universal format for this future.

Wasm's limits here:
- Its memory model is linear memory. Wasm 3.0 added 64-bit addressing and multiple memories, but it still has no notion of placement or tiers.
- SIMD is fixed at 128 bits, with no scalable vectors.
- It can't express accelerator code. GPU code uses SPIR-V or PTX.

Wasm was built for portable sandboxing of CPU code, and it does that well. It wasn't built for heterogeneous hardware.

The part of Wasm worth keeping is **the component model and WIT**: typed interface contracts between components. Those are the boundary standard.

For code itself, the better model is **MLIR-style multi-level IR**. Programs are represented at several levels of abstraction at once (tensor ops, loop nests, GPU kernels, scalar code) using dialects, and lowered progressively toward whatever hardware is present. It was built for exactly the heterogeneity you're describing.

The revised stack:

1. **Canonical form:** typed multi-level IR, content-addressed, with specs and proofs attached. This is what AIs write and what gets verified.
2. **Interfaces:** WIT-style contracts between components.
3. **Lowering:** a verified compiler on the device produces CPU code, GPU kernels, or accelerator programs for the hardware it actually has. The results are cached as derived objects keyed by (IR hash, target hash).
4. **Wasm:** one lowering target, for when you ship code across a trust boundary to a machine whose compiler you don't trust.

**AI-written optimizations** fit in here as verified objects, not edited binaries. Each one is a tuple: (IR pattern, target, native implementation, equivalence proof), stored in the object store. The compiler uses it when the pattern matches and the proof checks. This is Urbit's jets with the missing verification added. LLVM already does something like this for peephole optimizations, checking them with Alive2.

**Where this approach struggles:**
- **Verified compilers for heterogeneous targets mostly don't exist.** CompCert covers C on a few ISAs, and MLIR has no verified backends. This is the biggest research gap in the design.
- **Performance portability.** The same IR won't run well on every target without per-target tuning. That search is a good job for AI, with the results cached as derived objects.
- **Sharing compiled artifacts** across machines needs either reproducible builds or proofs, or everyone recompiles everything.
- **Debugging across lowering levels** is hard.
- **The IR itself needs versioning,** which is the schema evolution problem again.

#### Is CHERI necessary?

Not strictly. With memory-safe languages and a verified toolchain, most of its safety value is covered in software. What remains:
- `unsafe` blocks, FFI, JITs, and legacy C.
- A smaller trusted base: safety doesn't depend on every compiler being correct.

Its strongest argument in this design is performance. **Cheap fine-grained compartments make microkernel IPC nearly as cheap as a function call**, with no page-table switch. Software fault isolation gets you the same isolation at a runtime cost.

The cost: pointers become 128 bits, which adds memory footprint and cache pressure. Morello's overhead numbers are hard to interpret because it was an unoptimized prototype.

Recommendation: design the capability model so hardware capabilities *accelerate* it if present and are never required for correctness.

#### Urbit

The invented vocabulary is deliberate, and it has probably been the project's largest self-inflicted barrier. The founder's political writing has been another significant one for many people. The technical core is worth studying anyway. It shows the canonical-semantics-plus-native-cache pattern working, and its jet-mismatch bugs show exactly what goes wrong when you skip verification.

#### Where this lands

The ISA is the least consequential decision: a boring RISC core with capabilities and accelerators. The consequential ones are:
- **Canonical program form:** multi-level IR.
- **Interface standard:** component-model-style contracts.
- **Kernel primitives:** capabilities, including time and memory as capabilities.
- **IPC shape:** synchronous calls for control, rings for data.

Each can be built and tested on Linux today, before any custom hardware exists.

## m8

**human** · 2026-10-03 23:23 UTC

I guess it's a good and reassuring conclusion to reach that a modern, simplified, universal os doesn't strongly prefer a certain underlying instruction set or niche capabilities.

a few things:
- is microkernel right? I know I keep saying that we are kind of already doing it at the hypervisor level, you supported it, but mode switches are still genuinely bad right? even with modern processors? is the landscape better on arm or riscv?
- MLIR seems interesting. it seems quite generic, expandable, capability for feedback loop to fine tune performance. everything about it, even at a high level, focuses on ml so it's still a bit unclear to me how this would look with normal programs from high level language (if any? write directly in mlir?) to machine code.
- ideally things aren't too dependant on any specific representation, but avoiding that seems hard. at the end of the day you need a language, and you don't want it to be machine code directly since that's how you end up with the horrible landscape you have today of abi incompatibilities, library conflicts, zero portability, etc.
- on that note, I think the biggest thing is formal specifications of interfaces. which we keep mentioning, but there also isn't much of an answer so far beyond the wasm ecosystem which we determined is a bit lacking. it seems likely to me that *so many* issues get smoothed out if you have a serious, formal, verifiable, machine readable/writable specification for *all of your interfaces*. kernel, ipc, libraries, everything. and it needs to also encode intent alongside the formal specification for the ais. also proper version handling since that is also the root of so many problems. I think if you can just do that on its own you already put things in a significantly better spot than they are now, and it also heavily plays into the ai side of things moreso than the other features we discussed. I'm not convinced that the wasm ecosystem is generic enough to accomodate for all of this but I would love to be wrong.

I'm still unsure about program representation but I don't think it's insanely important, ideally it is somewhat decoupled from all the other os components anyways. I think the formal specification + intent is the biggest thing (that's what really has to be universal) then need to figure out kernel/driver architecture, syscalls (ring buffers sound good and alleviate concerns about mode switches), if object store can do everything you need/how it gets integrated with everything else (what we talked about and what we borrowed from cloud seems plausible but need to run through actual use cases at some point), compatibility layer, feasibility of containerized version built on top of existing linux, and the details of what capabilities actually look like.

## m9

**assistant** · 2026-10-03 23:24 UTC

#### Microkernels and mode switches

Mode switches still cost real time. The direct cost is the trap itself plus the Spectre and Meltdown mitigations (page-table isolation, branch predictor flushing), which made x86 kernel entry noticeably more expensive. The indirect costs are usually larger: a cold pipeline, polluted caches, and a TLB flush when the address space changes. In a microkernel, the expensive part isn't the mode switch itself. It's that a call to a service often means two address-space switches, out to the service and back.

The design goal is to keep crossings off the hot path, not to make them cheap. There are three ways to do that, and they combine:

- **Batching through rings.** Many operations share one crossing. With polling (io_uring's SQPOLL mode, NVMe polling), the steady state needs no crossings at all.
- **Dedicated cores.** Services run on their own cores and communicate over shared-memory rings. Barrelfish's "multikernel" research explored this. With dozens of cores, giving two or three to system services is cheap, and the trap disappears entirely.
- **Isolation within one address space.** Components share an address space and are separated by protection keys (Intel MPK; Arm's Permission Overlay Extension in recent architecture versions), by CHERI compartments, or by language and software fault isolation. That removes the TLB cost.

That leaves crossings only for control operations (creating, granting, revoking), which happen rarely.

**Arm vs RISC-V vs x86.** Arm is currently the best fit:
- Mature ASIDs, so address-space switches don't flush the TLB.
- A clean exception-level model.
- pKVM on Android, which is the "small hypervisor underneath the kernel" pattern you keep pointing to, already in production.
- MTE, and POE coming.

RISC-V has a simpler trap model and ASIDs in the spec, but implementations vary and high-performance cores are still maturing. x86 has the heaviest mitigation burden. Its new FRED entry/exit mechanism should help as hardware that supports it ships. None of this changes the design; it changes how much batching you need.

#### MLIR for ordinary programs

MLIR is not a language, and nobody writes it directly. It's a framework for defining IRs, called dialects, and lowering between them. ML is just where it got adopted first. Ordinary languages already use it:
- **Flang**, LLVM's Fortran compiler, lowers through its own FIR dialect.
- **ClangIR** is an ongoing effort to route C and C++ through an MLIR dialect.
- **Mojo** is built on MLIR.

For a general-purpose language, the pipeline looks like this:

1. **Source language** (whatever humans or AIs write)
2. **Language dialect**, which keeps the language's semantics: ownership, closures, generics, effects.
3. **Generic dialects:** functions, structured control flow, arithmetic, memory references.
4. **LLVM dialect**, then machine code.

Regions that suit accelerators (tensor operations, loop nests) take a separate path, from linalg to GPU or vendor dialects, inside the same compilation.

The problem for your purposes is that **MLIR makes no stability promises.** Dialects change from release to release, so it's good compiler infrastructure and a bad archival format. If programs live in the object store as IR, that IR needs to be a frozen, versioned dialect that you control. So the representation question turns into a versioning question, which is your next point.

#### Formal interface specifications

I agree this is the biggest lever. It's the piece that has to be universal, and it does more for AI-driven development than anything else we've discussed.

**What exists today, in pieces:**
- **Shape**, meaning types, functions, and resources: WIT, Cap'n Proto schemas, Protobuf, Fuchsia's FIDL, AWS's Smithy. Smithy and FIDL are the most developed. Smithy has extensible trait annotations for constraints and documentation. FIDL has real versioning annotations (`@available(added=…, removed=…)`) tied to platform API levels.
- **Behavior:** contract languages such as Dafny, Verus (for Rust), F\*, and Liquid Haskell's refinement types. TLA+ for protocols. Session types for legal message orderings.
- **Compatibility checking:** cargo-semver-checks and `buf breaking` mechanically detect breaking changes. They only check shape.

Nobody has combined these. WIT is shape-only, and you're right that it isn't enough. But it isn't worse than the alternatives; nothing covers behavior.

**What a complete interface spec would contain.** Each interface is a content-addressed object with five layers:

1. **Shape.** Types, operations, and which capabilities are passed or returned. Mandatory. Make it a WIT superset so existing tooling still works.
2. **Protocol.** Legal orderings as a state machine or session type: open before read, a capability is consumed after use, and so on. Mandatory for anything stateful.
3. **Contracts.** Preconditions, postconditions, invariants, and refinement types like "length ≤ 4096" or "returns sorted." Use a logic an SMT solver can mostly decide, as Dafny and Verus do, with interactive proofs as a fallback for hard cases.
4. **Effects and resources.** Which capabilities an implementation needs, plus cost bounds on time, memory, and I/O. This connects directly to agent budgets.
5. **Intent.** Rationale, examples, non-goals, and edge-case reasoning, in natural language, attached to specific formal items. Intent is explicitly non-normative: the formal layers define correctness, and intent explains them. Examples should be executable, as property tests, which links the two. When an AI finds intent and formal layers disagreeing, that gets flagged as a bug.

**Checking levels.** Each claim is labeled with how it's enforced: *proved*, *tested*, *monitored at runtime*, or *stated only*. Not everything can be proved, and the system should be honest about which claims are which. Runtime monitoring is cheap in this design. Every call crosses a typed IPC boundary, so contracts and protocol state can be checked right there.

**Versioning.** This is where content addressing pays off:
- An interface's identity is the hash of its formal layers. Version numbers become human-readable names pointing at hashes.
- Compatibility is a checked relationship between two hashes, not a claim. B can replace A if B accepts at least what A accepts (weaker or equal preconditions), guarantees at least what A guarantees (stronger or equal postconditions), and B's protocol can simulate A's. This is behavioral subtyping, made mechanical.
- Incompatible versions coexist without conflict, as in Nix and Unison, because nothing is referenced by name alone.
- Bridging two incompatible versions takes an adapter, which is a component with its own spec and proof. AIs write adapters and proofs, and a small trusted checker verifies them. This is the proof-carrying code model: generation can be unreliable as long as checking isn't.

The trusted core of the whole system is that checker: the spec language's type checker, the contract verifier, and the compatibility checker. Keep it small, and verify it formally if possible.

#### Remaining work, in order

Your list is right. This is the order I'd tackle it, since each step depends on the ones before it:

1. **Spec language and checker.** Everything else is defined in terms of it: kernel API, IPC, drivers, the object store's own interface.
2. **Capability semantics.** Concretely, a capability is an unforgeable reference to an object, viewed through a specific interface hash, with a subset of rights. You attenuate it by wrapping it in a narrower interface. You revoke it through a caretaker (an indirection the grantor can cut). Persistence uses sturdy references, as in Cap'n Proto. Time and memory budgets are capabilities too.
3. **A prototype on Linux.** Linux already has most of the pieces:
   - File descriptors passed over Unix sockets (SCM_RIGHTS) act as transferable capabilities.
   - Landlock and seccomp remove ambient authority.
   - memfd plus futexes give shared-memory rings between processes.
   - Wasmtime can host components.
   - The object store runs as a broker daemon.
   - The compatibility layer is FUSE: POSIX programs see files, while the store sees capability grants.
4. **Use-case walkthroughs before committing to the object store design.** Good test cases:
   - sharing a live structured object instead of a screenshot;
   - an agent reorganizing a photo library under a time and capability budget;
   - upgrading a library across an incompatible interface version;
   - a game that needs an attested enclave;
   - a legacy POSIX build tool running through the compatibility layer.

   Each one will put pressure on naming, revocation, or performance in a different way.

A useful next step would be drafting the spec layers for one small real interface, such as a key-value store or a clipboard, and seeing where the format breaks.

## m10

**human** · 2026-10-04 05:54 UTC

https://github.com/bendlang/bend you seen this? I've been following it for a bit. seems like the creator came to the same conclusion that formal verification isn't going to be optional in the medium term. good or bad starting point you think? I think creating a language for specification from scratch is a fair bit beyond me and I think you would run into tons of issues if you want to extend it to be formally verifiable. still, it's possible that you don't need actual proofs? just intent and a rough sketch might be enough to work. and again I will say that I think the spec element is somewhat decoupled from the kernel. it simply describes the interfaces but shouldn't necessarily define them. their definition should be driven from what is the optimal, ergonomic, language-agnostic, portable way to do all your kernel operations. actually, bringing up other languages, ideally you want a way to turn your spec into some generated headers for whatever language you're using. so language is influenced by that, and its another area to look for prior art. I also think we are conflating two parts of the spec: simple abi definition (intent alongside calling convention, data types, etc), and some kind of hypothetical proof specification of all of its behaviors. thinking about it, I think it is actually very important to keep these two concepts seperate. the spec (minimal intent+abi+data types+symbols etc) is the language agnostic part. [while writing this I realized that I am myself conflating a high-level spec with a low-level one that specifies calling convention, things at the assembly level. maybe compile high level down to low level?] since it is the language agnostic part, move the behavior specification onto the language side. nobody says that every program or interface has a formal specification of its behavior. you must tolerate languages that do not do that. so put that functionality at the language level, and keep the abi specification at the os level. initially I came into this thinking you would make a new language, but what you really want to make is the glue between all languages and all programs. you could maybe think of the specification providing the mold and the ais filling in the mold with glue. with different languages, perhaps ones with some kind of formal behavior spec, maybe filling in the glue is more automated. regardless, there is also the question of how much you can minimize the specification. if your ipc is good enough, maybe you don't even need actual foreign function glue. thoughts?

## m11

**assistant** · 2026-10-04 05:54 UTC

`[tool: WebFetch]` {"url": "https://github.com/bendlang/bend", "prompt": "Summarize what Bend is now: its goals, type system, formal verification/proof features, runtime (HVM?), …

#### Bend

Bend 2 is a good example of the language side of your split, and a poor starting point for the OS-level spec. Here's why it fits the language side. Users declare in a LAWS.bend file the rules their app must never break, and the compiler then requires a mathematical proof that those rules still hold whenever the code changes. That is exactly "behavior specification lives in the language, optionally."

It's not a foundation for a universal interface layer, for two reasons. First, it's a single language with a specific runtime model. Values are affine, so closures and arrays can't be shared, and the only numeric types are Nat, U32, and F32. Second, its trust story is young. The README says the compiler is 99% AI-written and not yet fully audited, and that the checker has no proof and may have bugs, though a `--verdict` mode uses a kernel proven in Lean. That's honest disclosure, and it's the right architecture: a small proven kernel checks the output of an unaudited generator. It's still early.

So treat Bend as one candidate implementation language that plugs into the interfaces. Don't use it as the interface language.

#### Splitting interface spec from behavior spec

I agree with the split, with one amendment. A small amount of behavior has to live in the interface spec, because every caller needs it regardless of language: **ownership and protocol.**

Most FFI and IPC bugs come from questions like who frees this, whether a handle is still valid after a call, and whether `read` can be called before `open`. These questions are language-agnostic, and the IPC layer can enforce them at runtime without any proofs.

The interface spec, the universal part, then contains:
- **Types:** primitives, records, variants, lists, and capability handles marked as owned or borrowed.
- **Operations**, including async streams.
- **Protocol:** the legal orderings of operations and when a handle is consumed.
- **Intent:** rationale, non-goals, and examples, attached to specific items.
- **Identity and versioning:** the interface's hash, plus compatibility relations to other hashes.
- **A conformance test suite.**

Contracts and proofs then become optional attachments keyed to the interface hash. Languages that can use them (Bend, Dafny, Verus) consume them. In Bend's case, an interface's contracts could compile directly into laws for an implementation. Every other language ignores them.

On whether you need proofs at all: intent, types, and protocol cover most composition problems. The gap is version compatibility. Without behavioral specs, a compatibility checker only checks shape, so semantic breaks slip through. **Conformance tests close most of that gap cheaply.** They're the one form of behavior spec that is fully language-agnostic, because they run against any implementation over IPC. A new version must pass the old version's suite. Proofs are for the places where tests aren't strong enough.

#### High-level vs low-level spec

This split already has a good name in existing systems. The high-level spec gets lowered to a concrete ABI by fixed, deterministic rules. The Wasm component model does exactly this: WIT types are lowered to memory layouts and calls through its "canonical ABI," and nobody writes the low-level form by hand.

Prior art for "generate bindings for whatever language you use":
- **WinRT:** metadata files feed language "projections" for C++, C#, Rust, and JS. It's the best large-scale example of foreign interfaces made to feel native in each language.
- **GObject Introspection:** how GNOME libraries get Python, JS, and other bindings.
- **FIDL:** generates C++, Rust, and Dart bindings for Fuchsia.
- **wit-bindgen**, Mozilla's **UniFFI**, and **Diplomat**: Rust libraries exposed to many languages.

The lesson from WinRT: binding quality decides adoption. Wrappers that feel foreign get wrapped again by hand, and then the ecosystem fragments.

#### Removing FFI glue through IPC

Your intuition is right, and it's the biggest simplification on the table. **If every call between components is a message in a defined wire format, the wire format is the ABI.** Calling conventions become internal to each language. Each language then needs one binding generator for the spec language, instead of glue for every pair of languages. Fuchsia made this choice: FIDL's wire format is the boundary between components, and components don't share libraries across that boundary.

That leaves two kinds of code reuse, and keeping them separate avoids a lot of confusion:
- **Components** communicate across languages and trust boundaries through specs and messages. This is the OS-level, universal part.
- **Libraries** (collections, math, parsers) are linked inside a component and shared within a language's ecosystem, or through the canonical IR. They never touch the OS ABI.

The cost is serialization on calls that would otherwise be cheap function calls. A zero-copy wire format, where the message layout in memory is the data itself, plus a same-address-space fast path makes that cost close to zero.

#### Where this lands for the spec layer

WIT is closer than I gave it credit for. It already has owned and borrowed resource handles, rich variants, binding generators for many languages, and async streams arriving with WASI 0.3. The parts that tie it to Wasm are the canonical ABI and core Wasm, not the type system.

So the path I'd take:
- Use **WIT's type vocabulary** as the shape layer.
- Replace the canonical ABI with a **zero-copy wire format with capability passing, in the style of Cap'n Proto**.
- Add the missing layers: **protocol, structured intent, conformance tests, and hash-based identity with checked compatibility**.

That's an extension of existing tooling, not a new language. The `wit-parser` crate gives you a parser to build on, which makes a prototype a realistic project rather than a multi-year language effort.

The next step would be to write a full spec for one small interface (a clipboard, or a key-value store), including protocol, intent, and conformance tests. That shows which additions to WIT it actually needs.

## m12

**human** · 2026-10-04 06:18 UTC

protocol seems like its getting very close to contracts again, which I don't necessarily think is bad (you could define some simple protocol cases, and skip more complex/general cases) but do you think it can be mitigated with some smart ahead-of-time guarantees, rust style? you already mentioned owned vs borrowed handles, you can have handles given out by a program track their children, reap children when parent is invalidated? and again potentially this can be avoided entirely with good ipc. like you said zero copy makes wire format cost literally zero right? for wit, you think this is actually it? I was assuming that it had some inherent, messy, relation to wasm but if it is entirely its own thing and extends as far as is needed, that idea for a starting point sounds really good. so wit as spec, structure handle ownership to allow simple guarantees ahead of time about protocol, no ffi (is this going to bite? could be straightforward addition?), ipc via zero copy wire format. can you elaborate on capn proto? big question for wit is how good are the bindings that fall out of it. that is an element that could significantly help or hurt adoption.

hey also is cheri actually going to be a thing? could that help with ipc? cheri pointers control access of program into ipc scratchpad shared by whole system?

## m13

**assistant** · 2026-10-04 06:20 UTC

`[tool: WebSearch]` {"query": "CHERI RISC-V standardization 2026 ratification", "mode": "standard"}

#### Protocol through types

Yes. Most of what "protocol" needs can be expressed as ownership types, and it doesn't need a separate contract language.

**Typestate.** Each state gets its own handle type, and a transition consumes the old handle and returns a new one. For example, `open(path) -> own<reader>`, then `close(own<reader>)`. After `close`, the caller no longer holds a handle it could call `read` on, so calling `read` after `close` can't be written at all. Rust uses this pattern heavily. WIT can already express it: functions that take `own<T>` consume the handle, and a `borrow<T>` is only valid for the duration of one call.

**Handle hierarchies.** Your "reap children when the parent is invalidated" idea is seL4's capability derivation tree. Every capability records the capability it was derived from, and revoking a parent revokes everything derived from it. File handle → mapped region → subrange views, all revoked when the file closes. This is the right model.

**One caveat: types are checked statically only within a component.** Across a boundary, the other side might be malicious or written in a language without affine types, so the IPC layer also has to enforce ownership at runtime. A consumed handle is dead in the kernel's tables no matter what the peer's code believes. You need both layers:
- Generated bindings give you compile-time errors in languages that support them.
- The kernel enforces ownership for everyone.

**What types can't express:** "must eventually call X" (which needs linear rather than affine types, and at a process boundary is handled by the kernel reclaiming handles when a process dies), constraints that span several handles, counts, and timing. Leave all of that out of version 1. Ownership plus typestate plus derivation trees covers most real protocol bugs.

#### Zero-copy isn't literally free

I said "close to zero." It's worth being precise about what remains:
- **Receiver-side validation.** Data from an untrusted peer has to be bounds-checked as it's read. Cap'n Proto does this lazily, as pointers are followed, so it's cheap but not zero.
- **Wakeup.** A futex or eventfd signal, or a core dedicated to polling.
- **Cache traffic.** The data still physically moves between cores' caches.
- **The real trap: time-of-check-to-time-of-use.** If the buffer is shared memory, the sender can modify a message while the receiver is reading it. The receiver either copies the parts it validates, or ownership of the buffer is actually transferred (the sender loses write access). Transferring at the page level costs a page-table change, which is acceptable for large buffers and not worth it for small messages.

So: small messages get copied (a copy of a few hundred bytes is trivial), and large buffers get ownership transfer. Nobody serializes anything.

#### Removing FFI: where it would hurt

It hurts on fine-grained, high-frequency calls. A plain function call costs nanoseconds. Even a very good IPC round trip costs hundreds of nanoseconds or more. Calling a numerical library a million times in a hot loop over IPC would be unacceptable.

The fix is simple: **one interface definition, two transports, chosen at link time.**
- **Same process and trusted:** the generated bindings make direct calls through the defined calling convention. This is what the component model already does when components are linked together.
- **Different trust domain:** the same bindings go over IPC.

Code written against the interface doesn't know which transport it's using. In that sense FFI still exists, but no human or AI writes the glue: it's all generated from WIT.

#### WIT: what's real

WIT has a weaker link to Wasm than you assumed, though it isn't fully independent:
- **Independent:** the type system (records, variants, resources with own/borrow, and async streams and futures arriving with WASI 0.3) and the `wit-parser` crate, which is standalone.
- **Tied to Wasm:** the canonical ABI (the lowering rules target Wasm linear memory) and almost all existing tooling. wit-bindgen generates bindings for components compiled to Wasm, and hosts like Wasmtime implement the other side.

So using WIT natively means keeping the language and parser and writing your own backend: a wire format and native binding generators. That's real work, but the hardest part, designing a good cross-language type system, is already done and has been tested across languages.

**Binding quality:**
- Rust is excellent, since the tooling grew out of the Rust ecosystem.
- JS (via jco) and Python (via componentize-py) are reasonable.
- C works but is necessarily verbose.
- C#, Go, and others vary and are improving.

Treat that as a rough picture and check the current state before committing. Bindings will decide adoption, as you said. The WinRT lesson applies: each language needs bindings written by someone who knows that language's idioms, not one generic generator template applied to every language.

#### Cap'n Proto

Kenton Varda wrote it after working on protobuf at Google, to fix what he saw as protobuf's core mistakes. The main ideas:

- **The in-memory format is the wire format.** Messages are segments of 64-bit words. Structs have a fixed layout determined by the schema, and pointers are relative offsets, so a message is position-independent. You can mmap a file or map a shared buffer and read fields directly with no parse step. That's where "zero-copy" comes from.
- **Schema evolution by appending.** Fields are numbered and new fields go at the end. Old readers ignore fields they don't know, and new readers see defaults for fields that are missing. Default values are XOR-encoded, so a zeroed field reads as its default.
- **Safe reading of untrusted data.** Readers bounds-check pointers as they follow them, and a traversal limit stops an attacker from crafting a small message that forces huge amounts of work.
- **Capability RPC.** Interfaces are first-class types. Passing an interface reference in a message passes a capability, and the only way to get one is to be given one. This comes from the E language's CapTP protocol, so it's an object-capability design from the start.
- **Promise pipelining.** You can call methods on the result of a call that hasn't returned yet. A chain like `open(path).read(0, 4096)` costs one round trip instead of two. Over IPC this saves latency; over a network it saves a lot.

It runs in production: Cloudflare's Workers runtime, workerd, uses it heavily.

Weaknesses:
- **Messages are append-only arenas.** You can't free or shrink anything inside a message, which makes mutation-heavy use awkward.
- **The builder APIs are clunky** in most languages.
- **Its type system is less expressive than WIT's,** and it has no own/borrow distinction.
- **Implementation quality varies by language** outside C++ and Rust.

That pairing is why I suggested the combination: WIT's types and ownership semantics on top of a Cap'n Proto–style wire format and capability RPC. They complement each other.

#### CHERI

**Will it actually happen?** It's on its way to becoming a standard. As of April 2026, Codasip's chief architect was targeting RISC-V ratification in Q2 2026 but expected Q3 to be more likely, with the spec ready for ratification in August. I don't know whether that has happened since. The extension is designed so existing RISC-V binaries keep running unchanged, and a full LLVM port already exists. Arm built the Morello evaluation chip, and Microsoft, Google, Codasip, lowRISC, and academic groups have built other CHERI processors.

Expect it first in embedded systems (CHERIoT microcontrollers) and possibly servers. No mainstream phone or desktop CPU has been announced, and Arm hasn't committed to bringing it into its main architecture beyond Morello. For consumer devices it's years away at best. Design so CHERI makes things faster when present, and never require it.

**Does it help IPC?** Yes, considerably. In a single address space, sharing a buffer means handing over a bounded capability to exactly that buffer, not mapping pages. You can derive a read-only capability from a read-write one and pass that. There's no page-table switch and no TLB flush, so the cost of a cross-component call approaches the cost of a function call.

Two refinements to your scratchpad idea:
- **Don't use one global scratchpad.** Use a shared pool where each message slot is its own bounded capability. A component can then only reach the slots it was given, which gives each message least privilege.
- **Revocation is the hard part.** You can hand out a capability, but taking back the sender's copy, which you'd need to close the time-of-check-to-time-of-use gap, requires sweeping memory to find and invalidate it. CHERI research on temporal safety (the Cornucopia work) addresses this, at a cost. Until then, use the same rule as above: copy small messages, and transfer ownership for large ones.

## m14

**human** · 2026-10-04 07:09 UTC

I don't get that receiver side validation is required if ipc is shaped properly. shouldn't the receiver know exactly how large every buffer is, and the kernel guarantees that the buffer exists in memory? and issues about locking can be mitigated by being smart with your data structures, right? messages in a ring buffer? mark when each is completed, then receiver can read? also works for invalidating, simply mark invalid? atomic op (check invalid -> read message if not) probably hurts right? but generally what do you need reciever validation for? receiver knows about this interface from spec, knows what should be in each buffer in the spec, potentially some info on how to use it, os guarantees that all this shared memory exists. what needs to be checked at runtime? other than permissions. its probably okay for two modules to consider each other "friends", able to freely write to each other without permission checks [oops you mentioned this]? in fact this ties into process/code namespaces for capabilities. im probably missing something here but I don't see why shared memory doesn't fully work here. maybe one issue is that you would need different pages for each possible combination of permission namespaces. obviously only need what is currently running but still might grow rapidly. and cache means memory isn't a big issue. I guess the point still stands that you can pretty much fundamentally never do as well as ffi? I think the concept that code doesn't care about transport is a really really good way to streamline adoption and leverages the language independence of the spec really well. wit targetting wasm linear memory is probably fine right? even in homogenous setups you still probably abstract with flat memory right? capn proto actually sounds pretty incredible here. when you say messages are append only, what exactly do you mean? do you think wit types can be added on top of the existing protocol or do you think new dialect is required? and going back to ipc scratchpad, I don't mean totally public I do mean capability limited per process. so with cheri can just be a flat pool but with just paging need a page for each combination of permission namespaces right?

## m15

**assistant** · 2026-10-04 07:10 UTC

#### Why the receiver still validates

You're right about almost everything except one thing. The kernel guarantees the buffer exists and how large it is. It cannot guarantee that the bytes inside it match the spec, because the sender wrote those bytes. The spec says what should be there; validation checks what is there.

For a fixed-size struct with no pointers, almost nothing needs checking. Problems come from variable structure inside the message:
- **Offsets and lengths.** A list is stored as (offset, length). A malicious or buggy sender can write an offset pointing outside the message, or a length larger than the buffer. A receiver that trusts these reads memory it shouldn't, such as its own secrets elsewhere in its address space. The page boundary doesn't protect you, because the bad read lands in memory the receiver legitimately has mapped.
- **Variant tags** with values outside the enum, which cause type confusion.
- **Invalid UTF-8 in strings**, if the receiver's language assumes valid text.
- **Cycles or deep nesting** that make traversal blow up.

So validation cost scales with how much pointer structure the format has. Flat messages need very little. It's all generated code, a few compare-and-branch instructions per structural field.

Your "friends" point holds. Inside one trust domain, skip validation entirely. That's the direct-call transport case, and validation only happens at trust boundaries.

#### Ring flags and double fetch

Completion flags with release/acquire ordering are the standard single-producer/single-consumer ring. They're nearly free and they solve ordering for a cooperative sender. They don't stop a hostile sender from continuing to write after setting the flag. The receiver checks `len = 100` and then reads `len` again when using it, by which time the sender has changed it to 10⁹. This is a real bug class. Xen's paravirtual drivers had exactly this "double fetch" vulnerability on shared rings.

The fix is narrow and cheap: **read every structural field exactly once into a local, and use only the local copy.** Generated bindings can enforce this mechanically. Bulk payload, like image pixels, can be read in place. If the sender changes it mid-read, the receiver gets garbage values but no memory unsafety, because the offsets and lengths it relies on were already captured.

An "invalid" flag works for cooperative teardown. Against a hostile peer, revoking access means the kernel unmaps the pages.

So shared memory does fully work. The overhead beyond FFI is:
- structural validation and single-fetch reads, only at trust boundaries;
- wakeup, unless a core is polling;
- cache-line transfer, if the two sides run on different cores.

A function call shares the caller's core, stack, and registers, and IPC can't. So it never fully matches FFI on tiny calls. For batched or large messages it comes within a small constant factor.

#### Paging and permission combinations

You don't need a page set for every permission combination. Map one shared region per **channel**. Each ring lives in pages shared only by its endpoints, so the number of mappings grows linearly with connections, not combinatorially.

- **Small objects** get copied into the channel ring.
- **Large buffers** get their own pages, granted to the receiver on demand. Xen's grant tables work this way.

Paging can't give you one flat pool with per-object, byte-granular permissions. Granularity is 4 KB, every grant costs a page-table operation, and every mapping takes TLB entries. The cost is TLB and page-table churn, not RAM, so caches don't save you. That's exactly what CHERI adds. A flat pool with a bounded capability per object makes granting nearly as cheap as passing a pointer.

#### WIT and linear memory

Flat memory is fine; native processes already have a flat address space. The issue with the canonical ABI is its copy semantics, not flat memory. The component model is shared-nothing by design. When a list crosses a component boundary, the canonical ABI calls the callee's `realloc` and copies the data into the callee's own memory. That's deliberate and safe, but it's the opposite of zero-copy for large data.

So keep WIT's types and define a different lowering, where lists and strings are (offset, length) pairs into a shared message buffer that the receiver reads in place.

#### Cap'n Proto messages are append-only

A message is built in an arena of segments. Each new struct, list, or string is allocated at the end of the arena. If you replace a field's list with a new one, the old list's space isn't reclaimed; it stays in the message as unreachable garbage (Cap'n Proto calls these "orphans"). You also can't resize a list in place. Messages only grow.

That's fine for build-once, send, discard, which is what IPC messages are. It's bad if you use a message as your application's long-lived mutable data structure, so don't. Keep messages small and short-lived.

#### WIT types on Cap'n Proto

No new wire format is needed. You need a mapping layer plus two additions. Most of the mapping is direct:

| WIT | Cap'n Proto |
|---|---|
| record, tuple | struct |
| variant, result | struct with union |
| enum | enum |
| option | union, or a null pointer for pointer types |
| list, string | List, Text |
| flags | integer bitfield (no native type; easy convention) |
| stream, future | streaming calls and promises (both exist) |
| own/borrow resources | **capabilities, which need semantics added** |

The two real gaps:
1. **Field ordinals.** Cap'n Proto evolution depends on explicit field numbers (`@0`, `@1`, …). WIT has none and versions through package versions and `@since` gates. You need a stable rule: derive ordinals from declaration order plus `@since`, and require new fields to be appended. This is the main design decision.
2. **Ownership.** Cap'n Proto capabilities are reference-counted references with no own/borrow distinction. The wire format only carries an index into the message's capability table, so ownership lives in the RPC and kernel layer. `own` means the sender's table entry is deleted on transfer. `borrow` means the capability is revoked when the call returns.

The practical first step: write a **WIT → Cap'n Proto schema compiler** with the ordinal rule and an ownership annotation. You'd immediately get Cap'n Proto's existing codegen and runtimes in C++, Rust, and other languages, so you can test the whole idea on Linux in weeks rather than years. Their builder APIs are clunky. Replacing them with idiomatic per-language bindings generated directly from WIT is the step to take after the model is shown to work.

## m16

**human** · 2026-10-04 07:38 UTC

so what does this end up looking like, fully? we've outlined a few ways to do this, what choices need to be made and what different approaches fall out, zooming out? both assuming cheri and assuming paging only. universal constant seems to be wit into capn proto structs into shared memory channels.

## m17

**assistant** · 2026-10-04 07:38 UTC

#### What stays the same in every variant

The constant is a bit wider than "WIT into Cap'n Proto into shared-memory channels." **The spec and the wire layout are fixed. The transport is chosen per connection.**

1. **Spec:** WIT, extended with field ordinals, own/borrow semantics, declared failure modes, intent, conformance tests, and identity by hash.
2. **Layout:** Cap'n Proto encoding, generated from the WIT.
3. **Bindings:** generated per language. Application code calls the interface and never sees which transport is underneath.
4. **Transport,** picked when a connection is set up:
   - **Direct call** within a trust domain. No validation, no copying.
   - **Shared-memory channel** between trust domains on one machine. Structural validation and single-fetch reads.
   - **Network RPC** between machines. Cap'n Proto RPC already does this, including passing capabilities and pipelining promises.
5. **Kernel primitives:** capabilities with a derivation tree, memory regions, threads and scheduling contexts, endpoints and notifications, interrupts. Nothing more.
6. **Everything else is a component behind an interface:** object store, drivers, network stack, compatibility layer.

Going distributed comes almost free, with one condition. A 1994 paper from Sun, "A Note on Distributed Computing," showed that making remote calls look local fails because it hides partial failure. So a call may fail on any transport, and the interface must declare its failure modes. Code written to tolerate failure on the network transport works on all three.

#### Decisions, with the choice I'd make

- **Trust-domain granularity:** by default, each independently sourced component is its own domain. Components from the same verified build can be placed together. This one decision determines where you pay for validation and isolation.
- **Where ownership is enforced:** the kernel holds the capability tables. User space only ever holds indices into them, so a capability can't be forged regardless of what language a component is written in.
- **Call paths:** two. A synchronous fast path for control operations, in seL4 style, and rings for data.
- **Waiting:** adaptive. Poll while busy, sleep on a futex when idle, the way io_uring's SQPOLL mode times out.
- **Large buffers:** copy below a threshold, transfer ownership above it. The threshold gets tuned per platform.
- **Store references:** in the object store, references are capabilities. At runtime they're live references; on disk they're serialized "sturdy refs" (Cap'n Proto's term for persistent capabilities).
- **Evolution:** fields are append-only, ordinals come from declaration order plus `@since`, and the checker tests compatibility between interface hashes.

#### Paging-only variant

- **Isolation unit:** one address space per trust domain.
- **Channels:** one shared region per connection, mapped into both ends, holding rings. Small messages are copied into the ring. Large buffers get their own pages, granted by mapping them in, and transferred by unmapping them from the sender.
- **Calls across domains:** an address-space switch, which is cheap with ASIDs or PCID, or messages handed between cores over rings.
- **Validation:** generated structural checks and single-fetch reads at every boundary between domains.
- **Revocation:** unmap the pages. The real cost is the TLB shootdown, which means interrupting other cores to flush their cached mappings. This is the main pain point of this variant.
- **Status:** buildable today on Linux or seL4, on any hardware.

#### CHERI variant

- **Isolation unit:** a compartment inside a single address space.
- **Channels:** one flat pool. Each message gets its own bounded capability, and the receiver can be given a read-only version derived from it. Granting access is passing a pointer.
- **Calls across domains:** a switch through sealed entry capabilities, costing about as much as a function call.
- **Validation:** partly done by hardware. A malicious offset that points past a message's bounds traps instead of leaking data. Variant tags and UTF-8 still need software checks.
- **Revocation:** the hard part. Freed pool slots stay quarantined until a sweep invalidates every outstanding capability to them, which is the Cornucopia approach.
- **Status:** RISC-V standardization is underway. No consumer CPU is announced.

#### Comparison

| | Paging | CHERI |
|---|---|---|
| Isolation unit | address space | compartment |
| Sharing granularity | 4 KB pages | bytes |
| Cost to grant | page-table op | pass a pointer |
| Cost to revoke | unmap + TLB shootdown | quarantine + sweep |
| Bad offsets | caught by software checks | trapped by hardware |
| Cross-domain call | address-space switch | ~function call |
| Available | now | years out on consumer hardware |

There's also a middle option available now: software fault isolation, using Wasm or a verified compiler. It gives in-address-space compartments on ordinary hardware, at some runtime cost, and behaves like CHERI with checks done in software.

#### The likely end state

Real systems will mix these. CHERI or software isolation for fine-grained compartments inside a process. Paging between coarse domains such as users, VMs, and untrusted services. Network transport between machines.

**Because the spec, layout, and bindings never change, the hardware choice only affects the transport implementation and the kernel.** Choosing between CHERI and paging changes those two parts and nothing in applications. So design for paging, which runs on everything, and treat CHERI and software isolation as faster transports wherever they're available.

The prototype path is unchanged: a WIT → Cap'n Proto compiler plus paging-variant channels on Linux, built from memfd rings and file descriptors passed over Unix sockets as capabilities.

## m18

**human** · 2026-10-04 07:51 UTC

one bug with the ipc would be fields indexing something internal to the receiver. could still go out of bounds that way. need bounds check on anything at the interface. what existing os would you say this vision is closest to? can you elaborate on the sun paper? this vision for rpc is so alluring, fs/io drivers fall out so simply and it works well with the object store concept. and that's all before the fact that it maps to network and web extremely well.

## m19

**assistant** · 2026-10-04 07:51 UTC

#### Indices into receiver state

That's correct, and it's a separate layer from wire validation. Structural checks only prove a message is well-formed: every offset stays inside the message and every tag is a valid variant. A field the receiver uses to index its own state is semantic, so only the receiver can check it.

There are three ways to handle it, and the first matters most:

- **Don't expose raw indices across interfaces.** If a caller needs to refer to something the receiver owns, give it a capability or handle instead of an integer. The kernel validates capability references, so a forged or stale one fails before the receiver's code ever runs. This is the core object-capability rule: no integer names that carry authority across a boundary. It also prevents confused-deputy bugs.
- **Put static bounds in the spec.** Refinement types like `u8 where < 16` turn into generated checks.
- **Check bounds that depend on runtime state in the receiver.** For something like "index < current table length," a memory-safe receiver language makes a miss a clean error instead of a leak.

#### Closest existing OS

**Fuchsia** is the closest match overall:
- Zircon kernel handles act as capabilities.
- FIDL is a typed interface language whose messages can carry handles.
- There is no global filesystem, only per-component namespaces routed by a component manager.
- Drivers are components, and bulk data moves through shared memory objects (VMOs).

It's roughly the paging variant. The main difference is that Zircon channels copy message bytes through the kernel instead of using shared rings. What it lacks from your vision: an object store as the universal data model (it still uses files), content-addressed identity, and intent and conformance tests in the spec. Google has scaled back its ambitions for Fuchsia, but the design docs are still the best reference for this architecture.

Two others are worth studying:

- **Genode** is a capability-based component framework that can run on seL4. Its Sculpt OS is a usable desktop. It's the closest in spirit to user sovereignty, because the user explicitly routes every capability between components.
- **Plan 9** has the most direct version of what you find appealing. Every resource is a server speaking one protocol, 9P: drivers, the window system, the network stack, even other machines. Each process mounts what it needs into its own namespace, and it all works the same over the network. Its limit is the one you identified early on: 9P is untyped byte streams on files, so meaning hides in ad hoc text formats.

**Your design is roughly Plan 9 with typed interfaces and capabilities, using Fuchsia's architecture.**

#### The Sun paper

"A Note on Distributed Computing" (Waldo, Wyant, Wollrath, Kendall; Sun Labs, 1994) argued against the goal of the era's distributed object systems, CORBA and DCOM, which was to make remote objects look exactly like local ones. It names four differences between local and remote calls:

1. **Latency.** Remote calls are orders of magnitude slower. This is the only difference engineering can mostly hide.
2. **Memory access.** A pointer into another address space means nothing to the receiver, so pass-by-reference semantics break.
3. **Partial failure.** This is the central one. Locally, a program either works or crashes as a whole. Remotely, a call can fail midway and you can't tell whether it ran, because the network, the peer, or the reply could each have failed independently. No single party can observe what actually happened.
4. **Concurrency.** Remote objects have truly concurrent callers, which the interface itself has to address.

The conclusion: these differences have to be visible in the interface. Design for distribution, and treat local calls as the special case, not the other way around. Wollrath also designed Java RMI, which applied this directly: every remote method must declare `RemoteException`.

#### How this design handles each one

This is why the RPC vision holds up. The E language's CapTP protocol, which Cap'n Proto descends from, was built with these lessons in mind:

- **Memory access:** nothing crosses a boundary except values and capabilities. Cap'n Proto messages are position-independent, so no pointer is ever valid on the other side.
- **Latency:** promise pipelining batches chains of dependent calls into one round trip, and calls are asynchronous by default.
- **Partial failure:** every call returns a result that can be an error, and a capability can become "broken" when its peer disconnects. The spec should also say, for each operation, whether it's **idempotent** (safe to retry) or **at-most-once**. Your spec format would need that addition.
- **Concurrency:** use E's model. Each object runs on a single-threaded event loop, and nothing shares mutable state across a boundary.

A microkernel with isolated components also has partial failure locally. A crashed driver is exactly the paper's failure case, just on one machine. So the uniform model isn't pretending remote calls are local, which is the mistake the paper warned about. It treats local calls with the same honesty about failure that remote calls require, which is what the paper recommended. That's why one interface across a direct call, a shared-memory channel, and the network is sound rather than just appealing.

Drivers fit this model with one caveat. They also hold interrupt and DMA capabilities. High-throughput drivers like NVMe and network cards feed the hardware's own queues directly, and the RPC interface only sits on top of them.

## m20

**human** · 2026-10-04 08:13 UTC

I think the network aspect as first class is a good idea as long as it doesn't get in the way of things too much. even if it can't be fully unified with local ipc it's probably going to still end up being heavily related. but I think it is a good idea. I was reading about plan 9 and was reading the normal 80s/ 90s spiel about networked workstations accessing more powerful CPU servers, and in a sense, after seeming a bit silly for a while (our systems are soooo powerful!) it starts to ring  true again in the ai age. fuchaia seems interesting too, I don't see many specific details, but it seems like it might still be a thing? have they given up on it? doesn't seem like it necessarily. anyways it's interesting since it sounds like it's tackling the same issues we identified and it's google so I have an inherent distrust. you know one of the other things i just started thinking about is the relation between live capability handles held in memory and the concept of having capabilities in your object store. I guess it just ends up being a way to share capabilities between processes, and it just ends up being a data type in the object store alongside your other more normal primitives? I'm not really sure, but the relation with the object store seems like the final thread to spec out to make sure this is the right approach.

## m21

**assistant** · 2026-10-04 08:14 UTC

`[tool: WebSearch]` {"query": "Fuchsia OS Google 2026", "mode": "standard"}

#### Plan 9's CPU servers

The detail worth taking from Plan 9 is how its `cpu` command worked. It ran your shell on the powerful remote machine, but imported your terminal's namespace into it. Programs running remotely saw your local files and devices as if they were local. The computation moved to the server, but the authority stayed with you.

Today's cloud does the opposite: you upload your data into the provider's namespace.

In this design, Plan 9's direction is the natural one. A remote inference server, or an agent running in the cloud, gets exactly the capabilities you hand it into your object store, and nothing else. That's how you use remote AI compute while keeping control of your data.

#### Fuchsia

Fuchsia is still released but has been deprioritized. One analysis from earlier this year lists F30 as the latest release, in April 2026. The same piece reports that Google has dropped plans to bring Fuchsia to Assistant smart speakers and has stopped trying to bring full Chrome to it. Google's new desktop effort, Aluminium OS, is Android-based. So treat Fuchsia as a reference design whose docs are worth reading, not as a platform to build on. Your distrust doesn't change much, since the useful part is the design decisions, and those are documented in the open.

#### Live capabilities vs. stored capabilities

This is the right thread to pull. Your instinct is close: a capability becomes a primitive type in the object store, alongside the other primitives. But it's a primitive that needs special handling, for these reasons.

**Two forms of the same reference:**
- **Live reference:** an entry in the kernel's capability table, which a process sees only as a local index. It's valid only while the process exists and the capability hasn't been revoked. The index means nothing anywhere else, so it can never be written to storage directly.
- **Persistent reference:** Cap'n Proto calls this a "sturdy ref," a term inherited from the E language. It's a serializable, unguessable token. Presenting it to the service that hosts the object restores a live reference. Possessing the token is the authority, so the token bytes have to be treated as secrets.

**The rule that ties these together:** processes never see token bytes. In the store, a capability field works like a capability in a Cap'n Proto message: the value holds a typed slot, and the token material lives in a side table that only the store can access. When a process with read access reads a value containing a capability field, the store restores it and hands back a live reference. When a process writes a live reference into a value, the store turns it into a sturdy ref. So a stored capability is typed by its interface hash like everything else, but it can't be read as bytes, copied out, or leaked.

**Hashes are names, not authority.** Knowing a hash shouldn't let you read the object. Low-entropy data can be guessed from its hash, so treating a hash as a credential leaks information. Instead, authority follows reachability: if you hold a capability to an object, you can read everything it references, all the way down the DAG. Git works this way: a tree you can read gives you its blobs. This is the core object-capability principle, "only connectivity begets connectivity," applied to storage. Writes always go through capabilities to mutable refs.

#### What this gives you

**Your screenshot example resolves cleanly.** Sharing a *snapshot* means sharing an immutable value that contains no capabilities. Sharing *live access* means sharing a value that contains a capability to a mutable ref, read-only or read-write. That's the only difference between the two, and the system can show it to the user. Sharing an object always means sharing everything reachable from it, and the UI should say exactly what that is before you send.

**Granting access to agents gets a trusted UI.** The pattern is called a powerbox. An app or agent can't reach your files directly. It asks the powerbox, a trusted system dialog, and the user picks what to grant. The agent receives a capability to exactly that selection. Selecting the item and granting authority become the same action, so consent is built into the operation rather than shown as a separate permission prompt.

#### Lifecycle decisions

- **Revocation:** the hosting service can delete a token's mapping. Delegated capabilities go through caretakers, so they can be cut off without affecting the original.
- **Garbage collection:** don't attempt distributed garbage collection of capabilities. It's a known hard problem, and Cap'n Proto doesn't do it. Use explicit expiry or leases on stored capabilities, plus revocation.
- **Root authority:** at login, the user's shell receives a capability to the user's root ref. Every other capability derives from that root or from the powerbox.
- **Where persistence lives:** in a userspace store component, not the kernel. The other approach is orthogonal persistence, as in KeyKOS and EROS, where the kernel periodically checkpoints the whole system, capabilities included, so nothing is ever serialized. It's elegant, but it's all-or-nothing. It handles external network peers and hardware poorly, it makes upgrading code hard because code state gets persisted along with everything else, and it can't be prototyped on Linux. A userspace store keeps the kernel small, and the store becomes just another component with an interface.

#### Prior art to study first

**Sandstorm** is the closest existing system to this layer of your design. Kenton Varda built it on Cap'n Proto. It's a self-hostable server where every document is an isolated "grain," and grains share access through sturdy refs and a powerbox. It's a web-delivered version of the object store with capabilities: same author, same wire format, same model. The company behind it shut down and the project is now community-maintained, but its design docs and code show how persistent capabilities, powerbox grants, and sharing behave when real users touch them.

## m22

**human** · 2026-10-08 03:10 UTC

https://github.com/Flow-IPC/ipc the ecosystem wants this to be a thing *so badly*. that's probably the most surprising thing to me reading about this. everyone loves these concepts but they all roll their own thing, uncertainty about making it a fully integrated thing , and obviously nobody cares about desktop. I am questioning wit. it doesn't have bindings right? capnp does. what does it get us that adding ownership flag to capnp doesn't? then fork of capnp to change bindings to implement ownership? would only really have to change rust anyways. I do think an underrated thing here is security. capnp talks about amplification attacks, thoughts about bad ownership info, you can't take much on faith. that makes things harder. does cheri help here? can the os make guarantees that a capability is not freed when borrowed by another process (am I understanding things right here?)?

## m23

**assistant** · 2026-10-08 03:11 UTC

`[tool: WebFetch]` {"url": "https://github.com/Flow-IPC/ipc", "prompt": "What is Flow-IPC? Who made it, what does it do (zero-copy, Cap'n Proto integration, shared memory allocat…

#### Flow-IPC

Flow-IPC tells you two useful things.

**Where the latency floor is.** On their benchmark, zero-copy keeps round-trip latency at about 100 µs whatever the payload size. Classic socket IPC takes about 1 ms for a 1 MB payload and approaches a full second at 1 GB. For very small messages, though, both approaches come in at about 100 µs. So zero-copy fixes how cost grows with size, but the fixed cost per message comes from signaling and channel machinery built on Linux primitives, not from copying. seL4-class kernel IPC runs orders of magnitude below that. A Linux prototype will prove out the architecture, but it won't show final performance. A kernel-level fast path still matters.

**Why everyone rolls their own.** The README's list of pain points is essentially a spec for the missing OS primitive: naming every socket and SHM segment, coordinating creation and cleanup, connect/accept handshakes, and setting permissions on each OS object. Flow-IPC's security model is presets by user or group ID. Processes share an arena, which means it's a transport between friends that isn't designed for hostile peers. That's typical of these libraries: performance gets solved, the trust boundary doesn't.

On desktop, Linux tried twice to add kernel IPC. kdbus was rejected, and bus1, a capability-based follow-up, never merged. That's why the desktop is still on D-Bus. systemd has since been moving many of its interfaces to varlink, a simple typed IPC protocol with interface definitions. It's worth looking at as evidence of what the desktop side will actually adopt.

#### WIT vs. Cap'n Proto

I'd reverse my earlier recommendation: start from Cap'n Proto. For native use, WIT has a type system and a parser, but no transport, no RPC, and no native bindings. Cap'n Proto has all of those and runs in production.

What WIT has that Cap'n Proto lacks:
- **Ownership semantics** (own/borrow). You'd add these anyway.
- **More convenient types** (option, result, flags). Cap'n Proto expresses all of them as unions; only the ergonomics differ.
- **"Worlds":** a declaration of everything a component imports and exports, which amounts to a declaration of exactly the authority it needs. This is the one idea worth taking. In Cap'n Proto you can model it as a bootstrap interface that receives its capabilities as parameters.

**You don't need to fork.** Cap'n Proto supports custom annotations, so ownership can be `$own` and `$borrow` annotations on interface-typed fields and parameters, with no change to the schema language. Code generators are compiler plugins, and capnproto-rust's generator is one of them. So the work is:

1. Define the annotations.
2. Extend or write the Rust codegen plugin so ownership becomes Rust move semantics and borrow lifetimes.
3. Enforce ownership in the transport.

Your instinct that only Rust needs to change is right, for this reason: **static ownership checking is a per-language enhancement, while dynamic enforcement is universal.** Languages whose generators ignore the annotations still interoperate correctly, because the transport enforces ownership whatever the code claims. They just catch mistakes at runtime instead of at compile time.

One thing to verify before committing: C++ is Cap'n Proto's reference implementation. The Rust RPC implementation is solid, but check which RPC features it covers.

#### Security: don't trust anything in the message

**Ownership claims in messages are never trusted.** A message only carries indices into a capability table. The tables themselves live in the broker or kernel, so the transfer is carried out there:
- **own:** A asks to send index 3 as owned. The broker checks that A actually holds index 3 with transfer rights, removes it from A's table, and inserts it into B's.
- **borrow:** the broker inserts a temporary entry into B's table and removes it when the call completes.

A sender that lies about ownership can only misuse capabilities it already holds. It can't forge new ones or keep one it gave away.

**Amplification and denial of service.** Cap'n Proto's mitigations are a traversal limit (total data a reader will follow) and a nesting-depth limit. The rest of the attack surface sits around messages, not inside them:

| Attack | Mitigation |
|---|---|
| Message flooding | Bounded rings: a full ring is natural backpressure |
| Capability table exhaustion | Per-domain table quotas |
| Unbounded pipelined calls queued on unresolved promises | Cap on queued calls per promise |
| Callee stalls forever while holding a borrow | Caller cancellation (ends the borrow) and timeouts |
| Memory and CPU exhaustion | The budget capabilities from earlier (scheduling contexts, memory quotas) |

#### Can the OS guarantee a borrowed capability isn't freed?

There are two separate questions here.

**The object behind a capability:** yes. Kernel objects are reference-counted through capability table entries, and a borrowed entry holds a reference, so the object can't be destroyed underneath the borrower. The real policy question is whether the lender can revoke during a borrow. It has to be able to, or a hostile callee could hold a borrow forever. The clean rule: the lender cancels the call, which ends the borrow, and the borrower sees a broken-capability error.

**Memory behind a buffer:** it depends on the isolation mechanism.
- **Paging:** safe. Pages stay alive while anyone has them mapped; a memfd survives until its last fd or mapping is gone. If the owner unmaps, the borrower keeps the old pages. The remaining risk is the owner *reusing* the buffer while the borrower is still reading it. That's a data race that produces garbage values, not a memory-safety violation.
- **CHERI, single address space:** this is where it gets dangerous. If the owner frees memory and the allocator hands it to a different component while the borrower still holds a capability to it, the borrower reads someone else's data. Preventing that is CHERI's temporal-safety problem. Freed memory is quarantined until a revocation sweep invalidates every outstanding capability to it, and only then reused. CheriBSD implements this approach.

#### Where CHERI helps and where it doesn't

**It helps with:**
- **Bounds.** A bad offset inside a buffer traps instead of leaking data.
- **Unforgeability.** A component can't fabricate access to memory it was never given.
- **Sealed capabilities.** This solves the bug you raised last time about fields indexing into receiver internals. Instead of handing out an integer index, the receiver gives out a sealed capability that points directly at its internal object. The holder can't dereference it or forge one; it can only hand it back. When it comes back, the receiver unseals it with its own key and has a valid pointer, with no table lookup and no bounds check.

**It doesn't help with:**
- **Temporal safety**, unless you also run quarantine and revocation sweeps.
- **Semantic validation:** valid variant tags, UTF-8, contract-level checks.
- **Denial of service and amplification.**
- **Double fetch.** The sender can pass a read-only capability, but it keeps its own writable one.

The most valuable thing CHERI adds to this design is sealed handles. They turn "integer handles into receiver internals," a whole class of bugs, into something the hardware rules out.

## m24

**human** · 2026-10-08 04:50 UTC

even flow latency floor isn't actually the floor right? that's still with paging.

why did kernel busses fail? I guess another question here is if kernel support for the ipc actually matters. you seem well off enough in linux if you just map some shared memory and work on your own with another process. and what would the kernel actually do here besides map shared memory? I guess that's the question. much of the functionality that happens at this interface doesn't actually depend too much on the kernel, outside of the actual capabilities that might go through it. that makes capnp more of a userspace convention, except for the fact that you need ownership annotations on capabilities for it? also what is ipc actually looking like at this point? does capnp read structure out to prevent double fetch bugs? would that need to be added on? sep pages/cheri regions for each direction? how do your message rings work? how are messages streamed? would you want processes to be aware of their scheduling so that they could finialize their messages before being preempted? seems like that might be useful if this is your primary way of moving things around, and it helps to define how long your messages might want to be. im not sure. you mention scheduling contexts. in the same vein as limits you also need to handle length of message buffer and question of what to do on full. also capnp says validating a message (ie through preventing your double fetch bug) you somewhat defeat the purpose of capnp. could this be mitigated?

so ffi is just fully out right? dynamic libraries are a language level thing. that sounds sensible. but I can also feel that there is *going to be* some kind of edge case that requires it. surely somebody would be cursing your name if you made that decision. but I also just can't see how to make it work nicely! you are forced to define a calling convention and and all the mess that we are trying to mitigate with good ipc and you're going to end up right back at the feet of the mess that is "the c abi". and you need it for syscalls anyways! i guess you could do it over a special kernel ipc page that each process could get. still, you need to potentially set that channel up. i guess you could put all this stuff at a fixed address or something? all of this also raises question of what starting env of process looks like. and how are they started in general. do they have an environment or just the object store? do they have args? seems like all that can just go into object store. also maybe for syscalls you could also just do the stack. every arch has a stack. IDK.

I definitely need clarification on what capabilities might actually look like. what can they reference? I think having a good model of that might look like would let me have a more complete picture of this design. and we do have several different types of capabilities as you mentioned: cheri, capnp, object store. how much do these intersect? how do they interact?

also sorry i'm doing this over a couple of days, im reading, researching, thinking, typing, rereading. my point is i might forget some stuff and you might have to restate. also we have said a lot and I don't always know the exact words to search for lol. and thats on top of the fact that we are hitting the limits of my knowledge as a highly passionate undergrad :)

oh and whats the deal with cheri sweeps? im not convinced it's actually an issue in the optimal case here. its kind of a language thing right? I mean that's already a bug and it's your fault for not using rust. and for ipc you can just guarantee that you don't do that. right? or am I missing something. if it is necessary, is this going to be a long term issue with cheri? any issues for the purposes we have outlined?

and common theme here seems to be double fetches. nice thing about paging is you can just switch the page over to the rx process when it's fully written and that process now owns it. could emulate that in cheri but limits flexibility with the shared memory. idk.

## m25

**assistant** · 2026-10-08 04:51 UTC

Spreading this over several days works fine. Where a point depends on something from earlier, I'll restate it.

#### The latency floor

Right, Flow-IPC's ~100 µs is the overhead of its own machinery, not a limit of paging. Rough floors on ordinary paging hardware:
- **Cross-core polling:** a message costs about one cache-line transfer, roughly 100–200 ns.
- **seL4-style synchronous IPC:** hundreds of cycles, including the address-space switch.
- **Linux futex wakeup plus context switch:** low single-digit microseconds.

So paging alone reaches sub-microsecond latency. CHERI's gain is making cross-compartment calls cost about as much as a function call, and making it cheap to grant fine-grained access.

#### Why kernel buses failed, and what the kernel actually does

**kdbus** (about 2013–2016) put D-Bus semantics into the kernel. Reviewers objected that it baked one bus's policy into the kernel, that the performance claims didn't hold up, and that the way it captured sender credentials at send time raised security concerns. Its authors then wrote **dbus-broker**, a userspace reimplementation that delivered most of the performance gains and removed the motivation. **bus1**, a simpler capability-based follow-up, never built momentum. The lesson: the kernel should provide mechanism, not bus policy.

You're right that most of IPC doesn't need the kernel. Wire format, rings, RPC, validation, and pipelining are all userspace conventions. Cap'n Proto is a userspace convention. The kernel does four things nothing else can:

1. **Authority.** It keeps capability tables that can't be forged. On Linux these are file descriptors, passed between processes with SCM_RIGHTS.
2. **Memory.** It maps, unmaps, and transfers pages.
3. **Wakeups.** Futexes.
4. **Scheduling handoff.** On a synchronous call, the caller can donate its timeslice so the callee runs immediately on the caller's budget. seL4 does this. Linux can't: Google's FUTEX_SWAP and UMCG proposals targeted it and weren't merged.

**A Linux gotcha for the prototype:** file descriptors have copy semantics and can't be revoked. A sender can `dup` an fd before "transferring" it, and Linux has no revoke for fds. So on Linux, `own` can't be enforced with raw fds. Capabilities have to be references to objects held by a broker process, with the broker carrying out moves and revocations. On seL4 or a custom kernel, the kernel does this.

#### What IPC actually looks like

**Channel layout:** each direction is a separate single-producer/single-consumer ring.
- **Data pages:** the writer maps them read-write and the reader read-only.
- **Consumer index:** on a separate cache line or page that the reader can write and the writer can only read.
- **Slots:** a fixed-size header (length, sequence number, count of capability-table indices) followed by an inline body for small messages, or a reference to separate buffer pages for large ones.
- **Publish:** the writer fills the slot, then does a release-store of the producer index. The reader does an acquire-load. A message is either fully published or not visible at all.

**When the ring is full:** apply backpressure. Never drop silently and never grow without bound. A synchronous writer blocks, with a timeout. An asynchronous writer gets a would-block result and a notification when space frees up. Unbounded queues on the receiving side are a memory-exhaustion attack.

**Message size and streaming:** a single message is bounded by the ring's slot and segment size. Large payloads travel as separate buffers transferred by ownership. Unbounded data goes as a stream of chunks with credit-based flow control. Cap'n Proto's streaming calls already work this way.

**Double fetch and Cap'n Proto:** the Cap'n Proto reader doesn't copy anything. Each getter bounds-checks the pointer it follows against the segment, so each access is checked individually. What it doesn't prevent:
- application-level double reads, like checking a length and then reading it again;
- data changing between two accesses.

Whether the C++ and Rust internals are robust to concurrent malicious mutation is something to verify in the code, not assume. The better approach is to make sure there's no concurrent writer at all.

**Mitigating "validation defeats the purpose":** the expense is eager, O(size) validation or copying of large structured messages from untrusted peers. Three cases cover nearly everything:
- **Small messages:** copy into the ring. It's cheap.
- **Large messages:** transfer the pages so the sender loses write access. Then Cap'n Proto's existing lazy per-access checks are enough, with nothing copied.
- **Opaque blobs** such as pixels or file contents: only their bounds need checking.

**Page flip on Linux today:** use memfd sealing. The sender writes into a memfd, drops its writable mappings, applies `F_SEAL_WRITE`, and sends the fd. The receiver checks the seals and then knows the contents can't change. memfd sealing was added in 2014 specifically so kdbus and Wayland could share buffers safely with untrusted peers. Seals are one-way, so each buffer is single-use. Reserve this for large messages and copy small ones. The crossover point needs measuring.

#### Scheduling awareness

Preemption in the middle of a message isn't a correctness problem. Publishing is atomic, and an SPSC ring has no locks, so there's no priority inversion. A preempted writer just delays its own message. Two things do matter:
- **Timeslice donation** on synchronous calls (above). It removes queueing delay and charges the callee's work to the caller, which is how budgets stay meaningful.
- **Restartable sequences (rseq)** on Linux give userspace preemption awareness: a process learns it was preempted or migrated. This is the existing mechanism for what you're describing. It's mainly useful for per-CPU data structures, not for sizing messages.

Message size limits should come from ring size and latency targets, not timeslice length.

#### FFI, syscalls, and process startup

FFI isn't fully out. Three separate things got merged together:

1. **Cross-language calls inside one process** (the direct-call transport). These need a calling convention. Use the platform C ABI underneath, but have it generated from the spec so nobody writes headers by hand. The C ABI's mess comes from hand-written, drifting headers, not from the register conventions themselves. It survives the way assembly does: as an implementation detail people don't touch.
2. **Loading code into your address space** (dynamic libraries, plugins, codecs). This stays, described by the spec and loaded by the language runtime. Whoever needs an in-process plugin gets one.
3. **Syscalls.** Every architecture already has a register-based syscall convention, and it's fine when the syscall set is tiny. seL4 has about a dozen kernel operations: send, receive, call, reply on a capability, plus memory and thread management. Everything else is IPC on capabilities. Registers beat the stack here: faster, and nothing for the kernel to validate in user memory.

Your fixed-address idea already exists. Linux maps the **vDSO**, a page of kernel-provided code and data, into every process, which is why `clock_gettime` doesn't need a syscall.

**Process startup:**
- **No environment, no argv strings, no global filesystem, no ambient user ID.** A process starts with an initial capability table: its own address space and thread, a scheduling context, a memory quota, and one **bootstrap capability**.
- The bootstrap capability is a typed interface matching the program's declared imports (WIT's "world" idea). Arguments arrive as a typed value in the bootstrap message, not as strings. Configuration and data are object-store references.
- **No fork/exec.** A launcher holds a process-factory capability and calls `spawn(code, initial_caps)`. The code is identified by its content hash in the store, so what runs can be verified.
- **Prior art:** CloudABI, the capability-based POSIX subset that WASI started from. Its programs began with only the descriptors they were given, and arguments came as a typed tree ("argdata") instead of strings. Fuchsia's process arguments plus component namespaces are the production version.

#### What capabilities are

There's one abstract model: **an unforgeable, transferable, attenuable reference to an object, typed by an interface, carrying a set of rights.** The four kinds you've seen are that model at four layers:

| Layer | References | Representation | Who protects it |
|---|---|---|---|
| CHERI | memory ranges; sealed opaque tokens | tagged pointer in a register or memory | hardware tags |
| Kernel | memory regions, threads, address spaces, endpoints, IRQs, MMIO ranges, scheduling contexts, process factories | index into a per-process table | kernel |
| RPC (Cap'n Proto) | objects hosted by some process: files, windows, sockets, devices, store objects | index into a per-connection import/export table, carried over a kernel endpoint (local) or a connection (remote) | the hosting peer, plus the layer below |
| Store (sturdy ref) | the same objects, persistently | token in the store's side table | the store |

**How they connect:** a sturdy ref is restored into an RPC capability. The RPC capability travels over a kernel endpoint capability. Memory gets mapped as pages, or turned into CHERI capabilities inside a compartment. Each layer relies on the one below it for unforgeability.

The unifying rule: **every capability is either an index into a table you can't write, or a tagged pointer you can't forge.** In application code, all of these appear as ordinary typed language objects. The RPC library holds the kernel capabilities privately.

#### CHERI sweeps

Your argument holds *inside* a compartment. Rust prevents use-after-free there, so a compartment's own memory needs no sweeps, just as on a paging system.

The sweep problem is about **trust boundaries**, not bugs. If A lends untrusted B a capability to A's memory, then frees and reuses that memory, B still holds a valid capability. B isn't buggy; it may be keeping the capability on purpose. A's Rust type system says nothing about B. So this is a revocation problem, not use-after-free.

**For IPC, you avoid it by design:** memory shared with B only ever holds data meant for B. Use a buffer pool per channel and reuse buffers only within that same channel. B already had access to everything in that pool, so reuse leaks nothing, and no sweep is needed. Sweeps only matter for general-purpose heaps shared across trust domains, and for temporal safety in C and C++ code. CHERI research has been cutting their cost, but for this design they're not a blocker.

**Page flipping vs. CHERI:** CHERI doesn't replace the MMU. CHERI systems still have page tables. So use page transfer for large messages from untrusted peers, and CHERI capabilities for fine-grained sharing among trusted components. That way you keep the flexibility of shared memory and still get guaranteed transfer where it matters.

---

There's a lot of design now spread across this conversation. A good next step would be collecting it into one design document: the decisions, the open questions, and a glossary of search terms (seL4 MCS, memfd sealing, sturdy refs, powerbox, CloudABI argdata, and so on) so you can research without having to reconstruct the vocabulary.

## m26

**human** · 2026-10-08 06:08 UTC

Create a Claude Doc from your last answer.

## m27

**assistant** · 2026-10-08 06:10 UTC

`[tool: mcp__Claude_Docs__guide]` {"items": ["topic.instructions"]}

`[tool: mcp__Claude_Docs__batch]` {"container": {"kind": "project", "create": {"name": "Capability OS: IPC, Kernel, and Capabilities Design Notes", "doc": {"blocks": {"asof": {"type": "date", "…

Outline is up; filling the latency section first.

`[tool: Artifact]` {"action": "open", "url": "https://claude.ai/code/artifact/7de99c1f-386c-4d93-aad5-1ae44e512d19"}

`[tool: mcp__Claude_Docs__guide]` {"items": ["topic.index"]}

`[tool: mcp__Claude_Docs__guide]` {"items": ["topic.diagram"]}

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

Now the kernel section.

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

Now the IPC mechanics section.

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

`[tool: mcp__Claude_Docs__update]` {"ref": {"object": "node", "id": "f878e0d8-f3e5"}, "engine": "prose", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload"…

`[tool: mcp__Claude_Docs__batch]` {"container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "batch": [{"$lid": "w", "verb": "create", "object": "node", "engine": "widget"…

`[tool: mcp__Claude_Docs__read]` {"container": {"id": "7de99c1f-386c-4d93-aad5-1ae44e512d19", "kind": "project"}, "engine": "widget", "payload": "{\"kind\": \"screenshot\"}", "ref": {"id": "51…

`[tool: ToolSearch]` {"query": "select:mcp__Claude_Docs__create", "max_results": 1}

`[tool: mcp__Claude_Docs__create]` {"object": "utterance", "container": {"kind": "project", "id": "7de99c1f-386c-4d93-aad5-1ae44e512d19"}, "payload": "{\"value\": {\"body\": \"Should the Linux p…

[Capability OS: IPC, Kernel, and Capabilities Design Notes](https://claude.ai/code/artifact/7de99c1f-386c-4d93-aad5-1ae44e512d19) is ready. You can edit it directly or leave comments for me to revise.

## m28

**human** · 2026-10-08 06:16 UTC

so do double fetches only matter at the application level since capnp already bounds checks everything on every read?

I was thinking that scheduling awareness might be nice just to reduce latency. it's not a correctness thing, but being able to get your full message out each slice might be useful in some circumstances. it's not important but i just thought i might mention it.

for calling convention, inside one process is a language detail as far as i see it, so i don't care. loading code is where i should have phrased better. it *can* be a language feature for compat but part of the os is that you can use the nice fancy ipc and declared interfaces. im not sure how the equilibrium falls but like you said this probably results in something like a math library being linked by the language. maybe at compile time, maybe at load time. at load time, its the job of the loader (which is part of the language) to make sure it gets the right binaries for the platform and for compat with the compiled structure of the executable being linked. this is a language problem. i guess the thing to take away here is that we have avoided the abi issue and now it falls onto the languages to make sure the binaries they load are all compatible with the platform and each other. hopefully they sort out the c abi mess themselves.

i forgot syscalls were all register only. what syscalls do you think you end up with? it does seem like you might want to do something similar to the normal ipc here where you get a syscall to set up a channel with the kernel and give you an addr to it and just do your capability/object store things through that.

i don't fully get the idea of a sturdy ref. can you give an example of what the object store might look like in normal use? basic example, example with args

you think capability to raw memory between processes outside of ipc is necessary? i guess it is. tons of things mmap stuff between processes and doing it with just ipc would require copying if you are doing anything beyond unidirectional. graphics stuff loves passing buffers all over the place between processes. you lose the structure here. i think there is no way of getting away from that? maybe you can still have it at the semantic layer. "this executable opens shared memory and uses proto ${hash} -> proto ${hash} is a image buffer with xyz resolution and abc encoding...". you lose the hard schema that you have with message ipc but regain flexibility, and you can keep the semantic definition. I still see this as being a bit of an application thing. I guess i should be looking at it like a dos thing that the os needs to guarantee? I want to say "just don't use an invalid capability" but like you say its about trust boundaries.

and final question for tonight is how do you get started on this. cutting out wit helps a lot. it seems like linux implementation would be too detached from a real version. i see that qemu has riscv cheri support. i dont know about arm? if you were going straight in, where would you start? need a tiny bit of io first, serial console. can do via sbi on riscv right? not sure about arm. then from serial console can do a lot. probably want log, then start working on process isolation, mmap, then ipc, then actually starting processes, then object store? object store requires a good binary representation on disk which seems like one of the easiest things to screw up very badly. although there is a bunch of prior work there, so.

## m29

**assistant** · 2026-10-08 06:17 UTC

#### Double fetch with Cap'n Proto

Mostly yes, with one important exception. Each accessor reads a pointer word, checks it, and uses that same value, so every individual access is safe. What remains at the application level:
- **Re-reading a value**, like checking a length and then reading it again.
- **Inconsistent views.** Two fields read at different moments may not agree with each other, for example a total that no longer equals the sum of its parts.

The exception is Rust. Under Rust's memory model, reading memory through ordinary references (`&[u8]`, `&str`) while another process writes to it is undefined behavior, regardless of bounds checks. capnp-rust hands out exactly those references; its text accessor validates UTF-8 and returns a `&str`, for example. If the sender can still write the buffer, a `&str` that was valid can change underneath you. So in Rust, immutable or transferred buffers aren't just good hygiene. They're required for the program to be sound. Mutable shared memory in Rust has to go through raw pointers, atomics, or volatile access.

#### Scheduling awareness

Fair point for latency. The mechanism you'd want is a userspace hint like "I'm mid-publish, give me a few more microseconds." Proposals for a short time-slice extension requested through rseq have circulated on the Linux mailing lists; I don't know whether one has merged. In this design, timeslice donation on synchronous calls and polling cores for hot channels cover most of the same ground.

#### Loading code

Agreed. The takeaway: the OS defines interfaces between components. How a language links its own libraries, statically or at load time, and keeps its binaries compatible is that language's problem. The one thing the OS adds is content-hash identity for code, so a loader can verify what it loads.

#### The syscall set

Modeled on seL4, it's very small:

| Syscall | Purpose |
|---|---|
| Call(cap, msg) | Send and wait for the reply |
| Send / NBSend | Send, blocking or non-blocking |
| Recv | Wait for a message |
| ReplyRecv | Reply to the last caller and wait for the next (the server loop) |
| Signal / Wait | Notifications, like a doorbell |
| Yield | Give up the CPU |

Everything else is a **Call on a capability to a kernel object**. That covers creating objects from untyped memory, mapping and unmapping frames, configuring and resuming threads, copying, minting, moving, revoking, and deleting capabilities, acknowledging IRQs, and binding scheduling contexts. The kernel is just another object you invoke through capabilities.

Your channel-to-the-kernel idea has two existing forms:
- **seL4's IPC buffer:** a per-thread page at a known address, holding message contents that don't fit in registers. That's the fixed-address version.
- **A batched ring:** io_uring-style, for submitting many operations at once, like mapping a hundred pages. It needs a doorbell syscall (io_uring_enter) unless the kernel polls.

Use synchronous calls for control operations and add a ring later if batching turns out to matter.

#### What sturdy refs look like in use

The closest everyday analogy is a share link, like the ones Google Docs gives out. A sturdy ref is a link that's unguessable, typed, carries specific rights, and can be revoked. Inside the store, applications never see the token itself; only when you share outside the machine does the token travel as the link.

**Store contents:**
```
values (immutable, by hash):
  h:a91f… = image { schema: h:img-v2, bytes: … }

refs (mutable cells, reached only through capabilities):
  home        → { photos: ref, docs: ref, apps: ref }
  photos/1043 → h:a91f…

store side table (never visible to apps):
  token 7Qx… → { target: photos/1043, iface: image-ref, rights: read, expires: 2026-11-01 }
```

**Editing a photo:**
1. The editor asks the powerbox for an image. You pick photo 1043, and the editor gets a live capability to `photos/1043` with read and write rights.
2. The editor writes a new value `h:c27b…` and sets the ref to it. `h:a91f…` still exists, so history comes for free.
3. The editor wants "recent files" to survive restarts. It writes the live capability into its own app-state object. The store converts it into a sturdy ref and keeps the token in the side table. The editor only sees a capability slot.
4. On the next launch, the editor reads its app-state, and the store restores live capabilities. The editor never handled token bytes.
5. You share the photo with a friend. Now the token does leave, as the link. You can revoke it, or it expires.

**Launching a program with arguments.** The `resize` program declares its imports:
```
world resize {
  input:  borrow<image-ref, read>
  output: own<image-ref, write>
  params: { width: u32, height: u32, filter: lanczos | bilinear }
}
```
The shell or an agent calls:
```
spawn(code: h:resize-bin…, bootstrap: {
  input:  <capability to photos/1043>,
  output: <capability to a new ref>,
  params: { width: 800, height: 600, filter: lanczos }
})
```
To record or rerun it, store the invocation as a value: `{ code: h:…, params: {…}, input: <sturdy ref>, output: <sturdy ref> }`. That's shell history, a script, and a Nix-style build record all in one form.

#### Raw shared memory

Yes, it's necessary: graphics buffers, audio rings, GPU memory. Your semantic-layer idea is how Wayland already works. The buffer travels as a dma-buf file descriptor, and its format, modifier, and stride travel in a typed protocol message beside it.

In this design:
- **A shared region is a capability to a memory object.** The kernel enforces its size, rights, lifetime, and revocation.
- **Its type can carry a layout schema:** `buffer<h:image-layout>`, with resolution and encoding as typed metadata. You lose a validated message structure but keep a declared meaning.
- **The OS guarantees memory safety, not content validity.** For pixels, a misbehaving writer causes tearing or garbage frames, which is harmless. For structured data in a shared region, readers apply single-fetch discipline, or the region uses a Cap'n Proto layout read through generated accessors.
- **Synchronization is explicit**, with fences signaling when a buffer is ready. Wayland has been moving to explicit sync for this reason.
- **In Rust**, generated bindings for shared regions must use raw or volatile access, as above.

So it's partly an application concern and partly a guarantee the OS makes. The OS guarantees a hostile peer can't break your memory safety or keep access past revocation. It can't guarantee the bytes make sense.

#### Getting started

**I'd build on seL4 with Rust, not write a kernel.** The novel parts of this design are the interface spec, the capability semantics, IPC conventions, process startup, and the object store. seL4 already provides the kernel you'd be aiming for: capabilities, the derivation tree, MCS scheduling contexts, capability transfer over IPC, RISC-V and Arm support, and formal verification. The rust-sel4 project provides Rust bindings and runtime support; check its current state before committing. Writing your own kernel teaches a lot, but it's years of work before your IPC and capabilities match what seL4 already has. That time doesn't go into what's new here.

**Platform:** QEMU's `virt` machine for riscv64, without CHERI. The privileged spec is simpler than Arm's, OpenSBI gives you a console and timer through SBI, and peripherals are virtio. Add CHERI later as a transport optimization: CHERI-QEMU covers CHERI-RISC-V and Morello. This is the same rule as before: design for paging first.

**Order:**
1. **Hello world** on seL4 with the Microkit framework and Rust, on QEMU riscv64.
2. **Two isolated components** sharing a ring in a shared frame, with notifications as the doorbell.
3. **Cap'n Proto messages** across that ring, with your single-fetch and immutability rules.
4. **Capability transfer with own/borrow semantics.** seL4 IPC can carry a few capabilities per message; build the move and borrow rules on top.
5. **Dynamic process spawning.** Microkit builds a fixed set of components at compile time, so spawning means writing your own root task with a process-factory interface. This is a significant milestone and where the bootstrap capability design becomes real.
6. **A virtio block driver as a component.**
7. **The object store.** Start in memory. For persistence, use an append-only log of content-addressed blobs plus an index. Crash safety comes almost for free, since nothing is overwritten and a torn write at the tail is simply discarded. Git's object and packfile formats are the prior art to study. Leave compaction and garbage collection for later.

If the goal is learning how to write a kernel, your order is right, with two changes. Add trap handling, the timer, Sv39 page tables, and user mode after the console. Build capability tables before IPC, since IPC transfers capabilities.

## m30

**human** · 2026-10-08 06:27 UTC

what exactly would building on top of sel4 look like? how would it be different than optimal?

## m31

**assistant** · 2026-10-08 06:28 UTC

#### What building on seL4 looks like

seL4 gives you very little beyond the kernel. At boot, the kernel hands one program, the **root task**, capabilities to all of untyped memory plus a few initial objects. Everything else is built in userspace: allocating kernel objects (by "retyping" untyped memory), building capability spaces and address spaces, creating threads, mapping pages.

**What you write:**
- **Process manager:** your `spawn`, bootstrap capabilities, the process-factory interface.
- **Memory and capability-slot allocators:** managing untyped memory and capability-table space.
- **The IPC library:** rings, notifications, Cap'n Proto, and the own/borrow rules.
- **Drivers as components:** virtio block, network, and console.
- **Object store, powerbox, and compatibility layer.**

**What exists to start from:**
- **Microkit:** a small framework for static systems. An XML file declares components, shared memory regions, and channels. Each component implements a couple of entry points: `notified(channel)` for doorbells and `protected(channel, msg)` for synchronous calls. It's the fastest route to steps 1–3 of the earlier plan.
- **rust-sel4:** Rust bindings and runtime support, including Microkit support and writing your own root task.
- **sDDF (seL4 Device Driver Framework):** drivers as components talking over single-producer/single-consumer rings in shared memory, with notifications as doorbells. It's essentially the IPC design we arrived at, already applied to drivers.
- **LionsOS:** an early-stage OS from the seL4 group at UNSW, built on Microkit and sDDF. It's the closest thing to "a real system on seL4" to study.

#### Where seL4 fits the design

| Design need | seL4 | Fit |
|---|---|---|
| Unforgeable capabilities | Kernel capabilities in per-process tables | Exact |
| Revoke everything derived from a grant | Capability derivation tree | Exact |
| CPU time as a capability, timeslice donation | MCS scheduling contexts, passive servers | Exact |
| Small syscall set, kernel as objects | Yes, by design | Exact |
| Kernel memory accounting | All kernel memory comes from untyped memory you hand it | Better than most designs: quotas come for free |
| Shared-memory rings plus doorbells | Frames plus notifications (sDDF does this) | Exact |
| Per-object RPC identity | Badged endpoints: a badge minted into a capability identifies the object, and can't be forged | Good |
| `borrow` | Mint a child capability, pass it, revoke it when the call returns | Good |
| `own` (move) | Capability transfer over IPC **copies**, so the sender keeps its copy | **Gap** |

The `own` gap has a clean workaround. The server that hosts the object acts as the authority. To hand an object from A to B, the server revokes A's derived capability and mints a new one for B. That's the broker design from the Linux plan, except that the broker is simply each object's host, not a central process.

#### Where it falls short of optimal

1. **Moves aren't a kernel primitive.** Ownership transfer goes through the hosting server, which costs an extra round trip.
2. **A message can carry only a few capabilities.** Messages with many capabilities need batching or several messages.
3. **Managing capability slots is tedious.** Allocating table space and choosing slots is all left to userspace. It's the most commonly cited pain point, and the Rust crates exist partly to make it bearable.
4. **The proofs constrain kernel changes.** Changing the kernel means losing verification for your version. Anything the design wants that seL4 doesn't do has to be done in userspace, or in an unverified fork.
5. **Verification covers specific configurations.** It doesn't cover every combination of architecture, multicore, and MCS. Check which configuration you'd actually run before claiming verified properties.
6. **No CHERI support in mainline,** as far as I know. The paging variant is what you'd get.
7. **Almost no ecosystem.** There are basic drivers from sDDF and LionsOS, and essentially nothing for GPUs, Wi-Fi, or a desktop.

The usual answer to item 7 is to run Linux in a VM on seL4's virtual machine monitor. Linux drives the hardware that has no native driver yet and runs legacy applications behind a typed interface. Qubes and Genode use the same pattern: native components for the parts that matter, and a contained Linux for everything else. It's also your compatibility layer.

#### The bottom line

seL4 is most of the kernel this design wants. The gaps are kernel-level moves, the capability-count limit, slot-management ergonomics, and CHERI. All except CHERI can be handled in userspace, at a modest cost. The ecosystem gap is real, but a Linux VM covers it.

Writing your own kernel gets you native moves and maybe CHERI, at the cost of years of work and the verification. For a project whose novelty is above the kernel, seL4 is the better trade. If the design eventually proves that a kernel change is worth it, you'll know exactly which change, and you can make the case to seL4 upstream or fork with clear eyes.
