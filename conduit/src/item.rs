//! The work-item page model: one generic shape for all three classes.
//!
//! A page is YAML frontmatter plus a markdown body. Rather than three typed
//! structs, [`Item`] wraps the frontmatter mapping whole and exposes typed
//! accessors for the fields the doors act on — every key the doors don't
//! own round-trips verbatim, and the body is never touched by a frontmatter
//! rewrite (the seal in [`crate::approval`] depends on that; both modules
//! share the split/assemble plumbing here).
//!
//! [`WorkCorpus`] loads every work-item page through the store and answers
//! the tree questions the lifecycle rules ask: an item's parent status, its
//! children's statuses, and identifier resolution (slug or id).

use serde_yaml_ng::{Mapping, Value};
use ulid::Ulid;

use crate::workitem::{Class, Status};

/// The wiki section conduit's work items live under (re-exported from the
/// lifecycle module for one source of truth).
pub use crate::workitem::WORK_ROOT;

#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("missing opening frontmatter delimiter `---`")]
    MissingOpenDelimiter,
    #[error("missing closing frontmatter delimiter `---`")]
    MissingCloseDelimiter,
    #[error("invalid frontmatter YAML: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("page {0}: missing or invalid `{1}` frontmatter")]
    BadField(String, &'static str),
}

// ── Page plumbing (shared with the approval seal) ──────────────────────────

/// The canonical body: leading newlines and trailing whitespace shed, every
/// interior byte kept (the house page-round-trip canon — what the seal
/// hashes).
pub(crate) fn canonical(body: &str) -> &str {
    body.trim_start_matches(['\r', '\n']).trim_end()
}

/// Split a page into (frontmatter mapping, raw body after the delimiters).
pub(crate) fn split(page: &str) -> Result<(Mapping, &str), ItemError> {
    let trimmed = page.trim_start();
    if !trimmed.starts_with("---") {
        return Err(ItemError::MissingOpenDelimiter);
    }
    let after_open = trimmed[3..].trim_start_matches(['\r', '\n']);
    let close = after_open
        .find("\n---")
        .ok_or(ItemError::MissingCloseDelimiter)?;
    // The newline before the delimiter belongs to the YAML block (lossless
    // round-trip for keep-chomping block scalars).
    let yaml = &after_open[..close + 1];
    let body = after_open[close + 4..].trim_start_matches(['\r', '\n']);
    let fm: Mapping = serde_yaml_ng::from_str(yaml)?;
    Ok((fm, body))
}

/// Reassemble a page from rewritten frontmatter and the untouched body, in
/// the house canonical form (blank line after the frontmatter, one trailing
/// newline).
pub(crate) fn assemble(fm: &Mapping, body: &str) -> Result<String, ItemError> {
    let yaml = serde_yaml_ng::to_string(fm)?;
    let mut out = String::from("---\n");
    out.push_str(&yaml);
    out.push_str("---\n");
    let body = canonical(body);
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    Ok(out)
}

// ── Item ───────────────────────────────────────────────────────────────────

/// One work-item page: the whole frontmatter, the body, typed access to the
/// fields the doors act on.
#[derive(Debug, Clone)]
pub struct Item {
    pub fm: Mapping,
    pub body: String,
}

impl Item {
    /// A fresh draft page of the given class.
    pub fn new(class: Class, title: &str, created: &str, body: &str) -> Self {
        let mut fm = Mapping::new();
        let mut set = |k: &str, v: &str| {
            fm.insert(Value::String(k.into()), Value::String(v.into()));
        };
        set("id", &Ulid::generate().to_string());
        set("title", title);
        set("type", class_name(class));
        set("status", "draft");
        set("created", created);
        Item {
            fm,
            body: canonical(body).to_string(),
        }
    }

    pub fn parse(slug: &str, text: &str) -> Result<Self, ItemError> {
        let (fm, body) = split(text).map_err(|e| match e {
            ItemError::Yaml(y) => ItemError::Yaml(y),
            other => other,
        })?;
        let item = Item {
            fm,
            body: body.to_string(),
        };
        // The fields every door decision hangs on must parse.
        item.class()
            .ok_or_else(|| ItemError::BadField(slug.into(), "type"))?;
        item.status()
            .ok_or_else(|| ItemError::BadField(slug.into(), "status"))?;
        Ok(item)
    }

    pub fn serialize(&self) -> Result<String, ItemError> {
        assemble(&self.fm, &self.body)
    }

    fn str_field(&self, key: &str) -> Option<&str> {
        self.fm.get(key).and_then(Value::as_str)
    }

    pub fn id(&self) -> Option<&str> {
        self.str_field("id")
    }

    pub fn title(&self) -> Option<&str> {
        self.str_field("title")
    }

    pub fn class(&self) -> Option<Class> {
        match self.str_field("type")? {
            "project" => Some(Class::Project),
            "story" => Some(Class::Story),
            "task" => Some(Class::Task),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<Status> {
        serde_yaml_ng::from_value(Value::String(self.str_field("status")?.into())).ok()
    }

    /// The parent reference this class stores (`project:` on a story,
    /// `story:` on a task); `None` for a project.
    pub fn parent_ref(&self) -> Option<&str> {
        match self.class()? {
            Class::Project => None,
            Class::Story => self.str_field("project"),
            Class::Task => self.str_field("story"),
        }
    }

    /// Set a string frontmatter field (door-owned state — never the body).
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.fm.insert(Value::String(key.into()), value.into());
    }

    pub fn set_status(&mut self, status: Status) {
        let v = serde_yaml_ng::to_value(status).expect("status serializes");
        self.fm.insert(Value::String("status".into()), v);
    }
}

/// The wiki type name for a class (matches `x-wiki-types` in the schemas).
pub fn class_name(class: Class) -> &'static str {
    match class {
        Class::Project => "project",
        Class::Story => "story",
        Class::Task => "task",
    }
}

/// Slug stem for a new page: `<class>-<kebab-title>`, capped for sanity.
pub fn stem(class: Class, title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = &slug[..slug.len().min(60)];
    format!("{}-{}", class_name(class), slug.trim_matches('-'))
}

// ── Corpus ─────────────────────────────────────────────────────────────────

/// One loaded page: slug, parsed item, and the raw text (the seal operates
/// on text, so the doors keep it).
#[derive(Debug, Clone)]
pub struct Loaded {
    pub slug: String,
    pub text: String,
    pub item: Item,
}

impl Loaded {
    /// True when a stored parent/child reference (an id or a slug, with or
    /// without the `work/` prefix) points at this page.
    pub fn is_target_of(&self, target: &str) -> bool {
        let t = target.trim();
        !t.is_empty()
            && (self.slug == t
                || self.slug.strip_prefix(&format!("{WORK_ROOT}/")) == Some(t)
                || self.item.id().is_some_and(|id| id.eq_ignore_ascii_case(t)))
    }
}

/// Every work-item page in the wiki, loaded through the store.
#[derive(Debug, Default)]
pub struct WorkCorpus {
    pub entries: Vec<Loaded>,
}

impl WorkCorpus {
    /// Resolve a caller-supplied identifier (slug or page id) to one entry.
    pub fn resolve(&self, input: &str) -> anyhow::Result<&Loaded> {
        let t = input.trim();
        if t.is_empty() {
            anyhow::bail!("empty work-item identifier");
        }
        match self.entries.iter().find(|e| e.is_target_of(t)) {
            Some(e) => Ok(e),
            None => anyhow::bail!(
                "no work item matches '{input}' (known: {})",
                if self.entries.is_empty() {
                    "none — the wiki has no work items yet".to_string()
                } else {
                    self.entries
                        .iter()
                        .map(|e| e.slug.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        }
    }

    /// The parent entry of an item, if its class has one and the reference
    /// resolves.
    pub fn parent_of(&self, of: &Loaded) -> Option<&Loaded> {
        let target = of.item.parent_ref()?;
        self.entries.iter().find(|e| e.is_target_of(target))
    }

    /// Direct children of an item (stories of a project, tasks of a story).
    pub fn children_of(&self, of: &Loaded) -> Vec<&Loaded> {
        self.entries
            .iter()
            .filter(|e| {
                e.item
                    .parent_ref()
                    .is_some_and(|target| of.is_target_of(target))
            })
            .collect()
    }

    /// The children's statuses — what the uphill close rule consumes.
    pub fn children_statuses(&self, of: &Loaded) -> Vec<Status> {
        self.children_of(of)
            .iter()
            .filter_map(|e| e.item.status())
            .collect()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_parse_round_trip_preserves_foreign_keys_and_body() {
        let mut item = Item::new(
            Class::Task,
            "Do the thing",
            "2026-08-16T00:00:00Z",
            "## Goal\n\nyes\n",
        );
        item.set("story", "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        item.set("x-foreign", "kept");
        let text = item.serialize().unwrap();
        let back = Item::parse("work/task-do-the-thing", &text).unwrap();
        assert_eq!(back.class(), Some(Class::Task));
        assert_eq!(back.status(), Some(Status::Draft));
        assert_eq!(back.title(), Some("Do the thing"));
        assert_eq!(back.parent_ref(), Some("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
        assert_eq!(
            back.fm.get("x-foreign").and_then(Value::as_str),
            Some("kept")
        );
        assert_eq!(canonical(&back.body), "## Goal\n\nyes");
        assert!(Ulid::from_string(back.id().unwrap()).is_ok());
    }

    #[test]
    fn status_rewrite_never_touches_the_body() {
        let item = Item::new(Class::Story, "S", "2026-08-16T00:00:00Z", "body text");
        let before = item.serialize().unwrap();
        let mut item = Item::parse("s", &before).unwrap();
        item.set_status(Status::InProgress);
        let after = item.serialize().unwrap();
        assert_eq!(split(&before).unwrap().1, split(&after).unwrap().1);
        assert!(after.contains("status: in-progress"));
    }

    #[test]
    fn stems_are_kebab_and_class_prefixed() {
        assert_eq!(
            stem(Class::Task, "Reject unsigned work items!"),
            "task-reject-unsigned-work-items"
        );
        assert_eq!(
            stem(Class::Project, "KB: v2 (alpha)"),
            "project-kb-v2-alpha"
        );
    }

    fn corpus() -> WorkCorpus {
        let mut entries = Vec::new();
        let mut add = |class: Class, title: &str, parent: Option<(&str, &str)>| {
            let mut item = Item::new(class, title, "2026-08-16T00:00:00Z", "b");
            if let Some((key, target)) = parent {
                item.set(key, target);
            }
            let slug = format!("{WORK_ROOT}/{}", stem(class, title));
            let text = item.serialize().unwrap();
            let item = Item::parse(&slug, &text).unwrap();
            entries.push(Loaded { slug, text, item });
        };
        add(Class::Project, "P", None);
        add(Class::Story, "S", Some(("project", "project-p")));
        add(Class::Task, "T one", Some(("story", "story-s")));
        add(Class::Task, "T two", Some(("story", "story-s")));
        WorkCorpus { entries }
    }

    #[test]
    fn resolution_and_tree_queries() {
        let c = corpus();
        // Slug with or without the work/ prefix, and by id.
        let story = c.resolve("story-s").unwrap();
        assert_eq!(c.resolve("work/story-s").unwrap().slug, story.slug);
        let by_id = c.resolve(story.item.id().unwrap()).unwrap();
        assert_eq!(by_id.slug, story.slug);

        let project = c.resolve("project-p").unwrap();
        assert_eq!(c.parent_of(story).unwrap().slug, project.slug);
        assert!(c.parent_of(project).is_none());
        assert_eq!(c.children_of(story).len(), 2);
        assert_eq!(c.children_of(project).len(), 1);
        assert_eq!(
            c.children_statuses(story),
            vec![Status::Draft, Status::Draft]
        );
        assert!(c.resolve("nope").is_err());
    }
}
