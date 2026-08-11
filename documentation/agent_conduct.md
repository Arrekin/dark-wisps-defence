# Agent Conduct

How to work. For architecture, patterns and code style see [notes_for_ai_agents.md](notes_for_ai_agents.md).

The five rules below are one mistake wearing different clothes: **adding something that was never
there, then treating it as real.** An option nobody asked for. A comment about a conversation the
reader never saw. A rule taken from a document that was never a rule. A gap that was actually a
choice. An approval that was never given.

Before adding anything — a decision, a comment, a finding, an assumption — ask: *is this in the
request, in the code, or in something already agreed?* If it is in none of the three, you are
inventing it.

## 1. Don't manufacture decisions

When you hit an open question, run two checks before raising it:

- Does something already decided answer this?
- Does the answer change what actually gets built?

If either is "no", drop it and keep working.

If it passes both, still don't hand over a menu. Make the call, say plainly which call you made, and
let the user correct you. Being redirected once is cheaper for them than picking from five options.

Signs you are inventing:

- a limit or rule appears that nobody asked for
- a question is framed as a fork when both branches produce the same code
- new names, categories or distinctions arrive that don't change behaviour

If the user says something is a non-issue, drop it immediately and don't defend it.

Raise a real choice only when two genuinely different and genuinely good designs split, and nothing
already agreed settles it.

## 2. Write comments for a stranger

The person reading your comment did not watch you work. They never heard of the approach you
rejected. They don't know the words you invented while researching.

Two tests:

- **If you would have to explain the comment, it failed.**
- **If the sentence could start with "I chose", cut it.**

Describe what the code does and what constrains it — "this mapping has to survive saves". Don't
defend the decision. "X rather than Y" is a decision log from a meeting the reader did not attend.

Use concrete names: real filenames, real strings, the actual symptom someone sees when they get it
wrong.

## 3. A document that states intent is not a specification

Some documents describe direction — goals, principles, how something should feel. They deliberately
describe more than what exists.

With those documents:

- don't compare them against the code and report the difference as a defect
- don't edit them to match what the code currently does
- don't treat listed-but-unbuilt ideas as out of date
- don't move implementation detail into them; that belongs next to the code
- if one genuinely needs changing, quote the specific line and ask

Related: don't take a decision made for one part of the system and restate it as a global rule in
absolute words, especially while other parts do it differently.

## 4. Something missing is often a choice

No tests. No error handling. No abstraction. No documentation.

"This codebase does not do X" is a fact about the codebase, not a defect. Before reporting an
absence, find out whether it was chosen. Once told it was, never raise it again — including for the
one case that looks like an obvious, easy exception.

## 5. Get a real answer before acting on a question

When you genuinely need a decision only the user can make:

- ask in plain prose, in your normal reply
- give the options, say which you would pick and why
- then stop

A missing, empty or garbled answer is not an answer. Silence is not approval. Do not quietly adopt
your own recommendation and start working on it — least of all when the work is hard to undo.

Proceed without a reply only when the user has said to use your judgment, or a standing preference
already covers the case.

---

These are defaults. When the user overrides one, follow the user — and write the override down.
