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
    r.launch(
        "kinestra-probe",
        Command::new("foot").args(["-a", "kinestra-probe", "cat"]),
    )?;
    let video = work.join("wayland.mp4");
    r.record(&video, |r| {
        r.type_text("Wayland capture", Duration::from_millis(5))?;
        r.key("Return", Duration::from_millis(400))
    })?;
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
