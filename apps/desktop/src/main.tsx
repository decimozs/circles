import { TooltipProvider } from "@circles/ui/components/tooltip";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "@/App";
import { ErrorBoundary } from "@/components/error-boundary";
import { Toaster } from "@/components/sonner";
import { ThemeProvider } from "@/components/theme-provider";
import "@xterm/xterm/css/xterm.css";
import "@/styles/globals.css";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <TooltipProvider>
          <ErrorBoundary>
            <App />
          </ErrorBoundary>
          <Toaster />
        </TooltipProvider>
      </ThemeProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
