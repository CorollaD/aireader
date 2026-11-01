import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/** The OCR SDK ships its own worker, unused here (see src/ocr/worker.ts), and much of OpenCV in it. */
function dropSdkWorker(): Plugin {
  return {
    name: "drop-sdk-worker",
    generateBundle(_, bundle) {
      for (const name of Object.keys(bundle)) if (name.includes("worker-entry-")) delete bundle[name];
    },
  };
}

// Tauri expects a fixed dev server port and serves the built files from ../dist.
export default defineConfig({
  plugins: [react(), dropSdkWorker()],
  clearScreen: false,
  // ONNX Runtime without WebGPU, whose WebAssembly build is twice the size.
  resolve: { alias: [{ find: /^onnxruntime-web$/, replacement: "onnxruntime-web/wasm" }] },
  worker: { format: "es", plugins: () => [dropSdkWorker()] },
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "safari15", outDir: "dist", emptyOutDir: true },
});
