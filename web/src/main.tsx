import { createRoot } from "react-dom/client";
import "@fontsource/vt323";
import "@/styles/index.css";
import { App } from "@/app/App";
import { StoreProvider } from "@/store";
import { ErrorBoundary } from "@/shared/ui/ErrorBoundary";

createRoot(document.getElementById("root")!).render(
  <ErrorBoundary label="Ứng dụng">
    <StoreProvider>
      <App />
    </StoreProvider>
  </ErrorBoundary>,
);
