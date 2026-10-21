use std::io::Write;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub fn is_valid_username(username: &str) -> bool {
    if username.is_empty() || username.len() > 64 {
        return false;
    }
    let mut chars = username.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphanumeric() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

pub fn polkit_rule_content(username: &str) -> Result<String, anyhow::Error> {
    if !is_valid_username(username) {
        anyhow::bail!(
            "Invalid username '{}': contains illegal characters or invalid format",
            username
        );
    }
    Ok(format!(
        r#"polkit.addRule(function(action, subject) {{
    if (action.id == "org.manjaro.pamac.commit" && subject.user == "{}") {{
        return polkit.Result.YES;
    }}
}});
"#,
        username
    ))
}

pub struct PrivilegeSession {
    pub username: String,
    stop_sender: Option<std::sync::mpsc::SyncSender<()>>,
    handle: Option<JoinHandle<()>>,
}

const POLKIT_RULE_PATH: &str = "/etc/polkit-1/rules.d/90-mdpkg-session.rules";

pub fn cleanup_stale_polkit_rule() -> Result<bool, anyhow::Error> {
    cleanup_stale_polkit_rule_at(std::path::Path::new(POLKIT_RULE_PATH))
}

pub fn cleanup_stale_polkit_rule_at(path: &std::path::Path) -> Result<bool, anyhow::Error> {
    if path.exists() {
        let status = Command::new("sudo")
            .args(["-n", "rm", "-f", path.to_str().unwrap_or(POLKIT_RULE_PATH)])
            .status()?;
        if !status.success() {
            eprintln!(
                "Warning: failed to remove stale polkit session rule at {}",
                path.display()
            );
            return Ok(false);
        }
        return Ok(true);
    }
    Ok(false)
}

pub(crate) extern "C" fn async_signal_safe_cleanup() {
    unsafe {
        let pid = libc::fork();
        if pid == 0 {
            let sudo_path = b"/usr/bin/sudo\0".as_ptr() as *const libc::c_char;
            let arg_sudo = b"sudo\0".as_ptr() as *const libc::c_char;
            let arg_n = b"-n\0".as_ptr() as *const libc::c_char;
            let arg_rm = b"rm\0".as_ptr() as *const libc::c_char;
            let arg_f = b"-f\0".as_ptr() as *const libc::c_char;
            let arg_rule =
                b"/etc/polkit-1/rules.d/90-mdpkg-session.rules\0".as_ptr() as *const libc::c_char;
            let args = [arg_sudo, arg_n, arg_rm, arg_f, arg_rule, std::ptr::null()];
            let env = [std::ptr::null()];
            libc::execve(sudo_path, args.as_ptr(), env.as_ptr());
            libc::_exit(1);
        } else if pid > 0 {
            let mut status = 0;
            libc::waitpid(pid, &mut status, 0);
        }
    }
}

extern "C" fn cleanup_polkit_rule() {
    let _ = Command::new("sudo")
        .args(["-n", "rm", "-f", POLKIT_RULE_PATH])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

extern "C" fn sigint_handler(sig: libc::c_int) {
    async_signal_safe_cleanup();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

pub fn resolve_current_username() -> Result<String, anyhow::Error> {
    // 1. Primary: query canonical system identity from libc
    unsafe {
        let uid = libc::getuid();
        let pwd = libc::getpwuid(uid);
        if !pwd.is_null() && !(*pwd).pw_name.is_null() {
            let cstr = std::ffi::CStr::from_ptr((*pwd).pw_name);
            if let Ok(s) = cstr.to_str() {
                let trimmed = s.trim();
                if is_valid_username(trimmed) {
                    return Ok(trimmed.to_string());
                }
            }
        }
    }
    // 2. Secondary: environment variables (strictly validated against POSIX username constraints)
    if let Ok(user) = std::env::var("USER") {
        let trimmed = user.trim();
        if is_valid_username(trimmed) {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(user) = std::env::var("LOGNAME") {
        let trimmed = user.trim();
        if is_valid_username(trimmed) {
            return Ok(trimmed.to_string());
        }
    }
    // 3. Fallback: id -un
    let output = Command::new("id").arg("-un").output()?;
    let username = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if is_valid_username(&username) {
        return Ok(username);
    }
    anyhow::bail!("Failed to determine valid current username");
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

        // Clean up any stale rule from an earlier aborted run before creating a new one
        let _ = cleanup_stale_polkit_rule();

        // 2. Determine username
        let username = resolve_current_username()?;

        // 3. Write Polkit rule
        let rule = polkit_rule_content(&username)?;
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
            eprintln!(
                "Warning: failed to write polkit session rule at {}",
                POLKIT_RULE_PATH
            );
        }

        // 4. Spawn background keep-alive thread with channel-based wakeup
        let (stop_sender, stop_receiver) = std::sync::mpsc::sync_channel::<()>(0);

        let handle = thread::spawn(move || loop {
            match stop_receiver.recv_timeout(Duration::from_secs(50)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let _ = Command::new("sudo")
                        .args(["-n", "-v"])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        });

        Ok(Self {
            username,
            stop_sender: Some(stop_sender),
            handle: Some(handle),
        })
    }
}

impl Drop for PrivilegeSession {
    fn drop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        cleanup_polkit_rule();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polkit_rule_content() {
        let rule = polkit_rule_content("testuser").expect("valid username should succeed");
        assert!(rule.contains("org.manjaro.pamac.commit"));
        assert!(rule.contains("subject.user == \"testuser\""));
        assert!(rule.contains("polkit.Result.YES"));

        // Malicious username with injection attempts must be rejected
        assert!(polkit_rule_content("user\"); malicious_js(); //").is_err());
        assert!(polkit_rule_content("user$(id)").is_err());
        assert!(polkit_rule_content("user`id`").is_err());
        assert!(polkit_rule_content("").is_err());
    }

    #[test]
    fn test_async_signal_safe_cleanup_runs_without_panicking() {
        async_signal_safe_cleanup();
    }

    #[test]
    fn test_cleanup_stale_polkit_rule_when_missing() {
        let nonexistent = std::path::Path::new("/tmp/nonexistent-rule-file-test.rules");
        let cleaned = cleanup_stale_polkit_rule_at(nonexistent)
            .expect("should succeed when file does not exist");
        assert!(!cleaned);
    }

    #[test]
    fn test_keepalive_worker_terminates_immediately_on_shutdown() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(0);
        let start = std::time::Instant::now();
        let handle = std::thread::spawn(move || loop {
            match rx.recv_timeout(Duration::from_secs(50)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        });
        let _ = tx.send(());
        let _ = handle.join();
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn test_resolve_current_username() {
        let username = resolve_current_username().expect("should resolve current username");
        assert!(!username.is_empty());
    }
}
