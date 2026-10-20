use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub fn polkit_rule_content(username: &str) -> String {
    format!(
        r#"polkit.addRule(function(action, subject) {{
    if (action.id == "org.manjaro.pamac.commit" && subject.user == "{}") {{
        return polkit.Result.YES;
    }}
}});
"#,
        username
    )
}

pub struct PrivilegeSession {
    pub username: String,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

const POLKIT_RULE_PATH: &str = "/etc/polkit-1/rules.d/90-mdpkg-session.rules";

extern "C" fn cleanup_polkit_rule() {
    let _ = Command::new("sudo")
        .args(["rm", "-f", POLKIT_RULE_PATH])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

extern "C" fn sigint_handler(sig: libc::c_int) {
    cleanup_polkit_rule();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

impl PrivilegeSession {
    pub fn acquire() -> Result<Self, anyhow::Error> {
        // Register signal and exit handlers
        unsafe {
            libc::signal(libc::SIGINT, sigint_handler as *const () as usize);
            libc::signal(libc::SIGTERM, sigint_handler as *const () as usize);
            libc::atexit(cleanup_polkit_rule);
        }

        // 1. Initial sudo authentication prompt
        let status = Command::new("sudo").arg("-v").status()?;
        if !status.success() {
            anyhow::bail!("Failed to acquire sudo credentials");
        }

        // 2. Determine username
        let output = Command::new("id").arg("-un").output()?;
        let username = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if username.is_empty() {
            anyhow::bail!("Failed to determine current username");
        }

        // 3. Write Polkit rule
        let rule = polkit_rule_content(&username);
        let mut child = Command::new("sudo")
            .args(["tee", POLKIT_RULE_PATH])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(rule.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            eprintln!("Warning: failed to write polkit session rule at {}", POLKIT_RULE_PATH);
        }

        // 4. Spawn background keep-alive thread
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&stop_flag);

        let handle = thread::spawn(move || {
            while !flag_clone.load(Ordering::Relaxed) {
                for _ in 0..500 {
                    if flag_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                let _ = Command::new("sudo")
                    .args(["-n", "-v"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        });

        Ok(Self {
            username,
            stop_flag,
            handle: Some(handle),
        })
    }
}

impl Drop for PrivilegeSession {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        // Remove polkit rule
        let _ = Command::new("sudo")
            .args(["rm", "-f", POLKIT_RULE_PATH])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polkit_rule_content() {
        let rule = polkit_rule_content("testuser");
        assert!(rule.contains("org.manjaro.pamac.commit"));
        assert!(rule.contains("subject.user == \"testuser\""));
        assert!(rule.contains("polkit.Result.YES"));
    }
}
