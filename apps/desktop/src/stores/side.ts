import { create } from "zustand";

/** One side question of a chat: what was asked, and the answer as it arrives. */
export type SideQuestion = {
  question: string;
  answer: string;
  status: "running" | "done" | "failed";
  error: string | null;
  /** The chat's own context is behind the answer. False: only the question was asked. */
  withContext: boolean;
};

type SideState = {
  bySession: Record<string, SideQuestion | undefined>;
  start: (sessionId: string, question: string) => void;
  event: (
    sessionId: string,
    kind: "started" | "delta" | "done" | "failed",
    text: string,
    withContext: boolean,
  ) => void;
  close: (sessionId: string) => void;
};

export const useSide = create<SideState>()((set) => ({
  bySession: {},
  start: (sessionId, question) =>
    set((s) => ({
      bySession: {
        ...s.bySession,
        [sessionId]: { question, answer: "", status: "running", error: null, withContext: true },
      },
    })),
  event: (sessionId, kind, text, withContext) =>
    set((s) => {
      const current = s.bySession[sessionId];
      // Events of a question that was closed are ignored.
      if (!current) return s;
      const next: SideQuestion =
        kind === "started"
          ? { ...current, withContext }
          : kind === "delta"
            ? { ...current, answer: current.answer + text }
            : kind === "done"
              ? { ...current, answer: text || current.answer, status: "done" }
              : { ...current, status: "failed", error: text };
      return { bySession: { ...s.bySession, [sessionId]: next } };
    }),
  close: (sessionId) =>
    set((s) => {
      const { [sessionId]: _gone, ...rest } = s.bySession;
      return { bySession: rest };
    }),
}));
