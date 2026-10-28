# How AI was used in this project

This document describes how an AI assistant (Claude, through Claude Code) was set up and used while building Rustvision. It is written so that the process can be checked, not just claimed.

## Principle

The project is a learning exercise. The goal was for the author to understand Rust and image processing, not to obtain working code quickly. The AI was therefore set up as a **tutor and reviewer**, not as a developer.

## How it was set up

The rules live in [CLAUDE.md](../CLAUDE.md), at the root of the repository, and are read at the start of every session. They define:

- **what the assistant must not do**: write code in `src/` during the learning phase, fix compiler errors in the author's place, or give the solution before a question;
- **what it may do**: explain compiler messages, review code by line number, write short illustrative snippets (10 lines at most) on unrelated constructs, write empty test skeletons on request, edit configuration and documentation;
- **how it reviews code**: correctness first (including edge cases), then Rust idioms, then the language concept the code could exploit better, with at most three points per review;
- **what counts as done**: a milestone is complete only when the author can explain how it works without re-reading the code.

The project's milestones are listed in `CLAUDE.md`. Once milestone 6 was validated, the file was updated to allow the assistant to modify code on request.

## How sessions worked

- Each change was committed separately, and the working tree was clean before each session.
- The author ran `cargo fmt`, `cargo clippy -- -W clippy::pedantic` and `cargo test` themselves, and pasted the output when asking for a review.
- Design questions were worked through in conversation. The assistant asked questions that pushed the author to justify a choice, for example: why a ratio rather than an absolute threshold for region size, how to measure velocity during an overlap, or how to combine distance and size in one cost.
- Expected results for tests were written before running them, so that a passing test could not be a surprise.

## Memory between sessions

The assistant keeps a short memory of the project's progress between sessions: which milestones are validated, and which points remain open. This memory is stored on the author's machine, outside the repository, and is not included here.

## What the assistant wrote

- In the learning phase: nothing in `src/`, by rule. Explanations, review comments and questions only.
- After milestone 6: changes to `src/` made on request, reviewed by the author, and checked with the same commands.
- Documentation: this file, [ARCHITECTURE.md](ARCHITECTURE.md), and the README. These were drafted by the assistant from the code and history, then checked against the code.

## Checks on AI output

- The statements in the documentation were checked against the code where possible (constants, state transitions, overlap conditions). Parts that were not checked line by line are the ones to re-read first.
- Tests and linting were run after each change, and the results are reported as observed, not assumed.
- Claims that could not be verified without running the program (for example, the behaviour with a real camera) are marked as such in the README.
