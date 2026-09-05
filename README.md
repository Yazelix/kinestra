# Kinestra

Typed Rust orchestration for recordings of real terminal applications. Kinestra
owns the private X11 display, child processes, FFmpeg capture and exports.
Consumers own their Rust recipes, application pins, appearance and media paths.

Capture supports **x86_64 Linux** only and does not use your desktop. Recorders
are compiled development tools; neither Rust nor the capture stack belongs in
the recorded application's installed runtime.

## Recording

For one application, with its terminal available on PATH:

```sh
nix run github:Yazelix/kinestra -- 960 540 8 demo.mp4 demo xterm -class demo -e your-app
```

The positional arguments are width, height, duration in whole seconds, MP4
destination, X11 window class and the application command with its arguments.
Dimensions must be positive and even; duration must be positive.

For multi-step demos, pin Kinestra in the consumer's flake and compile an ordinary
Rust recipe against its library:

```nix
inputs.kinestra.url = "github:Yazelix/kinestra";

# In outputs, for x86_64-linux:
recorder = kinestra.lib.x86_64-linux.mkRecorder {
  name = "record-demo";
  recipe = ./demo/record.rs;
  runtimeInputs = [ terminalPackage ];
  environment.APP_BIN = "${appPackage}/bin/your-app";
};
# Expose "${recorder}/bin/record-demo" as apps.x86_64-linux.record-demo.program.
```

The recipe supplies `main` and uses the public library directly:

```rust
use kinestra::{Recorder, Result, Size};
use std::{path::Path, process::{Command, ExitCode}, time::Duration};

fn record(r: &mut Recorder) -> Result<()> {
    r.display(Size::new(960, 540)?, None)?;
    r.launch("demo", Command::new("xterm")
        .args(["-class", "demo", "-e", "your-app"]))?;
    r.sleep(Duration::from_secs(1))?;
    r.record(Path::new("demo.mp4"), |r| r.sleep(Duration::from_secs(8)))?;
    r.stop_app()?;
    r.poster(Path::new("demo.mp4"), Duration::from_secs(2), Path::new("poster.png"))?;
    r.gif(Path::new("demo.mp4"), Path::new("demo.gif"), 640, 10)
}

fn main() -> ExitCode { kinestra::run(record) }
```

`mkRecorder` compiles the recipe together with the pinned library and supplies
its tools through Nix. The consumer needs no duplicate Cargo dependency pin,
runtime compiler, shell interpreter for its recipe, or custom scenario language.
Changing the recipe requires a rebuild.

## Lifecycle and API

`Recorder` supplies display setup, window launch/stop, recording, snapshots,
posters, GIF export, keystrokes and text entry. `exec` and `output` run explicit
`Command` arguments without shell parsing. `command` supplies the isolated
display environment; `work` exposes the private temporary directory.
`on_exit` registers consumer cleanup commands, such as deleting a named detached
Zellij session.

Capture is H.264/yuv420p at 30 FPS, without audio. GIF width and FPS are explicit.
Paths, durations, validated dimensions and failures have Rust types.
Signal notification uses `signal-hook`; orchestration is synchronous.

Use `Recorder::sleep`, `exec` and `output` so cancellation is checked while
waiting. SIGINT/SIGTERM stop the recipe, finalize active MP4 capture and clean up
owned processes. Each child gets its own process group. During shutdown, Kinestra
allows the launched process five seconds before escalating to SIGKILL and reporting
a timeout. After that process exits, Kinestra kills any remaining group members.
Consumer cleanup commands also have bounded waits. Detached servers require consumer cleanup.

Successful runs remove their temporary directory. Failures retain logs at the
printed path. Normal Rust unwinding also runs cleanup; SIGKILL, aborts and machine
failure cannot guarantee finalization. Recipes are trusted code with your
permissions, not a sandbox. Live rendering is not byte-identical across takes.

## Verification

```sh
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
nix flake check
nix run . -- --help
```

The Nix check runs the installed Rust test recipe against real Xvfb and FFmpeg:
two sequential recordings, poster/GIF dimensions, command failure, premature
application exit, SIGINT/SIGTERM, forced shutdown of an uncooperative child,
surviving children of an exited launcher, MP4 finalization, consumer cleanup and
display isolation. These checks do not require access to a live desktop.

## Origin

The capture behavior originates in Nova site's recording workflow at
`c5b67d8855c3110cf1e9c0f0c8a1069f81b9102b`. Kinestra owns capture mechanics;
Nova site and Anima own their respective demos.
