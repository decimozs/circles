import type { ClaudeAccount } from "@circles/bindings/ClaudeAccount";
import type { GithubAccounts } from "@circles/bindings/GithubAccounts";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDownIcon, Loader2Icon } from "lucide-react";
import type { ReactNode } from "react";
import { SiGithub } from "react-icons/si";
import { toast } from "sonner";
import { ClaudeIcon } from "@/components/claude-icon";
import { ipc } from "@/lib/ipc";

const githubKey = ["account", "github"] as const;
const claudeKey = ["account", "claude"] as const;

/** The same card as the Jira connection: a mark, what it is for, its state, and its buttons. */
function AccountCard({
  name,
  icon,
  description,
  state,
  details,
  hint,
  actions,
}: {
  name: string;
  icon: ReactNode;
  description: string;
  state: ReactNode;
  details?: string;
  hint?: ReactNode;
  actions: ReactNode;
}) {
  return (
    <section
      aria-label={name}
      className="mb-4 flex items-start gap-3 rounded-lg border border-border p-4"
    >
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center">{icon}</div>
      <div className="min-w-0 flex-1">
        <h2 className="text-sm font-medium">{name}</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
          {state}
          {details ? (
            <span className="text-muted-foreground" data-testid={`${name.toLowerCase()}-details`}>
              {details}
            </span>
          ) : null}
        </div>
        {hint ? <p className="mt-1 text-xs text-muted-foreground">{hint}</p> : null}
      </div>
      <div className="flex shrink-0 gap-2">{actions}</div>
    </section>
  );
}

const connected = (name: string) => (
  <Badge data-testid={`${name}-state`} className="bg-success/15 text-success">
    <span aria-hidden className="size-1.5 rounded-full bg-success" />
    Connected
  </Badge>
);

const notConnected = (name: string, label = "Not signed in") => (
  <Badge variant="secondary" data-testid={`${name}-state`}>
    {label}
  </Badge>
);

function Command({ children }: { children: string }) {
  return <code className="rounded bg-muted px-1 py-0.5 font-mono text-2xs">{children}</code>;
}

/** The GitHub account the GitHub CLI uses, and a way to switch when several are signed in. */
export function GithubAccountCard() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: githubKey,
    queryFn: ipc.githubAccounts,
    refetchOnWindowFocus: true,
  });
  const switchTo = useMutation({
    mutationFn: (login: string) => ipc.githubSwitchAccount(login),
    onSuccess: (accounts: GithubAccounts) => {
      queryClient.setQueryData(githubKey, accounts);
      // The picker of repositories belongs to the account that is active.
      queryClient.invalidateQueries({ queryKey: ["github"] });
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
  });
  const data = query.data;
  const active = data?.accounts.find((a) => a.active) ?? data?.accounts[0];
  const others = data?.accounts.filter((a) => a !== active) ?? [];

  let state: ReactNode = notConnected("github", "Checking");
  let hint: ReactNode = null;
  if (data && !data.installed) {
    state = notConnected("github", "Not installed");
    hint = (
      <>
        Install the GitHub CLI with <Command>{data.fix ?? "brew install gh"}</Command>.
      </>
    );
  } else if (data && !active) {
    state = notConnected("github");
    hint = (
      <>
        Run <Command>{data.fix ?? "gh auth login"}</Command> in a terminal, then come back.
      </>
    );
  } else if (active) {
    state = connected("github");
  }

  return (
    <AccountCard
      name="GitHub"
      icon={<SiGithub aria-hidden="true" className="size-6" />}
      description="Lets Circles list and clone your repositories, and open, check and merge pull requests, through the GitHub CLI. Circles never stores a token."
      state={state}
      details={
        active
          ? [`@${active.login}`, others.length > 0 ? `${others.length} more signed in` : null]
              .filter(Boolean)
              .join(" · ")
          : undefined
      }
      hint={hint}
      actions={
        <>
          {others.length > 0 ? (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<Button size="sm" variant="outline" disabled={switchTo.isPending} />}
              >
                {switchTo.isPending ? <Loader2Icon className="animate-spin" /> : null}
                Switch account
                <ChevronDownIcon />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {others.map((a) => (
                  <DropdownMenuItem key={a.login} onClick={() => switchTo.mutate(a.login)}>
                    @{a.login}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
          <Button
            size="sm"
            variant="outline"
            disabled={query.isFetching}
            onClick={() => query.refetch()}
          >
            Refresh
          </Button>
        </>
      }
    />
  );
}

const PLANS: Record<string, string> = {
  pro: "Pro",
  max: "Max",
  team: "Team",
  enterprise: "Enterprise",
};

function claudeDetails(account: ClaudeAccount): string {
  const way =
    account.method === "claude.ai"
      ? [account.plan ? `Claude ${PLANS[account.plan] ?? account.plan}` : "Claude subscription"]
      : account.method
        ? [account.method.replace(/_/g, " ")]
        : [];
  return [account.email, ...way].filter(Boolean).join(" · ");
}

/** The Claude account Claude Code is signed in with. */
export function ClaudeAccountCard() {
  const query = useQuery({
    queryKey: claudeKey,
    queryFn: ipc.claudeAccount,
    refetchOnWindowFocus: true,
  });
  const data = query.data;

  let state: ReactNode = notConnected("claude", "Checking");
  let hint: ReactNode = null;
  if (data && !data.found) {
    state = notConnected("claude", "Not installed");
    hint = (
      <>
        Install Claude Code from <Command>https://claude.com/claude-code</Command>, then run{" "}
        <Command>claude</Command> once to sign in.
      </>
    );
  } else if (data && !data.signed_in) {
    state = notConnected("claude");
    hint = (
      <>
        Run <Command>{data.fix ?? "claude"}</Command> in a terminal and sign in, then come back.
      </>
    );
  } else if (data) {
    state = connected("claude");
  }

  return (
    <AccountCard
      name="Claude"
      icon={<ClaudeIcon className="size-7" />}
      description="Runs your agents through Claude Code, with your own Claude subscription. Circles never sees your login."
      state={state}
      details={data?.signed_in ? claudeDetails(data) : undefined}
      hint={hint}
      actions={
        <Button
          size="sm"
          variant="outline"
          disabled={query.isFetching}
          onClick={() => query.refetch()}
        >
          Refresh
        </Button>
      }
    />
  );
}
