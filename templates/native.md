### Native code

A native app's code is a Rust crate compiled into a Ferrum distribution (reviewed platform
code, not sandboxed). Its crate, `ferrum-app-{{id}}` with its `Cargo.toml` next to this
`app.yaml`, depends on the platform crates (`ferrum-kernel`, `ferrum-meta`, `ferrum-domain`) by
git (`branch = "main"`), embeds `packages/` at compile time, and exposes `MANIFEST` (this `app.yaml`),
`packages()`, `register(&mut HookRegistry)`, `reports()` and `maintenance(&Kernel, tenant)` (the platform's app contract, *Upkeep*: the work of one
workspace, which the platform calls for each workspace with the app installed). The server
enables it as a cargo feature; the bundle this directory packs carries the manifest and packages
for the store.
