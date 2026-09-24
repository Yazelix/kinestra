use kinestra::{Recorder, Result, Size};
use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
    time::Duration,
};

fn record(r: &mut Recorder) -> Result<()> {
    let work = r.work().to_path_buf();
    r.wayland_display(Size::new(320, 180)?)?;
    let typed = work.join("typed.txt");
    let mut terminal = Command::new("foot");
    terminal
        .args([
            "-a",
            "kinestra-probe",
            "bash",
            "-c",
            "IFS= read -r line; printf '%s' \"$line\" > \"$1\"; exec cat",
            "_",
        ])
        .arg(&typed);
    r.launch("kinestra-probe", &mut terminal)?;
    let video = work.join("wayland.mp4");
    r.record(&video, |r| {
        r.type_text("Wayland capture", Duration::from_millis(5))?;
        r.key("Return", Duration::from_millis(400))
    })?;
    for _ in 0..20 {
        if typed.is_file() {
            break;
        }
        r.sleep(Duration::from_millis(100))?;
    }
    assert_eq!(fs::read_to_string(&typed)?, "Wayland capture");
    r.snapshot(&work.join("snapshot.png"))?;
    r.poster(&video, Duration::from_millis(400), &work.join("poster.png"))?;
    for path in ["snapshot.png", "poster.png"] {
        assert!(fs::metadata(work.join(path))?.len() > 0);
    }
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
                .arg(Path::new(&video))
        )?
        .trim(),
        "320,180"
    );
    Ok(())
}

fn main() -> ExitCode {
    kinestra::run(record)
}
