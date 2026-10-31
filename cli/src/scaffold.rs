//! `ferrum-app new`: a working app from the templates in `templates/`.

use crate::in_bundle;
use ferrum_apps::{Bundle, PLATFORM_VERSION, Runtime};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const APP_YAML: &str = include_str!("../../templates/app.yaml");
const README: &str = include_str!("../../templates/README.md");
const CHANGELOG: &str = include_str!("../../templates/CHANGELOG.md");
const LICENSE: &str = include_str!("../../templates/LICENSE");
const GITIGNORE: &str = include_str!("../../templates/gitignore");
const PACKAGE: &str = include_str!("../../templates/package/package.yaml");
const ENTITY: &str = include_str!("../../templates/package/entity.yaml");
const NATIVE_SECTION: &str = include_str!("../../templates/native.md");
const PLUGIN_JSON: &str = include_str!("../../templates/wasm/plugin.json");
const PLUGIN_CARGO: &str = include_str!("../../templates/wasm/Cargo.toml");
const PLUGIN_LIB: &str = include_str!("../../templates/wasm/lib.rs");
const PLUGIN_SECTION: &str = include_str!("../../templates/wasm/build.md");
/// An interface of its own that runs isolated (third-party apps): plain files and the bridge.
const UI_ISOLATED: [(&str, &str); 3] = [
    ("ui/index.html", include_str!("../../templates/ui/isolated/index.html")),
    ("ui/app.js", include_str!("../../templates/ui/isolated/app.js")),
    ("ui/style.css", include_str!("../../templates/ui/isolated/style.css")),
];
/// The bridge an isolated interface reaches Ferrum through, as the example ships it.
const UI_BRIDGE: &str = include_str!("../../examples/customer-cards/ui/ferrum.js");
/// An interface of its own built with Ferrum-Frontend's packages (native apps: Vite, React, TypeScript).
const UI_NATIVE: [(&str, &str); 10] = [
    ("ui/package.json", include_str!("../../templates/ui/native/package.json")),
    ("ui/vite.config.mjs", include_str!("../../templates/ui/native/vite.config.mjs")),
    ("ui/tsconfig.json", include_str!("../../templates/ui/native/tsconfig.json")),
    ("ui/index.html", include_str!("../../templates/ui/native/index.html")),
    ("ui/src/main.tsx", include_str!("../../templates/ui/native/src/main.tsx")),
    ("ui/src/records.ts", include_str!("../../templates/ui/native/src/records.ts")),
    ("ui/src/Notes.tsx", include_str!("../../templates/ui/native/src/Notes.tsx")),
    ("ui/src/greeting.ts", include_str!("../../templates/ui/native/src/greeting.ts")),
    ("ui/src/greeting.test.ts", include_str!("../../templates/ui/native/src/greeting.test.ts")),
    ("ui/src/styles.css", include_str!("../../templates/ui/native/src/styles.css")),
];

/// What `new` was asked for.
pub(crate) struct NewApp {
    pub id: String,
    pub runtime: Runtime,
    pub name: Option<String>,
    pub publisher: String,
    pub prefix: Option<String>,
    /// With an interface of its own at `/{id}/` (docs/plan/apps-ui.md §10.6).
    pub ui: bool,
}

/// Replaces `{{key}}` placeholders; a line holding only a placeholder that expands to nothing is
/// dropped.
fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    for line in template.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        let mut rendered = text.to_string();
        for (key, value) in vars {
            rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
        }
        if rendered.is_empty() && !text.is_empty() {
            continue;
        }
        out.push_str(&rendered);
        if line.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// `ev-charging` → `Ev Charging`.
fn humanize(id: &str) -> String {
    id.split('-').filter(|w| !w.is_empty()).map(capitalized).collect::<Vec<_>>().join(" ")
}

/// `ev-charging` → `EvCharging`.
fn pascal(id: &str) -> String {
    id.split('-').map(capitalized).collect()
}

/// An entity prefix from the id: whole capitalized words while they fit in 8 characters
/// (`ev-charging` → `Ev`), else the first 8 characters (`inventory-tools` → `Inventor`).
fn default_prefix(id: &str) -> Option<String> {
    let mut prefix = String::new();
    for word in id.split('-').filter(|w| !w.is_empty()).map(capitalized) {
        if prefix.len() + word.len() > 8 {
            break;
        }
        prefix.push_str(&word);
    }
    if prefix.len() < 2 {
        prefix = pascal(id).chars().take(8).collect();
    }
    let ok = (2..=8).contains(&prefix.len())
        && prefix.starts_with(|c: char| c.is_ascii_uppercase() && c != 'X')
        && prefix.chars().all(|c| c.is_ascii_alphanumeric());
    ok.then_some(prefix)
}

/// `>=0.3, <0.4` for app API 0.3.x (`>=1.2, <2` from 1.0 on).
fn platform_requirement() -> Result<String, String> {
    let v = semver::Version::parse(PLATFORM_VERSION).map_err(|e| e.to_string())?;
    Ok(if v.major == 0 { format!(">=0.{}, <0.{}", v.minor, v.minor + 1) } else { format!(">={}.{}, <{}", v.major, v.minor, v.major + 1) })
}

/// Every file of the new app, by path relative to its directory.
pub(crate) fn generate(app: &NewApp) -> Result<BTreeMap<String, String>, String> {
    let id = app.id.as_str();
    let id_ok = (2..=40).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !id_ok {
        return Err(format!("app id '{id}' must be 2-40 characters: a lower-case letter, then lower-case letters, digits or '-'"));
    }
    let name = app.name.as_deref().map(str::trim).map(str::to_string).unwrap_or_else(|| humanize(id));
    if name.is_empty() || name.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        return Err("--name must be one line of text without quotes or backslashes".into());
    }
    let third_party = app.runtime != Runtime::Native;
    let prefix = match &app.prefix {
        Some(p) => Some(p.clone()),
        None if third_party => Some(default_prefix(id).ok_or_else(|| {
            format!("cannot derive an entity prefix from '{id}'; choose one with --prefix (PascalCase, 2-8 letters or digits, not starting with X)")
        })?),
        None => None,
    };
    let module = id.replace('-', "_");
    let entity = format!("{}Note", prefix.clone().unwrap_or_else(|| pascal(id)));
    let role = format!("{name} User");
    let platform = platform_requirement()?;
    let runtime = app.runtime.to_string();
    let wasm = app.runtime == Runtime::Wasm;
    let crate_name = format!("{id}-plugin");
    let wasm_file = format!("{module}_plugin.wasm");
    let prefix_line =
        prefix.as_ref().map(|p| format!("# Namespace of this app's entities (the store reserves it for your publisher id).\nprefix: {p}")).unwrap_or_default();
    let plugin_line = if wasm { "# The plugin manifest; the compiled module is plugin.wasm.\nplugin: plugin.json" } else { "" };
    let section_vars = [("id", id), ("wasm_file", wasm_file.as_str())];
    let build_section = match app.runtime {
        Runtime::Wasm => render(PLUGIN_SECTION, &section_vars),
        Runtime::Native => render(NATIVE_SECTION, &section_vars),
        Runtime::Meta => String::new(),
    };
    let plugin_summary = if wasm { "- A plugin that tidies note titles before they are saved" } else { "" };
    let native = app.runtime == Runtime::Native;
    let ui_block = if !app.ui {
        String::new()
    } else {
        let (comment, dist) = if native {
            ("# Its own interface at /{id}/: ui/ (Vite, React, TypeScript), built into ui/dist.", "ui/dist")
        } else {
            ("# Its own interface at /{id}/: ui/, which runs isolated and reaches Ferrum through ui/ferrum.js.", "ui")
        };
        // An entry's id takes letters, digits and '_'; its route, the app's id.
        format!(
            "{}\nui:\n  dist: {dist}\ncontributes:\n  shell.app:\n    - {{id: {module}, route: {id}, title: \"{name}\", description: \"Notes with a status for your team\", icon: layout-grid}}",
            comment.replace("{id}", id)
        )
    };
    let ui_section = match (app.ui, native) {
        (false, _) => String::new(),
        (true, false) => format!(
            "### Its interface\n\n`ui/` is the app's own page at `/{id}/`: plain HTML, a module script and a stylesheet, no build step. It runs\nisolated, in a sandboxed frame under the workspace's bar: no session, no storage of the workspace's site,\nand everything through the bridge, `ui/ferrum.js` (its calls reach this app's entities and what `access`\nin `app.yaml` declares, within the person's rights). `ferrum-app dev --watch --install` uploads each\nchange, so the page you reload is the one you just saved.\n"
        ),
        (true, true) => format!(
            "### Its interface\n\n`ui/` is the app's own interface at `/{id}/`, built with Ferrum-Frontend's packages from a checkout of\nFerrum next to this app (`../../ferrum/web`; the packages are not on npm yet). In `ui/`: `npm run dev`\nstarts Vite, and on a Ferrum server run with `FERRUM_DEV_UI=1`, `ferrum-app dev --ui http://localhost:5173\n--no-upload` serves the app's pages from it, behind the real session, bar and bootstrap, with hot reload.\n`npm run build` writes `ui/dist`: commit it, as the crate embeds it at compile time and the server\nregisters it at start (Ferrum's ferrum-app-pos shows how); `npm run check` (CI) fails unless\n`ui/dist` matches its sources and keeps the size budgets, `npm test` runs the tests.\n"
        ),
    };
    let vars = [
        ("id", id),
        ("name", name.as_str()),
        ("publisher", app.publisher.as_str()),
        ("runtime", runtime.as_str()),
        ("platform", platform.as_str()),
        ("package", id),
        ("module", module.as_str()),
        ("entity", entity.as_str()),
        ("role", role.as_str()),
        ("crate", crate_name.as_str()),
        ("wasm_file", wasm_file.as_str()),
        ("prefix_line", prefix_line.as_str()),
        ("plugin_line", plugin_line),
        ("plugin_summary", plugin_summary),
        ("build_section", build_section.as_str()),
        ("ui_block", ui_block.as_str()),
        ("ui_section", ui_section.as_str()),
    ];
    let mut files = BTreeMap::new();
    let mut add = |path: String, template: &str| files.insert(path, render(template, &vars));
    add("app.yaml".into(), APP_YAML);
    add("README.md".into(), README);
    add("CHANGELOG.md".into(), CHANGELOG);
    add("LICENSE".into(), LICENSE);
    add(".gitignore".into(), GITIGNORE);
    add(format!("packages/{id}/package.yaml"), PACKAGE);
    add(format!("packages/{id}/entities/{}.yaml", ferrum_meta::snake(&entity)), ENTITY);
    if wasm {
        add("plugin.json".into(), PLUGIN_JSON);
        add("plugin/Cargo.toml".into(), PLUGIN_CARGO);
        add("plugin/src/lib.rs".into(), PLUGIN_LIB);
    }
    if app.ui {
        for (path, template) in if native { &UI_NATIVE[..] } else { &UI_ISOLATED[..] } {
            add((*path).into(), template);
        }
    }
    if app.ui && !native {
        files.insert("ui/ferrum.js".into(), UI_BRIDGE.to_string());
    }
    if app.ui && native {
        // The interface builds with Ferrum-Frontend's libraries (a link the developer makes).
        if let Some(ignore) = files.get_mut(".gitignore") {
            ignore.push_str("/ui/node_modules\n");
        }
    }
    check(&files, wasm, native)?;
    Ok(files)
}

/// The generated app must be valid as generated (a wasm app once its module is built).
fn check(files: &BTreeMap<String, String>, wasm: bool, native: bool) -> Result<(), String> {
    // A native app's bundle takes only its built interface (`ui.dist`, `walk_app`), which a new app
    // has yet to build.
    let bundled = |p: &str| in_bundle(p) && !(native && p.starts_with("ui/"));
    let mut bundle: BTreeMap<String, Vec<u8>> = files.iter().filter(|(p, _)| bundled(p)).map(|(p, t)| (p.clone(), t.clone().into_bytes())).collect();
    if wasm {
        // Stands in for the module the developer builds from plugin/.
        bundle.insert("plugin.wasm".into(), b"\0asm\x01\0\0\0".to_vec());
    }
    Bundle::from_files(bundle).map(drop).map_err(|e| e.to_string())
}

/// Writes the app into `dir`, which must not exist or be empty.
pub(crate) fn create(dir: &Path, files: &BTreeMap<String, String>) -> Result<(), String> {
    if dir.exists() && fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.next().is_some() {
        return Err(format!("{} already exists and is not empty", dir.display()));
    }
    for (rel, text) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, runtime: Runtime) -> NewApp {
        NewApp { id: id.into(), runtime, name: None, publisher: "acme".into(), prefix: None, ui: false }
    }

    #[test]
    fn every_runtime_scaffolds_an_interface_of_its_own() {
        for runtime in [Runtime::Meta, Runtime::Wasm, Runtime::Native] {
            let files = generate(&NewApp { ui: true, ..app("ev-charging", runtime) }).unwrap_or_else(|e| panic!("{runtime}: {e}"));
            for (path, text) in files.iter().filter(|(p, _)| *p != "ui/ferrum.js") {
                assert!(!text.contains("{{"), "{runtime} {path} keeps a placeholder");
            }
            let yaml = &files["app.yaml"];
            let manifest = ferrum_apps::AppManifest::parse(yaml).unwrap_or_else(|e| panic!("{runtime}: {e}\n{yaml}"));
            let dist = &manifest.ui.as_ref().expect("a ui section").dist;
            assert_eq!(dist, if runtime == Runtime::Native { "ui/dist" } else { "ui" });
            assert!(yaml.contains("route: ev-charging"), "{yaml}");
            if runtime == Runtime::Native {
                assert!(files.contains_key("ui/src/main.tsx") && files.contains_key("ui/package.json"));
                assert!(files["ui/src/Notes.tsx"].contains("const ENTITY = \"EvChargingNote\""));
                assert!(files[".gitignore"].contains("/ui/node_modules"));
                assert!(files["ui/package.json"].contains("cli.mjs dev"));
            } else {
                assert!(files.contains_key("ui/ferrum.js") && files.contains_key("ui/app.js"));
                assert!(files["ui/app.js"].contains("const ENTITY = \"EvNote\""));
            }
            assert!(files["README.md"].contains("### Its interface"));
        }
        let plain = generate(&app("ev-charging", Runtime::Meta)).unwrap();
        assert!(!plain["app.yaml"].contains("ui:") && !plain.keys().any(|p| p.starts_with("ui/")), "no interface unless asked");
    }

    #[test]
    fn every_runtime_scaffolds_a_valid_app_without_leftover_placeholders() {
        for runtime in [Runtime::Meta, Runtime::Wasm, Runtime::Native] {
            let files = generate(&app("ev-charging", runtime)).unwrap_or_else(|e| panic!("{runtime}: {e}"));
            for (path, text) in &files {
                assert!(!text.contains("{{"), "{runtime} {path} keeps a placeholder");
            }
            let yaml = &files["app.yaml"];
            assert!(yaml.contains("runtime: ") && yaml.contains("platform: \">=0.3, <0.4\""), "{yaml}");
            assert_eq!(files.contains_key("plugin.json"), runtime == Runtime::Wasm);
            assert_eq!(yaml.contains("prefix: Ev"), runtime != Runtime::Native);
            let entity =
                if runtime == Runtime::Native { "packages/ev-charging/entities/ev_charging_note.yaml" } else { "packages/ev-charging/entities/ev_note.yaml" };
            assert!(files.contains_key(entity), "{runtime}: {:?}", files.keys());
        }
    }

    #[test]
    fn names_and_prefixes() {
        assert_eq!(humanize("ev-charging"), "Ev Charging");
        assert_eq!(default_prefix("ev-charging").as_deref(), Some("Ev"));
        assert_eq!(default_prefix("helpdesk").as_deref(), Some("Helpdesk"));
        assert_eq!(default_prefix("inventory-tools").as_deref(), Some("Inventor"));
        assert_eq!(default_prefix("a-b").as_deref(), Some("AB"));
        assert_eq!(default_prefix("xero-sync"), None);
        assert!(generate(&app("xero-sync", Runtime::Meta)).unwrap_err().contains("--prefix"));
        assert!(generate(&NewApp { prefix: Some("Xr".into()), ..app("xero-sync", Runtime::Meta) }).unwrap_err().contains("cannot start with X"));
        assert!(generate(&NewApp { prefix: Some("Zr".into()), ..app("xero-sync", Runtime::Meta) }).is_ok());
        assert!(generate(&app("Bad_Id", Runtime::Meta)).unwrap_err().contains("id 'Bad_Id'"));
        assert!(generate(&NewApp { publisher: "Not Valid".into(), ..app("ok-id", Runtime::Meta) }).unwrap_err().contains("publisher"));
        assert!(generate(&NewApp { name: Some("Say \"hi\"".into()), ..app("ok-id", Runtime::Meta) }).is_err());
    }

    #[test]
    fn render_drops_empty_placeholder_lines_only() {
        assert_eq!(render("a\n{{x}}\nb {{x}}\n\nc\n", &[("x", "")]), "a\nb \n\nc\n");
        assert_eq!(render("{{x}}\n", &[("x", "1\n2")]), "1\n2\n");
    }
}
