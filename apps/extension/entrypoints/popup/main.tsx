import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

/**
 * Popup entry point.
 *
 * The popup is a pure client: it sends messages to the service worker and
 * renders replies. It never holds a value — a fill or a copy happens in the
 * content script of the active tab.
 */

const container = document.getElementById("root");
if (!container) {
  throw new Error("the popup root element is missing");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
