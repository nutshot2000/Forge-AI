//! forge core: the world model, physics and scripting. Compiles natively (the engine,
//! editor host and MCP server live in the `forge` binary) and to WebAssembly (the web
//! player used for exported games, in `web/`).

pub mod physics;
pub mod sim;
pub mod world;
