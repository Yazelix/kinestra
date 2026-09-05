//! Synchronous, Linux-only capture orchestration. Recipes are ordinary Rust programs.
use signal_hook::{
    SigId,
    consts::{SIGINT, SIGTERM},
};
use std::{
    ffi::{OsStr, OsString},
    fmt, fs, io,
    os::unix::{
        ffi::OsStringExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(String),
    Failed(String, ExitStatus),
    Interrupted(i32),
    Timeout(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Invalid(message) => f.write_str(message),
            Self::Failed(command, status) => write!(f, "{command}: {status}"),
            Self::Interrupted(signal) => write!(f, "interrupted by signal {signal}"),
            Self::Timeout(message) => write!(f, "timed out: {message}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Positive, even pixel dimensions, suitable for the H.264/yuv420p output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    width: u32,
    height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err(Error::Invalid(
                "display dimensions must be positive and even".into(),
            ));
        }
        Ok(Self { width, height })
    }
}
impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

const POLL: Duration = Duration::from_millis(20);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct Process {
    child: Child,
    label: String,
    signal: &'static str,
    stopped: bool,
}

impl Process {
    fn spawn(command: &mut Command, signal: &'static str) -> Result<Self> {
        let label = format!("{command:?}");
        let child = command
            .process_group(0)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("{label}: {e}")))?;
        Ok(Self {
            child,
            label,
            signal,
            stopped: false,
        })
    }

    fn signal(&mut self, signal: &str) -> io::Result<()> {
        // Only the process group created by spawn is targeted, never the caller's group.
        let status = Command::new("kill")
            .args([signal, "--", &format!("-{}", self.child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() && self.child.try_wait()?.is_none() {
            return Err(io::Error::other(format!("could not signal {}", self.label)));
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<ExitStatus> {
        self.signal(self.signal)?;
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait()? {
                // The launcher exiting does not mean its process group is empty.
                self.signal("-KILL")?;
                self.stopped = true;
                return Ok(status);
            }
            if Instant::now() >= deadline {
                self.signal("-KILL")?;
                self.child.wait()?;
                self.stopped = true;
                return Err(Error::Timeout(format!("stopping {}", self.label)));
            }
            thread::sleep(POLL);
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.stopped
            && let Err(error) = self.stop()
        {
            eprintln!("Kinestra cleanup: {error}");
        }
    }
}

/// Owns the private display and every capture child. Use `run` to retain logs on error.
pub struct Recorder {
    work: PathBuf,
    interrupted: Arc<AtomicUsize>,
    signals: Vec<SigId>,
    display: Option<(Size, String)>,
    xvfb: Option<Process>,
    compositor: Option<Process>,
    app: Option<Process>,
    capture: Option<Process>,
    cleanup: Vec<Command>,
    command_number: usize,
    success: bool,
}

/// Runs one trusted recipe and maps command failures and SIGINT/SIGTERM to exit codes.
pub fn run(recipe: impl FnOnce(&mut Recorder) -> Result<()>) -> ExitCode {
    let result = (|| {
        let mut recorder = Recorder::new()?;
        recipe(&mut recorder)?;
        recorder.check()?;
        recorder.close()?;
        recorder.check()?;
        recorder.success = true;
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Kinestra: {error}");
            let code = match error {
                Error::Interrupted(signal) => 128 + signal,
                Error::Failed(_, status) => status
                    .code()
                    .filter(|code| *code != 0)
                    .or_else(|| status.signal().map(|signal| 128 + signal))
                    .unwrap_or(1),
                _ => 1,
            };
            ExitCode::from(u8::try_from(code).unwrap_or(1))
        }
    }
}

impl Recorder {
    fn new() -> Result<Self> {
        let output = Command::new("mktemp")
            .args(["-d", "-t", "kinestra.XXXXXX"])
            .output()?;
        if !output.status.success() {
            return Err(Error::Failed("mktemp".into(), output.status));
        }
        let mut recorder = Self {
            work: PathBuf::from(OsString::from_vec(
                output
                    .stdout
                    .strip_suffix(b"\n")
                    .unwrap_or(&output.stdout)
                    .to_vec(),
            )),
            interrupted: Arc::new(AtomicUsize::new(0)),
            signals: Vec::new(),
            display: None,
            xvfb: None,
            compositor: None,
            app: None,
            capture: None,
            cleanup: Vec::new(),
            command_number: 0,
            success: false,
        };
        for signal in [SIGINT, SIGTERM] {
            recorder.signals.push(signal_hook::flag::register_usize(
                signal,
                Arc::clone(&recorder.interrupted),
                signal as usize,
            )?);
        }
        Ok(recorder)
    }

    pub fn work(&self) -> &Path {
        &self.work
    }

    fn check(&mut self) -> Result<()> {
        let signal = self.interrupted.load(Ordering::Relaxed);
        if signal != 0 {
            return Err(Error::Interrupted(signal as i32));
        }
        for process in [
            &mut self.capture,
            &mut self.app,
            &mut self.compositor,
            &mut self.xvfb,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(status) = process.child.try_wait()? {
                return Err(Error::Failed(
                    format!("{} exited unexpectedly", process.label),
                    status,
                ));
            }
        }
        Ok(())
    }

    pub fn sleep(&mut self, duration: Duration) -> Result<()> {
        let start = Instant::now();
        loop {
            self.check()?;
            let Some(left) = duration.checked_sub(start.elapsed()) else {
                return Ok(());
            };
            thread::sleep(left.min(POLL));
        }
    }

    fn environment(&self, command: &mut Command) {
        if let Some((_, display)) = &self.display {
            command.env("DISPLAY", display);
        }
        command
            .env("WINIT_UNIX_BACKEND", "x11")
            .env("TERM", "xterm-256color");
        for name in [
            "WAYLAND_DISPLAY",
            "XDG_SESSION_TYPE",
            "XDG_CURRENT_DESKTOP",
            "NO_COLOR",
        ] {
            command.env_remove(name);
        }
    }

    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        self.environment(&mut command);
        command
    }

    /// Executes without a shell; cancellation stops the command's process group.
    pub fn exec(&mut self, command: &mut Command) -> Result<()> {
        self.check()?;
        self.environment(command);
        let mut process = Process::spawn(command, "-TERM")?;
        loop {
            self.check()?;
            if let Some(status) = process.child.try_wait()? {
                process.stopped = true;
                return if status.success() {
                    Ok(())
                } else {
                    Err(Error::Failed(process.label.clone(), status))
                };
            }
            thread::sleep(POLL);
        }
    }

    /// Captures textual output to a file, avoiding pipe deadlocks while polling cancellation.
    pub fn output(&mut self, command: &mut Command) -> Result<String> {
        self.command_number += 1;
        let path = self
            .work
            .join(format!("command-{}.stdout", self.command_number));
        command.stdout(fs::File::create(&path)?);
        self.exec(command)?;
        Ok(fs::read_to_string(path)?)
    }

    /// Registers consumer-owned cleanup, such as deleting one named Zellij session.
    pub fn on_exit(&mut self, mut command: Command) {
        self.environment(&mut command);
        self.cleanup.push(command);
    }

    pub fn display(&mut self, size: Size, wallpaper: Option<&Path>) -> Result<()> {
        if self.display.is_some() || self.xvfb.is_some() {
            return Err(Error::Invalid("display already started".into()));
        }
        let ready = self.work.join("display");
        self.xvfb = Some(Process::spawn(
            Command::new("Xvfb")
                .args([
                    "-displayfd",
                    "1",
                    "-screen",
                    "0",
                    &format!("{size}x24"),
                    "-nolisten",
                    "tcp",
                    "+extension",
                    "Composite",
                ])
                .stdout(fs::File::create(&ready)?)
                .stderr(fs::File::create(self.work.join("xvfb.log"))?),
            "-TERM",
        )?);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.check()?;
            let value = fs::read_to_string(&ready)?;
            if value.ends_with('\n') {
                let number: u32 = value
                    .trim()
                    .parse()
                    .map_err(|_| Error::Invalid("invalid Xvfb display number".into()))?;
                self.display = Some((size, format!(":{number}")));
                break;
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout("starting Xvfb".into()));
            }
            self.sleep(POLL)?;
        }
        if let Some(wallpaper) = wallpaper {
            self.exec(Command::new("xwallpaper").arg("--zoom").arg(wallpaper))?;
            let mut command = self.command("picom");
            command
                .args(["--backend", "xrender", "--vsync", "--config", "/dev/null"])
                .stdout(fs::File::create(self.work.join("picom.log"))?)
                .stderr(Stdio::inherit());
            self.compositor = Some(Process::spawn(&mut command, "-TERM")?);
            self.sleep(Duration::from_secs(1))?;
        }
        Ok(())
    }

    fn display_info(&self) -> Result<(Size, &str)> {
        self.display
            .as_ref()
            .map(|(size, display)| (*size, display.as_str()))
            .ok_or_else(|| Error::Invalid("start a display before capturing or launching".into()))
    }

    pub fn stop_app(&mut self) -> Result<()> {
        if let Some(mut app) = self.app.take() {
            app.stop()?;
        }
        Ok(())
    }

    pub fn launch(&mut self, class: &str, command: &mut Command) -> Result<()> {
        let (size, _) = self.display_info()?;
        self.stop_app()?;
        self.environment(command);
        let log = fs::File::create(self.work.join("app.log"))?;
        command.stdout(log.try_clone()?).stderr(log);
        self.app = Some(Process::spawn(command, "-TERM")?);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.check()?;
            match self.output(
                Command::new("xdotool")
                    .args(["search", "--class", class])
                    .stderr(Stdio::null()),
            ) {
                Ok(windows) => {
                    if let Some(window) = windows.lines().next() {
                        return self.exec(Command::new("xdotool").args([
                            "windowmove",
                            window,
                            "0",
                            "0",
                            "windowsize",
                            window,
                            &size.width.to_string(),
                            &size.height.to_string(),
                            "windowfocus",
                            "--sync",
                            window,
                        ]));
                    }
                }
                Err(Error::Failed(_, _)) => (),
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout(format!("waiting for window class {class}")));
            }
            self.sleep(Duration::from_millis(100))?;
        }
    }

    pub fn record(
        &mut self,
        output: &Path,
        recipe: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        if self.capture.is_some() {
            return Err(Error::Invalid("recording already active".into()));
        }
        let (size, display) = self.display_info()?;
        let progress = self.work.join("progress");
        fs::File::create(&progress)?;
        let mut command = self.command("ffmpeg");
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "x11grab",
                "-draw_mouse",
                "0",
                "-framerate",
                "30",
                "-video_size",
                &size.to_string(),
                "-i",
                display,
                "-an",
                "-c:v",
                "libx264",
                "-crf",
                "18",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-stats_period",
                "0.1",
                "-progress",
            ])
            .arg(&progress)
            .arg("-y")
            .arg(output)
            .stdout(Stdio::null())
            .stderr(fs::File::create(self.work.join("ffmpeg.log"))?);
        self.capture = Some(Process::spawn(&mut command, "-INT")?);
        let result = (|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while fs::metadata(&progress)?.len() == 0 {
                if Instant::now() >= deadline {
                    return Err(Error::Timeout("starting FFmpeg".into()));
                }
                self.sleep(POLL)?;
            }
            recipe(self)
        })();
        let finalized = self.stop_capture();
        result?;
        finalized?;
        self.exec(
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "csv=p=0",
                ])
                .arg(output),
        )
    }

    fn stop_capture(&mut self) -> Result<()> {
        if let Some(mut capture) = self.capture.take() {
            let status = capture.stop()?;
            if !status.success() && status.code() != Some(255) {
                return Err(Error::Failed(capture.label.clone(), status));
            }
        }
        Ok(())
    }

    pub fn snapshot(&mut self, output: &Path) -> Result<()> {
        let (size, display) = self.display_info()?;
        let mut command = self.command("ffmpeg");
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "x11grab",
                "-draw_mouse",
                "0",
                "-video_size",
                &size.to_string(),
                "-i",
                display,
                "-frames:v",
                "1",
                "-y",
            ])
            .arg(output);
        self.exec(&mut command)
    }

    pub fn poster(&mut self, input: &Path, offset: Duration, output: &Path) -> Result<()> {
        self.exec(
            Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-ss",
                    &offset.as_secs_f64().to_string(),
                ])
                .arg("-i")
                .arg(input)
                .args(["-frames:v", "1", "-y"])
                .arg(output),
        )
    }

    pub fn gif(&mut self, input: &Path, output: &Path, width: u32, fps: u32) -> Result<()> {
        if width == 0 || fps == 0 || fps > 100 {
            return Err(Error::Invalid(
                "GIF width must be positive; FPS must be 1..=100".into(),
            ));
        }
        self.exec(Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(input).arg("-filter_complex")
            .arg(format!("fps={fps},scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse=dither=bayer"))
            .args(["-loop", "0", "-y"]).arg(output))
    }

    pub fn key(&mut self, chord: &str, pause: Duration) -> Result<()> {
        self.exec(Command::new("xdotool").args(["key", "--clearmodifiers", chord]))?;
        self.sleep(pause)
    }

    pub fn type_text(&mut self, text: &str, delay: Duration) -> Result<()> {
        self.exec(Command::new("xdotool").args([
            "type",
            "--delay",
            &delay.as_millis().to_string(),
            "--",
            text,
        ]))
    }

    fn close(&mut self) -> Result<()> {
        let mut result = self.stop_capture();
        for mut command in self.cleanup.drain(..).rev() {
            let cleanup = (|| {
                let mut process = Process::spawn(&mut command, "-TERM")?;
                let deadline = Instant::now() + STOP_TIMEOUT;
                loop {
                    if let Some(status) = process.child.try_wait()? {
                        process.stopped = true;
                        return if status.success() {
                            Ok(())
                        } else {
                            Err(Error::Failed(process.label.clone(), status))
                        };
                    }
                    if Instant::now() >= deadline {
                        return Err(Error::Timeout(process.label.clone()));
                    }
                    thread::sleep(POLL);
                }
            })();
            if let Err(error) = cleanup {
                eprintln!("Kinestra consumer cleanup: {error}");
                result = result.and(Err(error));
            }
        }
        for process in [&mut self.app, &mut self.compositor, &mut self.xvfb] {
            if let Some(mut process) = process.take() {
                result = result.and(process.stop().map(|_| ()));
            }
        }
        result
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            self.success = false;
            eprintln!("Kinestra cleanup: {error}");
        }
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
        if self.success && !thread::panicking() {
            if let Err(error) = fs::remove_dir_all(&self.work) {
                eprintln!("Kinestra cleanup: {error}");
            }
        } else {
            eprintln!("Kinestra logs: {}", self.work.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_reject_invalid_h264_sizes() {
        for (w, h) in [(0, 180), (320, 0), (319, 180), (320, 179)] {
            assert!(Size::new(w, h).is_err());
        }
        assert_eq!(Size::new(320, 180).unwrap().to_string(), "320x180");
    }
}
