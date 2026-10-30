import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { WorkStep } from "./timeline";
import { ChatStoppedContext } from "./tool-calls";
import { WorkBlock } from "./work-block";

const unfinished: WorkStep[] = [
  { kind: "tool", key: "t1", call: { id: "c1", name: "Bash", input: {}, children: [] } },
];

describe("WorkBlock", () => {
  it("spins for a call without a result while the chat is working", () => {
    render(<WorkBlock id="w" steps={unfinished} />);
    expect(screen.getByLabelText("Running")).toBeInTheDocument();
  });

  it("stops spinning for a call without a result once the chat is not working", () => {
    render(
      <ChatStoppedContext.Provider value>
        <WorkBlock id="w" steps={unfinished} />
      </ChatStoppedContext.Provider>,
    );
    expect(screen.queryByLabelText("Running")).not.toBeInTheDocument();
  });
});
