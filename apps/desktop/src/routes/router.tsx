import { createHashRouter, type RouteObject } from "react-router";
import { AppShell } from "@/components/shell/app-shell";
import { AgentDebug } from "@/features/agent/agent-debug";
import { AuditPage } from "@/features/composers/audit-page";
import { ComposerPage } from "@/features/composers/composer-page";
import { ComposersHome } from "@/features/composers/composers-home";
import { HomePage } from "@/features/home/home-page";
import { PullRequestsPage } from "@/features/prs/pull-requests-page";
import { SettingsPage } from "@/features/settings/settings-page";
import { DesignGallery } from "@/routes/design";
import { DebugPage, NotFoundPage, WorkspacePage } from "@/routes/pages";
import { RouteError } from "@/routes/route-error";

export const routes: RouteObject[] = [
  {
    element: <AppShell />,
    // The shell itself failed. Nothing of it can be shown.
    errorElement: <RouteError />,
    children: [
      {
        // A page that fails shows the error inside the shell, so the sidebar stays.
        errorElement: <RouteError />,
        children: [
          { index: true, element: <HomePage /> },
          { path: "workspace/:id", element: <WorkspacePage /> },
          { path: "composers", element: <ComposersHome /> },
          { path: "pull-requests", element: <PullRequestsPage /> },
          { path: "composers/audit", element: <AuditPage /> },
          { path: "composer/:id", element: <ComposerPage /> },
          { path: "composer/:id/run/:runId", element: <ComposerPage /> },
          { path: "settings/*", element: <SettingsPage /> },
          { path: "design", element: <DesignGallery /> },
          { path: "debug", element: <DebugPage /> },
          { path: "debug/agent", element: <AgentDebug /> },
          { path: "*", element: <NotFoundPage /> },
        ],
      },
    ],
  },
];

// Hash routing keeps deep links working inside the Tauri webview without a server.
export const router = createHashRouter(routes);
