# Roadmap

Improvements planned for Rustvision, in priority order. Each item says why it matters and how to know it is done.

The current state is described in [ARCHITECTURE.md](ARCHITECTURE.md). The known limits are listed in the README.

## 1. Measure the tracker on real footage

**Why.** The unit tests check the tracking logic on synthetic scenes. The real camera shows a different result: crossings fail after an object disappears, and the mask produces many small boxes. Without a measure, the project cannot say how well it works.

**Done when:**

- a set of recorded sequences exists, with at least: one object moving alone, two objects crossing, one object stopping, one object bouncing;
- for each sequence, the share of frames where the identifier stays on the right object is written down;
- the result is reported in the README, including the failures.

## 2. Remove noise with a morphological opening

**Why.** Single pixels and thin speckles create many small regions. The pipeline step "opening" was planned in the CLAUDE.md and is not implemented yet.

**Done when:** the number of regions per frame drops on the same recorded sequences, and the single-object sequence keeps its identifier.

## 3. Merge fragments of the same object

**Why.** A person, or any object with parts of different contrast, is split into several regions, and each region gets its own identifier. Identifiers climb quickly for one object.

**Done when:** regions closer than a set distance are joined before tracking, and a multi-part object keeps one identifier on the recorded sequences.

## 4. Adaptive background

**Why.** The background is fixed, so an object that stops becomes part of it and disappears. A lighting change also breaks detection until the background is recaptured with `Space`.

**Done when:** a stopped object remains visible for a set time, and a slow lighting change no longer requires a manual recapture.

## 5. Split `main.rs`

**Why.** `main.rs` still mixes camera capture, keyboard handling, the pipeline and the error policy. The target architecture has a `capture.rs` module for the camera.

**Done when:** `main.rs` only wires the modules together, the camera code lives in `capture.rs`, and the tests still pass.

## 6. Typed errors in modules

**Why.** The CLAUDE.md asks for `thiserror` in the modules and `anyhow` only in `main.rs`. The code currently uses `anyhow` everywhere, so callers cannot match on error kinds.

**Done when:** the camera and capture modules return a `thiserror` error type, and `main.rs` converts it with `anyhow`.

## 7. Calibrate the speed

**Why.** Velocity is in pixels per frame. It cannot be compared to a real speed without a known scale.

**Done when:** a reference length in the scene is measured, and the speed can be reported in units of length per second.

## 8. Learned detector, as a second stage

**Why.** Background subtraction detects motion, not objects. A learned detector would recognise what an object is, including when it is still.

**Done when:** the background subtraction proposes regions, a small model confirms which regions are the target class, and the added dependency is approved before use, as the CLAUDE.md requires.
