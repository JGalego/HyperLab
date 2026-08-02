/// <reference types="vite/client" />

/** The WebAssembly module's bytes, as the URL Vite serves them from. */
declare module '*.wasm?url' {
  const url: string;
  export default url;
}
