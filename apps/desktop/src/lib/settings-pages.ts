/** Settings pages, shared by the settings screens (phase 13) and the command palette. */
export const SETTINGS_PAGES = [
  { id: "general", title: "General", group: "Personal" },
  { id: "account", title: "Accounts", group: "Personal" },
  { id: "appearance", title: "Appearance", group: "Personal" },
  { id: "models", title: "Default models", group: "Personal" },
  { id: "git", title: "Git", group: "Personal" },
  { id: "agents", title: "Agents", group: "Agents & environment" },
  { id: "smart", title: "Smart titles", group: "Agents & environment" },
  { id: "environment", title: "Environment", group: "Agents & environment" },
  { id: "composers", title: "Composers", group: "Agents & environment" },
  { id: "integrations", title: "Integrations", group: "Agents & environment" },
  // Only listed while the service is connected. Its own settings are here, the connection is on the
  // Integrations page.
  { id: "jira", title: "Jira", group: "Integrations" },
  { id: "experimental", title: "Experimental", group: "More" },
  { id: "advanced", title: "Advanced", group: "More" },
] as const;

export type SettingsPageId = (typeof SETTINGS_PAGES)[number]["id"];
