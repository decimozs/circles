import { useEffect, useRef } from "react";
import { useLocation, useNavigate, useNavigationType } from "react-router";

type HistoryState = { idx?: number } | null;

/** Back and forward availability for the title bar buttons. */
export function useHistoryNav() {
  const navigate = useNavigate();
  const location = useLocation();
  const navType = useNavigationType();
  const maxIdx = useRef(0);

  const idx = (window.history.state as HistoryState)?.idx ?? 0;

  // biome-ignore lint/correctness/useExhaustiveDependencies: recompute when the location changes
  useEffect(() => {
    maxIdx.current = navType === "PUSH" ? idx : Math.max(maxIdx.current, idx);
  }, [location, navType, idx]);

  const canGoBack = idx > 0;
  const canGoForward = idx < maxIdx.current;

  return {
    canGoBack,
    canGoForward,
    back: () => navigate(-1),
    forward: () => navigate(1),
  };
}
