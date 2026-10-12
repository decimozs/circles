import { useEffect } from "react";
import { isRouteErrorResponse, useNavigate, useRouteError } from "react-router";
import { ErrorScreen } from "@/components/error-screen";

/** What a page that threw, or a request that failed, shows instead of the router's own page. */
export function RouteError() {
  const caught = useRouteError();
  const navigate = useNavigate();
  useEffect(() => {
    console.error("A page failed", caught);
  }, [caught]);

  if (isRouteErrorResponse(caught)) {
    const missing = caught.status === 404;
    const error = new Error(
      missing
        ? "There is nothing at this address."
        : `${caught.status} ${caught.statusText}`.trim(),
    );
    return (
      <ErrorScreen
        error={error}
        title={missing ? "Page not found" : "Something went wrong"}
        onHome={() => navigate("/")}
      />
    );
  }
  const error = caught instanceof Error ? caught : new Error(String(caught));
  return <ErrorScreen error={error} onHome={() => navigate("/")} />;
}
