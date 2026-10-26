---
name: code-quality
description: Rules for writing, changing and reviewing code in any language - comments, naming, abstraction, errors, tests, scope. Always loaded for developers and the reviewer; load it again after a context compaction.
---

# Code quality

Write code the way a careful maintainer of this code base would: the next reader should not notice who wrote it.

## Style and scope

- Follow the style of the surrounding code: naming, structure, error handling, test layout. Read the neighbours before writing.
- Do what the task asks. Leave the code you touch a little clearer, but do not refactor beyond the task or reformat what you do not change.
- No speculative abstraction. Repetition is cheaper than a wrong abstraction: extract on the third use, or earlier when the concept is real and has a name. A helper that needs a flag argument to serve its callers is the wrong one.
- No dead code, no commented-out code, no unused parameters. Version control keeps history.

## Names and functions

- Prefer a name to a comment: rename, or extract a function whose name says what the block does.
- Keep functions small and focused on one job. Do not split a function only to make it shorter; a long function that reads top to bottom is fine.

## Comments

By default write no comment. A comment must first prove it is needed: without it a reader would get the code wrong or lose time. Good reasons:

- a non-obvious reason ("why", never "what");
- a workaround, with what it works around;
- an invariant or a safety condition the code relies on;
- a pointer to the document that explains the rest.

Keep it short and to the point. A comment is not a second specification or a second code base: behaviour lives in the specs, structure in the architecture document, and the comment points there. Do not retell the code, and do not leave notes about the change you are making (they belong to the commit message).

## Errors

- Handle an error or pass it on with its context. Never swallow it: no empty `catch`, no ignored result.
- Fail at the boundary where the input arrives; inside, trust what the boundary checked.
- An error message says what failed and with what input.

## Tests

- A test checks behaviour through the public interface, not how the code does it: a refactoring that keeps behaviour must not break it.
- A test would fail if the code were wrong. Cover the edge and the error cases, not only the happy path.
- Share setup, but keep each test's action and assertion readable in its own body.
- Fix a bug with a test that fails before the fix.
