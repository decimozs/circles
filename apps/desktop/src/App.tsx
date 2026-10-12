import { RouterProvider } from "react-router";
import { router } from "@/routes/router";

export function App() {
  return <RouterProvider router={router} />;
}
