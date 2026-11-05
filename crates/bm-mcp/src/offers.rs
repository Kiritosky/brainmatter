//! What a client is offered: its prompt list, and the skills an app that shows no
//! prompts finds in its skill folder instead. Both come from here, so a name means
//! the same in either place.

use std::collections::HashSet;

use bm_core::{
    CoreError, Engine,
    model::{Attachment, SkillInfo, TemplateInfo},
};
use rmcp::model::{Prompt, PromptArgument};

use crate::{
    Client, Scope,
    prompts::{COMMANDS, Command},
    tools,
};

/// A skill's prompt is its name after this, so it never takes the name of a built-in
/// command. A template named like that keeps the name, and the skill is reached
/// through `get_skill` only.
pub(crate) const SKILL_PROMPT: &str = "skill-";
/// The longest `description` the Agent Skills format allows.
const MAX_DESCRIPTION_CHARS: usize = 1024;

pub(crate) enum Offer {
    Command(&'static Command),
    Template(TemplateInfo),
    Skill(SkillInfo),
}

impl Offer {
    pub(crate) fn name(&self) -> String {
        match self {
            Offer::Command(c) => c.name.to_string(),
            Offer::Template(t) => t.slug.clone(),
            Offer::Skill(s) => format!("{SKILL_PROMPT}{}", s.slug),
        }
    }

    pub(crate) fn prompt(&self) -> Prompt {
        match self {
            Offer::Command(c) => c.prompt(),
            Offer::Template(t) => {
                let arguments = t
                    .arguments
                    .iter()
                    .map(|a| PromptArgument::new(a).with_required(true))
                    .collect();
                Prompt::new(self.name(), Some(t.node.title.clone()), Some(arguments))
            }
            Offer::Skill(s) => Prompt::new(self.name(), Some(s.description.clone()), None),
        }
    }
}

/// Clients ask for the prompt list on their own as soon as they connect, so this
/// never triggers Touch ID: while locked (no `engine`), only the built-in commands
/// are offered. A template or skill is offered only after the user has seen it.
/// Otherwise an AI could plant text that later arrives in another session looking
/// like the user's request. A reviewed template replaces the command of its name.
pub(crate) fn offers(engine: Option<&Engine>, client: &Client) -> Result<Vec<Offer>, CoreError> {
    if !client.can(Scope::Read) {
        return Ok(Vec::new());
    }
    let vault = match engine {
        Some(engine) => vault_offers(engine, client)?,
        None => Vec::new(),
    };
    let replaced: HashSet<String> = vault.iter().map(Offer::name).collect();
    let mut out: Vec<Offer> = COMMANDS
        .iter()
        .filter(|c| c.usable_by(client) && !replaced.contains(c.name))
        .map(Offer::Command)
        .collect();
    out.extend(vault);
    Ok(out)
}

fn vault_offers(engine: &Engine, client: &Client) -> Result<Vec<Offer>, CoreError> {
    let scope = tools::project_scope(engine, client)?;
    let mut out: Vec<Offer> = engine
        .templates(None)?
        .into_iter()
        .filter(|t| t.reviewed && !t.slug.is_empty() && tools::template_in_scope(scope.as_ref(), t))
        .map(Offer::Template)
        .collect();
    for skill in engine.skills(None)? {
        let name = format!("{SKILL_PROMPT}{}", skill.slug);
        let fits = skill.reviewed
            && !skill.slug.is_empty()
            && tools::skill_in_scope(scope.as_ref(), &skill);
        if fits && !out.iter().any(|o| o.name() == name) {
            out.push(Offer::Skill(skill));
        }
    }
    Ok(out)
}

/// One folder in a client's skill folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    /// The name of its prompt.
    pub name: String,
    /// What the client picks it by.
    pub description: String,
    /// What `SKILL.md` says below its front matter.
    pub body: String,
    /// Files to write next to it, at the path their name gives.
    pub files: Vec<Attachment>,
}

/// What a client's skill folder holds: everything its prompt list offers. A skill the
/// user put on disk comes as it is. Everything else stays in the vault, and its
/// `SKILL.md` tells the model how to fetch it from the MCP server named `server`.
pub fn skill_menu(
    engine: &Engine,
    client: &Client,
    server: &str,
) -> Result<Vec<MenuItem>, CoreError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for offer in offers(Some(engine), client)? {
        let name = offer.name();
        if !seen.insert(name.clone()) {
            continue;
        }
        let (description, body, files) = match offer {
            Offer::Command(c) => (
                c.description.to_string(),
                from_template(server, &name, c.argument_names()),
                vec![],
            ),
            Offer::Template(t) => {
                let arguments = t.arguments.iter().map(String::as_str);
                let body = from_template(server, &name, arguments);
                (format!("Brainmatter template: {}", t.node.title), body, vec![])
            }
            Offer::Skill(s) if s.on_disk => (s.description, s.body, s.files),
            Offer::Skill(s) => (s.description.clone(), from_skill(server, &s), vec![]),
        };
        let description: String = description.trim().chars().take(MAX_DESCRIPTION_CHARS).collect();
        out.push(MenuItem { name, description: description.trim_end().to_string(), body, files });
    }
    Ok(out)
}

fn from_template<'a>(server: &str, name: &str, arguments: impl Iterator<Item = &'a str>) -> String {
    let arguments: Vec<String> = arguments.map(|a| format!("`{a}`")).collect();
    let mut text = format!(
        "This command is kept in Brainmatter. Call `get_template` of the MCP server `{server}` with `template` set to `{name}`"
    );
    if !arguments.is_empty() {
        text += &format!(
            ", and fill `values` with {} from the user's request as far as it gives them",
            arguments.join(", ")
        );
    }
    text + ". Then follow the text it returns.\n"
}

fn from_skill(server: &str, skill: &SkillInfo) -> String {
    let mut text = format!(
        "This skill is kept in Brainmatter. Call `get_skill` of the MCP server `{server}` with `skill` set to `{}`, then follow the instructions it returns.",
        skill.slug
    );
    if !skill.files.is_empty() {
        text += " Read the files it lists with `read_attachment` when the instructions need them.";
    }
    text + "\n"
}
