// {{name}}'s own interface at /{{id}}/ (Ferrum docs/plan/apps-ui.md §10): its home under the
// platform's bar, and the records of its entities in the records frame, each page loaded when
// first opened (§12.9: the first script within 200 KB compressed).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, Link, Outlet, RouterProvider, useRouteError } from "react-router";
import { routeBase, shell } from "@ferrum/client";
import { ConfirmHost, EmptyState, ErrorState, Toaster } from "@ferrum/kit";
import { RequireAuth, setFrameMode } from "@ferrum/records/frame";
import { Notes } from "./Notes";
import "./styles.css";

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 } } });

function RouteError() {
  return <ErrorState error={useRouteError()} onRetry={() => window.location.reload()} />;
}

/** Every page tells the shell it started. */
function Started() {
  useEffect(() => shell().ready(), []);
  return null;
}

setFrameMode("app", ["{{package}}"]);

const records = () => import("./records");
const router = createBrowserRouter(
  [
    {
      path: "/",
      element: (
        <>
          <Started />
          <Outlet />
        </>
      ),
      errorElement: <RouteError />,
      children: [
        { index: true, element: <Notes /> },
        {
          element: <RequireAuth />,
          children: [
            { path: "e/:entity", lazy: () => records().then((m) => ({ Component: m.ListView })) },
            { path: "e/:entity/:id", lazy: () => records().then((m) => ({ Component: m.FormView })) },
          ],
        },
        {
          path: "*",
          element: (
            <EmptyState title="Page not found">
              <Link to="/">Go to {{name}}</Link>
            </EmptyState>
          ),
        },
      ],
    },
  ],
  { basename: routeBase("{{id}}") },
);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster />
      <ConfirmHost />
    </QueryClientProvider>
  </StrictMode>,
);
