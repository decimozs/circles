import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import { RootProvider } from "fumadocs-ui/provider/next";
import type { Metadata } from "next";
import { siteDescription, siteName, siteUrl } from "@/lib/site";
import "./global.css";

export const metadata: Metadata = {
  metadataBase: siteUrl(),
  title: { default: siteName, template: `%s | ${siteName}` },
  description: siteDescription,
};

export default function Layout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
