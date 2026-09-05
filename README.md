# Kinestra

Repeatable recordings of real terminal applications. Kinestra supplies a pinned
X11 capture environment and FFmpeg helpers; your repository supplies the demo.
The capture package supports **x86_64 Linux**. It does not require access to your
desktop and is a development tool, not an application runtime dependency.

```sh
nix run github:Yazelix/kinestra -- ./demo/record.sh
```

A scenario is a trusted Bash script, sourced with `set -euo pipefail`. It can
define functions, run commands and use the following helpers:

| Helper | Purpose |
| --- | --- |
| `kinestra_display WIDTH HEIGHT [WALLPAPER]` | Start a private display with even pixel dimensions; wallpaper enables compositing |
| `kinestra_launch CLASS COMMAND...` | Launch an application, wait for its X11 class, size and focus its window |
| `kinestra_record OUTPUT.mp4 COMMAND...` | Capture at 30 FPS while a function or command performs the scenario |
| `kinestra_stop_app` | Stop the launched application before launching another |
| `kinestra_snapshot OUTPUT.png` | Capture the current display |
| `kinestra_poster INPUT.mp4 SECONDS OUTPUT.png` | Extract a poster |
| `kinestra_gif INPUT.mp4 OUTPUT.gif [WIDTH] [FPS]` | Export a looping, palette-optimized GIF; defaults to 960 pixels and 15 FPS |

For example, with Xterm available in the consumer's recording environment:

```bash
kinestra_display 960 540
kinestra_launch demo xterm -class demo -e your-app
sleep 1
kinestra_record ./demo.mp4 sleep 8
kinestra_poster ./demo.mp4 2 ./poster.png
kinestra_gif ./demo.mp4 ./demo.gif
```

The launcher finalizes an active recording and stops its application, compositor
and display on exit or interruption. A consumer that starts detached servers can
define `kinestra_cleanup_hook` to stop its own named sessions. Keep consumer state
in the temporary `kinestra_work` directory created by `kinestra_display`.
Successful runs remove this directory; failed runs retain capture logs in the printed
temporary directory. Scripts are executed with your permissions, not sandboxed.

Pin Kinestra as a flake input and expose a separate `record-demo` app. Keep the
terminal/product revision, appearance, keyboard choreography, fixtures, output
paths and publication policy in the consumer. Keep recordings out of this repo.
Pinned tools make the environment repeatable; live applications and keyboard
timing do not promise byte-identical videos.

## Verification

```sh
nix flake check
nix run . -- --help
```

The check records a private display, verifies GIF dimensions and a poster, then
proves that failed and interrupted scenarios preserve their exit codes, finalize
their recordings and stop their displays.

## Origin

Extracted from Yazelix Nova's `nova-site/drafts/recordings/record.sh` at
`c5b67d8855c3110cf1e9c0f0c8a1069f81b9102b`. Kinestra owns capture mechanics;
Nova site and Anima own their respective demos.
