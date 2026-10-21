// The setting of the client, read before it starts (src/lib/api/config.ts).
// Nothing is set here: the server that serves the page is the core. The browser
// version of the hub (tools/web-bundle.sh) replaces this file with
//   globalThis.__NOTES_API__ = { core: { module: "/wasm/notes_wasm.js", packages: "/wasm/packages.json" } };
