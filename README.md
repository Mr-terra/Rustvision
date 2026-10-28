# Rustvision

Real-time detection and tracking of a moving object from a webcam, written in Rust. The image processing is implemented by hand: no OpenCV and no image-processing crate. The only dependencies are for the camera, the window and error handling.

The project is a learning exercise. The goal is not the code itself but the understanding behind it: image processing, ownership on frame buffers, and typed state machines, with a longer-term aim of transferring these techniques to embedded Rust.

## What it does

- Captures the webcam stream and displays it in a window.
- Converts each frame to grayscale and subtracts a reference background.
- Thresholds the difference, groups changed pixels into regions (connected components), and filters out noise by area.
- Tracks each region across frames, keeps a stable identifier per object, and copes with objects that leave the frame, get occluded, or cross each other.
- Draws a box, a centre cross and an identifier per track, coloured by state.

<!-- Suggested: add a screenshot or short GIF of the tracker here, e.g. docs/demo.gif -->

## Pipeline

Each frame goes through the following steps:

1. **Grayscale**: weighted average of the RGB channels, written into a buffer allocated once.
2. **Background subtraction**: each pixel is compared to a reference image captured at startup (and on demand with `Space`).
3. **Thresholding**: a pixel whose difference exceeds the threshold becomes white, the others black.
4. **Connected components**: white pixels are grouped with an iterative flood fill (a queue, not recursion). Each group becomes a region: bounding box, area and centre. Regions below the minimum area are discarded.
5. **Tracking**: each track predicts its position from its velocity, then matches the closest region. Positions and velocities are smoothed with an alpha-beta filter.
6. **Display**: boxes and crosses are drawn into the display buffer before it is shown.

## Tracking design

Each track is a small state machine, represented as a Rust `enum` and matched exhaustively (no catch-all branch):

| State | Colour | Meaning |
|---|---|---|
| `Recherche` | yellow | New track, not yet confirmed (must match several frames in a row) |
| `Verrouille` | green | Confirmed track, matched to a detection on each frame |
| `Perdu` | red | No detection for a while: the track coasts on its velocity, then is deleted |
| `Recouvrement` | orange | Two tracks overlap inside a single region: they ignore that region and keep their size |

Notable decisions:

- **Identifiers are never reused.** A deleted track's identifier is not given to a new object, so a stale ID cannot silently attach to the wrong object.
- **Matching is global, not per track.** Candidate pairs are sorted by cost and accepted greedily, so one detection is never claimed by two tracks, and the result does not depend on detection order (tested).
- **Overlaps are handled explicitly.** When two predicted boxes touch and a single region covers them, both tracks enter `Recouvrement`. Their size history is frozen so the merged region does not corrupt it. A separation is recognised when two distinct regions reappear close to the two predictions. If the overlap lasts more than five frames, the normal path resumes and one track is lost rather than given the wrong object.
- **Size breaks ties at separation.** Position alone cannot tell a bounce from a crossing. Right after separation, the relative change of each region's size against its track's size history gets extra weight in the matching cost, and returns to normal on the next frame.
- **Known limit.** Two objects of identical size that bounce off each other can swap identifiers. The test suite documents this case as expected behaviour rather than hiding it.

## Engineering choices

- **No allocation in the hot loop.** Buffers (RGB frame, grayscale, background, mask, queue, track list, matching work arrays) are allocated once before the loop and reused. Camera frames are decoded into an existing buffer.
- **No `unwrap()` on runtime paths.** Errors are propagated with `Result`. The camera counts consecutive failures against a one-second timeout: a single failed frame is skipped, and a disconnected camera ends the program with a clear message instead of a panic.
- **Panic-free drawing.** Boxes are clipped to the image, and pixel writes are bounds-checked. Arithmetic that can overflow on extreme coordinates saturates.
- **Decisions as pure functions.** The camera-timeout decision is a function of elapsed time only, so it is unit-tested without a camera or a wait.
- **Lints.** The code is checked with `cargo clippy -- -W clippy::pedantic`. Each `allow` attribute that remains has a comment explaining why.

## Tests

`cargo test` runs 18 unit tests that need no camera: region measurement and filtering, drawing of out-of-frame tracks, tracker behaviour (association, ordering, locking, deletion and identifier reuse, crossing, bounce with different and identical sizes, stopped object, overlap timeout, recovery after loss), and the camera-timeout decision.

## Running

Requirements: Windows (camera access goes through Media Foundation), a webcam, and a recent Rust toolchain with `cargo`.

```
cargo run --release
```

Controls:

| Key | Action |
|---|---|
| `Space` | Recapture the background |
| `Up` / `Down` | Raise / lower the detection threshold (value printed on release) |
| `Right` / `Left` | Raise / lower the minimum region area (value printed on release) |
| `Escape` | Quit |

Checks run before each commit:

```
cargo fmt
cargo clippy -- -W clippy::pedantic
cargo test
```

## Project structure

```
src/
  main.rs            main loop, wiring, camera error handling
  pipeline/
    mod.rs           module declarations
    blobs.rs         connected components -> regions
    display.rs       drawing of boxes, crosses and identifiers
    tracker.rs       tracks, state machine, matching, overlaps
```

The target architecture also has a `capture.rs` module; capture still lives in `main.rs` for now.

## Development process

The project was built in milestones, each one validated before the next began:

1. Camera capture and display
2. Grayscale and adjustable threshold
3. Background subtraction
4. Connected components and bounding boxes
5. Frame-to-frame tracking with stable identifiers
6. Robustness: partial occlusion, leaving the frame, camera failure, without panics

The history in `git log` shows the progression. The rule for each milestone was that the author must be able to explain how it works without re-reading the code.

## How AI was used

AI was used as a **tutor and reviewer**, not as a code generator. The rules are written in [CLAUDE.md](CLAUDE.md), which is part of the repository:

- The assistant does not write code in `src/`. It explains compiler messages, points to problems by line number, and asks guiding questions before giving any answer.
- Reviews follow a fixed order: correctness (including edge cases), then Rust idioms, then the language concept the code could exploit better. At most three points per review.
- Small illustrative snippets (10 lines at most) are allowed only for constructs that are not the one being solved. Empty test skeletons are allowed on request.
- The assistant can edit configuration and documentation, and propose an architecture or an order of implementation.

Design questions were worked through in conversation: for example, comparing an absolute threshold with a relative one for region size, how to measure velocity during an overlap, and how to choose a cost that mixes distance and size. The author made the choices and wrote the code.

The assistant also kept a memory of progress between sessions (milestones validated, open points), so that each session restarted from the actual state of the project.

This README was drafted with AI assistance from the project's own code and history, and checked against the code by the author.

## Limitations

- **Identical-size bounces** can swap identifiers (see above).
- **Two similar objects crossing** remain a hard case: the tracker relies on size and position only.
- **Fixed background**: a lighting change degrades detection until the background is recaptured.
- **Thresholds were tuned on examples**, not on a recorded test sequence.
- **Camera disconnection** was tested by disabling the device in Device Manager, not by physically unplugging it.
- **Speed is in pixels per frame**, with no conversion to real units, since no scale is known.

## Licence

Licensed under the Apache License 2.0. See [LICENSE](LICENSE).
