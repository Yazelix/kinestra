use kinestra::{Error, Recorder, Result, Size};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    time::Duration,
};

fn recipe(r: &mut Recorder) -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    r.display(Size::new(320, 180)?, None)?;
    if let Some(action) = args.first() {
        let parent = PathBuf::from(&args[1]);
        fs::write(
            parent.join("child-work"),
            r.work().as_os_str().as_encoded_bytes(),
        )?;
        let display = r
            .command("true")
            .get_envs()
            .find(|(key, _)| *key == "DISPLAY")
            .unwrap()
            .1
            .unwrap()
            .to_owned();
        fs::write(parent.join("display"), display.as_encoded_bytes())?;
        let mut cleanup = Command::new("touch");
        cleanup.arg(parent.join("cleanup-ran"));
        r.on_exit(cleanup);
        return r.record(&parent.join("interrupted.mp4"), |r| {
            if action == "interrupt" || action == "interrupt-int" {
                let signal = if action == "interrupt" {
                    "-TERM"
                } else {
                    "-INT"
                };
                r.exec(Command::new("kill").args([signal, &std::process::id().to_string()]))?;
                r.sleep(Duration::from_secs(1))
            } else if action == "command-fail" {
                r.exec(Command::new(env::current_exe()?).arg("--exit42"))
            } else if action == "stubborn" {
                r.exec(
                    Command::new(env::current_exe()?)
                        .arg("--stubborn")
                        .arg(std::process::id().to_string())
                        .arg(parent.join("stubborn-pid")),
                )
            } else if action == "app-exit" {
                r.launch("missing-window", &mut Command::new("true"))
            } else {
                Err(Error::Invalid("intentional scenario failure".into()))
            }
        });
    }

    let work = r.work().to_path_buf();
    for name in ["first", "second"] {
        r.record(&work.join(format!("{name}.mp4")), |r| {
            r.sleep(Duration::from_millis(500))
        })?;
    }
    r.poster(
        &work.join("first.mp4"),
        Duration::ZERO,
        &work.join("poster.png"),
    )?;
    r.gif(&work.join("first.mp4"), &work.join("test.gif"), 160, 10)?;
    assert_eq!(
        r.output(
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height",
                    "-of",
                    "csv=p=0"
                ])
                .arg(work.join("test.gif"))
        )?
        .trim(),
        "160,90"
    );
    assert!(fs::metadata(work.join("poster.png"))?.len() > 0);
    for (action, expected) in [
        ("fail", 1),
        ("interrupt", 143),
        ("interrupt-int", 130),
        ("command-fail", 42),
        ("app-exit", 1),
        ("stubborn", 143),
    ] {
        let status = Command::new(env::current_exe()?)
            .arg(action)
            .arg(&work)
            .status()?;
        assert_eq!(status.code(), Some(expected));
        if action == "stubborn" {
            assert!(
                !Command::new("kill")
                    .args(["-0", &fs::read_to_string(work.join("stubborn-pid"))?])
                    .stderr(Stdio::null())
                    .status()?
                    .success()
            );
        }
        assert!(work.join("cleanup-ran").exists());
        fs::remove_file(work.join("cleanup-ran"))?;
        let display = fs::read_to_string(work.join("display"))?;
        assert!(
            !Command::new("xdotool")
                .arg("getdisplaygeometry")
                .env("DISPLAY", display)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?
                .success()
        );
        r.exec(
            Command::new("ffprobe")
                .args(["-v", "error"])
                .arg(work.join("interrupted.mp4")),
        )?;
        let child_work = PathBuf::from(fs::read_to_string(work.join("child-work"))?);
        assert!(
            child_work
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("kinestra.")
        );
        fs::remove_dir_all(child_work)?;
    }
    // Child cleanup must not disturb the parent's independent display.
    r.snapshot(&work.join("after.png"))
}

fn main() -> ExitCode {
    if env::args_os().nth(1).is_some_and(|arg| arg == "--exit42") {
        return ExitCode::from(42);
    }
    if env::args_os().nth(1).is_some_and(|arg| arg == "--stubborn") {
        let ignored = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, ignored).unwrap();
        fs::write(
            env::args_os().nth(3).unwrap(),
            std::process::id().to_string(),
        )
        .unwrap();
        Command::new("kill")
            .arg("-TERM")
            .arg(env::args_os().nth(2).unwrap())
            .status()
            .unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    kinestra::run(recipe)
}
