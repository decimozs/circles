//! The Circles Jira tools on top of Atlassian's MCP tools. A tool that is not in this file cannot
//! be reached: Atlassian may add or change tools at any time, and none of that gets to an agent.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::integrations::jira::tools::{issue_key, project_key, str_arg};
use crate::integrations::Integrations;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// Every argument must be one the tool takes now, and every required one must be given.
pub fn check_args(schemas: &HashMap<String, Value>, tool: &str, args: &Value) -> AppResult<()> {
    let schema = schemas.get(tool).ok_or_else(|| {
        invalid(format!(
            "Atlassian no longer offers \"{tool}\". Use an API token instead, in Settings, Integrations."
        ))
    })?;
    if let Some(properties) = schema["properties"].as_object() {
        for key in args.as_object().into_iter().flat_map(|m| m.keys()) {
            if !properties.contains_key(key) {
                return Err(invalid(format!(
                    "Atlassian changed \"{tool}\": it no longer takes \"{key}\". Use an API token instead, in Settings, Integrations."
                )));
            }
        }
    }
    for key in schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if args.get(key).is_none() {
            return Err(invalid(format!(
                "Atlassian changed \"{tool}\": it now needs \"{key}\". Use an API token instead, in Settings, Integrations."
            )));
        }
    }
    Ok(())
}

fn get<'a>(value: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter()
        .map(|k| &value[*k])
        .find(|v| !v.is_null())
        .unwrap_or(&Value::Null)
}

/// One issue as the Circles search tool reports it, from either shape Atlassian uses.
fn issue_line(issue: &Value) -> Value {
    let f = if issue["fields"].is_object() {
        &issue["fields"]
    } else {
        issue
    };
    let name = |v: &Value| {
        v["name"]
            .as_str()
            .or_else(|| v["displayName"].as_str())
            .or_else(|| v.as_str())
            .map_or(Value::Null, |s| json!(s))
    };
    json!({
        "key": issue["key"],
        "summary": f["summary"],
        "status": name(&f["status"]),
        "type": name(get(f, &["issuetype", "issueType"])),
        "priority": name(&f["priority"]),
        "assignee": name(&f["assignee"]),
        "project": get(&f["project"], &["key"]),
        "updated": f["updated"],
    })
}

pub async fn run(hub: &Integrations, tool: &str, input: &Value) -> AppResult<Value> {
    let cloud = hub
        .load_oauth()?
        .map(|s| s.cloud_id)
        .ok_or_else(|| invalid("Jira is not connected."))?;
    let cloud = cloud.as_str();
    match tool {
        "jira_search" => {
            let mut args = json!({
                "cloudId": cloud,
                "jql": str_arg(input, "jql")?,
                "maxResults": input["max_results"].as_u64().unwrap_or(20).clamp(1, 50),
                "fields": ["summary", "status", "assignee", "issuetype", "priority", "updated", "project"],
            });
            if let Some(token) = input["next_page_token"].as_str() {
                args["nextPageToken"] = json!(token);
            }
            let found = hub.call_atlassian("searchJiraIssuesUsingJql", args).await?;
            Ok(json!({
                "issues": found["issues"].as_array().map(|a| a.iter().map(issue_line).collect::<Vec<_>>()).unwrap_or_default(),
                "next_page_token": found["nextPageToken"],
            }))
        }
        "jira_get_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            hub.call_atlassian(
                "getJiraIssue",
                json!({ "cloudId": cloud, "issueIdOrKey": key }),
            )
            .await
        }
        "jira_list_projects" => {
            hub.call_atlassian("listJiraProjects", json!({ "cloudId": cloud }))
                .await
        }
        "jira_get_transitions" => {
            let key = issue_key(str_arg(input, "key")?)?;
            hub.call_atlassian(
                "listJiraIssueTransitions",
                json!({ "cloudId": cloud, "issueIdOrKey": key }),
            )
            .await
        }
        "jira_get_create_fields" => {
            let project = project_key(str_arg(input, "project_key")?)?;
            let wanted = input["issue_type"].as_str().unwrap_or("Task");
            let types = hub
                .call_atlassian(
                    "listJiraProjectIssueTypesMetadata",
                    json!({ "cloudId": cloud, "projectIdOrKey": project }),
                )
                .await?;
            let list = types["issueTypes"]
                .as_array()
                .or_else(|| types["values"].as_array())
                .or_else(|| types.as_array())
                .cloned()
                .unwrap_or_default();
            let found = list
                .iter()
                .find(|t| {
                    t["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
                })
                .ok_or_else(|| {
                    invalid(format!(
                        "The project has no issue type called \"{wanted}\"."
                    ))
                })?;
            let id = found["id"].as_str().unwrap_or_default();
            hub.call_atlassian(
                "getJiraIssueTypeMetaWithFields",
                json!({ "cloudId": cloud, "projectIdOrKey": project, "issueTypeId": id }),
            )
            .await
        }
        "jira_add_comment" => {
            let key = issue_key(str_arg(input, "key")?)?;
            hub.call_atlassian(
                "addOrEditJiraIssueComment",
                json!({ "cloudId": cloud, "issueIdOrKey": key, "commentBody": str_arg(input, "text")? }),
            )
            .await?;
            Ok(json!({ "key": key, "commented": true }))
        }
        "jira_transition_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            hub.call_atlassian(
                "transitionJiraIssue",
                json!({ "cloudId": cloud, "issueIdOrKey": key, "transition": { "id": str_arg(input, "transition_id")? } }),
            )
            .await?;
            if let Some(comment) = input["comment"].as_str() {
                hub.call_atlassian(
                    "addOrEditJiraIssueComment",
                    json!({ "cloudId": cloud, "issueIdOrKey": key, "commentBody": comment }),
                )
                .await?;
            }
            Ok(json!({ "key": key, "moved": true }))
        }
        "jira_assign_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let assignee = match str_arg(input, "assignee")? {
                "unassigned" => Value::Null,
                "me" => {
                    let me = hub.call_atlassian("atlassianUserInfo", json!({})).await?;
                    let id = get(&me, &["account_id", "accountId"])
                        .as_str()
                        .unwrap_or_default();
                    if id.is_empty() {
                        return Err(invalid("Atlassian did not say who you are."));
                    }
                    json!({ "accountId": id })
                }
                other => json!({ "accountId": other }),
            };
            hub.call_atlassian(
                "editJiraIssue",
                json!({ "cloudId": cloud, "issueIdOrKey": key, "fields": { "assignee": assignee } }),
            )
            .await?;
            Ok(json!({ "key": key, "assigned": true }))
        }
        "jira_edit_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let mut fields = serde_json::Map::new();
            for name in ["summary", "description"] {
                if let Some(v) = input[name].as_str() {
                    fields.insert(name.into(), json!(v));
                }
            }
            if let Some(labels) = input["labels"].as_array() {
                fields.insert("labels".into(), json!(labels));
            }
            if fields.is_empty() {
                return Err(invalid("Give a summary, description or labels to change."));
            }
            hub.call_atlassian(
                "editJiraIssue",
                json!({ "cloudId": cloud, "issueIdOrKey": key, "fields": fields }),
            )
            .await?;
            Ok(json!({ "key": key, "changed": true }))
        }
        "jira_create_issue" => {
            let project = project_key(str_arg(input, "project_key")?)?;
            let mut args = json!({
                "cloudId": cloud,
                "projectKey": project,
                "issueTypeName": input["issue_type"].as_str().unwrap_or("Task"),
                "summary": str_arg(input, "summary")?,
            });
            if let Some(description) = input["description"].as_str() {
                args["description"] = json!(description);
            }
            let mut extra = input["fields"].as_object().cloned().unwrap_or_default();
            if let Some(labels) = input["labels"].as_array() {
                extra.insert("labels".into(), json!(labels));
            }
            if !extra.is_empty() {
                args["additional_fields"] = Value::Object(extra);
            }
            let made = hub.call_atlassian("createJiraIssue", args).await?;
            Ok(json!({ "key": made["key"] }))
        }
        other => Err(invalid(format!(
            "There is no Jira tool called \"{other}\"."
        ))),
    }
}
