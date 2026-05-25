use anyhow::{Context, Result, anyhow};
use std::process::{Command, Stdio};

pub fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run(program: &str, args: &[&str], sudo: bool) -> Result<String> {
    run_with_stdin(program, args, sudo, None)
}

pub fn run_with_stdin(
    program: &str,
    args: &[&str],
    sudo: bool,
    stdin_data: Option<&[u8]>,
) -> Result<String> {
    let mut command = if sudo {
        let mut c = Command::new("sudo");
        c.arg("-n").arg(program).args(args);
        c
    } else {
        let mut c = Command::new(program);
        c.args(args);
        c
    };

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            if sudo {
                format!("failed to run sudo {program}")
            } else {
                format!("failed to run {program}")
            }
        })?;

    if let Some(data) = stdin_data {
        use std::io::Write;
        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(data)?;
        }
        drop(child.stdin.take());
    }

    let output = child.wait_with_output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        Err(anyhow!(
            "{} failed: {}",
            if sudo {
                format!("sudo {program}")
            } else {
                program.to_string()
            },
            detail
        ))
    }
}

pub fn run_shell(script: &str, sudo: bool) -> Result<String> {
    if sudo {
        run("sh", &["-lc", script], true)
    } else {
        run("sh", &["-lc", script], false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_exists_finds_sh() {
        assert!(command_exists("sh"));
    }

    #[test]
    fn command_exists_rejects_nonexistent() {
        assert!(!command_exists("nonexistent_command_xyz_12345"));
    }

    #[test]
    fn command_exists_not_injectable() {
        assert!(!command_exists("sh; echo injected"));
        assert!(!command_exists("$(echo injected)"));
    }
}
