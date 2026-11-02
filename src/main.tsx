import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { mobile } from "./platform";

document.documentElement.classList.toggle("mobile", mobile);

async function start() {
  // In a plain browser during development, fake the Rust backend.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./dev/mock")).installMock();
  }
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

start();
