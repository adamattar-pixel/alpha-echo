import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "./globals.css";
import { IntelligenceDesk } from "./intelligence-desk";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Missing #root element");
}

createRoot(rootElement).render(
  <StrictMode>
    <IntelligenceDesk />
  </StrictMode>,
);
