//! End to end through the binary: new → check → pack → keygen → sign → verify → inspect for meta
//! and wasm apps, and publish / dev against a local HTTP endpoint.

use ferrum_apps::Bundle;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;

fn ferrum_app(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrum-app"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FERRUM_TOKEN")
        .env_remove("FERRUM_STORE_TOKEN")
        .env_remove("FERRUM_SERVER")
        .env_remove("FERRUM_STORE")
        .output()
        .expect("run ferrum-app")
}

/// Runs a command that must succeed; returns its standard output.
fn ok(args: &[&str], cwd: &Path) -> String {
    let out = ferrum_app(args, cwd);
    assert!(out.status.success(), "ferrum-app {args:?} failed:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

/// Runs a command that must fail; returns its standard error.
fn fails(args: &[&str], cwd: &Path) -> String {
    let out = ferrum_app(args, cwd);
    assert!(!out.status.success(), "ferrum-app {args:?} should have failed:\n{}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8(out.stderr).unwrap()
}

/// A minimal plugin: imports only ferrum.host_call and answers `{}` (an empty reply).
const PLUGIN_WAT: &str = r#"(module
  (import "ferrum" "host_call" (func (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "ferrum_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "ferrum_hook") (param i32 i32) (result i64) (i64.const 0)))"#;

/// A tiny platform tree with one package and one role.
fn platform(dir: &Path, role: &str) {
    let core = dir.join("platform/core");
    fs::create_dir_all(core.join("entities")).unwrap();
    fs::write(
        core.join("package.yaml"),
        format!("package: core\nversion: 0.3.0\nroles:\n  - name: {role}\n    permissions: [{{entity: Company, read: true}}]\n"),
    )
    .unwrap();
    fs::write(core.join("entities/company.yaml"), "name: Company\nlabel: Company\nkind: master\nfields:\n  - {name: company_name, type: data}\n").unwrap();
}

/// Signs `bundle` as publisher and store with fresh keys; returns the two trusted-key lines.
fn sign_both(bundle: &str, t: &Path) -> (String, String) {
    let publisher = ok(&["keygen", "-o", "publisher.pk8"], t).trim().to_string();
    let store = ok(&["keygen", "-o", "store.pk8", "--role", "store"], t).trim().to_string();
    assert!(publisher.starts_with("publisher:") && store.starts_with("store:"), "{publisher} {store}");
    assert!(ok(&["sign", bundle, "--key", "publisher.pk8"], t).contains("as publisher"));
    assert!(ok(&["sign", bundle, "--key", "store.pk8", "--role", "store"], t).contains("as store"));
    (publisher, store)
}

#[test]
fn the_board_example_is_a_valid_app() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let out = ok(&["check", "board"], &examples);
    assert!(out.contains("ok: board 1.0.1 (meta app, "), "{out}");
}

#[test]
fn meta_app_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    assert!(ok(&["new", "notes-kit", "--runtime", "meta", "--publisher", "acme"], t).contains("created notes-kit"));
    for file in
        ["app.yaml", "README.md", "CHANGELOG.md", "LICENSE", ".gitignore", "packages/notes-kit/package.yaml", "packages/notes-kit/entities/notes_kit_note.yaml"]
    {
        assert!(t.join("notes-kit").join(file).is_file(), "{file}");
    }
    assert!(fails(&["new", "notes-kit", "--runtime", "meta"], t).contains("not empty"));
    assert!(ok(&["check", "notes-kit"], t).contains("ok: notes-kit 0.1.0 (meta app, 6 files)"));

    // Together with the platform's packages. A platform role may be extended with grants on the
    // app's own entities, nothing else.
    platform(t, "Notes Kit User");
    ok(&["check", "notes-kit", "--platform-packages", "platform"], t);
    let package_yaml = t.join("notes-kit/packages/notes-kit/package.yaml");
    let package = fs::read_to_string(&package_yaml).unwrap();
    fs::write(&package_yaml, format!("{package}      - {{entity: Company, read: true}}\n")).unwrap();
    let err = fails(&["check", "notes-kit", "--platform-packages", "platform"], t);
    assert!(err.contains("role 'Notes Kit User' already exists (platform package core)") && err.contains("not Company"), "{err}");
    fs::write(&package_yaml, &package).unwrap();

    // Errors the server would also refuse are reported before packing.
    let app_yaml = t.join("notes-kit/app.yaml");
    let original = fs::read_to_string(&app_yaml).unwrap();
    fs::write(&app_yaml, original.replace("categories: [productivity]", "categories: [games]")).unwrap();
    assert!(fails(&["check", "notes-kit"], t).contains("unknown category 'games'"));
    fs::write(&app_yaml, &original).unwrap();
    fs::write(t.join("notes-kit/packages/notes-kit/sql.txt"), "x").unwrap();
    assert!(fails(&["pack", "notes-kit"], t).contains("unexpected file packages/notes-kit/sql.txt"));
    fs::remove_file(t.join("notes-kit/packages/notes-kit/sql.txt")).unwrap();
    // Hidden files (editor and OS litter) are not part of the bundle.
    fs::write(t.join("notes-kit/packages/notes-kit/.DS_Store"), "x").unwrap();

    let packed = ok(&["pack", "notes-kit", "-o", "kit.fapp"], t);
    assert!(packed.contains("packed notes-kit 0.1.0 into kit.fapp (6 files"), "{packed}");
    ok(&["pack", "notes-kit", "-o", "again.fapp"], t);
    assert_eq!(fs::read(t.join("kit.fapp")).unwrap(), fs::read(t.join("again.fapp")).unwrap(), "packing is deterministic");
    ok(&["pack", "notes-kit"], t);
    assert!(t.join("notes-kit-0.1.0.fapp").is_file(), "default output name");

    let (publisher, store) = sign_both("kit.fapp", t);
    assert!(fails(&["keygen", "-o", "store.pk8"], t).contains("never overwritten"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(t.join("store.pk8")).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let verified = ok(&["verify", "kit.fapp", "--trust", &store], t);
    assert!(verified.contains("verified store signature") && !verified.contains("publisher signature"), "{verified}");
    let both = ok(&["verify", "kit.fapp", "--trust", &store, &publisher], t);
    assert_eq!(both.matches("verified").count(), 2, "{both}");
    let stranger = ok(&["keygen", "-o", "stranger.pk8", "--role", "store"], t);
    assert!(fails(&["verify", "kit.fapp", "--trust", stranger.trim()], t).contains("no trusted key signed"));
    assert!(fails(&["verify", "again.fapp", "--trust", &store], t).contains("not signed"));
    assert!(fails(&["verify", "kit.fapp", "--trust", "admin:abc"], t).contains("publisher or store"));

    let text = ok(&["inspect", "kit.fapp"], t);
    for needle in
        ["notes-kit 0.1.0: Notes Kit", "runtime      meta", "signature    publisher", "signature    store", "new entity: NotesKitNote", "role: Notes Kit User"]
    {
        assert!(text.contains(needle), "{needle} in:\n{text}");
    }
    let json: serde_json::Value = serde_json::from_str(&ok(&["inspect", "kit.fapp", "--json"], t)).unwrap();
    let bundle = Bundle::read(&fs::read(t.join("kit.fapp")).unwrap()).unwrap();
    assert_eq!(json["manifest"]["id"], "notes-kit");
    assert_eq!(json["digest"], bundle.digest());
    assert_eq!(json["signatures"]["signatures"].as_array().unwrap().len(), 2);
    assert_eq!(json["permissions_digest"], bundle.permissions().unwrap().digest());
    assert_eq!(json["files"].as_array().unwrap().len(), 6);
}

#[test]
fn wasm_app_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    ok(&["new", "gw-demo", "--runtime", "wasm", "--publisher", "acme", "--name", "Gateway Demo"], t);
    for file in ["plugin.json", "plugin/Cargo.toml", "plugin/src/lib.rs"] {
        assert!(t.join("gw-demo").join(file).is_file(), "{file}");
    }
    assert!(fails(&["check", "gw-demo"], t).contains("plugin.wasm is missing"));

    // A module with an import the sandbox does not offer is refused by the plugin host's validator.
    let bad = wat::parse_str(r#"(module (import "env" "abort" (func)) (memory (export "memory") 1))"#).unwrap();
    fs::write(t.join("gw-demo/plugin.wasm"), bad).unwrap();
    assert!(fails(&["check", "gw-demo"], t).contains("may only import ferrum.host_call"));
    fs::write(t.join("gw-demo/plugin.wasm"), "(module)").unwrap();
    assert!(fails(&["check", "gw-demo"], t).contains("not a binary WebAssembly module"));

    fs::write(t.join("gw-demo/plugin.wasm"), wat::parse_str(PLUGIN_WAT).unwrap()).unwrap();
    assert!(ok(&["check", "gw-demo"], t).contains("ok: gw-demo 0.1.0 (wasm app, 8 files)"));
    platform(t, "Accounts User");
    ok(&["check", "gw-demo", "--platform-packages", "platform"], t);
    // Hooks must name entities that exist.
    let plugin_json = t.join("gw-demo/plugin.json");
    let original = fs::read_to_string(&plugin_json).unwrap();
    fs::write(&plugin_json, original.replace("\"entity\": \"GwDemoNote\"", "\"entity\": \"Invoice\"")).unwrap();
    assert!(fails(&["check", "gw-demo", "--platform-packages", "platform"], t).contains("unknown entity Invoice"));
    fs::write(&plugin_json, &original).unwrap();

    ok(&["pack", "gw-demo", "-o", "gw.fapp"], t);
    let (_, store) = sign_both("gw.fapp", t);
    assert!(ok(&["verify", "gw.fapp", "--trust", &store], t).contains("verified store signature"));
    let text = ok(&["inspect", "gw.fapp"], t);
    for needle in ["gw-demo 0.1.0: Gateway Demo", "runtime      wasm", "runs during: GwDemoNote before_validate", "new entity: GwDemoNote"] {
        assert!(text.contains(needle), "{needle} in:\n{text}");
    }
    let bundle = Bundle::read(&fs::read(t.join("gw.fapp")).unwrap()).unwrap();
    assert!(bundle.file("plugin/src/lib.rs").is_none(), "the plugin's source stays out of the bundle");
    assert_eq!(bundle.plugin().unwrap().unwrap().0, wat::parse_str(PLUGIN_WAT).unwrap());
}

/// One recorded request: request line, lower-case headers, body.
type Recorded = (String, Vec<(String, String)>, Vec<u8>);

/// Answers one connection per status (in order) and returns what was sent.
fn serve(statuses: Vec<u16>) -> (u16, thread::JoinHandle<Vec<Recorded>>) {
    serve_replies(statuses.into_iter().map(|s| (s, r#"{"ok":true}"#)).collect())
}

/// [`serve`] with a reply body per connection.
fn serve_replies(replies: Vec<(u16, &'static str)>) -> (u16, thread::JoinHandle<Vec<Recorded>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        replies
            .into_iter()
            .map(|(status, reply)| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    let (k, v) = line.split_once(':').unwrap();
                    headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                }
                let len: usize = headers.iter().find(|(k, _)| k == "content-length").map_or(0, |(_, v)| v.parse().unwrap());
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {status} Done\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
                (request_line.trim().to_string(), headers, body)
            })
            .collect()
    });
    (port, handle)
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> &'a str {
    headers.iter().find(|(k, _)| k == name).map_or("", |(_, v)| v.as_str())
}

#[test]
fn publish_and_dev_upload() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    ok(&["new", "notes-kit", "--runtime", "meta"], t);
    ok(&["pack", "notes-kit", "-o", "kit.fapp"], t);
    sign_both("kit.fapp", t);
    assert!(fails(&["publish", "kit.fapp", "--store", "http://store.example", "--token", "t"], t).contains("use https"));

    let (port, server) = serve(vec![201, 200, 403]);
    let base = format!("http://127.0.0.1:{port}");
    assert!(ok(&["publish", "kit.fapp", "--store", &base, "--token", "store-token"], t).contains("201"));
    assert!(ok(&["dev", "notes-kit", "--server", &format!("{base}/"), "--token", "dev-token"], t).contains("uploaded notes-kit-0.1.0-dev."));
    assert!(fails(&["publish", "kit.fapp", "--store", &base, "--token", "store-token"], t).contains("HTTP 403"));
    let requests = server.join().unwrap();

    let (line, headers, body) = &requests[0];
    assert_eq!(line, "POST /v1/releases HTTP/1.1");
    assert_eq!(header(headers, "authorization"), "Bearer store-token");
    assert_eq!(header(headers, "content-type"), "application/octet-stream");
    assert_eq!(body, &fs::read(t.join("kit.fapp")).unwrap());

    let (line, headers, body) = &requests[1];
    assert_eq!(line, "POST /api/admin/apps/upload HTTP/1.1");
    assert_eq!(header(headers, "authorization"), "Bearer dev-token");
    let boundary = header(headers, "content-type").strip_prefix("multipart/form-data; boundary=").expect("multipart").to_string();
    let head = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"bundle\"; filename=\"notes-kit-0.1.0-dev.");
    let tail = format!("\r\n--{boundary}--\r\n");
    assert!(body.starts_with(head.as_bytes()) && body.ends_with(tail.as_bytes()));
    let start = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let uploaded = Bundle::read(&body[start..body.len() - tail.len()]).unwrap();
    assert_eq!(uploaded.manifest.id, "notes-kit");
    assert!(uploaded.manifest.version.to_string().starts_with("0.1.0-dev."), "each dev upload is a release of its own");
    assert!(uploaded.signatures.is_none(), "dev uploads the directory as it is, unsigned");

    // With --install, each upload is installed in the workspace, agreeing to the plan it gets.
    let (port, server) = serve_replies(vec![(201, r#"{"ok":true}"#), (200, r#"{"digest":"d1g35t","increases":["reads Customer"]}"#), (200, r#"{"ok":true}"#)]);
    let out = ok(&["dev", "notes-kit", "--server", &format!("http://127.0.0.1:{port}"), "--token", "dev-token", "--install"], t);
    assert!(out.contains("installed notes-kit 0.1.0-dev."), "{out}");
    let requests = server.join().unwrap();
    let version = requests[1].0.split("version=").nth(1).and_then(|v| v.split(' ').next()).unwrap().to_string();
    assert!(requests[1].0.starts_with("GET /api/admin/apps/notes-kit/plan?version=0.1.0-dev."), "{}", requests[1].0);
    assert_eq!(requests[2].0, "POST /api/admin/apps/notes-kit/install HTTP/1.1");
    let sent: serde_json::Value = serde_json::from_slice(&requests[2].2).unwrap();
    assert_eq!(sent, serde_json::json!({"version": version, "consent": "d1g35t"}));
}

#[test]
fn schema_agrees_with_the_library() {
    let schema: serde_json::Value = serde_json::from_str(include_str!("../../schemas/app.schema.json")).unwrap();
    let list = |v: &serde_json::Value| v.as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(list(&schema["properties"]["categories"]["items"]["enum"]), ferrum_apps::CATEGORIES);
    assert_eq!(list(&schema["definitions"]["setting"]["properties"]["type"]["enum"]), ferrum_apps::SETTING_TYPES);
    assert_eq!(list(&schema["required"]), ["id", "name", "version", "publisher", "summary", "categories", "runtime", "platform"]);
}
