import type { JiraSite } from "@circles/bindings/JiraSite";
import type { JiraStatus } from "@circles/bindings/JiraStatus";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { Loader2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { JiraIcon } from "@/components/jira-icon";
import { ipc } from "@/lib/ipc";
import {
  useChooseJiraSite,
  useConnectJira,
  useDisconnectJira,
  useJiraStatus,
  useSignInJira,
} from "./api";

/** The form for an API token, for when signing in with Atlassian is not possible. */
function TokenForm({
  status,
  onDone,
  onBack,
}: {
  status: JiraStatus | undefined;
  onDone: () => void;
  onBack: () => void;
}) {
  const connect = useConnectJira();
  const reconnecting = status?.method === "token";
  const [site, setSite] = useState(reconnecting ? (status?.site ?? "") : "");
  const [email, setEmail] = useState(reconnecting ? (status?.email ?? "") : "");
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = () => {
    setError(null);
    connect.mutate(
      { site, email, token },
      {
        onSuccess: (done) => {
          // The token is never kept once the form is gone.
          setToken("");
          toast(`Connected to Jira as ${done.account_name ?? done.email}`);
          onDone();
        },
        onError: (e) => setError((e as { message?: string }).message ?? String(e)),
      },
    );
  };

  return (
    <>
      <p className="text-sm text-muted-foreground">
        Jira Cloud only. Create an API token at id.atlassian.com, under Security. It has the same
        permissions as your account, so use an account that can only see what you want agents to
        see. The token is kept in the macOS Keychain, never in a file.
      </p>
      <div className="flex flex-col gap-3 text-sm">
        <div className="flex flex-col gap-1">
          <span className="font-medium">Site</span>
          <Input
            aria-label="Jira site"
            value={site}
            onChange={(e) => setSite(e.target.value)}
            placeholder="your-team.atlassian.net"
            autoComplete="off"
          />
        </div>
        <div className="flex flex-col gap-1">
          <span className="font-medium">Email</span>
          <Input
            aria-label="Atlassian email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="you@company.com"
            autoComplete="off"
          />
        </div>
        <div className="flex flex-col gap-1">
          <span className="font-medium">API token</span>
          <Input
            aria-label="API token"
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </div>
        {error ? (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        ) : null}
      </div>
      <DialogFooter className="sm:justify-between">
        <Button variant="ghost" onClick={onBack}>
          Sign in with Atlassian instead
        </Button>
        <Button
          disabled={connect.isPending || !site.trim() || !email.trim() || !token}
          onClick={submit}
        >
          {connect.isPending ? <Loader2Icon className="animate-spin" /> : null}
          {reconnecting ? "Reconnect" : "Connect"}
        </Button>
      </DialogFooter>
    </>
  );
}

/** After signing in with an account that reaches several Jira sites: which one to use. */
function SitePicker({ sites, onDone }: { sites: JiraSite[]; onDone: () => void }) {
  const choose = useChooseJiraSite();
  const [picked, setPicked] = useState(sites[0]?.cloud_id ?? "");
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <fieldset className="flex flex-col gap-1 text-sm">
        <legend className="mb-1 font-medium">Which Jira site?</legend>
        {sites.map((site) => (
          <label
            key={site.cloud_id}
            className="flex items-center gap-2 rounded-md border border-border p-2"
          >
            <input
              type="radio"
              name="jira-site"
              checked={picked === site.cloud_id}
              onChange={() => setPicked(site.cloud_id)}
            />
            <span className="min-w-0 flex-1 truncate">{site.name}</span>
            <span className="truncate text-xs text-muted-foreground">
              {site.url.replace("https://", "")}
            </span>
          </label>
        ))}
      </fieldset>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}
      <DialogFooter>
        <Button
          disabled={choose.isPending || !picked}
          onClick={() =>
            choose.mutate(picked, {
              onSuccess: (done) => {
                toast(`Connected to Jira as ${done.account_name}`);
                onDone();
              },
              onError: (e) => setError((e as { message?: string }).message ?? String(e)),
            })
          }
        >
          Use this site
        </Button>
      </DialogFooter>
    </>
  );
}

/** The browser sign-in: one button, then a wait while the person finishes in the browser. */
function SignInPane({
  onDone,
  onToken,
  waiting,
}: {
  onDone: () => void;
  onToken: () => void;
  waiting: ReturnType<typeof useSignInJira>;
}) {
  const [error, setError] = useState<string | null>(null);
  const start = () => {
    setError(null);
    waiting.mutate(undefined, {
      onSuccess: (status) => {
        if (status.connected) {
          toast(`Connected to Jira as ${status.account_name}`);
          onDone();
        }
      },
      onError: (e) => setError((e as { message?: string }).message ?? String(e)),
    });
  };
  if (waiting.isPending) {
    return (
      <div className="flex flex-col items-center gap-4 py-6">
        <Loader2Icon className="size-8 animate-spin text-muted-foreground" />
        <p className="text-sm">Waiting for the browser…</p>
        <Button
          variant="outline"
          onClick={() => {
            ipc.jiraCancelSignIn().catch(() => {});
          }}
        >
          Cancel
        </Button>
      </div>
    );
  }
  return (
    <>
      <p className="text-sm text-muted-foreground">
        Your browser opens Atlassian's page. You approve what Circles may do, then come back here.
        Circles can read and change Jira issues as you, and cannot delete anything. You can end the
        access any time from your Atlassian account.
      </p>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}
      <DialogFooter className="sm:justify-between">
        <Button variant="ghost" onClick={onToken}>
          Use an API token instead
        </Button>
        <Button onClick={start}>Sign in with Atlassian</Button>
      </DialogFooter>
    </>
  );
}

function ConnectDialog({
  open,
  onClose,
  status,
}: {
  open: boolean;
  onClose: () => void;
  status: JiraStatus | undefined;
}) {
  const reconnecting = !!status?.connected;
  const signIn = useSignInJira();
  const [mode, setMode] = useState<"signin" | "token">(
    status?.method === "token" ? "token" : "signin",
  );
  const choosing = (status?.choose_site.length ?? 0) > 0;
  const close = () => {
    if (signIn.isPending) ipc.jiraCancelSignIn().catch(() => {});
    onClose();
  };
  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{reconnecting ? "Reconnect Jira" : "Connect Jira"}</DialogTitle>
          <DialogDescription className="sr-only">
            Connect Jira Cloud by signing in with Atlassian, or with an API token.
          </DialogDescription>
        </DialogHeader>
        {choosing ? (
          <SitePicker sites={status?.choose_site ?? []} onDone={onClose} />
        ) : mode === "signin" ? (
          <SignInPane onDone={onClose} onToken={() => setMode("token")} waiting={signIn} />
        ) : (
          <TokenForm status={status} onDone={onClose} onBack={() => setMode("signin")} />
        )}
      </DialogContent>
    </Dialog>
  );
}

/** The Jira connection: who it is, and Connect, Reconnect and Disconnect. */
export function JiraCard() {
  const status = useJiraStatus().data;
  const disconnect = useDisconnectJira();
  const [dialog, setDialog] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const connected = !!status?.connected;

  return (
    <section
      aria-label="Jira"
      className="mb-4 flex items-start gap-3 rounded-lg border border-border p-4"
    >
      <JiraIcon className="mt-0.5 size-7" />
      <div className="min-w-0 flex-1">
        <h2 className="text-sm font-medium">Jira</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          Lets chats and Composers search and read issues, and, when you allow it, comment, move,
          assign, edit and create them.
        </p>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
          {!connected ? (
            <Badge variant="secondary" data-testid="jira-state">
              Not connected
            </Badge>
          ) : status?.needs_reconnect ? (
            <Badge
              data-testid="jira-state"
              className="bg-warning/15 text-warning"
              title={
                status.method === "oauth"
                  ? "The Atlassian sign-in ended. Sign in again."
                  : "Jira turned the token down. Reconnect with a new token."
              }
            >
              <span aria-hidden className="size-1.5 rounded-full bg-warning" />
              Reconnect needed
            </Badge>
          ) : (
            <Badge data-testid="jira-state" className="bg-success/15 text-success">
              <span aria-hidden className="size-1.5 rounded-full bg-success" />
              Connected
            </Badge>
          )}
          {connected ? (
            <span className="text-muted-foreground" data-testid="jira-details">
              {[
                status?.account_name && status.account_name !== "your account"
                  ? status.account_name
                  : null,
                status?.site,
                status?.method === "oauth" ? "Atlassian sign-in" : "API token",
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
          ) : null}
        </div>
        {connected && status?.needs_reconnect ? (
          <p className="mt-1 text-xs text-muted-foreground">
            {status.method === "oauth"
              ? "The Atlassian sign-in ended. Sign in again."
              : "Jira turned the token down. Reconnect with a new token."}
          </p>
        ) : null}
      </div>
      <div className="flex shrink-0 gap-2">
        {connected ? (
          <>
            <Button size="sm" variant="outline" onClick={() => setDialog(true)}>
              Reconnect
            </Button>
            <Button size="sm" variant="outline" onClick={() => setConfirming(true)}>
              Disconnect
            </Button>
          </>
        ) : (
          <Button size="sm" onClick={() => setDialog(true)}>
            Connect
          </Button>
        )}
      </div>
      {dialog ? <ConnectDialog open onClose={() => setDialog(false)} status={status} /> : null}
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Disconnect Jira?</DialogTitle>
            <DialogDescription>
              The token is removed from the Keychain and Jira is switched off for every Composer.
              Chats lose it too, until you connect again.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={disconnect.isPending}
              onClick={() =>
                disconnect.mutate(undefined, {
                  onSuccess: () => {
                    setConfirming(false);
                    toast("Jira disconnected");
                  },
                  onError: (e) => toast.error(String((e as { message?: string }).message ?? e)),
                })
              }
            >
              Disconnect
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}
