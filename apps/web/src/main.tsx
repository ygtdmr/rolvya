import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { mountStyles } from "./platform/styles";
import { App } from "./app/App";
mountStyles();
const root = document.getElementById("root");
if (!root) throw Error("Application root missing");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
