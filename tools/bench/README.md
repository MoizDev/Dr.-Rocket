# Benchmarks

`editor.json` is an automation script (see `src/automation.rs`) that opens
`racket/visualize.rkt`, sits idle, scrolls hard and types, from the repo root. Run it with
frame stats to see how the app keeps up:

    DRROCKET_FRAME_STATS=1 DRROCKET_AUTOMATION=tools/bench/editor.json target/release/drrocket

The Windows bench workflow runs it on a GPU-less Windows machine, where it measures the
software renderer.
