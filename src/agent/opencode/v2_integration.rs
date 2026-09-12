//! Official OpenCode V2 CLI-only integration. No service/database mutation.
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

const SPEC: &str = "./luvus-v2";
const TUI: &str = include_str!("luvus-v2.js");
const PACKAGE: &str = r#"{"name":"luvus-v2","private":true,"type":"module","exports":{"./tui":"./tui.js"},"peerDependencies":{"solid-js":">=1.9.0"}}"#;

fn directory() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::integration::home().join(".config"))
        .join("opencode")
}

pub(super) fn install() -> Result<()> {
    let directory = directory();
    let config = directory.join("cli.json");
    let contents = match fs::read_to_string(&config) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}\n".into(),
        Err(error) => return Err(error.into()),
    };
    // Validate before writing assets; preserve formatting, comments and plugins.
    let updated = super::config::enable_for(&contents, "plugins", SPEC, SPEC)?;
    let package = directory.join("luvus-v2");
    let assets = [("package.json", PACKAGE), ("tui.js", TUI)];
    let mut previous = Vec::new();
    for (name, expected) in assets {
        let path = package.join(name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => {
                anyhow::ensure!(
                    bytes == expected.as_bytes(),
                    "preserving modified integration asset {}",
                    path.display()
                );
                Some(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        previous.push((path, bytes));
    }
    fs::create_dir_all(&package)?;
    let result = (|| {
        for (name, contents) in assets {
            crate::integration::write_bytes_atomic(&package.join(name), contents.as_bytes())?;
        }
        crate::integration::write_bytes_atomic(&config, updated.as_bytes())
    })();
    if result.is_err() {
        for (path, bytes) in previous {
            if let Some(bytes) = bytes {
                let _ = crate::integration::write_bytes_atomic(&path, &bytes);
            } else {
                let _ = fs::remove_file(path);
            }
        }
        let _ = fs::remove_dir(package);
    }
    result
}

pub(super) fn uninstall() -> Result<()> {
    let directory = directory();
    let config = directory.join("cli.json");
    match fs::read_to_string(&config) {
        Ok(contents) => {
            let updated = super::config::disable_for(&contents, "plugins", SPEC, SPEC)?;
            if updated != contents {
                crate::integration::write_bytes_atomic(&config, updated.as_bytes())?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // Leave user-edited assets and every unrelated neighbor intact.
    let package = directory.join("luvus-v2");
    for (name, expected) in [("package.json", PACKAGE), ("tui.js", TUI)] {
        let path = package.join(name);
        if fs::read_to_string(&path).is_ok_and(|contents| contents == expected) {
            fs::remove_file(path)?;
        }
    }
    let _ = fs::remove_dir(package);
    Ok(())
}

pub(super) fn is_installed() -> bool {
    let directory = directory();
    directory.join("luvus-v2/tui.js").is_file()
        && directory.join("luvus-v2/package.json").is_file()
        && fs::read_to_string(directory.join("cli.json"))
            .is_ok_and(|contents| super::config::enabled_for(&contents, "plugins", SPEC))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    const HARNESS: &str = include_str!("v2_transport_harness.mjs");
    // The plugin's only runtime dependency in OpenCode V2. Re-running the
    // collected effects is how the harness stands in for a reactive read.
    #[cfg(unix)]
    const SOLID_STUB: &str = "globalThis.__luvusEffects = []\n\
        export function createEffect(effect) { globalThis.__luvusEffects.push(effect); effect() }\n";

    /// Drive the installed asset under `node` and return the requests the fake
    /// server saw. Returns `None` when `node` is unavailable, which is the only
    /// thing that can make this unavailable on a developer machine.
    #[cfg(unix)]
    fn run_harness(scenario: &str, failures: u32, route: &str) -> Option<Vec<serde_json::Value>> {
        use std::process::Command;
        if !Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return None;
        }
        let sandbox = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-state/v2-transport")
            .join(format!("{scenario}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&sandbox);
        let solid = sandbox.join("node_modules/solid-js");
        fs::create_dir_all(&solid).unwrap();
        fs::write(sandbox.join("package.json"), r#"{"type":"module"}"#).unwrap();
        fs::write(sandbox.join("tui.js"), TUI).unwrap();
        fs::write(sandbox.join("harness.mjs"), HARNESS).unwrap();
        fs::write(
            solid.join("package.json"),
            r#"{"name":"solid-js","version":"1.9.0","type":"module","main":"index.js"}"#,
        )
        .unwrap();
        fs::write(solid.join("index.js"), SOLID_STUB).unwrap();

        // Keep the socket short: a Unix path has a hard length limit.
        let socket =
            std::env::temp_dir().join(format!("lv2-{}-{scenario}.sock", std::process::id()));
        let _ = fs::remove_file(&socket);
        let output = Command::new("node")
            .arg("harness.mjs")
            .arg(scenario)
            .current_dir(&sandbox)
            .env("LUVUS_ENV", "1")
            .env("LUVUS_PANE_ID", "7")
            .env("LUVUS_API_ADDRESS", &socket)
            .env("HARNESS_FAILURES", failures.to_string())
            .env("HARNESS_ROUTE", route)
            .output()
            .unwrap();
        let _ = fs::remove_file(&socket);
        let _ = fs::remove_dir_all(&sandbox);
        assert!(
            output.status.success(),
            "harness failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let transcript = String::from_utf8(output.stdout).unwrap();
        let last = transcript.lines().next_back().expect("harness transcript");
        Some(serde_json::from_str(last).unwrap())
    }

    #[cfg(unix)]
    fn request(value: &serde_json::Value) -> (String, String) {
        (
            value["method"].as_str().unwrap_or_default().to_string(),
            value["params"]["session_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    }

    /// A socket error or timeout must not drop the selected session until the
    /// next reactive event: the plugin keeps the report and retries it.
    #[cfg(unix)]
    #[test]
    fn v2_plugin_retries_a_report_after_a_transient_transport_failure() {
        let Some(requests) = run_harness("retry", 1, "ses_a") else {
            return; // no node on this machine
        };
        assert_eq!(requests.len(), 2, "expected a retry: {requests:?}");
        for value in &requests {
            assert_eq!(
                request(value),
                ("pane.report_session".into(), "ses_a".into())
            );
        }
        assert_eq!(requests[0]["params"]["pane"], "7");
    }

    /// While a report for one session is failing, the user can move to another.
    /// The newer report takes the older one's place -- session A is never
    /// retried -- and it keeps a retry budget of its own.
    #[cfg(unix)]
    #[test]
    fn v2_plugin_prefers_a_newer_report_over_an_older_retry() {
        let Some(requests) = run_harness("newer-report-wins", 2, "ses_a") else {
            return;
        };
        let seen = requests.iter().map(request).collect::<Vec<_>>();
        let report = |session: &str| ("pane.report_session".to_string(), session.to_string());
        assert_eq!(
            seen,
            vec![report("ses_a"), report("ses_b"), report("ses_b")],
            "unexpected transcript: {requests:?}"
        );
    }

    /// Leaving the reported root session hands that exact binding back.
    #[cfg(unix)]
    #[test]
    fn v2_plugin_releases_the_session_it_reported_after_navigation() {
        let Some(requests) = run_harness("release", 0, "ses_a") else {
            return;
        };
        assert_eq!(requests.len(), 2, "unexpected transcript: {requests:?}");
        assert_eq!(
            request(&requests[0]),
            ("pane.report_session".into(), "ses_a".into())
        );
        assert_eq!(
            request(&requests[1]),
            ("pane.release_session".into(), "ses_a".into()),
            "the release names the exact binding this pane reported"
        );
        assert_eq!(requests[1]["params"]["pane"], "7");
        assert_eq!(requests[1]["params"]["agent"], "opencode");
    }

    /// A child session, or a root session belonging to another directory, is
    /// not this pane's conversation and is never reported. A root session in
    /// this directory is, whichever shape the CLI publishes its path in.
    #[cfg(unix)]
    #[test]
    fn v2_plugin_ignores_child_and_wrong_directory_sessions() {
        let Some(requests) = run_harness("ignored", 0, "ses_child") else {
            return;
        };
        let seen = requests.iter().map(request).collect::<Vec<_>>();
        let report = |session: &str| ("pane.report_session".to_string(), session.to_string());
        assert_eq!(
            seen,
            vec![report("ses_legacy"), report("ses_a")],
            "unexpected transcript: {requests:?}"
        );
    }

    #[test]
    fn v2_cli_install_is_surgical_idempotent_and_rejects_invalid_config() {
        let _env = crate::persist::test_env("opencode-v2-cli");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-state/v2-cli");
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &root);
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let config = directory.join("cli.json");
        fs::write(
            &config,
            "{\n // keep\n \"plugins\": [\"other\",],\n \"theme\": \"custom\"\n}\n",
        )
        .unwrap();
        install().unwrap();
        let installed = fs::read_to_string(&config).unwrap();
        install().unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), installed);
        assert!(is_installed());
        assert!(installed.contains("// keep"));
        assert!(installed.contains("\"plugins\""));
        assert!(!directory.join("opencode.json").exists());
        fs::write(directory.join("luvus-v2/user.txt"), "keep").unwrap();
        uninstall().unwrap();
        assert!(!is_installed());
        assert!(directory.join("luvus-v2/user.txt").is_file());
        let remaining = fs::read_to_string(&config).unwrap();
        assert!(remaining.contains("other") && remaining.contains("// keep"));
        fs::write(&config, "{\"plugins\":\"invalid\"}").unwrap();
        assert!(install().is_err());
        assert!(!directory.join("luvus-v2/tui.js").exists());
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        fs::remove_dir_all(root).unwrap();
    }
}
