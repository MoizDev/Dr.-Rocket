# Benchmarks

Automation scripts (see `src/automation.rs`), run from the repo root. They work on a copy in
`target/bench/`, never on repo files (`editor.json` types into the file it opens):

    mkdir -p target/bench && cp racket/visualize.rkt target/bench/
    DRROCKET_CONFIG_DIR=$(mktemp -d) DRROCKET_FRAME_STATS=1 \
      DRROCKET_AUTOMATION=tools/bench/editor.json target/release/drrocket

- `editor.json`: idle, then scroll hard, then type
- `idle-big.json`, `idle-small.json`, `idle-tiny.json`: an open file at three window sizes
- `welcome.json`: the welcome screen, almost nothing drawn

`DRROCKET_FRAME_STATS` also takes `continuous` (redraw every frame, to measure throughput),
`causes` (what requested each repaint) and `paint` (draw calls and vertices per frame).

The Windows bench workflow runs it on a GPU-less Windows machine, where it measures the
software renderer.
