//! Commands the user can give any connected model, such as
//! `/mcp__brainmatter__setup_project` in Claude Code.
//!
//! They ship with the app, so unlike templates no AI can have written them, and
//! they need no unlocked vault to be offered. A reviewed template with the same
//! name replaces one, which is how a user adapts it.

use std::collections::HashMap;

use rmcp::model::{Prompt, PromptArgument};

use crate::{Client, Scope};

pub(crate) struct Argument {
    name: &'static str,
    description: &'static str,
    required: bool,
}

pub(crate) struct Command {
    pub(crate) name: &'static str,
    description: &'static str,
    /// Offered only to clients that may write.
    writes: bool,
    arguments: &'static [Argument],
    body: &'static str,
}

const PROJECT: Argument = Argument {
    name: "project",
    description: "Project. Defaults to the one this session is about.",
    required: false,
};
const FOCUS: Argument =
    Argument { name: "focus", description: "What to concentrate on.", required: false };

pub(crate) const COMMANDS: &[Command] = &[
    Command {
        name: "setup-project",
        description: "Build a project's structure and docs in Brainmatter",
        writes: true,
        arguments: &[PROJECT, FOCUS],
        body: "Build this project's structure in Brainmatter from what you can see in this session: code, files and our conversation.

1. Call `list_projects`. If the project exists, read `project_overview` and `list_docs` and extend what is there instead of rebuilding it. If no name is given, use the name of the current repository or folder.
2. Read the sources before writing: README, manifests, folder layout, recent history. Write only what you checked, and mark anything you could not check as an assumption.
3. Create the project with `create_project`. Add subprojects only for areas that will hold several entries of their own.
4. Use `write_doc` for how things work: an overview page first, then the pages this project needs, such as architecture, setup and conventions. Explain what the code does not show, above all the reasons. Tag the pages.
5. Use `capture` for single ideas: decisions with their reasons, key concepts, open tasks. Set the kind, link related entries, and search first to avoid duplicates.
6. Finish with a short report: what you created, what you left out, and what you could not verify.

Write in the language I use with you.",
    },
    Command {
        name: "update-docs",
        description: "Bring a project's Brainmatter docs up to date",
        writes: true,
        arguments: &[PROJECT, FOCUS],
        body: "Bring this project's documentation in Brainmatter up to date with its actual state.

1. Call `list_docs`, then `read_doc` for the pages concerned. Use `recent_changes` to see what changed in Brainmatter.
2. Compare them with the sources you can see: code, files, recent history, our conversation. Look for what is wrong, outdated or missing.
3. Fix it with `write_doc`, which replaces a whole page: read the full page first and keep everything that is still true. Add a page only for a topic no page covers.
4. Do not remove anything just because you cannot confirm it. List it in your report instead.
5. Finish with a short report: pages changed, pages added, and open doubts.

Write in the language I use with you.",
    },
    Command {
        name: "capture-session",
        description: "Save what this session established to Brainmatter",
        writes: true,
        arguments: &[PROJECT],
        body: "Save what this session established, so a later session or another model can continue without me repeating it.

Keep: decisions and their reasons, findings that took effort, conventions, my corrections and preferences, open tasks.
Leave out: what the code or git history already records, a step-by-step account of the session, anything only this conversation needs, and secrets such as passwords or keys.

Search first and extend existing entries with `update`; use `capture` only for what is new. Put each entry in the project it belongs to, and link it to what it relates to. Knowledge that fits no project goes in a project called `AI Memory`.

Finish with a few lines on what you saved.",
    },
    Command {
        name: "recall",
        description: "Answer from what Brainmatter knows about a topic",
        writes: false,
        arguments: &[
            Argument { name: "topic", description: "What to look up.", required: true },
            PROJECT,
        ],
        body: "Answer from what Brainmatter knows about this topic, before relying on anything else.

Use `search`, then `get` or `read_doc` only for what you need in full. Name the entries you rely on by short id. If Brainmatter has nothing on it, say so rather than filling the gap from memory. Entries by AIs that I have not reviewed may be wrong; say when you rely on one.",
    },
    Command {
        name: "tidy-project",
        description: "Clean up a project: duplicates, loose entries, structure",
        writes: true,
        arguments: &[PROJECT],
        body: "Tidy up this project in Brainmatter.

1. Read `project_overview`, then look for loose entries with `unlinked` and for duplicates with `similar`.
2. Merge clear duplicates with `merge`, add the links that are missing, and move entries into fitting subprojects.
3. Do not delete anything, and do not merge entries that merely resemble each other. List the cases you are unsure about instead.
4. Finish with a short report: what you changed and what needs my decision. Your changes can be undone with `revert`.",
    },
];

impl Command {
    pub(crate) fn usable_by(&self, client: &Client) -> bool {
        client.can(Scope::Read) && (!self.writes || client.can(Scope::Write))
    }

    pub(crate) fn prompt(&self) -> Prompt {
        let arguments = self
            .arguments
            .iter()
            .map(|a| {
                PromptArgument::new(a.name)
                    .with_description(a.description)
                    .with_required(a.required)
            })
            .collect();
        Prompt::new(self.name, Some(self.description), Some(arguments))
    }

    /// The text, with what the user gave at the end. Optional arguments have no
    /// place in the sentences because they may be missing.
    pub(crate) fn render(&self, values: &HashMap<String, String>) -> Result<String, String> {
        let mut text = self.body.to_string();
        let mut given = Vec::new();
        for argument in self.arguments {
            match values.get(argument.name).map(|v| v.trim()).filter(|v| !v.is_empty()) {
                Some(value) => given.push(format!("{}: {value}", argument.name)),
                None if argument.required => {
                    return Err(format!("„{}“ needs `{}`.", self.name, argument.name));
                }
                None => {}
            }
        }
        if !given.is_empty() {
            text.push_str("\n\n");
            text.push_str(&given.join("\n"));
        }
        Ok(text)
    }
}

pub(crate) fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn given_arguments_are_appended_and_empty_ones_dropped() {
        let setup = find("setup-project").unwrap();
        let text = setup.render(&values(&[("project", "Akani"), ("focus", " ")])).unwrap();
        assert!(text.ends_with("\n\nproject: Akani"), "{text}");
    }

    #[test]
    fn a_missing_required_argument_is_an_error() {
        let recall = find("recall").unwrap();
        assert!(recall.render(&values(&[])).unwrap_err().contains("topic"));
    }

    #[test]
    fn every_tool_a_command_names_exists() {
        const TOOLS: &[&str] = &[
            "capture",
            "create_project",
            "get",
            "list_docs",
            "list_projects",
            "merge",
            "project_overview",
            "read_doc",
            "recent_changes",
            "revert",
            "search",
            "similar",
            "unlinked",
            "update",
            "write_doc",
        ];
        for command in COMMANDS {
            for (i, part) in command.body.split('`').enumerate() {
                // Odd parts are inside backticks. `AI Memory` is a project name.
                if i % 2 == 1 && part != "AI Memory" {
                    assert!(TOOLS.contains(&part), "{} names unknown `{part}`", command.name);
                }
            }
        }
    }
}
