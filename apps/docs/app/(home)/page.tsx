import Link from "next/link";
import { siteDescription, siteName } from "@/lib/site";

const START = [
  {
    href: "/docs/getting-started",
    title: "Getting started",
    text: "Install Circles, finish the setup checklist and start your first chat.",
  },
  {
    href: "/docs/composers",
    title: "Composers",
    text: "A manager agent that starts workspaces and agents for you.",
  },
  {
    href: "/docs/pull-requests",
    title: "Pull requests",
    text: "Browse, read and draft pull requests, and see their checks.",
  },
  {
    href: "/docs/troubleshooting",
    title: "Troubleshooting",
    text: "When macOS will not open Circles, or something does not work.",
  },
];

export default function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-3xl flex-1 flex-col justify-center gap-10 px-6 py-16">
      <div className="flex flex-col gap-3">
        <h1 className="text-4xl font-semibold tracking-tight">{siteName}</h1>
        <p className="text-lg text-fd-muted-foreground">{siteDescription}</p>
        <div>
          <Link
            href="/docs"
            className="inline-flex rounded-md bg-fd-primary px-4 py-2 text-sm font-medium text-fd-primary-foreground hover:opacity-90"
          >
            Read the guide
          </Link>
        </div>
      </div>
      <ul className="grid gap-3 sm:grid-cols-2">
        {START.map((item) => (
          <li key={item.href}>
            <Link
              href={item.href}
              className="flex h-full flex-col gap-1 rounded-lg border p-4 hover:bg-fd-accent"
            >
              <span className="font-medium">{item.title}</span>
              <span className="text-sm text-fd-muted-foreground">{item.text}</span>
            </Link>
          </li>
        ))}
      </ul>
    </main>
  );
}
