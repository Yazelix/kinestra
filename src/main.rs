use kinestra::{Error, Recorder, Result, Size};
use std::{
    env,
    path::Path,
    process::{Command, ExitCode},
    time::Duration,
};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.is_empty() || args[0] == "--help" {
        println!(
            "Kinestra: typed terminal recordings\nUsage: kinestra WIDTH HEIGHT SECONDS OUTPUT.mp4 CLASS COMMAND [ARGS...]\nFor multi-step demos, compile a Rust recipe with lib.x86_64-linux.mkRecorder."
        );
        return ExitCode::SUCCESS;
    }
    if args.len() == 1 && args[0] == "--version" {
        println!("kinestra {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    kinestra::run(|recorder: &mut Recorder| -> Result<()> {
        if args.len() < 6 {
            return Err(Error::Invalid(
                "expected WIDTH HEIGHT SECONDS OUTPUT.mp4 CLASS COMMAND [ARGS...]".into(),
            ));
        }
        let number = |i: usize| {
            args[i]
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| Error::Invalid(format!("invalid unsigned integer: {:?}", args[i])))
        };
        let size = Size::new(number(0)?, number(1)?)?;
        let duration = Duration::from_secs(number(2)? as u64);
        if duration.is_zero() {
            return Err(Error::Invalid("duration must be positive".into()));
        }
        let class = args[4]
            .to_str()
            .ok_or_else(|| Error::Invalid("class must be UTF-8".into()))?;
        recorder.display(size, None)?;
        recorder.launch(class, Command::new(&args[5]).args(&args[6..]))?;
        recorder.record(Path::new(&args[3]), |r| r.sleep(duration))
    })
}
