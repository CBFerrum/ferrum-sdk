### Native code

A native app's code is a Rust crate compiled into a Ferrum distribution (reviewed platform
code, not sandboxed). Its crate, `ferrum-app-{{id}}` with its `Cargo.toml` next to this
`app.yaml`, depends on the platform crates (`ferrum-kernel`, `ferrum-meta`, `ferrum-domain`) by
git tag, embeds `packages/` at compile time, and exposes `MANIFEST` (this `app.yaml`),
`packages()`, `register(&mut HookRegistry)`, `reports()` and `maintenance(&Kernel)`. The server
enables it as a cargo feature; the bundle this directory packs carries the manifest and packages
for the store.
