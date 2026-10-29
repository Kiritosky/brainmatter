//! The write side of a change set: every mutation records a revision so the
//! change set can be reverted as a unit.

use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    error::Result,
    events::Event,
    ids::{ActorId, ChangeSetId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Entity {
    Node,
    Edge,
    Project,
    Membership,
}

impl Entity {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Edge => "edge",
            Self::Project => "project",
            Self::Membership => "membership",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "node" => Self::Node,
            "edge" => Self::Edge,
            "project" => Self::Project,
            "membership" => Self::Membership,
            _ => return None,
        })
    }
}

/// Snapshot encoding: one format byte, then the payload. 0 = raw JSON
/// (leaves room for compressed formats later without a migration).
const SNAPSHOT_JSON: u8 = 0;

pub(crate) fn encode_snapshot<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut out = vec![SNAPSHOT_JSON];
    serde_json::to_writer(&mut out, value)?;
    Ok(out)
}

pub(crate) fn decode_snapshot<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    match bytes.split_first() {
        Some((&SNAPSHOT_JSON, json)) => Ok(serde_json::from_slice(json)?),
        _ => Err(crate::CoreError::Invalid("unknown snapshot format".into())),
    }
}

pub(crate) struct Writer<'c> {
    pub conn: &'c Connection,
    pub cs: ChangeSetId,
    pub actor: ActorId,
    pub actor_is_ai: bool,
    pub now: i64,
    revisions: usize,
    events: Vec<Event>,
}

impl<'c> Writer<'c> {
    pub(crate) fn new(
        conn: &'c Connection,
        cs: ChangeSetId,
        actor: ActorId,
        actor_is_ai: bool,
        now: i64,
    ) -> Self {
        Self { conn, cs, actor, actor_is_ai, now, revisions: 0, events: Vec::new() }
    }

    pub(crate) fn record<T: Serialize>(
        &mut self,
        entity: Entity,
        id: &str,
        rev: i64,
        before: Option<&T>,
        after: Option<&T>,
    ) -> Result<()> {
        let before = before.map(encode_snapshot).transpose()?;
        let after = after.map(encode_snapshot).transpose()?;
        self.conn.execute(
            "INSERT INTO revisions (change_set_id, entity_type, entity_id, rev, before, after)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![self.cs, entity.as_str(), id, rev, before, after],
        )?;
        self.revisions += 1;
        Ok(())
    }

    pub(crate) fn emit(&mut self, event: Event) {
        self.events.push(event);
    }

    pub(crate) fn finish(self) -> (usize, Vec<Event>) {
        (self.revisions, self.events)
    }
}
