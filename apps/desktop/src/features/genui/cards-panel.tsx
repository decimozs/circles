import { useCards } from "./api";
import { CardView } from "./card-view";

/**
 * The cards of a chat, above its message box: the ones that wait for an answer, and the last
 * couple that were answered or closed, so what was asked and answered stays in view.
 */
export function CardsPanel({ sessionId }: { sessionId: string }) {
  const cards = useCards(sessionId).data ?? [];
  const waiting = cards.filter((c) => c.status === "pending");
  const recent = cards.filter((c) => c.status !== "pending" && c.status !== "shown").slice(-2);
  const shown = cards.filter((c) => c.status === "shown").slice(-1);
  if (waiting.length === 0 && recent.length === 0 && shown.length === 0) return null;
  return (
    <section
      aria-label="Cards from the agent"
      className="mx-auto flex max-h-[50vh] w-full max-w-3xl flex-col gap-2 overflow-y-auto px-6 pb-2"
    >
      {[...shown, ...recent, ...waiting].map((stored) => (
        <CardView key={stored.uid} stored={stored} />
      ))}
    </section>
  );
}
