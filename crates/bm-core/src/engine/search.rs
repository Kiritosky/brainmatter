//! Full-text search over the FTS5 index (title weighted 5×, tags 2×).

use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};

use super::projects;
use crate::{
    error::Result,
    model::{NodeBrief, SearchHit, SearchQuery},
};

const MAX_LIMIT: usize = 200;

/// German and English function words. They occur in almost every entry, so
/// they barely affect ranking but make BM25 score nearly every row.
const STOPWORDS: &[&str] = &[
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "und",
    "oder", "aber", "ist", "sind", "war", "wird", "werden", "hat", "haben", "mit", "von", "zu",
    "zum", "zur", "im", "in", "an", "am", "auf", "aus", "bei", "für", "nach", "über", "unter",
    "vor", "wie", "was", "wer", "wo", "auch", "nicht", "noch", "nur", "so", "dass", "es", "er",
    "sie", "wir", "ich", "du", "man", "sich", "als", "um", "the", "a", "an", "and", "or", "of",
    "to", "in", "on", "at", "for", "with", "is", "are", "was", "be", "it", "this", "that", "as",
    "by", "from", "not", "but", "if", "then", "so",
];

/// A term in more than this share of all entries says little and is slow to
/// rank, since BM25 has to score nearly every row.
const COMMON_TERM_SHARE: f64 = 0.4;

fn content_terms(text: &str) -> Vec<&str> {
    let words: Vec<&str> =
        text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    let content: Vec<&str> =
        words.iter().copied().filter(|w| !STOPWORDS.contains(&w.to_lowercase().as_str())).collect();
    if content.is_empty() { words } else { content }
}

/// Quotes every word as a prefix term, which neutralizes FTS operators in user
/// input. All terms must match.
fn expression(terms: &[&str]) -> Option<String> {
    let terms: Vec<String> = terms.iter().map(|t| format!("\"{t}\"*")).collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Turns free text into a safe FTS5 expression. Stopwords are dropped unless
/// the query consists only of stopwords.
#[cfg(test)]
pub(crate) fn fts_expression(text: &str) -> Option<String> {
    expression(&content_terms(text))
}

/// Same, but also drops terms occurring in most entries; if all of them do,
/// only the rarest one stays.
fn selective_expression(conn: &Connection, text: &str) -> Result<Option<String>> {
    let terms = content_terms(text);
    if terms.len() < 2 {
        return Ok(expression(&terms));
    }
    let total: i64 = conn.query_row("SELECT coalesce(max(seq), 0) FROM nodes", [], |r| r.get(0))?;
    let mut stmt = conn.prepare_cached("SELECT doc FROM nodes_fts_vocab WHERE term = ?1")?;
    let mut counted = Vec::with_capacity(terms.len());
    for term in &terms {
        let doc: Option<i64> = stmt.query_row([term.to_lowercase()], |r| r.get(0)).optional()?;
        counted.push((*term, doc.unwrap_or(0)));
    }
    let limit = (total as f64 * COMMON_TERM_SHARE) as i64;
    let selective: Vec<&str> =
        counted.iter().filter(|(_, d)| *d <= limit).map(|(t, _)| *t).collect();
    if !selective.is_empty() {
        return Ok(expression(&selective));
    }
    let rarest = counted.iter().min_by_key(|(_, d)| *d).map(|(t, _)| *t);
    Ok(rarest.and_then(|t| expression(&[t])))
}

pub(crate) fn search(conn: &Connection, q: &SearchQuery) -> Result<Vec<SearchHit>> {
    let limit = q.limit.clamp(1, MAX_LIMIT) as i64;
    let mut params: Vec<Value> = Vec::new();

    let expr = selective_expression(conn, &q.text)?;
    // Phase 1 ranks and filters on cheap columns only; phase 2 computes
    // snippets for the final page instead of for every match.
    let mut sql = match &expr {
        Some(expr) => {
            params.push(Value::Text(expr.clone()));
            String::from(
                "WITH top AS (
                   SELECT f.rowid AS seq, f.rank AS rank FROM nodes_fts f
                   JOIN nodes n ON n.seq = f.rowid
                   WHERE nodes_fts MATCH ?1 AND n.deleted_at IS NULL",
            )
        }
        // No search terms: most recently changed entries matching the filters.
        None => String::from(
            "SELECT n.id, n.title, n.kind, substr(n.body, 1, 160), 0.0
             FROM nodes n WHERE n.deleted_at IS NULL",
        ),
    };

    if let Some(project) = &q.project {
        let ids = if q.include_subprojects {
            projects::subtree(conn, project)?
        } else {
            vec![project.clone()]
        };
        // Correlated EXISTS probes the (project_id, node_id) key per candidate
        // instead of materializing every member of the project.
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM project_members m WHERE m.node_id = n.id AND m.project_id IN (",
        );
        push_list(&mut sql, &mut params, ids.iter().map(|p| p.as_str().to_string()));
        sql.push_str("))");
    }
    if !q.kinds.is_empty() {
        sql.push_str(" AND n.kind IN (");
        push_list(&mut sql, &mut params, q.kinds.iter().cloned());
        sql.push(')');
    }
    if !q.tags.is_empty() {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM node_tags t WHERE t.node_id = n.id AND t.tag IN (",
        );
        push_list(
            &mut sql,
            &mut params,
            q.tags.iter().map(|t| t.trim_start_matches('#').to_lowercase()),
        );
        sql.push_str("))");
    }

    params.push(Value::Integer(limit));
    let limit_param = params.len();
    if expr.is_some() {
        sql.push_str(&format!(
            " ORDER BY f.rank LIMIT ?{limit_param})
             SELECT n.id, n.title, n.kind, snippet(nodes_fts, 1, '**', '**', '…', 16), -top.rank
             FROM top
             JOIN nodes_fts ON nodes_fts.rowid = top.seq AND nodes_fts MATCH ?1
             JOIN nodes n ON n.seq = top.seq
             ORDER BY top.rank"
        ));
    } else {
        sql.push_str(&format!(" ORDER BY n.updated_at DESC LIMIT ?{limit_param}"));
    }

    let mut stmt = conn.prepare(&sql)?;
    let hits = stmt
        .query_map(params_from_iter(params.iter()), |r| {
            Ok(SearchHit {
                node: NodeBrief { id: r.get(0)?, title: r.get(1)?, kind: r.get(2)? },
                snippet: r.get(3)?,
                score: r.get(4)?,
                semantic: false,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(hits)
}

fn push_list(sql: &mut String, params: &mut Vec<Value>, items: impl Iterator<Item = String>) {
    let mut first = true;
    for item in items {
        params.push(Value::Text(item));
        if !first {
            sql.push_str(", ");
        }
        sql.push_str(&format!("?{}", params.len()));
        first = false;
    }
    if first {
        // Empty list: match nothing rather than producing invalid SQL.
        sql.push_str("NULL");
    }
}

#[cfg(test)]
mod tests {
    use super::fts_expression;

    #[test]
    fn neutralizes_operators() {
        assert_eq!(
            fts_expression("tokio Nebenläufigkeit").unwrap(),
            r#""tokio"* "Nebenläufigkeit"*"#
        );
        assert_eq!(
            fts_expression("foo OR bar NEAR \"baz\"").unwrap(),
            r#""foo"* "bar"* "NEAR"* "baz"*"#,
            "operators become plain quoted terms"
        );
        assert_eq!(fts_expression("  -*( ) "), None);
        assert_eq!(
            fts_expression("Die Tokio und der Scheduler").unwrap(),
            r#""Tokio"* "Scheduler"*"#
        );
        assert_eq!(
            fts_expression("die und").unwrap(),
            r#""die"* "und"*"#,
            "only stopwords: keep them"
        );
    }
}
