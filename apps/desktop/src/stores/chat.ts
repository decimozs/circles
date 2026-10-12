import type { Attachment } from "@circles/bindings/Attachment";
import { create } from "zustand";
import { persist } from "zustand/middleware";

export type FollowUp = "steer" | "queue";

export type QueuedMessage = { id: string; text: string; attachments: Attachment[] };

type ChatState = {
  /** `enter` sends with Enter and breaks the line with Shift+Enter. `mod-enter` is the other way. */
  sendKey: "enter" | "mod-enter";
  /** What Enter does while Claude is working. Cmd+Enter does the other one. */
  followUp: FollowUp;
  /** Open chat tab per workspace. */
  activeTab: Record<string, string>;
  /** Text typed but not sent, per chat. */
  drafts: Record<string, string>;
  /** Messages waiting for the current turn to end, per chat. */
  queues: Record<string, QueuedMessage[]>;
  /** Chats with news the user has not looked at. */
  unread: Record<string, true>;
  setSendKey: (key: ChatState["sendKey"]) => void;
  setFollowUp: (mode: FollowUp) => void;
  selectTab: (workspaceId: string, sessionId: string) => void;
  setDraft: (sessionId: string, text: string) => void;
  enqueue: (sessionId: string, message: QueuedMessage) => void;
  updateQueued: (sessionId: string, id: string, text: string) => void;
  removeQueued: (sessionId: string, id: string) => void;
  shiftQueue: (sessionId: string) => QueuedMessage | undefined;
  markUnread: (workspaceId: string) => void;
  markRead: (workspaceId: string) => void;
};

export const useChat = create<ChatState>()(
  persist(
    (set, get) => ({
      sendKey: "enter",
      followUp: "steer",
      activeTab: {},
      drafts: {},
      queues: {},
      unread: {},
      setSendKey: (sendKey) => set({ sendKey }),
      setFollowUp: (followUp) => set({ followUp }),
      selectTab: (workspaceId, sessionId) =>
        set((s) => ({ activeTab: { ...s.activeTab, [workspaceId]: sessionId } })),
      setDraft: (sessionId, text) =>
        set((s) => {
          const { [sessionId]: _old, ...rest } = s.drafts;
          return { drafts: text ? { ...rest, [sessionId]: text } : rest };
        }),
      enqueue: (sessionId, message) =>
        set((s) => ({
          queues: { ...s.queues, [sessionId]: [...(s.queues[sessionId] ?? []), message] },
        })),
      updateQueued: (sessionId, id, text) =>
        set((s) => ({
          queues: {
            ...s.queues,
            [sessionId]: (s.queues[sessionId] ?? []).map((m) => (m.id === id ? { ...m, text } : m)),
          },
        })),
      removeQueued: (sessionId, id) =>
        set((s) => ({
          queues: {
            ...s.queues,
            [sessionId]: (s.queues[sessionId] ?? []).filter((m) => m.id !== id),
          },
        })),
      shiftQueue: (sessionId) => {
        const [first, ...rest] = get().queues[sessionId] ?? [];
        if (first) set((s) => ({ queues: { ...s.queues, [sessionId]: rest } }));
        return first;
      },
      markUnread: (workspaceId) => set((s) => ({ unread: { ...s.unread, [workspaceId]: true } })),
      markRead: (workspaceId) =>
        set((s) => {
          if (!s.unread[workspaceId]) return s;
          const { [workspaceId]: _read, ...rest } = s.unread;
          return { unread: rest };
        }),
    }),
    {
      name: "circles-chat",
      partialize: (s) => ({
        sendKey: s.sendKey,
        followUp: s.followUp,
        activeTab: s.activeTab,
        drafts: s.drafts,
        unread: s.unread,
      }),
    },
  ),
);
