use crate::domain::{Status, Task};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
}

pub fn providers(path: Option<&str>) -> Result<BTreeMap<String, Provider>> {
    let demo = Provider {
        name: "demo".into(),
        program: std::env::current_exe()?.to_string_lossy().into(),
        args: vec!["--demo-worker".into()],
        cwd: std::env::current_dir()?.to_string_lossy().into(),
    };
    let mut result = BTreeMap::from([("demo".into(), demo)]);
    if let Some(path) = path {
        for mut provider in serde_json::from_str::<Vec<Provider>>(&std::fs::read_to_string(path)?)?
        {
            anyhow::ensure!(
                !provider.name.is_empty()
                    && provider.name.len() <= 64
                    && !result.contains_key(&provider.name),
                "invalid/duplicate provider name"
            );
            anyhow::ensure!(
                Path::new(&provider.program).is_absolute()
                    && Path::new(&provider.cwd).is_absolute(),
                "provider paths must be absolute"
            );
            provider.program = std::fs::canonicalize(&provider.program)?
                .to_string_lossy()
                .into();
            provider.cwd = std::fs::canonicalize(&provider.cwd)?
                .to_string_lossy()
                .into();
            anyhow::ensure!(
                Path::new(&provider.program).is_file() && Path::new(&provider.cwd).is_dir(),
                "invalid provider paths"
            );
            result.insert(provider.name.clone(), provider);
        }
    }
    Ok(result)
}

#[derive(Debug)]
pub struct Outcome {
    pub status: Status,
    pub output: String,
    pub detail: String,
}

/// Retain only the first 8 KiB per stream, but drain the entire pipe to avoid deadlocks.
async fn capture(mut stream: impl AsyncRead + Unpin) -> String {
    let mut kept = Vec::new();
    let mut buffer = [0; 4096];
    let mut truncated = false;
    while let Ok(size) = stream.read(&mut buffer).await {
        if size == 0 {
            break;
        }
        let count = size.min(8192_usize.saturating_sub(kept.len()));
        kept.extend_from_slice(&buffer[..count]);
        truncated |= count < size;
    }
    let mut result = String::from_utf8_lossy(&kept).into_owned();
    if truncated {
        result.push_str("\n[output truncated after 8192 bytes]");
    }
    result
}

pub async fn run(task: Task, provider: Provider, cancel: CancellationToken) -> Outcome {
    match run_inner(&task, provider, cancel).await {
        Ok(outcome) => outcome,
        Err(error) => Outcome {
            status: Status::Failed,
            output: String::new(),
            detail: format!("process error: {error:#}"),
        },
    }
}

async fn run_inner(task: &Task, provider: Provider, cancel: CancellationToken) -> Result<Outcome> {
    let mut command = Command::new(provider.program);
    command
        .args(provider.args)
        .current_dir(provider.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // A task owns its process group; cancellation also terminates ordinary descendants.
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().context("spawn provider")?;
    let pid = child.id();
    let stdout = tokio::spawn(capture(child.stdout.take().unwrap()));
    let stderr = tokio::spawn(capture(child.stderr.take().unwrap()));
    let mut stdin = child.stdin.take().unwrap();
    let mut input = serde_json::to_vec(&serde_json::json!({"schema_version": 1, "task": task}))?;
    input.push(b'\n');
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
    });
    let (status, detail) = tokio::select! {
        biased;
        _ = cancel.cancelled() => (Status::Cancelled, "cancelled by user or shutdown".into()),
        _ = tokio::time::sleep(Duration::from_millis(task.spec.timeout_ms)) => (Status::TimedOut, "execution timeout".into()),
        result = child.wait() => match result {
            Ok(exit) if exit.success() => (Status::Succeeded, "process exited successfully".into()),
            Ok(exit) => (Status::Failed, format!("process exited: {exit}")),
            Err(error) => (Status::Failed, format!("wait failed: {error}")),
        }
    };
    #[cfg(unix)]
    if let Some(pid) = pid {
        // SAFETY: negative PID targets only the process group created above.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    writer.abort();
    async fn finish(mut handle: tokio::task::JoinHandle<String>) -> String {
        match tokio::time::timeout(Duration::from_millis(500), &mut handle).await {
            Ok(Ok(output)) => output,
            _ => {
                handle.abort();
                "[output stream did not close]".into()
            }
        }
    }
    let (out, err) = tokio::join!(finish(stdout), finish(stderr));
    Ok(Outcome {
        status,
        output: format!("stdout:\n{out}\nstderr:\n{err}"),
        detail,
    })
}

pub fn demo_worker() -> Result<()> {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin().take(64000).read_to_string(&mut input)?;
    let envelope: serde_json::Value = serde_json::from_str(&input)?;
    let task: Task = serde_json::from_value(envelope["task"].clone())?;
    println!("Demo agent started: {}", task.spec.title);
    std::thread::sleep(Duration::from_millis(task.spec.demo_duration_ms));
    println!("Demo result for prompt: {}", task.spec.prompt);
    if task.spec.demo_fail {
        anyhow::bail!("requested demo failure");
    }
    println!("Completed task {}", task.id);
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn spec(timeout: u64) -> Task {
        Task::new(
            serde_json::from_value(serde_json::json!({"title":"test", "timeout_ms":timeout}))
                .unwrap(),
        )
    }
    fn shell(script: &str) -> Provider {
        Provider {
            name: "test".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: "/tmp".into(),
        }
    }
    #[tokio::test]
    async fn captures_exit_status() {
        let result = run(
            spec(1000),
            shell("echo hello; echo error >&2; exit 7"),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.status, Status::Failed);
        assert!(result.output.contains("hello") && result.output.contains("error"));
    }
    #[tokio::test]
    async fn timeout_terminates_descendants_and_closes_pipes() {
        let started = std::time::Instant::now();
        let result = run(spec(50), shell("sleep 30 & wait"), CancellationToken::new()).await;
        assert_eq!(result.status, Status::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!result.output.contains("stream did not close"));
    }
    #[tokio::test]
    async fn cancellation_reaps_child() {
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            signal.cancel();
        });
        assert_eq!(
            run(spec(30000), shell("sleep 30"), cancel).await.status,
            Status::Cancelled
        );
    }
    #[tokio::test]
    async fn chatty_worker_does_not_block_or_grow_output() {
        let result = run(
            spec(3000),
            shell("yes x | head -c 100000"),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.status, Status::Succeeded);
        assert!(result.output.len() < 9000);
        assert!(result.output.contains("truncated"));
    }
}
