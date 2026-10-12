import type { ComposerInput } from "@circles/bindings/ComposerInput";
import type { Limits } from "@circles/bindings/Limits";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Loader2Icon } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { useSettingsView } from "@/features/settings/api";
import { useUi } from "@/stores/ui";
import { useCreateComposer } from "./api";
import {
  InstructionsField,
  IntegrationsField,
  NameModeFields,
  RepositoriesField,
} from "./composer-fields";
import { useComposerDialog } from "./store";

const EMPTY_LIMITS: Limits = {
  max_steps: 100,
  max_children_at_once: 3,
  max_children_total: 10,
  max_minutes: 120,
  max_budget_usd: null,
};

const blank = (): ComposerInput => ({
  name: "",
  instructions: "",
  mode: null,
  autonomy: {},
  projects: null,
  integrations: [],
  limits: { ...EMPTY_LIMITS },
  allow_override: false,
  notes: "",
  model: null,
  effort: null,
});

/**
 * Creates a Composer. The model, what it may do on its own and its limits are not asked here: a
 * new one starts with the safe defaults, or a template's. Editing one is done in its settings.
 */
export function ComposerDialog() {
  const dialog = useComposerDialog((s) => s.dialog);
  const close = useComposerDialog((s) => s.close);
  // Like a chat, a new Composer starts in the permission mode the settings name.
  const settingsMode = useSettingsView().data?.settings.agents.permission_mode ?? "manual";
  const navigate = useNavigate();
  const create = useCreateComposer();
  const [form, setForm] = useState<ComposerInput>(blank);
  const [error, setError] = useState<string | null>(null);

  // The form starts from what is being edited, or from the template.
  // biome-ignore lint/correctness/useExhaustiveDependencies: filled once each time it opens
  useEffect(() => {
    if (!dialog) return;
    setError(null);
    setForm({
      ...blank(),
      ...dialog.seed,
      mode: dialog.seed?.mode ?? settingsMode,
      limits: { ...EMPTY_LIMITS, ...dialog.seed?.limits },
    });
  }, [dialog]);

  const set = <K extends keyof ComposerInput>(key: K, value: ComposerInput[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const save = () => {
    setError(null);
    // A new Composer opens on its chat, with the right pane out of the way.
    create.mutate(form, {
      onSuccess: (composer) => {
        close();
        toast("Composer created");
        useUi.getState().setRightOpen(false);
        navigate(`/composer/${composer.id}`);
      },
      onError: (e) => setError((e as { message?: string })?.message ?? String(e)),
    });
  };
  const busy = create.isPending;

  return (
    <Dialog open={dialog !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent className="max-h-[88vh] min-w-0 overflow-x-hidden overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>New Composer</DialogTitle>
          <DialogDescription>
            A Composer manages work for you: it creates workspaces, starts agents in them and
            follows their progress. Choose what it may do on its own.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-w-0 flex-col gap-4">
          <NameModeFields
            name={form.name}
            mode={form.mode}
            onName={(name) => set("name", name)}
            onMode={(mode) => set("mode", mode)}
          />
          <InstructionsField
            value={form.instructions}
            onChange={(value) => set("instructions", value)}
          />

          <RepositoriesField
            value={form.projects}
            onChange={(projects) => set("projects", projects)}
          />

          <IntegrationsField
            value={form.integrations}
            onChange={(integrations) => set("integrations", integrations)}
            onSettings={() => {
              close();
              navigate("/settings/integrations");
            }}
          />

          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            Cancel
          </Button>
          <Button
            disabled={busy || !form.name.trim() || form.projects?.length === 0}
            onClick={save}
          >
            {busy ? <Loader2Icon className="animate-spin" /> : null}
            Create Composer
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
