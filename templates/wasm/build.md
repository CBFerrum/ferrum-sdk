### The plugin

The plugin's Rust source is in `plugin/`; it is compiled to WebAssembly and shipped as
`plugin.wasm` (which `ferrum-app check` and `pack` need):

    rustup target add wasm32-unknown-unknown
    cargo build --manifest-path plugin/Cargo.toml --release --target wasm32-unknown-unknown
    cp plugin/target/wasm32-unknown-unknown/release/{{wasm_file}} plugin.wasm

`plugin.json` declares what the plugin receives and may do (hooks, routes, sockets, schedules,
records, hosts); administrators approve exactly that when they install the app.
