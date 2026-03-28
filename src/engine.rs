use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_INDEX: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldConfig {
    pub name: String,
    pub boost: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexConfig {
    pub searchable_fields: Vec<FieldConfig>,
    #[serde(default)]
    pub filterable_fields: Vec<String>,
    #[serde(default)]
    pub sortable_fields: Vec<String>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            searchable_fields: vec![
                FieldConfig {
                    name: "title".to_string(),
                    boost: 3.0,
                },
                FieldConfig {
                    name: "tags".to_string(),
                    boost: 2.0,
                },
                FieldConfig {
                    name: "description".to_string(),
                    boost: 1.0,
                },
            ],
            filterable_fields: Vec::new(),
            sortable_fields: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDocument {
    pub id: String,
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredIndex {
    pub name: String,
    pub config: IndexConfig,
    pub documents: HashMap<String, StoredDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    indexes: HashMap<String, StoredIndex>,
}

#[derive(Debug, Clone)]
struct Posting {
    doc_id: String,
    field: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
    pub snippets: HashMap<String, String>,
    pub document: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexSummary {
    pub name: String,
    pub documents: usize,
    pub searchable_fields: Vec<FieldConfig>,
    pub filterable_fields: Vec<String>,
    pub sortable_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStats {
    pub name: String,
    pub documents: usize,
    pub searchable_fields: Vec<FieldConfig>,
    pub filterable_fields: Vec<String>,
    pub sortable_fields: Vec<String>,
    pub vocabulary_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentListResponse {
    pub documents: Vec<Value>,
    pub total: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("document is missing required string field `id`")]
    MissingId,
    #[error("index config must include at least one searchable field")]
    EmptyConfig,
    #[error("index `{0}` was not found")]
    IndexNotFound(String),
    #[error("index `{0}` already exists")]
    IndexExists(String),
    #[error("index name cannot be empty")]
    EmptyIndexName,
    #[error("document `{0}` was not found")]
    DocumentNotFound(String),
    #[error("document patch must be a JSON object")]
    InvalidDocumentPatch,
    #[error("filter field `{0}` is not configured as filterable")]
    InvalidFilterField(String),
    #[error("sort field `{0}` is not configured as sortable")]
    InvalidSortField(String),
    #[error("invalid sort value `{0}`; expected field:asc or field:desc")]
    InvalidSort(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("failed to persist data: {0}")]
    Persist(#[from] io::Error),
    #[error("failed to serialize persisted data: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Clone)]
struct SearchIndex {
    stored: StoredIndex,
    inverted_index: HashMap<String, Vec<Posting>>,
    vocabulary: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    pub field: String,
    pub direction: SortDirection,
}

impl SearchIndex {
    fn new(name: String, config: IndexConfig) -> Self {
        let mut index = Self {
            stored: StoredIndex {
                name,
                config,
                documents: HashMap::new(),
            },
            inverted_index: HashMap::new(),
            vocabulary: BTreeSet::new(),
        };
        index.rebuild_index();
        index
    }

    fn from_stored(stored: StoredIndex) -> Self {
        let mut index = Self {
            stored,
            inverted_index: HashMap::new(),
            vocabulary: BTreeSet::new(),
        };
        index.rebuild_index();
        index
    }

    fn summary(&self) -> IndexSummary {
        IndexSummary {
            name: self.stored.name.clone(),
            documents: self.stored.documents.len(),
            searchable_fields: self.stored.config.searchable_fields.clone(),
            filterable_fields: self.stored.config.filterable_fields.clone(),
            sortable_fields: self.stored.config.sortable_fields.clone(),
        }
    }

    fn stats(&self) -> IndexStats {
        IndexStats {
            name: self.stored.name.clone(),
            documents: self.stored.documents.len(),
            searchable_fields: self.stored.config.searchable_fields.clone(),
            filterable_fields: self.stored.config.filterable_fields.clone(),
            sortable_fields: self.stored.config.sortable_fields.clone(),
            vocabulary_size: self.vocabulary.len(),
        }
    }

    fn set_config(&mut self, config: IndexConfig) -> EngineResult<()> {
        if config.searchable_fields.is_empty() {
            return Err(EngineError::EmptyConfig);
        }
        self.stored.config = config;
        self.rebuild_index();
        Ok(())
    }

    fn add_documents(&mut self, docs: Vec<Value>) -> EngineResult<usize> {
        let mut added = 0usize;
        for value in docs {
            let object = value.as_object().cloned().ok_or(EngineError::MissingId)?;
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .ok_or(EngineError::MissingId)?
                .to_string();

            self.stored
                .documents
                .insert(id.clone(), StoredDocument { id, fields: object });
            added += 1;
        }
        self.rebuild_index();
        Ok(added)
    }

    fn list_documents(&self) -> DocumentListResponse {
        let mut documents: Vec<Value> = self
            .stored
            .documents
            .values()
            .map(|document| Value::Object(document.fields.clone()))
            .collect();

        documents.sort_by(|left, right| {
            let left_id = left.get("id").and_then(Value::as_str).unwrap_or_default();
            let right_id = right.get("id").and_then(Value::as_str).unwrap_or_default();
            left_id.cmp(right_id)
        });

        let total = documents.len();
        DocumentListResponse { documents, total }
    }

    fn get_document(&self, id: &str) -> EngineResult<Value> {
        let document = self
            .stored
            .documents
            .get(id)
            .ok_or_else(|| EngineError::DocumentNotFound(id.to_string()))?;
        Ok(Value::Object(document.fields.clone()))
    }

    fn patch_document(&mut self, id: &str, patch: Value) -> EngineResult<Value> {
        let patch = patch.as_object().ok_or(EngineError::InvalidDocumentPatch)?;
        let updated = {
            let document = self
                .stored
                .documents
                .get_mut(id)
                .ok_or_else(|| EngineError::DocumentNotFound(id.to_string()))?;

            for (key, value) in patch {
                if key == "id" {
                    continue;
                }
                document.fields.insert(key.clone(), value.clone());
            }

            Value::Object(document.fields.clone())
        };
        self.rebuild_index();
        Ok(updated)
    }

    fn delete_document(&mut self, id: &str) -> EngineResult<()> {
        self.stored
            .documents
            .remove(id)
            .ok_or_else(|| EngineError::DocumentNotFound(id.to_string()))?;
        self.rebuild_index();
        Ok(())
    }

    fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
        filters: &HashMap<String, String>,
        sort: Option<&SortSpec>,
    ) -> SearchResponse {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return SearchResponse {
                query: query.to_string(),
                hits: Vec::new(),
                total: 0,
                offset,
                limit,
            };
        }

        let mut scores: HashMap<String, f32> = HashMap::new();
        let mut matched_terms: HashMap<String, HashSet<String>> = HashMap::new();

        for query_token in &query_tokens {
            let mut local_updates: HashMap<String, f32> = HashMap::new();

            if let Some(postings) = self.inverted_index.get(query_token) {
                self.apply_postings(&mut local_updates, postings, 10.0, 0.0);
            }

            for term in self.prefix_terms(query_token) {
                if term == *query_token {
                    continue;
                }
                if let Some(postings) = self.inverted_index.get(&term) {
                    self.apply_postings(&mut local_updates, postings, 6.0, 0.0);
                }
            }

            for term in self.typo_terms(query_token) {
                if term == *query_token || term.starts_with(query_token) {
                    continue;
                }
                if let Some(postings) = self.inverted_index.get(&term) {
                    let distance = levenshtein(query_token, &term) as f32;
                    self.apply_postings(&mut local_updates, postings, 4.0, distance * 2.0);
                }
            }

            for (doc_id, score) in local_updates {
                scores
                    .entry(doc_id.clone())
                    .and_modify(|current| *current += score)
                    .or_insert(score);
                matched_terms
                    .entry(doc_id)
                    .or_default()
                    .insert(query_token.clone());
            }
        }

        let mut hits: Vec<SearchHit> = scores
            .into_iter()
            .filter_map(|(doc_id, score)| {
                let terms = matched_terms.get(&doc_id)?;
                if terms.len() != query_tokens.len() {
                    return None;
                }
                let document = self.stored.documents.get(&doc_id)?;
                if !matches_filters(&document.fields, filters) {
                    return None;
                }
                Some(SearchHit {
                    id: doc_id,
                    score,
                    snippets: build_snippets(&document.fields, &self.stored.config, &query_tokens),
                    document: Value::Object(document.fields.clone()),
                })
            })
            .collect();

        if let Some(sort_spec) = sort {
            hits.sort_by(|left, right| {
                compare_sort_values(
                    extract_sort_value(&left.document, &sort_spec.field),
                    extract_sort_value(&right.document, &sort_spec.field),
                    &sort_spec.direction,
                )
                .then_with(|| right.score.total_cmp(&left.score))
                .then_with(|| left.id.cmp(&right.id))
            });
        } else {
            hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        }
        let total = hits.len();
        let hits = hits.into_iter().skip(offset).take(limit).collect();

        SearchResponse {
            query: query.to_string(),
            hits,
            total,
            offset,
            limit,
        }
    }

    fn apply_postings(
        &self,
        scores: &mut HashMap<String, f32>,
        postings: &[Posting],
        base_boost: f32,
        typo_penalty: f32,
    ) {
        for posting in postings {
            let field_boost = self.field_boost(&posting.field);
            let score = (base_boost * field_boost) - typo_penalty;
            if score <= 0.0 {
                continue;
            }
            scores
                .entry(posting.doc_id.clone())
                .and_modify(|current| {
                    if score > *current {
                        *current = score;
                    }
                })
                .or_insert(score);
        }
    }

    fn field_boost(&self, field: &str) -> f32 {
        self.stored
            .config
            .searchable_fields
            .iter()
            .find(|config| config.name == field)
            .map(|config| config.boost)
            .unwrap_or(1.0)
    }

    fn prefix_terms(&self, prefix: &str) -> Vec<String> {
        self.vocabulary
            .iter()
            .filter(|term| term.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn typo_terms(&self, query_token: &str) -> Vec<String> {
        let max_distance = if query_token.len() <= 4 { 1 } else { 2 };
        self.vocabulary
            .iter()
            .filter(|term| {
                let len_diff = term.len().abs_diff(query_token.len());
                len_diff <= max_distance && levenshtein(term, query_token) <= max_distance
            })
            .cloned()
            .collect()
    }

    fn rebuild_index(&mut self) {
        self.inverted_index.clear();
        self.vocabulary.clear();

        for document in self.stored.documents.values() {
            for field in &self.stored.config.searchable_fields {
                let raw_text = document
                    .fields
                    .get(&field.name)
                    .map(extract_text)
                    .unwrap_or_default();

                for token in tokenize(&raw_text) {
                    self.vocabulary.insert(token.clone());
                    self.inverted_index.entry(token).or_default().push(Posting {
                        doc_id: document.id.clone(),
                        field: field.name.clone(),
                    });
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct SearchService {
    indexes: HashMap<String, SearchIndex>,
    storage_path: PathBuf,
}

impl SearchService {
    pub fn open(storage_path: impl Into<PathBuf>) -> EngineResult<Self> {
        let storage_path = storage_path.into();
        if storage_path.exists() {
            let bytes = fs::read(&storage_path)?;
            let persisted: PersistedState = serde_json::from_slice(&bytes)?;
            let indexes = persisted
                .indexes
                .into_iter()
                .map(|(name, stored)| (name, SearchIndex::from_stored(stored)))
                .collect();
            Ok(Self {
                indexes,
                storage_path,
            })
        } else {
            let mut service = Self {
                indexes: HashMap::new(),
                storage_path,
            };
            service.create_index(DEFAULT_INDEX.to_string(), IndexConfig::default())?;
            Ok(service)
        }
    }

    pub fn total_documents(&self) -> usize {
        self.indexes
            .values()
            .map(|index| index.stored.documents.len())
            .sum()
    }

    pub fn index_count(&self) -> usize {
        self.indexes.len()
    }

    pub fn list_indexes(&self) -> Vec<IndexSummary> {
        let mut items: Vec<_> = self.indexes.values().map(SearchIndex::summary).collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
    }

    pub fn get_index(&self, name: &str) -> EngineResult<IndexSummary> {
        self.indexes
            .get(name)
            .map(SearchIndex::summary)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))
    }

    pub fn create_index(
        &mut self,
        name: String,
        config: IndexConfig,
    ) -> EngineResult<IndexSummary> {
        if name.trim().is_empty() {
            return Err(EngineError::EmptyIndexName);
        }
        if config.searchable_fields.is_empty() {
            return Err(EngineError::EmptyConfig);
        }
        if self.indexes.contains_key(&name) {
            return Err(EngineError::IndexExists(name));
        }
        let index = SearchIndex::new(name.clone(), config);
        let summary = index.summary();
        self.indexes.insert(name, index);
        self.persist()?;
        Ok(summary)
    }

    pub fn delete_index(&mut self, name: &str) -> EngineResult<()> {
        self.indexes
            .remove(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        self.persist()
    }

    pub fn set_index_config(
        &mut self,
        name: &str,
        config: IndexConfig,
    ) -> EngineResult<IndexSummary> {
        let index = self
            .indexes
            .get_mut(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        index.set_config(config)?;
        let summary = index.summary();
        self.persist()?;
        Ok(summary)
    }

    pub fn add_documents(&mut self, name: &str, docs: Vec<Value>) -> EngineResult<usize> {
        let index = self
            .indexes
            .get_mut(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        let added = index.add_documents(docs)?;
        self.persist()?;
        Ok(added)
    }

    pub fn search(
        &self,
        name: &str,
        query: &str,
        offset: usize,
        limit: usize,
        filters: &HashMap<String, String>,
        sort: Option<&SortSpec>,
    ) -> EngineResult<SearchResponse> {
        let index = self
            .indexes
            .get(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        validate_filters(&index.stored.config, filters)?;
        validate_sort(&index.stored.config, sort)?;
        Ok(index.search(query, offset, limit, filters, sort))
    }

    pub fn get_index_stats(&self, name: &str) -> EngineResult<IndexStats> {
        self.indexes
            .get(name)
            .map(SearchIndex::stats)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))
    }

    pub fn list_documents(&self, name: &str) -> EngineResult<DocumentListResponse> {
        let index = self
            .indexes
            .get(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        Ok(index.list_documents())
    }

    pub fn get_document(&self, name: &str, id: &str) -> EngineResult<Value> {
        let index = self
            .indexes
            .get(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        index.get_document(id)
    }

    pub fn patch_document(&mut self, name: &str, id: &str, patch: Value) -> EngineResult<Value> {
        let index = self
            .indexes
            .get_mut(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        let document = index.patch_document(id, patch)?;
        self.persist()?;
        Ok(document)
    }

    pub fn delete_document(&mut self, name: &str, id: &str) -> EngineResult<()> {
        let index = self
            .indexes
            .get_mut(name)
            .ok_or_else(|| EngineError::IndexNotFound(name.to_string()))?;
        index.delete_document(id)?;
        self.persist()
    }

    fn persist(&self) -> EngineResult<()> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let state = PersistedState {
            indexes: self
                .indexes
                .iter()
                .map(|(name, index)| (name.clone(), index.stored.clone()))
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&state)?;
        fs::write(&self.storage_path, bytes)?;
        Ok(())
    }
}

fn extract_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(extract_text).collect::<Vec<_>>().join(" "),
        Value::Object(map) => map.values().map(extract_text).collect::<Vec<_>>().join(" "),
    }
}

fn validate_filters(config: &IndexConfig, filters: &HashMap<String, String>) -> EngineResult<()> {
    for field in filters.keys() {
        if !config.filterable_fields.iter().any(|item| item == field) {
            return Err(EngineError::InvalidFilterField(field.clone()));
        }
    }
    Ok(())
}

fn validate_sort(config: &IndexConfig, sort: Option<&SortSpec>) -> EngineResult<()> {
    if let Some(sort) = sort {
        if !config
            .sortable_fields
            .iter()
            .any(|item| item == &sort.field)
        {
            return Err(EngineError::InvalidSortField(sort.field.clone()));
        }
    }
    Ok(())
}

fn extract_sort_value(document: &Value, field: &str) -> Option<SortValue> {
    let object = document.as_object()?;
    let value = object.get(field)?;
    SortValue::from_json(value)
}

#[derive(Debug, Clone, PartialEq)]
enum SortValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl SortValue {
    fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(text) => Some(Self::String(text.to_lowercase())),
            Value::Number(number) => number.as_f64().map(Self::Number),
            Value::Bool(boolean) => Some(Self::Bool(*boolean)),
            _ => None,
        }
    }
}

fn compare_sort_values(
    left: Option<SortValue>,
    right: Option<SortValue>,
    direction: &SortDirection,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let ordering = match (left, right) {
        (Some(SortValue::String(left)), Some(SortValue::String(right))) => left.cmp(&right),
        (Some(SortValue::Number(left)), Some(SortValue::Number(right))) => left.total_cmp(&right),
        (Some(SortValue::Bool(left)), Some(SortValue::Bool(right))) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    };

    match direction {
        SortDirection::Asc => ordering,
        SortDirection::Desc => ordering.reverse(),
    }
}

fn matches_filters(fields: &Map<String, Value>, filters: &HashMap<String, String>) -> bool {
    filters.iter().all(|(field, expected)| {
        fields
            .get(field)
            .map(|value| value_matches_filter(value, expected))
            .unwrap_or(false)
    })
}

fn value_matches_filter(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => text.eq_ignore_ascii_case(expected),
        Value::Bool(boolean) => boolean.to_string().eq_ignore_ascii_case(expected),
        Value::Number(number) => number.to_string().eq_ignore_ascii_case(expected),
        Value::Array(items) => items
            .iter()
            .any(|item| value_matches_filter(item, expected)),
        _ => false,
    }
}

fn build_snippets(
    fields: &Map<String, Value>,
    config: &IndexConfig,
    query_tokens: &[String],
) -> HashMap<String, String> {
    let mut snippets = HashMap::new();

    for field in &config.searchable_fields {
        let Some(value) = fields.get(&field.name) else {
            continue;
        };
        let text = extract_text(value);
        if text.is_empty() {
            continue;
        }
        if let Some(snippet) = snippet_for_text(&text, query_tokens) {
            snippets.insert(field.name.clone(), snippet);
        }
    }

    snippets
}

fn snippet_for_text(text: &str, query_tokens: &[String]) -> Option<String> {
    let lowercase = text.to_lowercase();
    let mut first_match: Option<(usize, usize)> = None;

    for token in query_tokens {
        if let Some(index) = lowercase.find(token) {
            let current = (index, token.len());
            if first_match.is_none_or(|best| current.0 < best.0) {
                first_match = Some(current);
            }
            continue;
        }

        for candidate in tokenize(text) {
            if levenshtein(&candidate, token) <= if token.len() <= 4 { 1 } else { 2 } {
                if let Some(index) = lowercase.find(&candidate) {
                    let current = (index, candidate.len());
                    if first_match.is_none_or(|best| current.0 < best.0) {
                        first_match = Some(current);
                    }
                }
                break;
            }
        }
    }

    let (index, len) = first_match?;
    let start = index.saturating_sub(24);
    let end = (index + len + 48).min(text.len());
    let mut snippet = text[start..end].trim().to_string();

    if start > 0 {
        snippet = format!("...{snippet}");
    }
    if end < text.len() {
        snippet.push_str("...");
    }

    Some(snippet)
}

pub fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

pub fn levenshtein(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right_chars.len()).collect();
    let mut current = vec![0; right_chars.len() + 1];

    for (i, left_char) in left.chars().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right_chars.iter().enumerate() {
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            let substitution = previous[j] + usize::from(left_char != *right_char);
            current[j + 1] = insertion.min(deletion).min(substitution);
        }
        previous.clone_from(&current);
    }

    previous[right_chars.len()]
}

pub fn default_storage_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join("data").join("indexes.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn tokenization_is_lowercase_and_punctuation_aware() {
        assert_eq!(
            tokenize("Rust-based, Search Engine!"),
            vec!["rust", "based", "search", "engine"]
        );
    }

    #[test]
    fn typo_distance_is_computed() {
        assert_eq!(levenshtein("kitten", "sitten"), 1);
        assert_eq!(levenshtein("search", "serch"), 1);
    }

    #[test]
    fn search_supports_exact_prefix_and_typos() {
        let dir = tempdir().unwrap();
        let path = default_storage_path(dir.path());
        let mut service = SearchService::open(path).unwrap();
        service
            .add_documents(
                DEFAULT_INDEX,
                vec![
                    json!({
                        "id": "1",
                        "title": "Rust Search Engine",
                        "tags": ["rust", "search"],
                        "description": "Fast local search for docs"
                    }),
                    json!({
                        "id": "2",
                        "title": "Recipe Book",
                        "tags": ["cooking"],
                        "description": "Home kitchen guide"
                    }),
                ],
            )
            .unwrap();

        let filters = HashMap::new();
        assert_eq!(
            service
                .search(DEFAULT_INDEX, "rust", 0, 10, &filters, None)
                .unwrap()
                .hits[0]
                .id,
            "1"
        );
        assert_eq!(
            service
                .search(DEFAULT_INDEX, "sea", 0, 10, &filters, None)
                .unwrap()
                .hits[0]
                .id,
            "1"
        );
        assert_eq!(
            service
                .search(DEFAULT_INDEX, "serch", 0, 10, &filters, None)
                .unwrap()
                .hits[0]
                .id,
            "1"
        );
    }

    #[test]
    fn service_persists_documents_and_multiple_indexes() {
        let dir = tempdir().unwrap();
        let path = default_storage_path(dir.path());
        let mut service = SearchService::open(&path).unwrap();
        service
            .create_index(
                "books".to_string(),
                IndexConfig {
                    searchable_fields: vec![FieldConfig {
                        name: "title".to_string(),
                        boost: 5.0,
                    }],
                    filterable_fields: Vec::new(),
                    sortable_fields: Vec::new(),
                },
            )
            .unwrap();
        service
            .add_documents(
                "books",
                vec![json!({
                    "id": "doc-1",
                    "title": "Persistent Search"
                })],
            )
            .unwrap();

        let reopened = SearchService::open(path).unwrap();
        assert_eq!(reopened.index_count(), 2);
        assert_eq!(reopened.get_index("books").unwrap().documents, 1);
        let filters = HashMap::new();
        assert_eq!(
            reopened
                .search("books", "persistent", 0, 10, &filters, None)
                .unwrap()
                .hits[0]
                .id,
            "doc-1"
        );
    }

    #[test]
    fn sorting_and_filter_validation_work() {
        let dir = tempdir().unwrap();
        let path = default_storage_path(dir.path());
        let mut service = SearchService::open(path).unwrap();
        service
            .set_index_config(
                DEFAULT_INDEX,
                IndexConfig {
                    searchable_fields: IndexConfig::default().searchable_fields,
                    filterable_fields: vec!["category".to_string()],
                    sortable_fields: vec!["title".to_string()],
                },
            )
            .unwrap();
        service
            .add_documents(
                DEFAULT_INDEX,
                vec![
                    json!({"id":"1","title":"Zeta","description":"guide","category":"docs"}),
                    json!({"id":"2","title":"Alpha","description":"guide","category":"docs"}),
                ],
            )
            .unwrap();

        let mut filters = HashMap::new();
        filters.insert("category".to_string(), "docs".to_string());
        let response = service
            .search(
                DEFAULT_INDEX,
                "guide",
                0,
                10,
                &filters,
                Some(&SortSpec {
                    field: "title".to_string(),
                    direction: SortDirection::Asc,
                }),
            )
            .unwrap();
        assert_eq!(response.hits[0].id, "2");

        let mut bad_filters = HashMap::new();
        bad_filters.insert("tags".to_string(), "docs".to_string());
        assert!(matches!(
            service.search(DEFAULT_INDEX, "guide", 0, 10, &bad_filters, None),
            Err(EngineError::InvalidFilterField(_))
        ));
    }
}
