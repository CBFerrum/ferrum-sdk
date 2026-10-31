//! `ferrum-app`: the Ferrum app developer CLI. Bundles are built, checked, signed and verified
//! with `ferrum-apps`, the same code the Ferrum server and the store use, so what passes here
//! passes there.

mod scaffold;
mod upload;

use clap::{Parser, Subcommand, ValueEnum};
use ferrum_apps::{Bundle, MAX_FILES, MAX_UNPACKED, PLATFORM_VERSION, Permissions, Role, Runtime, Signer, TrustedKey};
use ferrum_meta::{Catalog, Package};
use ferrum_plugins::{NewPlugin, PluginHost};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

type Result<T> = std::result::Result<T, String>;

/// Root files a bundle may hold; `packages/`, `assets/` and `ui/` are taken whole (hidden files skipped).
const ROOT_FILES: &[&str] = &["app.yaml", "README.md", "CHANGELOG.md", "LICENSE", "plugin.json", "plugin.wasm"];
/// Largest file `sign`, `verify`, `inspect` and `publish` read as a bundle.
const MAX_BUNDLE_FILE: u64 = 64 << 20;

#[derive(Parser)]
#[command(name = "ferrum-app", version, about = "Create, check, pack, sign and publish Ferrum apps")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum RuntimeArg {
    /// Sandboxed WebAssembly plugin plus metadata
    Wasm,
    /// Metadata only (entities, roles, menus, workflows, seed data)
    Meta,
    /// Rust compiled into a Ferrum distribution
    Native,
}

#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    Publisher,
    Store,
}

impl From<RoleArg> for Role {
    fn from(r: RoleArg) -> Role {
        match r {
            RoleArg::Publisher => Role::Publisher,
            RoleArg::Store => Role::Store,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Create a new app: manifest, docs, a package with one entity (and a plugin crate for wasm)
    New {
        /// App id: a lower-case letter, then lower-case letters, digits or '-' (2-40 characters)
        id: String,
        #[arg(long, value_enum)]
        runtime: RuntimeArg,
        /// Directory to create [default: ./<id>]
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Display name [default: from the id]
        #[arg(long)]
        name: Option<String>,
        /// Your store publisher id
        #[arg(long, default_value = "local")]
        publisher: String,
        /// PascalCase prefix of the app's entities (wasm and meta apps) [default: from the id]
        #[arg(long)]
        prefix: Option<String>,
        /// With an interface of its own at /<id>/: plain files run isolated (wasm and meta apps), or
        /// Vite, React and TypeScript with Ferrum-Frontend's packages (native apps)
        #[arg(long)]
        ui: bool,
    },
    /// Validate an app directory as the server would: manifest, packages, plugin
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Also validate the packages together with the platform's (a directory of packages)
        #[arg(long, value_name = "DIR")]
        platform_packages: Option<PathBuf>,
    },
    /// Check an app directory and build its deterministic .fapp bundle
    Pack {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Bundle to write [default: <id>-<version>.fapp]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also validate the packages together with the platform's (a directory of packages)
        #[arg(long, value_name = "DIR")]
        platform_packages: Option<PathBuf>,
    },
    /// Create an Ed25519 signing key (PKCS#8 DER) and print its public key as a trusted key
    Keygen {
        /// Private key file to create (never overwritten)
        #[arg(short, long)]
        output: PathBuf,
        /// Role the printed trusted key is for
        #[arg(long, value_enum, default_value = "publisher")]
        role: RoleArg,
    },
    /// Sign a bundle (in place)
    Sign {
        bundle: PathBuf,
        /// Private key from keygen
        #[arg(long)]
        key: PathBuf,
        #[arg(long, value_enum, default_value = "publisher")]
        role: RoleArg,
    },
    /// Verify a bundle's digest and signatures against trusted keys
    Verify {
        bundle: PathBuf,
        /// Trusted keys: store:<base64 public key> or publisher:<base64 public key>
        #[arg(long, required = true, num_args = 1.., value_name = "ROLE:KEY")]
        trust: Vec<String>,
    },
    /// Show a bundle's manifest, digest, signatures and permissions
    Inspect {
        bundle: PathBuf,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// Submit a bundle to a store (POST {store}/v1/releases)
    Publish {
        bundle: PathBuf,
        /// Store base URL
        #[arg(long, env = "FERRUM_STORE")]
        store: String,
        /// Publisher API token
        #[arg(long, env = "FERRUM_STORE_TOKEN", hide_env_values = true)]
        token: String,
    },
    /// Upload an app directory to a Ferrum server as a private app (POST {server}/api/admin/apps/upload)
    Dev {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Server base URL
        #[arg(long, env = "FERRUM_SERVER")]
        server: String,
        /// Administrator API token
        #[arg(long, env = "FERRUM_TOKEN", hide_env_values = true)]
        token: String,
        /// Upload again whenever a file of the app changes
        #[arg(long)]
        watch: bool,
        /// After each upload, install it in the token's workspace (or update it there), agreeing
        /// to its install plan: its permissions and dependencies
        #[arg(long)]
        install: bool,
        /// Serve the app's pages in the token's workspace from its interface's Vite server (run
        /// `npm run dev` in ui/), behind the real session, bar and bootstrap, with hot reload; `off`
        /// serves them from the app's release again. The server must run with FERRUM_DEV_UI=1.
        #[arg(long, value_name = "URL|off")]
        ui: Option<String>,
        /// Upload nothing: only point the app's pages (`--ui`), for an app already installed or
        /// compiled into the server
        #[arg(long)]
        no_upload: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::New { id, runtime, dir, name, publisher, prefix, ui } => {
            let runtime = match runtime {
                RuntimeArg::Wasm => Runtime::Wasm,
                RuntimeArg::Meta => Runtime::Meta,
                RuntimeArg::Native => Runtime::Native,
            };
            let files = scaffold::generate(&scaffold::NewApp { id: id.clone(), runtime, name, publisher, prefix, ui })?;
            let dir = dir.unwrap_or_else(|| PathBuf::from(&id));
            scaffold::create(&dir, &files)?;
            println!("created {} ({runtime} app {id})", dir.display());
            if runtime == Runtime::Wasm {
                println!("build the plugin into plugin.wasm (see README.md), then run: ferrum-app check {}", dir.display());
            } else {
                println!("next: ferrum-app check {}", dir.display());
            }
            Ok(())
        }
        Command::Check { dir, platform_packages } => {
            let bundle = load(&dir)?;
            for w in check(&bundle, platform_packages.as_deref())? {
                eprintln!("warning: {w}");
            }
            let m = &bundle.manifest;
            println!("ok: {} {} ({} app, {} files)", m.id, m.version, m.runtime, bundle.paths().count());
            Ok(())
        }
        Command::Pack { dir, output, platform_packages } => {
            let bundle = load(&dir)?;
            for w in check(&bundle, platform_packages.as_deref())? {
                eprintln!("warning: {w}");
            }
            let bytes = bundle.write().map_err(|e| e.to_string())?;
            let m = &bundle.manifest;
            let out = output.unwrap_or_else(|| PathBuf::from(format!("{}-{}.fapp", m.id, m.version)));
            write_atomic(&out, &bytes)?;
            println!("packed {} {} into {} ({} files, {})", m.id, m.version, out.display(), bundle.paths().count(), size(bytes.len() as u64));
            println!("digest {}", bundle.digest());
            Ok(())
        }
        Command::Keygen { output, role } => keygen(&output, role.into()),
        Command::Sign { bundle, key, role } => {
            let role: Role = role.into();
            let mut b = read_bundle(&bundle)?;
            let der = fs::read(&key).map_err(|e| format!("{}: {e}", key.display()))?;
            let signer = Signer::from_pkcs8(&der).map_err(|e| format!("{}: {e}", key.display()))?;
            b.sign(&signer, role).map_err(|e| e.to_string())?;
            write_atomic(&bundle, &b.write().map_err(|e| e.to_string())?)?;
            println!("signed {} {} as {role} with key {}", b.manifest.id, b.manifest.version, signer.key_id());
            Ok(())
        }
        Command::Verify { bundle, trust } => {
            let keys = trust.iter().map(|t| TrustedKey::parse(t)).collect::<std::result::Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
            let b = read_bundle(&bundle)?;
            let verified = b.verify(&keys).map_err(|e| format!("{}: {e}", bundle.display()))?;
            println!("{} {}: digest {}", b.manifest.id, b.manifest.version, b.digest());
            for (key_id, role) in verified {
                println!("verified {role} signature by key {key_id}");
            }
            Ok(())
        }
        Command::Inspect { bundle, json } => inspect(&read_bundle(&bundle)?, json),
        Command::Publish { bundle, store, token } => {
            let url = upload::endpoint(&store, "v1/releases")?;
            let bytes = read_file(&bundle)?;
            let b = Bundle::read(&bytes).map_err(|e| format!("{}: {e}", bundle.display()))?;
            if !b.signatures.as_ref().is_some_and(|s| s.signatures.iter().any(|e| e.role == Role::Publisher)) {
                eprintln!("warning: the bundle has no publisher signature; stores expect one (ferrum-app sign --role publisher)");
            }
            let (status, body) = upload::post(&url, &token, "application/octet-stream", bytes)?;
            println!("{status} {body}");
            if !(200..300).contains(&status) {
                return Err(format!("the store refused {} {} (HTTP {status})", b.manifest.id, b.manifest.version));
            }
            Ok(())
        }
        Command::Dev { dir, server, token, watch, install, ui, no_upload } => {
            if no_upload && ui.is_none() {
                return Err("--no-upload only makes sense with --ui".into());
            }
            if let Some(ui) = &ui {
                // The pages first: an upload that waits for changes (--watch) never returns.
                if !no_upload {
                    dev(&dir, &server, &token, false, install)?;
                }
                dev_ui(&dir, &server, &token, ui)?;
                if no_upload || !watch {
                    return Ok(());
                }
            }
            dev(&dir, &server, &token, watch, install)
        }
    }
}

/// Visits `rel` under `root` (a file, or every non-hidden file below a directory, sorted).
/// Missing paths are skipped; links and special files are refused.
fn walk(root: &Path, rel: &str, visit: &mut dyn FnMut(&str, &Path, &fs::Metadata) -> Result<()>) -> Result<()> {
    let path = root.join(rel);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let kind = meta.file_type();
    if kind.is_symlink() {
        return Err(format!("{rel} is a symbolic link; bundles hold only regular files"));
    }
    if kind.is_dir() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&path).map_err(|e| format!("{}: {e}", path.display()))? {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            let name = entry.file_name().into_string().map_err(|n| format!("{rel}/{}: file names must be UTF-8", n.to_string_lossy()))?;
            if !name.starts_with('.') {
                names.push(name);
            }
        }
        names.sort();
        for name in names {
            walk(root, &format!("{rel}/{name}"), visit)?;
        }
        return Ok(());
    }
    if !kind.is_file() {
        return Err(format!("{rel} is not a regular file"));
    }
    visit(rel, &path, &meta)
}

/// Visits every file of an app directory that belongs in its bundle. Of `ui/`: nothing for a native
/// app (its crate embeds its interface), only the built directory of an interface built under it
/// (`ui.dist: ui/dist`; its sources stay out), else all of it. Of `packages/`: a native app's listed
/// packages (see [`native_packages`]), else all of it.
fn walk_app(dir: &Path, visit: &mut dyn FnMut(&str, &Path, &fs::Metadata) -> Result<()>) -> Result<()> {
    for rel in ROOT_FILES.iter().chain(&["assets"]) {
        walk(dir, rel, visit)?;
    }
    match native_packages(dir) {
        Some(listed) => {
            for name in listed {
                walk(dir, &format!("packages/{name}"), visit)?;
            }
        }
        None => walk(dir, "packages", visit)?,
    }
    match ui_dir(dir) {
        Some(ui) => walk(dir, &ui, visit),
        None => Ok(()),
    }
}

/// The part of `ui/` a bundle takes (see [`walk_app`]); a manifest that does not parse gives all of
/// `ui/`, and the bundle's check reports the manifest. A native app's `ui/` holds the sources of its
/// interface (Vite, TypeScript): its bundle takes only the built files `ui.dist` names, which the
/// store reviews (its server embeds its own copy), and nothing when it declares none.
fn ui_dir(dir: &Path) -> Option<String> {
    let manifest = fs::read_to_string(dir.join("app.yaml")).ok().and_then(|yaml| ferrum_apps::AppManifest::parse(&yaml).ok());
    let Some(m) = manifest else { return Some("ui".into()) };
    let dist = m.ui.map(|u| u.dist.trim_end_matches('/').to_string()).filter(|d| d.starts_with("ui/") && !d.contains(".."));
    if m.runtime == ferrum_apps::Runtime::Native {
        return dist;
    }
    Some(dist.unwrap_or_else(|| "ui".into()))
}

/// A native app's packages as its manifest lists them, or `None` for any other app (and when the
/// manifest does not parse; the bundle's check reports it). A native app's repository may also hold
/// packages its crate installs itself, such as a connector's when the app it joins is installed
/// too (CRM's `crm_erp`); they are not the app's to declare, so its bundle leaves them out.
fn native_packages(dir: &Path) -> Option<Vec<String>> {
    let m = fs::read_to_string(dir.join("app.yaml")).ok().and_then(|yaml| ferrum_apps::AppManifest::parse(&yaml).ok())?;
    if m.runtime != ferrum_apps::Runtime::Native {
        return None;
    }
    if let Ok(entries) = fs::read_dir(dir.join("packages")) {
        let mut others: Vec<String> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.') && !m.packages.contains(n) && dir.join("packages").join(n).is_dir())
            .collect();
        others.sort();
        for name in others {
            eprintln!("note: packages/{name}/ stays out of the bundle: app.yaml does not list it (the app's crate installs it)");
        }
    }
    Some(m.packages)
}

/// Whether a path of an app directory goes into its bundle (what `walk_app` visits).
pub(crate) fn in_bundle(rel: &str) -> bool {
    ROOT_FILES.contains(&rel) || rel.starts_with("packages/") || rel.starts_with("assets/") || rel.starts_with("ui/")
}

/// The files of an app directory that go into its bundle, exactly as `Bundle::from_files` takes them.
fn collect(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    if !dir.join("app.yaml").is_file() {
        return Err(format!("{}: no app.yaml here (create an app with ferrum-app new)", dir.display()));
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    walk_app(dir, &mut |rel, path, meta| {
        total += meta.len();
        if total > MAX_UNPACKED || files.len() > MAX_FILES {
            return Err(format!("{}: more than a bundle may hold ({MAX_FILES} files, {} MiB)", dir.display(), MAX_UNPACKED >> 20));
        }
        files.insert(rel.to_string(), fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?);
        Ok(())
    })?;
    Ok(files)
}

fn load(dir: &Path) -> Result<Bundle> {
    Bundle::from_files(collect(dir)?).map_err(|e| e.to_string())
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    let len = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len > MAX_BUNDLE_FILE {
        return Err(format!("{}: {} is too large for a bundle", path.display(), size(len)));
    }
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_bundle(path: &Path) -> Result<Bundle> {
    Bundle::read(&read_file(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// Replaces `path` only once `data` is completely written.
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let name = path.file_name().ok_or_else(|| format!("{}: not a file name", path.display()))?;
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KiB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

/// What the server checks beyond the bundle format: the plugin as the plugin host reads it and,
/// given the platform's packages, the packages as the catalog and a workspace schema take them.
/// Returns warnings; errors stop.
fn check(b: &Bundle, platform_packages: Option<&Path>) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let platform = semver::Version::parse(PLATFORM_VERSION).map_err(|e| e.to_string())?;
    if !b.manifest.compatible_with(&platform) {
        warnings.push(format!("platform \"{}\" excludes {PLATFORM_VERSION}, the app API this SDK targets", b.manifest.platform));
    }
    for doc in ["README.md", "LICENSE"] {
        if b.file(doc).is_none() {
            warnings.push(format!("no {doc}: the store shows it to people choosing the app"));
        }
    }
    let plugin = match b.plugin().map_err(|e| e.to_string())? {
        Some((wasm, json)) => {
            let p: NewPlugin = serde_json::from_value(json).map_err(|e| format!("plugin.json: the server cannot read it: {e}"))?;
            let host = PluginHost::new(false).map_err(|e| format!("cannot start the WebAssembly engine: {e}"))?;
            host.validate(&wasm).map_err(|e| format!("plugin.wasm: {e}"))?;
            Some(p)
        }
        None => None,
    };
    if let Some(dir) = platform_packages {
        warnings.extend(check_with_platform(b, dir, plugin.as_ref())?);
    }
    Ok(warnings)
}

fn check_with_platform(b: &Bundle, dir: &Path, plugin: Option<&NewPlugin>) -> Result<Vec<String>> {
    let platform = Catalog::load_dir(dir).map_err(|e| format!("platform packages in {}: {e}", dir.display()))?;
    let app = b.packages().map_err(|e| e.to_string())?;
    let names: BTreeSet<String> = app.iter().map(|p| p.name().to_string()).collect();
    // The app's packages replace same-named copies in the platform tree (apps moving out of it).
    let base: Vec<Package> = platform.packages().filter(|p| !names.contains(p.name())).cloned().collect();
    // Installing merges a package's role into an existing role of the same name (that is how
    // platform packages share roles). A third-party app may extend a platform role only with
    // grants on its own entities, never widen what the role's holders see of everything else.
    if b.manifest.runtime != Runtime::Native {
        let roles: BTreeMap<&str, &str> = base.iter().flat_map(|p| p.manifest.roles.iter().map(move |r| (r.name.as_str(), p.name()))).collect();
        let own: BTreeSet<&str> = app.iter().flat_map(|p| p.entities.iter().map(|e| e.name.as_str())).collect();
        for p in &app {
            for r in &p.manifest.roles {
                let Some(owner) = roles.get(r.name.as_str()) else { continue };
                if let Some(grant) = r.permissions.iter().find(|g| !own.contains(g.entity.as_str())) {
                    return Err(format!(
                        "package {}: role '{}' already exists (platform package {owner}); this app may extend it only with its own entities, not {}",
                        p.name(),
                        r.name,
                        grant.entity
                    ));
                }
            }
        }
    }
    let mut all = base;
    all.extend(app);
    let catalog = Catalog::from_packages(all).map_err(|e| format!("the packages do not fit the platform: {e}"))?;
    let installed: Vec<String> = names.into_iter().collect();
    let schema = ferrum_meta::effective::build(&catalog, &installed, &[], 1).map_err(|e| format!("the packages do not merge into a workspace schema: {e}"))?;
    if let Some(p) = plugin {
        let own: BTreeSet<&str> = p.manifest.entities.iter().map(|e| e.name.as_str()).collect();
        let named = p.manifest.hooks.iter().map(|h| &h.entity).chain(p.manifest.permissions.iter().map(|p| &p.entity));
        for entity in named {
            if entity != "*" && !own.contains(entity.as_str()) && catalog.entity(entity).is_none() {
                return Err(format!("plugin.json: unknown entity {entity} (not in the platform, the app's packages or the plugin's entities)"));
            }
        }
    }
    Ok(schema.warnings.iter().map(|w| format!("{}: {}", w.entity, w.message)).collect())
}

fn keygen(output: &Path, role: Role) -> Result<()> {
    let der = Signer::generate_pkcs8().map_err(|e| e.to_string())?;
    let signer = Signer::from_pkcs8(&der).map_err(|e| e.to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(output).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{} already exists; keys are never overwritten", output.display()),
        _ => format!("{}: {e}", output.display()),
    })?;
    file.write_all(&der).and_then(|()| file.sync_all()).map_err(|e| format!("{}: {e}", output.display()))?;
    let trusted = TrustedKey { key_id: signer.key_id(), public_key: signer.public_key(), role };
    println!("{trusted}");
    eprintln!("private key {} written to {}; keep it secret. Share the line above to have your {role} signatures trusted.", trusted.key_id, output.display());
    Ok(())
}

fn inspect(b: &Bundle, json: bool) -> Result<()> {
    let m = &b.manifest;
    let permissions = b.permissions().map_err(|e| e.to_string())?;
    let files: Vec<(&str, usize)> = b.paths().map(|p| (p, b.file(p).map_or(0, <[u8]>::len))).collect();
    if json {
        let report = serde_json::json!({
            "manifest": m,
            "digest": b.digest(),
            "signatures": b.signatures,
            "permissions": permissions,
            "permissions_digest": permissions.digest(),
            "files": files.iter().map(|(path, size)| serde_json::json!({"path": path, "size": size})).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let platform = semver::Version::parse(PLATFORM_VERSION).map_err(|e| e.to_string())?;
    println!("{} {}: {}", m.id, m.version, m.name);
    println!("summary      {}", m.summary);
    println!("publisher    {}", m.publisher);
    println!("runtime      {}", m.runtime);
    let fits = if m.compatible_with(&platform) { "fits" } else { "does not fit" };
    println!("platform     {} ({fits} app API {PLATFORM_VERSION})", m.platform);
    if !m.requires.is_empty() {
        println!("requires     {}", m.requires.iter().map(|(id, req)| format!("{id} {req}")).collect::<Vec<_>>().join(", "));
    }
    if !m.packages.is_empty() {
        println!("packages     {}", m.packages.join(", "));
    }
    println!("categories   {}", m.categories.join(", "));
    println!("files        {} ({})", files.len(), size(files.iter().map(|(_, n)| *n as u64).sum()));
    println!("digest       {}", b.digest());
    match &b.signatures {
        None => println!("signatures   none"),
        Some(s) => {
            if s.digest != b.digest() {
                println!("signatures   for other content (signed digest {}): the bundle was changed after signing", s.digest);
            }
            for e in &s.signatures {
                println!("signature    {} key {} (not verified here; use ferrum-app verify)", e.role, e.key_id);
            }
        }
    }
    let lines = permissions.increases(&Permissions::default());
    println!("permissions  {} (digest {})", if lines.is_empty() { "none" } else { "as follows" }, permissions.digest());
    for line in lines {
        println!("  {line}");
    }
    Ok(())
}

/// Changes whenever a bundle file of the app is added, removed or modified.
fn stamp(dir: &Path) -> Result<Vec<(String, u64, Option<SystemTime>)>> {
    let mut out = Vec::new();
    walk_app(dir, &mut |rel, _, meta| {
        out.push((rel.to_string(), meta.len(), meta.modified().ok()));
        Ok(())
    })?;
    Ok(out)
}

fn dev(dir: &Path, server: &str, token: &str, watch: bool, install: bool) -> Result<()> {
    let url = upload::endpoint(server, "api/admin/apps/upload")?;
    let mut last = None;
    loop {
        let now = stamp(dir);
        if last.as_ref() != Some(&now) {
            let outcome = now.clone().and_then(|_| dev_upload(dir, &url, token)).and_then(|(message, id, version)| {
                if !install {
                    return Ok(message);
                }
                println!("{message}");
                dev_install(server, token, &id, &version)
            });
            last = Some(now);
            match outcome {
                Ok(message) => println!("{message}"),
                Err(e) if watch => eprintln!("error: {e}"),
                Err(e) => return Err(e),
            }
        }
        if !watch {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Points the app's pages in the token's workspace at its Vite server (`url`), or back at its
/// release (`off`).
fn dev_ui(dir: &Path, server: &str, token: &str, url: &str) -> Result<()> {
    let yaml = std::fs::read_to_string(dir.join("app.yaml")).map_err(|e| format!("{}: {e}", dir.join("app.yaml").display()))?;
    let m = ferrum_apps::AppManifest::parse(&yaml).map_err(|e| e.to_string())?;
    if m.ui.is_none() {
        return Err(format!("{} has no interface of its own (no ui section in app.yaml)", m.id));
    }
    let endpoint = upload::endpoint(server, &format!("api/admin/apps/{}/dev-ui", m.id))?;
    let (status, text) = if url == "off" { upload::delete(&endpoint, token)? } else { upload::put_json(&endpoint, token, &serde_json::json!({"url": url}))? };
    match status {
        200..=299 if url == "off" => println!("{}'s pages come from its release again", m.id),
        200..=299 => println!("{}'s pages come from {url} (until --ui off or the server restarts)", m.id),
        404 if text.contains("FERRUM_DEV_UI") => return Err("the server serves no development pages: start it with FERRUM_DEV_UI=1".into()),
        _ => return Err(format!("the server would not point {}'s pages at {url} (HTTP {status}): {text}", m.id)),
    }
    Ok(())
}

/// Gives the app and each of its packages the pre-release version `<version>-dev.<n>` (or
/// `<pre-release>.dev.<n>`): a server refuses changed content under a version it already has, so
/// every `dev` upload is a release of its own. Only the uploaded copy changes, not the files.
fn dev_versions(files: &mut BTreeMap<String, Vec<u8>>, n: u64) {
    for (path, content) in files.iter_mut() {
        let manifest = path == "app.yaml" || (path.starts_with("packages/") && path.ends_with("/package.yaml") && path.matches('/').count() == 2);
        let Ok(text) = std::str::from_utf8(content) else { continue };
        if !manifest {
            continue;
        }
        let mut done = false;
        let lines: Vec<String> = text
            .lines()
            .map(|line| match line.strip_prefix("version:") {
                Some(v) if !done => {
                    done = true;
                    let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
                    let base = v.split('+').next().unwrap_or(v);
                    let dev = if base.contains('-') { format!("{base}.dev.{n}") } else { format!("{base}-dev.{n}") };
                    format!("version: \"{dev}\"")
                }
                _ => line.to_string(),
            })
            .collect();
        let mut out = lines.join("\n");
        if text.ends_with('\n') {
            out.push('\n');
        }
        *content = out.into_bytes();
    }
}

/// Installs (or updates to) an uploaded version in the token's workspace, agreeing to its plan.
fn dev_install(server: &str, token: &str, app: &str, version: &str) -> Result<String> {
    let plan_url = upload::endpoint(server, &format!("api/admin/apps/{app}/plan?version={version}"))?;
    let (status, text) = upload::get(&plan_url, token)?;
    if status != 200 {
        return Err(format!("the server would not plan {app} {version} (HTTP {status}): {text}"));
    }
    let plan: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("the server's plan is not JSON: {e}"))?;
    let digest = plan.get("digest").and_then(serde_json::Value::as_str).ok_or("the server's plan has no digest")?;
    for w in plan
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .chain(plan.get("increases").and_then(serde_json::Value::as_array).into_iter().flatten())
    {
        eprintln!("note: {}", w.as_str().map(str::to_string).unwrap_or_else(|| w.to_string()));
    }
    let url = upload::endpoint(server, &format!("api/admin/apps/{app}/install"))?;
    let body = serde_json::json!({"version": version, "consent": digest}).to_string().into_bytes();
    let (status, text) = upload::post(&url, token, "application/json", body)?;
    if !(200..300).contains(&status) {
        return Err(format!("installing {app} {version} failed (HTTP {status}): {text}"));
    }
    Ok(format!("installed {app} {version}"))
}

fn dev_upload(dir: &Path, url: &reqwest::Url, token: &str) -> Result<(String, String, String)> {
    let mut files = collect(dir)?;
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    dev_versions(&mut files, n);
    let b = Bundle::from_files(files).map_err(|e| e.to_string())?;
    for w in check(&b, None)? {
        eprintln!("warning: {w}");
    }
    let bytes = b.write().map_err(|e| e.to_string())?;
    let file_name = format!("{}-{}.fapp", b.manifest.id, b.manifest.version);
    let (content_type, body) = upload::multipart("bundle", &file_name, &bytes, &b.digest()[..32]);
    let (status, text) = upload::post(url, token, &content_type, body)?;
    if !(200..300).contains(&status) {
        return Err(format!("the server refused {file_name} (HTTP {status}): {text}"));
    }
    Ok((format!("uploaded {file_name} (HTTP {status})"), b.manifest.id.clone(), b.manifest.version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{collect, dev_versions};
    use std::collections::BTreeMap;
    use std::fs;

    /// A native app's interface sources stay out of its bundle; its built files go in, so the store
    /// can review them.
    #[test]
    fn a_native_bundle_takes_the_built_interface_only() {
        let dir = std::env::temp_dir().join(format!("ferrum-app-native-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let write = |rel: &str, text: &str| {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write(
            "app.yaml",
            "id: till\nname: Till\nversion: 0.1.0\npublisher: acme\nsummary: A till\ncategories: [pos]\nruntime: native\nplatform: \">=0.3, <0.4\"\n\
             packages: []\nui:\n  dist: ui/dist\ncontributes:\n  shell.app:\n    - {id: till, route: till, title: Till}\n",
        );
        write("ui/package.json", "{}");
        write("ui/src/main.tsx", "export {};");
        write("ui/dist/index.html", "<!doctype html><html lang=\"en\"><head><title>Till</title></head><body></body></html>");
        write("ui/dist/assets/index-a1b2.js", "export {};");
        let files = collect(&dir).expect("a bundle");
        let ui: Vec<&String> = files.keys().filter(|p| p.starts_with("ui/")).collect();
        assert_eq!(ui, ["ui/dist/assets/index-a1b2.js", "ui/dist/index.html"]);
        // Without an interface it declares, nothing of ui/ goes in.
        write(
            "app.yaml",
            &fs::read_to_string(dir.join("app.yaml"))
                .unwrap()
                .replace("ui:\n  dist: ui/dist\ncontributes:\n  shell.app:\n    - {id: till, route: till, title: Till}\n", ""),
        );
        assert!(!collect(&dir).expect("a bundle").keys().any(|p| p.starts_with("ui/")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_native_bundle_takes_its_listed_packages_only() {
        let dir = std::env::temp_dir().join(format!("ferrum-app-native-packages-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let write = |rel: &str, text: &str| {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        let manifest = "id: desk\nname: Desk\nversion: 0.1.0\npublisher: acme\nsummary: A desk\ncategories: [crm]\nruntime: native\nplatform: \">=0.3, <0.4\"\n\
                        packages: [desk]\n";
        write("app.yaml", manifest);
        write("packages/desk/package.yaml", "package: desk\nversion: 0.1.0\n");
        // A connector package the app's crate installs when the app it joins is there.
        write("packages/desk_erp/package.yaml", "package: desk_erp\nversion: 0.1.0\n");
        let files = collect(&dir).expect("a bundle");
        assert!(files.contains_key("packages/desk/package.yaml"));
        assert!(!files.keys().any(|p| p.starts_with("packages/desk_erp/")), "{:?}", files.keys());
        // Any other app's bundle takes all of packages/, and its check refuses what app.yaml does not list.
        write("app.yaml", &manifest.replace("runtime: native", "runtime: meta"));
        assert!(collect(&dir).expect("a bundle").contains_key("packages/desk_erp/package.yaml"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dev_uploads_get_their_own_versions() {
        let mut files = BTreeMap::from([
            ("app.yaml".to_string(), b"id: kit\nversion: 0.2.0\nplatform: \">=0.3\"\n".to_vec()),
            ("packages/kit/package.yaml".to_string(), b"package: kit\nversion: \"1.0.0-beta.2\"\n".to_vec()),
            ("packages/kit/entities/note.yaml".to_string(), b"version: untouched\n".to_vec()),
        ]);
        dev_versions(&mut files, 42);
        assert_eq!(files["app.yaml"], b"id: kit\nversion: \"0.2.0-dev.42\"\nplatform: \">=0.3\"\n");
        assert_eq!(files["packages/kit/package.yaml"], b"package: kit\nversion: \"1.0.0-beta.2.dev.42\"\n");
        assert_eq!(files["packages/kit/entities/note.yaml"], b"version: untouched\n", "only manifests change");
    }
}
