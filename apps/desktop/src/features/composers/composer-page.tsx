import type { Composer } from "@circles/bindings/Composer";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import { Textarea } from "@circles/ui/components/textarea";
import { Loader2Icon } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { ComposerOrb } from "@/components/composer-orb";
import { JiraIcon } from "@/components/jira-icon";
import { ChatPanel } from "@/features/chat/chat-panel";
import { useJiraStatus } from "@/features/integrations/api";
import { useWorkspace } from "@/features/workspaces/api";
import { useChat } from "@/stores/chat";
import { autonomyOf, useComposer, useFirstPrompt, useStartRun } from "./api";

/** Counts what a Composer will do on its own, will ask about, and cannot do. */
export function autonomySummary(
  composer: Composer,
  tools: { name: string; tier: "read" | "write" | "external" | "destructive" }[],
) {
  const counts = { allow: 0, ask: 0, off: 0 };
  for (const t of tools) counts[autonomyOf(composer, t.name, t.tier)] += 1;
  return counts;
}

/** The outside services this Composer uses, and whether each one works now. Nothing when none. */
function IntegrationBadges({ composer }: { composer: Composer }) {
  const jira = useJiraStatus().data;
  if (!composer.integrations.includes("jira")) return null;
  const connected = !!jira?.connected && !jira.needs_reconnect;
  return (
    <ul aria-label="Integrations" className="flex flex-wrap items-center gap-1.5">
      <li>
        <Badge
          variant="outline"
          className={connected ? "gap-1.5 text-success" : "gap-1.5 text-warning"}
        >
          <JiraIcon className="size-3" />
          Jira
          <span className="flex items-center gap-1">
            <span
              aria-hidden
              className={`size-1.5 rounded-full ${connected ? "bg-success" : "bg-warning"}`}
            />
            {connected ? "Connected" : jira?.needs_reconnect ? "Reconnect needed" : "Not connected"}
          </span>
        </Badge>
      </li>
    </ul>
  );
}

/**
 * What a Composer shows before its chat exists: what it is, and a box to give it something to do.
 * Its instructions are not repeated here. They are in Edit.
 */
function StartRun({ composer }: { composer: Composer }) {
  const navigate = useNavigate();
  const start = useStartRun();
  const [text, setText] = useState("");
  const example = useFirstPrompt(composer);

  const go = () =>
    start.mutate(
      { composerId: composer.id, message: text.trim() || undefined },
      {
        onSuccess: (session) => {
          useChat.getState().selectTab(composer.workspace_id, session.id);
          navigate(`/composer/${composer.id}/run/${session.id}`);
        },
        onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
      },
    );

  return (
    <div className="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 px-6">
      <div className="flex items-center gap-3">
        <ComposerOrb seed={composer.id} className="size-10" />
        <div className="flex flex-col gap-1.5">
          <h1 className="text-lg font-semibold">{composer.name}</h1>
          <IntegrationBadges composer={composer} />
        </div>
      </div>
      <Textarea
        aria-label="What should it do?"
        rows={4}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && text.trim()) go();
          // Tab takes the example into an empty box.
          if (e.key === "Tab" && !e.shiftKey && !text && example) {
            e.preventDefault();
            setText(example);
          }
        }}
        placeholder={
          example
            ? `${example}  (Tab)`
            : "What should it do? For example: add dark mode to portfolio and fix the failing tests."
        }
      />
      <div className="flex justify-end">
        <Button disabled={start.isPending} onClick={go}>
          {start.isPending ? <Loader2Icon className="animate-spin" /> : null}
          Run
        </Button>
      </div>
    </div>
  );
}

/** One Composer: its runs as chats, in the same chat the workspaces use. */
export function ComposerPage() {
  const { id, runId } = useParams();
  const composers = useComposerLookup(id);
  const composer = composers.composer;
  const workspace = useWorkspace(composer?.workspace_id).data;
  const select = useChat((s) => s.selectTab);

  // A link to a run opens that run's tab.
  useEffect(() => {
    if (runId && composer) select(composer.workspace_id, runId);
  }, [runId, composer, select]);

  if (composers.loading) return null;
  if (!composer || !workspace) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        That Composer does not exist any more.
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <ChatPanel
        workspace={workspace}
        composer={composer}
        empty={<StartRun composer={composer} />}
      />
    </div>
  );
}

function useComposerLookup(id: string | undefined) {
  const { data, isLoading } = useComposer(id);
  return { composer: data, loading: isLoading };
}
