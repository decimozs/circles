import { Component, type ErrorInfo, type ReactNode } from "react";
import { ErrorScreen } from "@/components/error-screen";

type State = { error: Error | null };

/** Catches a crash in the screen so the person gets a way out and something to report. */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("The screen crashed", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return <ErrorScreen error={error} />;
  }
}
