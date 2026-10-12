//! The Jira tools, written once. Composers reach them through the dispatcher and chats through the
//! chat door of the same socket. Every key and id is checked before it is put in an address, so no
//! input can point the call anywhere but the issue, project or transition it names.

use reqwest::Method;
use serde_json::{json, Value};

use super::adf;
use super::client::{JiraClient, JiraFailure};
use crate::error::{AppError, AppResult};
use crate::integrations::Integrations;
use crate::tools::{Tier, ToolDef};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn key() -> Value {
    json!({ "type": "string", "minLength": 3, "maxLength": 40, "description": "The issue key, such as PROJ-123." })
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 20000, "description": description })
}

fn short(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 255, "description": description })
}

fn labels() -> Value {
    json!({ "type": "array", "maxItems": 20, "description": "Labels, each one word without spaces." })
}

/// The tools, in the order they are listed.
pub fn defs() -> Vec<ToolDef> {
    let def = |name, description, tier, schema, repeat_safe, untrusted| ToolDef {
        name,
        description,
        tier,
        schema,
        repeat_safe,
        untrusted,
    };
    vec![
        def("jira_search", "Search Jira issues with JQL, for example `project = PROJ AND status = \"In Progress\" ORDER BY updated DESC`. Returns key, summary, status, assignee, type, priority and when each was updated. Use next_page_token for more.", Tier::Read,
            schema(json!({
                "jql": text("The JQL query."),
                "max_results": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Default 20." },
                "next_page_token": short("From the previous page."),
            }), &["jql"]), true, true),
        def("jira_get_issue", "One Jira issue: summary, status, type, priority, assignee, reporter, labels, parent, linked issues, description as text and the latest comments.", Tier::Read,
            schema(json!({ "key": key() }), &["key"]), true, true),
        def("jira_list_projects", "The Jira projects this account can see: key, name and type.", Tier::Read,
            schema(json!({}), &[]), true, true),
        def("jira_get_transitions", "The statuses an issue can be moved to now, each with the id that jira_transition_issue needs.", Tier::Read,
            schema(json!({ "key": key() }), &["key"]), true, false),
        def("jira_add_comment", "Add a comment to a Jira issue. Everyone who can see the issue sees it. Plain text; lines starting with `- ` become a list and fenced blocks become code.", Tier::External,
            schema(json!({ "key": key(), "text": text("The comment.") }), &["key", "text"]), false, false),
        def("jira_transition_issue", "Move an issue to another status. Get the transition id from jira_get_transitions first.", Tier::External,
            schema(json!({
                "key": key(),
                "transition_id": short("The id from jira_get_transitions."),
                "comment": text("Optional comment to add with the move."),
            }), &["key", "transition_id"]), true, false),
        def("jira_assign_issue", "Assign an issue. `assignee` is `me`, `unassigned`, or an account id.", Tier::External,
            schema(json!({ "key": key(), "assignee": short("me, unassigned, or an account id.") }), &["key", "assignee"]), true, false),
        def("jira_edit_issue", "Change an issue's summary, description or labels. Only what is given changes. Labels replace the old ones.", Tier::External,
            schema(json!({
                "key": key(),
                "summary": short("The new summary."),
                "description": text("The new description, replacing the old one."),
                "labels": labels(),
            }), &["key"]), true, false),
        def("jira_get_create_fields", "The fields Jira asks for when creating an issue of a type in a project: id, name, whether it is required, its kind and the values it allows. Look here when jira_create_issue says a field is required, and pass those fields in its `fields` input.", Tier::Read,
            schema(json!({
                "project_key": short("The project key, such as PROJ."),
                "issue_type": short("Task, Bug, Story and so on. Default Task."),
            }), &["project_key"]), true, true),
        def("jira_create_issue", "Create a Jira issue. Returns its key. If the project requires more fields, get them with jira_get_create_fields and pass them in `fields`, keyed by field id, for example {\"components\": [{\"name\": \"Backend\"}], \"customfield_10010\": \"x\"}.", Tier::External,
            schema(json!({
                "project_key": short("The project key, such as PROJ."),
                "summary": short("One line."),
                "issue_type": short("Task, Bug, Story and so on. Default Task."),
                "description": text("What it is about."),
                "labels": labels(),
                "fields": json!({ "type": "object", "maxProperties": 20, "description": "Other fields the project asks for, keyed by field id, as listed by jira_get_create_fields." }),
            }), &["project_key", "summary"]), false, false),
    ]
}

pub fn is_jira_tool(name: &str) -> bool {
    name.starts_with("jira_")
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// `PROJ-123`: letters, digits and `_` for the project, a dash, digits.
pub fn issue_key(raw: &str) -> AppResult<String> {
    let raw = raw.trim().to_ascii_uppercase();
    let ok = raw.split_once('-').is_some_and(|(project, number)| {
        project
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && project
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
    });
    if ok {
        Ok(raw)
    } else {
        Err(invalid("That is not an issue key. It looks like PROJ-123."))
    }
}

pub(crate) fn project_key(raw: &str) -> AppResult<String> {
    let raw = raw.trim().to_ascii_uppercase();
    let ok = raw.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(raw)
    } else {
        Err(invalid("That is not a project key. It looks like PROJ."))
    }
}

fn digits(raw: &str, what: &str) -> AppResult<String> {
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
        Ok(raw.to_string())
    } else {
        Err(invalid(format!("{what} must be a number.")))
    }
}

fn account_id(raw: &str) -> AppResult<String> {
    if !raw.is_empty()
        && raw.len() <= 128
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
    {
        Ok(raw.to_string())
    } else {
        Err(invalid("That is not an account id."))
    }
}

pub(crate) fn str_arg<'a>(input: &'a Value, name: &str) -> AppResult<&'a str> {
    input[name]
        .as_str()
        .ok_or_else(|| invalid(format!("\"{name}\" is required.")))
}

fn label_list(input: &Value) -> AppResult<Option<Vec<String>>> {
    let Some(items) = input["labels"].as_array() else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for item in items {
        let label = item.as_str().unwrap_or("").trim();
        if label.is_empty() || label.len() > 255 || label.contains(char::is_whitespace) {
            return Err(invalid("Each label must be one word without spaces."));
        }
        out.push(label.to_string());
    }
    Ok(Some(out))
}

/// Fields the create tool fills in from its own inputs.
const OWN_FIELDS: &[&str] = &["project", "issuetype", "summary", "description", "labels"];

/// What Jira asks for when creating an issue of this type in this project.
async fn create_fields(
    hub: &Integrations,
    client: &JiraClient,
    project: &str,
    issue_type: &str,
) -> AppResult<Vec<Value>> {
    let types = call(
        hub,
        client,
        Method::GET,
        &format!("/rest/api/3/issue/createmeta/{project}/issuetypes?maxResults=100"),
        None,
    )
    .await?;
    let found = types["issueTypes"]
        .as_array()
        .or_else(|| types["values"].as_array())
        .and_then(|all| {
            all.iter().find(|t| {
                t["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(issue_type))
            })
        })
        .ok_or_else(|| {
            invalid(format!(
                "The project has no issue type called \"{issue_type}\"."
            ))
        })?;
    let id = digits(found["id"].as_str().unwrap_or(""), "The issue type id")?;
    let fields = call(
        hub,
        client,
        Method::GET,
        &format!("/rest/api/3/issue/createmeta/{project}/issuetypes/{id}?maxResults=100"),
        None,
    )
    .await?;
    Ok(fields["fields"]
        .as_array()
        .or_else(|| fields["values"].as_array())
        .cloned()
        .unwrap_or_default())
}

fn field_line(field: &Value) -> Value {
    let allowed: Vec<Value> = field["allowedValues"]
        .as_array()
        .map(|all| {
            all.iter()
                .take(25)
                .map(|v| {
                    v["name"]
                        .as_str()
                        .or_else(|| v["value"].as_str())
                        .map_or(Value::Null, |s| json!(s))
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": field["fieldId"],
        "name": field["name"],
        "required": field["required"],
        "kind": field["schema"]["type"],
        "allowed_values": allowed,
        "set_by_the_tool": field["fieldId"].as_str().is_some_and(|id| OWN_FIELDS.contains(&id)),
    })
}

fn name_of(person: &Value) -> Value {
    person["displayName"].clone()
}

fn issue_line(issue: &Value) -> Value {
    let f = &issue["fields"];
    json!({
        "key": issue["key"],
        "summary": f["summary"],
        "status": f["status"]["name"],
        "type": f["issuetype"]["name"],
        "priority": f["priority"]["name"],
        "assignee": name_of(&f["assignee"]),
        "project": f["project"]["key"],
        "updated": f["updated"],
    })
}

async fn call(
    hub: &Integrations,
    client: &JiraClient,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> AppResult<Value> {
    client.request(method, path, body).await.map_err(|failure| {
        if failure.is_rejection() {
            hub.jira_rejected();
        }
        AppError::Invalid(match failure {
            JiraFailure::Rejected => {
                "Jira turned the token down. Reconnect Jira in Settings, Integrations.".into()
            }
            other => other.message(),
        })
    })
}

/// Runs one Jira tool. The error text goes back to the model, so it never holds the token.
pub async fn run(hub: &Integrations, tool: &str, input: &Value) -> AppResult<Value> {
    // Signed in with Atlassian: the same tools, answered through Atlassian's MCP server.
    if hub.is_oauth() {
        return crate::integrations::atlassian::tools::run(hub, tool, input).await;
    }
    let client = hub.jira_client()?;
    let client = &client;
    match tool {
        "jira_search" => {
            let mut body = json!({
                "jql": str_arg(input, "jql")?,
                "maxResults": input["max_results"].as_u64().unwrap_or(20).clamp(1, 50),
                "fields": ["summary", "status", "assignee", "issuetype", "priority", "updated", "project"],
            });
            if let Some(token) = input["next_page_token"].as_str() {
                body["nextPageToken"] = json!(token);
            }
            let found = call(
                hub,
                client,
                Method::POST,
                "/rest/api/3/search/jql",
                Some(&body),
            )
            .await?;
            Ok(json!({
                "issues": found["issues"].as_array().map(|a| a.iter().map(issue_line).collect::<Vec<_>>()).unwrap_or_default(),
                "next_page_token": found["nextPageToken"],
            }))
        }
        "jira_get_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let path = format!(
                "/rest/api/3/issue/{key}?fields=summary,status,issuetype,priority,assignee,reporter,labels,parent,project,description,issuelinks,created,updated,comment"
            );
            let issue = call(hub, client, Method::GET, &path, None).await?;
            let f = &issue["fields"];
            let comments: Vec<Value> = f["comment"]["comments"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .rev()
                        .take(10)
                        .rev()
                        .map(|c| {
                            json!({
                                "author": name_of(&c["author"]),
                                "created": c["created"],
                                "text": adf::to_text(&c["body"]),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let links: Vec<Value> = f["issuelinks"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .map(|l| {
                            let (direction, other) = if l["outwardIssue"].is_object() {
                                (&l["type"]["outward"], &l["outwardIssue"])
                            } else {
                                (&l["type"]["inward"], &l["inwardIssue"])
                            };
                            json!({
                                "relation": direction,
                                "key": other["key"],
                                "summary": other["fields"]["summary"],
                                "status": other["fields"]["status"]["name"],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(json!({
                "key": issue["key"],
                "summary": f["summary"],
                "status": f["status"]["name"],
                "type": f["issuetype"]["name"],
                "priority": f["priority"]["name"],
                "assignee": name_of(&f["assignee"]),
                "reporter": name_of(&f["reporter"]),
                "labels": f["labels"],
                "parent": f["parent"]["key"],
                "project": f["project"]["key"],
                "created": f["created"],
                "updated": f["updated"],
                "description": adf::to_text(&f["description"]),
                "links": links,
                "comments": comments,
            }))
        }
        "jira_list_projects" => {
            let found = call(
                hub,
                client,
                Method::GET,
                "/rest/api/3/project/search?maxResults=100",
                None,
            )
            .await?;
            Ok(json!({
                "projects": found["values"].as_array().map(|a| a.iter().map(|p| json!({
                    "key": p["key"], "name": p["name"], "type": p["projectTypeKey"],
                })).collect::<Vec<_>>()).unwrap_or_default(),
            }))
        }
        "jira_get_transitions" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let found = call(
                hub,
                client,
                Method::GET,
                &format!("/rest/api/3/issue/{key}/transitions"),
                None,
            )
            .await?;
            Ok(json!({
                "transitions": found["transitions"].as_array().map(|a| a.iter().map(|t| json!({
                    "id": t["id"], "name": t["name"], "to": t["to"]["name"],
                })).collect::<Vec<_>>()).unwrap_or_default(),
            }))
        }
        "jira_add_comment" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let body = json!({ "body": adf::from_text(str_arg(input, "text")?) });
            let made = call(
                hub,
                client,
                Method::POST,
                &format!("/rest/api/3/issue/{key}/comment"),
                Some(&body),
            )
            .await?;
            Ok(json!({ "key": key, "comment_id": made["id"] }))
        }
        "jira_transition_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let id = digits(str_arg(input, "transition_id")?, "The transition id")?;
            let mut body = json!({ "transition": { "id": id } });
            if let Some(comment) = input["comment"].as_str() {
                body["update"] =
                    json!({ "comment": [{ "add": { "body": adf::from_text(comment) } }] });
            }
            call(
                hub,
                client,
                Method::POST,
                &format!("/rest/api/3/issue/{key}/transitions"),
                Some(&body),
            )
            .await?;
            Ok(json!({ "key": key, "moved": true }))
        }
        "jira_assign_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let who = match str_arg(input, "assignee")? {
                "unassigned" => Value::Null,
                "me" => json!(
                    client
                        .myself()
                        .await
                        .map_err(|f| invalid(f.message()))?
                        .account_id
                ),
                other => json!(account_id(other)?),
            };
            call(
                hub,
                client,
                Method::PUT,
                &format!("/rest/api/3/issue/{key}/assignee"),
                Some(&json!({ "accountId": who })),
            )
            .await?;
            Ok(json!({ "key": key, "assigned": true }))
        }
        "jira_edit_issue" => {
            let key = issue_key(str_arg(input, "key")?)?;
            let mut fields = serde_json::Map::new();
            if let Some(summary) = input["summary"].as_str() {
                fields.insert("summary".into(), json!(summary));
            }
            if let Some(description) = input["description"].as_str() {
                fields.insert("description".into(), adf::from_text(description));
            }
            if let Some(labels) = label_list(input)? {
                fields.insert("labels".into(), json!(labels));
            }
            if fields.is_empty() {
                return Err(invalid("Give a summary, description or labels to change."));
            }
            call(
                hub,
                client,
                Method::PUT,
                &format!("/rest/api/3/issue/{key}"),
                Some(&json!({ "fields": fields })),
            )
            .await?;
            Ok(json!({ "key": key, "changed": true }))
        }
        "jira_get_create_fields" => {
            let project = project_key(str_arg(input, "project_key")?)?;
            let list = create_fields(
                hub,
                client,
                &project,
                input["issue_type"].as_str().unwrap_or("Task"),
            )
            .await?;
            Ok(
                json!({ "project": project, "fields": list.iter().map(field_line).collect::<Vec<_>>() }),
            )
        }
        "jira_create_issue" => {
            let project = project_key(str_arg(input, "project_key")?)?;
            let issue_type = input["issue_type"].as_str().unwrap_or("Task");
            let mut fields = json!({
                "project": { "key": project },
                "summary": str_arg(input, "summary")?,
                "issuetype": { "name": issue_type },
            });
            if let Some(extra) = input["fields"].as_object().filter(|m| !m.is_empty()) {
                // Only fields that Jira lists for this project and type, and never the ones this
                // tool sets itself, so the call cannot reach anything else.
                let known = create_fields(hub, client, &project, issue_type).await?;
                for (id, value) in extra {
                    let listed = known.iter().any(|f| f["fieldId"].as_str() == Some(id));
                    if !listed || OWN_FIELDS.contains(&id.as_str()) {
                        return Err(invalid(format!(
                            "\"{id}\" is not a field you can pass here. Use jira_get_create_fields to see them."
                        )));
                    }
                    fields[id] = value.clone();
                }
            }
            if let Some(description) = input["description"].as_str() {
                fields["description"] = adf::from_text(description);
            }
            if let Some(labels) = label_list(input)? {
                fields["labels"] = json!(labels);
            }
            let made = call(
                hub,
                client,
                Method::POST,
                "/rest/api/3/issue",
                Some(&json!({ "fields": fields })),
            )
            .await?;
            Ok(json!({ "key": made["key"] }))
        }
        other => Err(invalid(format!(
            "There is no Jira tool called \"{other}\"."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_checked_before_they_reach_an_address() {
        assert_eq!(issue_key(" proj-12 ").unwrap(), "PROJ-12");
        for bad in [
            "PROJ",
            "PROJ-",
            "PROJ-1/../x",
            "../PROJ-1",
            "PROJ-1?x=1",
            "1PROJ-1",
            "PR OJ-1",
            "",
        ] {
            assert!(issue_key(bad).is_err(), "{bad}");
        }
        assert!(project_key("AB_1").is_ok());
        assert!(project_key("A/B").is_err());
        assert!(digits("12", "x").is_ok());
        assert!(digits("1a", "x").is_err());
        assert!(account_id("5b10ac8d82e05b22cc7d4ef5").is_ok());
        assert!(account_id("a/b").is_err());
    }

    #[test]
    fn the_catalogue_is_sound() {
        let defs = defs();
        assert_eq!(defs.len(), 10);
        for d in &defs {
            assert!(is_jira_tool(d.name));
            for name in d.schema["required"].as_array().unwrap() {
                assert!(
                    d.schema["properties"].get(name.as_str().unwrap()).is_some(),
                    "{}",
                    d.name
                );
            }
            if d.tier == Tier::Read {
                assert!(
                    d.repeat_safe && d.untrusted == (d.name != "jira_get_transitions"),
                    "{}",
                    d.name
                );
            } else {
                // Anything that changes Jira is seen by other people.
                assert_eq!(d.tier, Tier::External, "{}", d.name);
            }
        }
    }

    use crate::integrations::secrets::{Credentials, MemorySecrets, SecretStore};
    use crate::integrations::testing::fake_jira_routes;

    async fn hub_with(
        routes: Vec<(&'static str, &'static str)>,
    ) -> (
        std::sync::Arc<Integrations>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        let (base, seen) = fake_jira_routes(routes).await;
        let secrets = MemorySecrets::default();
        secrets
            .save(&Credentials::new("t.atlassian.net", "me@x.com", "tok").unwrap())
            .unwrap();
        (Integrations::for_test(Box::new(secrets), base), seen)
    }

    const TYPES: &str =
        r#"{"issueTypes":[{"id":"10001","name":"Task"},{"id":"10002","name":"Bug"}]}"#;
    const FIELDS: &str = r#"{"fields":[
        {"fieldId":"summary","name":"Summary","required":true,"schema":{"type":"string"}},
        {"fieldId":"components","name":"Components","required":true,"schema":{"type":"array"},"allowedValues":[{"name":"Backend"},{"name":"Web"}]},
        {"fieldId":"customfield_10010","name":"Team","required":false,"schema":{"type":"string"}}
    ]}"#;

    #[tokio::test]
    async fn the_fields_a_project_requires_can_be_listed() {
        let (hub, seen) =
            hub_with(vec![("issuetypes/10001", FIELDS), ("/issuetypes?", TYPES)]).await;
        let out = run(
            &hub,
            "jira_get_create_fields",
            &json!({"project_key": "proj", "issue_type": "task"}),
        )
        .await
        .unwrap();
        let fields = out["fields"].as_array().unwrap();
        let components = fields.iter().find(|f| f["id"] == "components").unwrap();
        assert_eq!(components["required"], true);
        assert_eq!(components["allowed_values"], json!(["Backend", "Web"]));
        assert_eq!(
            fields.iter().find(|f| f["id"] == "summary").unwrap()["set_by_the_tool"],
            true
        );
        assert!(
            seen.lock().unwrap()[0].starts_with("GET /rest/api/3/issue/createmeta/PROJ/issuetypes")
        );
    }

    #[tokio::test]
    async fn an_issue_is_created_with_the_extra_fields_that_jira_lists() {
        let (hub, seen) = hub_with(vec![
            ("issuetypes/10001", FIELDS),
            ("/issuetypes?", TYPES),
            ("POST /rest/api/3/issue", r#"{"key":"PROJ-9"}"#),
        ])
        .await;
        let out = run(
            &hub,
            "jira_create_issue",
            &json!({
                "project_key": "PROJ",
                "summary": "Fix it",
                "fields": { "components": [{"name": "Backend"}], "customfield_10010": "Core" }
            }),
        )
        .await
        .unwrap();
        assert_eq!(out["key"], "PROJ-9");
        let post = seen.lock().unwrap().last().unwrap().clone();
        assert!(post.contains(r#""components":[{"name":"Backend"}]"#));
        assert!(post.contains(r#""customfield_10010":"Core""#));
        assert!(post.contains(r#""summary":"Fix it""#));
    }

    #[tokio::test]
    async fn fields_jira_does_not_list_or_the_tool_sets_itself_are_refused() {
        let (hub, seen) = hub_with(vec![
            ("issuetypes/10001", FIELDS),
            ("/issuetypes?", TYPES),
            ("POST /rest/api/3/issue", r#"{"key":"PROJ-9"}"#),
        ])
        .await;
        for bad in ["reporter", "summary", "project", "customfield_99999"] {
            let err = run(
                &hub,
                "jira_create_issue",
                &json!({"project_key": "PROJ", "summary": "x", "fields": { bad: "v" }}),
            )
            .await
            .unwrap_err();
            assert!(
                err.to_string().contains("not a field you can pass"),
                "{bad}"
            );
        }
        assert!(
            !seen.lock().unwrap().iter().any(|r| r.starts_with("POST")),
            "nothing was created"
        );
    }
}
