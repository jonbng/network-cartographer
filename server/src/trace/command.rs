use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

pub struct CommandOutput {
    pub output: String,
    pub warning: Option<String>,
}

#[derive(Clone, Copy)]
enum OutputStream {
    Stdout,
    Stderr,
}

/// Run an external command, forwarding complete output lines while the child is alive.
/// The two pipes are drained concurrently so a noisy command cannot deadlock on a full pipe.
pub fn run_command_timeout<F>(
    program: &str,
    args: &[String],
    timeout: Duration,
    mut on_line: F,
) -> Result<CommandOutput, String>
where
    F: FnMut(&str),
{
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

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (line_tx, line_rx) = mpsc::channel::<(OutputStream, String)>();
    let stdout_reader = spawn_reader(stdout, OutputStream::Stdout, line_tx.clone());
    let stderr_reader = spawn_reader(stderr, OutputStream::Stderr, line_tx);

    let start = std::time::Instant::now();
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    let mut timed_out = false;
    loop {
        while let Ok((stream, line)) = line_rx.try_recv() {
            record_line(
                stream,
                line,
                &mut stdout_text,
                &mut stderr_text,
                &mut on_line,
            );
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    let _ = child.wait();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    while let Ok((stream, line)) = line_rx.try_recv() {
        record_line(
            stream,
            line,
            &mut stdout_text,
            &mut stderr_text,
            &mut on_line,
        );
    }

    // Bad flags / missing tool: fail this attempt so the engine can try the next method.
    if looks_like_hard_cli_error(&stderr_text) && !looks_like_hop_table(&stderr_text) {
        return Err(stderr_text.trim().to_string());
    }
    if looks_like_hard_cli_error(&stdout_text) && !looks_like_hop_table(&stdout_text) {
        return Err(stdout_text.trim().to_string());
    }

    if looks_like_hop_table(&stderr_text) {
        if looks_like_hop_table(&stdout_text) {
            stdout_text.push_str(&stderr_text);
        } else {
            stdout_text = stderr_text;
        }
    } else if stdout_text.trim().is_empty() && !stderr_text.trim().is_empty() {
        // Tools sometimes print the hop table to stderr, or only errors.
        let lower = stderr_text.to_ascii_lowercase();
        if lower.contains("cannot")
            || lower.contains("permission")
            || lower.contains("not found")
            || lower.contains("usage:")
        {
            // Prefer error if it looks like a hard failure and stdout empty
            if !looks_like_hop_table(&stderr_text) {
                return Err(stderr_text.trim().to_string());
            }
        }
        stdout_text = stderr_text;
    }

    if stdout_text.trim().is_empty() {
        if timed_out {
            return Err(format!("timed out after {}s", timeout.as_secs()));
        }
        return Err("empty output".into());
    }

    Ok(CommandOutput {
        output: stdout_text,
        warning: timed_out.then(|| format!("timed out after {}s", timeout.as_secs())),
    })
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: OutputStream,
    tx: mpsc::Sender<(OutputStream, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if tx.send((stream, line)).is_err() {
                break;
            }
        }
    })
}

fn record_line<F: FnMut(&str)>(
    stream: OutputStream,
    line: String,
    stdout: &mut String,
    stderr: &mut String,
    on_line: &mut F,
) {
    let target = match stream {
        OutputStream::Stdout => stdout,
        OutputStream::Stderr => stderr,
    };
    target.push_str(&line);
    target.push('\n');
    on_line(&line);
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn forwards_lines_before_process_exit() {
        let started = Instant::now();
        let mut arrivals = Vec::new();
        let args = vec![
            "-c".into(),
            "printf '1  192.0.2.1  1 ms\\n'; sleep 0.2; printf '2  192.0.2.2  2 ms\\n'".into(),
        ];

        let output = run_command_timeout("sh", &args, Duration::from_secs(2), |_| {
            arrivals.push(started.elapsed());
        })
        .expect("command should complete");

        assert_eq!(arrivals.len(), 2);
        assert!(arrivals[1].saturating_sub(arrivals[0]) >= Duration::from_millis(100));
        assert!(output.output.contains("192.0.2.2"));
        assert!(output.warning.is_none());
    }

    #[test]
    fn uses_hop_table_from_stderr_when_stdout_only_has_a_header() {
        let args = vec![
            "-c".into(),
            "printf 'traceroute header\\n'; printf '1  192.0.2.1  1 ms\\n' >&2".into(),
        ];

        let output = run_command_timeout("sh", &args, Duration::from_secs(2), |_| {})
            .expect("stderr hop table should be accepted");

        assert!(output.output.contains("192.0.2.1"));
    }
}
