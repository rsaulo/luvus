use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::super::types::IntegrationOperations;
use crate::integration;

pub(super) const OPERATIONS: IntegrationOperations = IntegrationOperations {
    install,
    uninstall,
    is_installed,
};

const PLUGIN: &str = r#"// luvus opencode integration (docs/23) — reports the session id for native resume.
// Auto-installed at <config>/opencode/plugin/luvus.js by `luvus integration install opencode`.
import { spawn } from "node:child_process"

export const luvus = async () => {
  let last = ""
  const luvusBin = process.env.LUVUS_BIN_PATH || "luvus"
  const report = (id) => {
    if (!id || id === last || !process.env.LUVUS_SOCKET_PATH) return
    last = id
    try {
      spawn(luvusBin, ["pane", "report", "--agent", "opencode", "--session", String(id)], {
        stdio: "ignore",
        detached: true,
      }).unref()
    } catch {}
  }
  return {
    event: async ({ event }) => {
      if (event?.type === "session.created" || event?.type === "session.updated") {
        const p = event.properties || {}
        report(p.info?.id ?? p.sessionID ?? p.id ?? p.session?.id)
      }
    },
  }
}

// V2 also auto-loads this directory, but it requires a `default` export and
// rejects the V1 named-export shape above, which made it log a schema error on
// every start. This inert plugin satisfies that contract without changing V1
// behaviour; the real V2 integration is the package installed beside it.
export default {
  id: "luvus-v1-compat",
  setup() {},
}
"#;

/// OpenCode V2 keeps sessions in a database and loads a different plugin shape,
/// so it gets its own package rather than a second file in `plugin/`.
///
/// The package is a directory with an `index.js` entrypoint: V2 rejects a
/// configured plugin directory that has none ("configured plugin directory has
/// no index entrypoint"), and a bare `.js` file registered in `cli.json` is
/// resolved as an npm package, so it is silently skipped.
///
/// The server half exists only to advertise the TUI half through `tui: true`.
/// The reporting itself has to run in the TUI: `LUVUS_PANE_ID` and
/// `LUVUS_SOCKET_PATH` exist only in the TUI process inside a Luvus pane, and
/// the shared background server sees every client's sessions at once, so it
/// could not tell which session belongs to which pane.
const PLUGIN_MANIFEST: &str = r#"{
  "name": "luvus-opencode",
  "version": "1.0.0",
  "type": "module",
  "private": true,
  "exports": {
    ".": "./index.js",
    "./tui": "./tui.js"
  },
  "oc-plugin": ["server", "tui"]
}
"#;

const PLUGIN_INDEX: &str = r#"// luvus opencode V2 integration — server half.
// Auto-installed by `luvus integration install opencode`; see ./tui.js.
//
// This half does nothing on its own. `tui: true` is what makes OpenCode load
// the package's `./tui` export inside each TUI, which is the only place a
// pane's own session can be identified.
export default {
  id: "luvus",
  tui: true,
  setup() {},
}
"#;

const PLUGIN_TUI: &str = r#"// luvus opencode V2 integration — TUI half.
// Auto-installed by `luvus integration install opencode`.
//
// Binds the pane to the session currently open in it, so Mission Control can
// attribute that session's persisted tokens and cost to this pane and so the
// pane can be resumed natively.
import { spawn } from "node:child_process"

// The open route is read, not subscribed to, so this polls. One second is far
// below human switching speed and costs two property reads per tick.
const POLL_INTERVAL_MS = 1000

export default {
  id: "luvus",
  setup(context) {
    // Outside a Luvus pane there is nothing to report to.
    if (!process.env.LUVUS_SOCKET_PATH) return

    const bin = process.env.LUVUS_BIN_PATH || "luvus"
    let last = ""

    const report = (id) => {
      last = id
      try {
        spawn(bin, ["pane", "report", "--agent", "opencode", "--session", String(id)], {
          stdio: "ignore",
          detached: true,
        }).unref()
      } catch {
        // Best-effort: never take the TUI down over a status report.
      }
    }

    const sync = () => {
      let route
      try {
        route = context.ui.router.current()
      } catch {
        return
      }
      if (route?.type !== "session" || !route.sessionID) return
      // Bind the pane to the root session so a subagent turn does not retarget
      // it. Root resolution is best-effort: the route's own id is already
      // correct, so a cold session tree must not suppress the report.
      let id = route.sessionID
      try {
        id = context.data.session.root(id) || id
      } catch {
        // Tree not synced yet; the route id stands.
      }
      if (id !== last) report(id)
    }

    sync()
    const timer = setInterval(sync, POLL_INTERVAL_MS)
    return () => clearInterval(timer)
  },
}
"#;

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| integration::home().join(".config"))
        .join("opencode")
}

fn plugin_dir() -> PathBuf {
    config_dir().join("plugin")
}

fn plugin_path() -> PathBuf {
    plugin_dir().join("luvus.js")
}

fn package_dir() -> PathBuf {
    config_dir().join("luvus")
}

fn server_config_path() -> PathBuf {
    config_dir().join("opencode.json")
}

/// V2 addresses configured plugins as URLs.
fn package_spec() -> String {
    format!("file://{}", package_dir().display())
}

/// An earlier layout registered a bare file in `cli.json`. It never loaded, so
/// installs clean it up instead of leaving dead configuration behind.
fn legacy_file() -> PathBuf {
    config_dir().join("luvus-v2.js")
}

fn legacy_spec() -> String {
    format!("file://{}", legacy_file().display())
}

enum Config {
    /// No file yet: creating one is safe.
    Missing,
    Object(Value),
    /// Present but not a JSON object we understand, e.g. JSONC with comments.
    /// Never rewrite one of these; that would destroy user configuration.
    Foreign,
}

fn read_config(path: &Path) -> Config {
    let Ok(text) = fs::read_to_string(path) else {
        return Config::Missing;
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) if value.is_object() => Config::Object(value),
        _ => Config::Foreign,
    }
}

fn write_config(path: &Path, config: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(config)?;
    text.push('\n');
    fs::write(path, text)?;
    Ok(())
}

fn listed(config: &Value, key: &str, spec: &str) -> bool {
    config
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|entries| entries.iter().any(|entry| entry.as_str() == Some(spec)))
}

/// Add the package to the server config's `plugin` list while preserving every
/// unrelated setting and any other plugin the user configured.
fn register() -> Result<()> {
    let path = server_config_path();
    let mut config = match read_config(&path) {
        Config::Object(config) => config,
        Config::Missing => json!({}),
        Config::Foreign => return Ok(()),
    };
    if listed(&config, "plugin", &package_spec()) {
        return Ok(());
    }
    let Some(object) = config.as_object_mut() else {
        return Ok(());
    };
    let plugins = object
        .entry("plugin")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = plugins.as_array_mut() else {
        // Never discard a value we do not understand.
        return Ok(());
    };
    list.push(Value::String(package_spec()));
    write_config(&path, &config)
}

/// Drop only our entries, leaving the rest of each file untouched.
fn unregister() -> Result<()> {
    let path = server_config_path();
    if let Config::Object(mut config) = read_config(&path) {
        if listed(&config, "plugin", &package_spec()) {
            if let Some(list) = config.get_mut("plugin").and_then(Value::as_array_mut) {
                list.retain(|entry| entry.as_str() != Some(&package_spec()));
            }
            write_config(&path, &config)?;
        }
    }
    let cli = config_dir().join("cli.json");
    if let Config::Object(mut config) = read_config(&cli) {
        if listed(&config, "plugins", &legacy_spec()) {
            if let Some(list) = config.get_mut("plugins").and_then(Value::as_array_mut) {
                list.retain(|entry| entry.as_str() != Some(&legacy_spec()));
            }
            write_config(&cli, &config)?;
        }
    }
    Ok(())
}

fn install() -> Result<()> {
    let dir = plugin_dir();
    fs::create_dir_all(&dir)?;
    fs::write(plugin_path(), PLUGIN)?;
    let _ = fs::remove_file(dir.join("bohay.js"));

    let package = package_dir();
    fs::create_dir_all(&package)?;
    fs::write(package.join("package.json"), PLUGIN_MANIFEST)?;
    fs::write(package.join("index.js"), PLUGIN_INDEX)?;
    fs::write(package.join("tui.js"), PLUGIN_TUI)?;

    let _ = fs::remove_file(legacy_file());
    register()
}

fn uninstall() -> Result<()> {
    let _ = fs::remove_file(plugin_path());
    let _ = fs::remove_file(plugin_dir().join("bohay.js"));
    let _ = fs::remove_dir_all(package_dir());
    let _ = fs::remove_file(legacy_file());
    unregister()
}

fn is_installed() -> bool {
    plugin_path().exists()
        && package_dir().join("tui.js").exists()
        && match read_config(&server_config_path()) {
            Config::Object(config) => listed(&config, "plugin", &package_spec()),
            _ => false,
        }
}
