//! Shared plumbing for taps-level integration tests: stand up an ephemeral
//! llm-wiki appliance over streamable HTTP and tear it down with the test.
//! Everything here uses real in-tree binaries and real transports — never
//! product internals.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Env gate: taps integration tests only run under `just integration`.
pub fn gated() -> bool {
    if std::env::var("TAPS_INTEGRATION").as_deref() == Ok("1") {
        return false;
    }
    eprintln!("skipped: set TAPS_INTEGRATION=1 (or run `just integration`)");
    true
}

/// Path to an in-tree debug binary (built by `just integration` beforehand).
pub fn bin(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/debug")
        .join(name);
    assert!(
        path.is_file(),
        "{} not built — run `just integration`, which builds it first",
        path.display()
    );
    path
}

/// An ephemeral appliance: scratch registry + one space, served over HTTP.
/// The serve process dies with this struct.
pub struct Appliance {
    child: Child,
    _dir: tempfile::TempDir,
    /// Streamable-HTTP MCP endpoint.
    pub url: String,
    /// Name of the space it hosts.
    pub wiki: String,
}

impl Appliance {
    /// Create a wiki named `wiki` in a scratch registry and serve it.
    pub fn launch(wiki: &str) -> Appliance {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        let llm_wiki = bin("llm-wiki");

        let create = Command::new(&llm_wiki)
            .env("LLM_WIKI_CONFIG", &config)
            .args(["admin", "create"])
            .arg(dir.path().join("wiki"))
            .args(["--name", wiki, "--set-default"])
            .output()
            .expect("admin create");
        assert!(
            create.status.success(),
            "admin create failed: {}",
            String::from_utf8_lossy(&create.stderr)
        );

        // A free port, found by binding then releasing it.
        let port = TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("addr")
            .port();

        let child = Command::new(&llm_wiki)
            .env("LLM_WIKI_CONFIG", &config)
            .args(["serve", "--http", &port.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("serve");

        let url = format!("http://127.0.0.1:{port}/mcp");
        wait_for_port(port);

        Appliance {
            child,
            _dir: dir,
            url,
            wiki: wiki.to_string(),
        }
    }

    /// A connected KB client targeting this appliance's space.
    pub async fn client(&self) -> como_kb_client::KbClient {
        let target = como_kb_client::KbTarget {
            url: self.url.clone(),
            wiki: Some(self.wiki.clone()),
        };
        como_kb_client::KbClient::connect(&target)
            .await
            .expect("connect to appliance")
    }
}

impl Drop for Appliance {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_port(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("appliance never opened port {port}");
}
