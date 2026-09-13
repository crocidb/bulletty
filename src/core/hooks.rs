use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use tracing::{error, info};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct AppHooks {
    pub before_tui: Option<String>,
    pub after_tui: Option<String>,
    pub open_link: Option<String>,
}

impl AppHooks {
    pub fn run_before_tui(&self) {
        if let Some(cmd) = &self.before_tui {
            Self::run_shell_command(cmd, "before_tui");
        }
    }

    pub fn run_after_tui(&self) {
        if let Some(cmd) = &self.after_tui {
            Self::run_shell_command(cmd, "after_tui");
        }
    }

    pub fn run_open_link(&self, url: &str) -> bool {
        let Some(ref template) = self.open_link else {
            return false;
        };
        let cmd = template.replace("%s", url);
        if cmd.trim().is_empty() {
            error!("open_link hook is empty after substitution");
            return true;
        }

        info!("Running hook 'open_link': {}", cmd);
        match Self::shell_command(&cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !stdout.trim().is_empty() {
                    info!("open_link stdout: {}", stdout.trim());
                }

                if !stderr.trim().is_empty() {
                    error!("open_link stderr: {}", stderr.trim());
                }

                if !output.status.success() {
                    error!("Hook 'open_link' exited with status: {}", output.status);
                }
            }
            Err(e) => error!("Failed to execute hook 'open_link': {}", e),
        }
        true
    }

    fn shell_command(cmd: &str) -> Command {
        #[cfg(target_os = "windows")]
        {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut c = Command::new("sh");
            c.arg("-c").arg(cmd);
            c
        }
    }

    fn run_shell_command(cmd: &str, hook_name: &str) {
        info!("Running hook '{}': {}", hook_name, cmd);
        match Self::shell_command(cmd).status() {
            Ok(s) if s.success() => {}
            Ok(s) => error!("Hook '{}' exited with status: {}", hook_name, s),
            Err(e) => error!("Failed to execute hook '{}': {}", hook_name, e),
        }
    }
}
