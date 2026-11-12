//! Bounded output from fixed tools, never interpreted by a shell.
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Output {
    pub code: i32,
    pub text: String,
    pub error: String,
    truncated: bool,
}
fn drain(mut pipe: impl Read) -> (String, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buf = [0; 4096];
    while let Ok(n) = pipe.read(&mut buf) {
        if n == 0 {
            break;
        }
        let remaining = 65536_usize.saturating_sub(kept.len());
        truncated |= n > remaining;
        kept.extend_from_slice(&buf[..n.min(remaining)]);
    }
    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}
pub fn run(program: &str, args: &[&str], timeout: Option<Duration>) -> Result<Output, String> {
    let mut child = Command::new(program)
        .process_group(0)
        .args(args)
        .env("LC_ALL", "C")
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let stdout = child.stdout.take().ok_or("Output unavailable")?;
    let stderr = child.stderr.take().ok_or("Output unavailable")?;
    let out = thread::spawn(move || drain(stdout));
    let err = thread::spawn(move || drain(stderr));
    let began = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.code().unwrap_or(-1)),
            Ok(None) if timeout.is_none_or(|limit| began.elapsed() < limit) => thread::sleep(Duration::from_millis(25)),
            _ => {
                // Child has its own process group. Kill read-only query descendants
                // as well, so an inherited pipe cannot keep the reader blocked.
                // kill accepts an integer group ID; no pointers cross this boundary.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let (text, out_truncated) = out.join().unwrap_or_default();
    let (error, err_truncated) = err.join().unwrap_or_default();
    Ok(Output { code: code.ok_or("The command timed out.")?, text, error, truncated: out_truncated || err_truncated })
}
pub fn query(program: &str, args: &[&str]) -> Result<Output, String> {
    let output = run(program, args, Some(Duration::from_secs(30)))?;
    if output.truncated {
        Err("The command output was too large to verify. Use your software manager.".into())
    } else {
        Ok(output)
    }
}
pub fn success(output: Output) -> Result<String, String> {
    if output.code == 0 {
        Ok(output.text)
    } else if !output.error.trim().is_empty() {
        Err(output.error.trim().into())
    } else {
        Err(format!("The command failed ({}). {}", output.code, output.text.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drains_large_outputs_and_times_out_queries() {
        let output = run("/usr/bin/seq", &["1", "100000"], Some(Duration::from_secs(3))).unwrap();
        assert_eq!(output.code, 0);
        assert_eq!(output.text.len(), 65536);
        assert!(output.truncated);
        assert!(query("/usr/bin/seq", &["1", "100000"]).is_err());
        let began = Instant::now();
        assert!(run("/usr/bin/sleep", &["3"], Some(Duration::from_millis(30))).is_err());
        assert!(began.elapsed() < Duration::from_secs(2));
    }
}
