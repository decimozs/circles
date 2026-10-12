import { FileIcon } from "lucide-react";
import { SiMarkdown, SiNpm, SiReact, SiRust, SiTypescript } from "react-icons/si";
import { describe, expect, it } from "vitest";
import { glyphFor } from "./file-icon";

describe("file icons", () => {
  it("picks an icon from the extension", () => {
    expect(glyphFor("main.rs").icon).toBe(SiRust);
    expect(glyphFor("index.ts").icon).toBe(SiTypescript);
    expect(glyphFor("App.TSX").icon).toBe(SiReact);
    expect(glyphFor("README.md").icon).toBe(SiMarkdown);
  });

  it("knows some files by name before the extension", () => {
    expect(glyphFor("package.json").icon).toBe(SiNpm);
    expect(glyphFor(".env.local").tone).toBe("text-warning");
  });

  it("falls back to a plain file", () => {
    expect(glyphFor("data.xyz").icon).toBe(FileIcon);
    expect(glyphFor("Makefile").icon).toBe(FileIcon);
  });
});
