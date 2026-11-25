import { createRoot } from "react-dom/client";
import { App } from "./App";
import { StoreProvider } from "./store";
import { ErrorBoundary } from "./ErrorBoundary";
import "@fontsource/vt323";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <ErrorBoundary label="Ứng dụng">
    <StoreProvider>
      <App />
    </StoreProvider>
  </ErrorBoundary>,
);
