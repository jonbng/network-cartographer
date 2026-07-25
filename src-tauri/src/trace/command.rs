use std::process::{Command, Stdio};
use std::time::Duration;

/// Run an external command, kill it if it exceeds `timeout`, return stdout (or stderr fallback).
pub fn run_command_timeout(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "not found (install traceroute?)".into()
            } else {
                e.to_string()
            }
        })?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    use std::io::Read;
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    let _ = child.wait();

    // Bad flags / missing tool: fail this attempt so the engine can try the next method.
    if looks_like_hard_cli_error(&stderr) && !looks_like_hop_table(&stderr) {
        return Err(stderr.trim().to_string());
    }
    if looks_like_hard_cli_error(&stdout) && !looks_like_hop_table(&stdout) {
        return Err(stdout.trim().to_string());
    }

    if stdout.trim().is_empty() && !stderr.trim().is_empty() {
        // Tools sometimes print the hop table to stderr, or only errors.
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("cannot")
            || lower.contains("permission")
            || lower.contains("not found")
            || lower.contains("usage:")
        {
            // Prefer error if it looks like a hard failure and stdout empty
            if !looks_like_hop_table(&stderr) {
                return Err(stderr.trim().to_string());
            }
        }
        stdout = stderr;
    }

    if stdout.trim().is_empty() {
        return Err("empty output".into());
    }

    Ok(stdout)
}

fn looks_like_hard_cli_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("illegal option")
        || lower.contains("invalid option")
        || lower.contains("unknown option")
        || lower.contains("unrecognized option")
        || lower.contains("usage:")
        || lower.contains("not found")
}

fn looks_like_hop_table(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim();
        t.chars().next().is_some_and(|c| c.is_ascii_digit())
    })
}
