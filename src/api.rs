use crate::engine::{
    DEFAULT_INDEX, DocumentListResponse, EngineError, IndexConfig, IndexStats, IndexSummary,
    SearchService, SortDirection, SortSpec, default_storage_path,
};
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<RwLock<SearchService>>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DocumentsPayload {
    Wrapped { documents: Vec<Value> },
    Raw(Vec<Value>),
}

impl DocumentsPayload {
    fn into_documents(self) -> Vec<Value> {
        match self {
            Self::Wrapped { documents } => documents,
            Self::Raw(documents) => documents,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateIndexRequest {
    pub name: String,
    pub searchable_fields: Option<Vec<crate::engine::FieldConfig>>,
    pub filterable_fields: Option<Vec<String>>,
    pub sortable_fields: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DocumentPath {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub indexes: usize,
    pub documents: usize,
}

#[derive(Debug, Serialize)]
pub struct WriteResponse {
    pub success: bool,
    pub documents: usize,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct IndexListResponse {
    pub indexes: Vec<IndexSummary>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

pub fn build_router(data_root: impl AsRef<Path>) -> Router {
    build_router_with_auth(data_root, None)
}

pub fn build_router_with_auth(data_root: impl AsRef<Path>, api_key: Option<String>) -> Router {
    let storage_path = default_storage_path(data_root);
    let service = SearchService::open(storage_path).expect("failed to open search service");

    Router::new()
        .route("/health", get(health))
        .route("/indexes", get(list_indexes).post(create_index))
        .route(
            "/indexes/{name}",
            get(get_index).put(update_index).delete(delete_index),
        )
        .route("/indexes/{name}/stats", get(get_index_stats))
        .route(
            "/indexes/{name}/documents",
            get(list_documents_in_index).post(add_documents_to_index),
        )
        .route(
            "/indexes/{name}/documents/{id}",
            get(get_document_in_index)
                .patch(patch_document_in_index)
                .delete(delete_document_in_index),
        )
        .route("/indexes/{name}/search", get(search_index))
        .route("/index", post(set_default_index))
        .route("/documents", post(add_default_documents))
        .route("/search", get(search_default_index))
        .with_state(AppState {
            service: Arc::new(RwLock::new(service)),
            api_key,
        })
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let service = state.service.read().expect("service lock poisoned");
    Json(HealthResponse {
        status: "ok",
        indexes: service.index_count(),
        documents: service.total_documents(),
    })
}

async fn list_indexes(State(state): State<AppState>) -> impl IntoResponse {
    let service = state.service.read().expect("service lock poisoned");
    Json(IndexListResponse {
        indexes: service.list_indexes(),
    })
}

async fn get_index(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    Ok((StatusCode::OK, Json(service.get_index(&name)?)))
}

async fn create_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateIndexRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    let CreateIndexRequest {
        name,
        searchable_fields,
        filterable_fields,
        sortable_fields,
    } = request;
    let config = if let Some(searchable_fields) = searchable_fields {
        IndexConfig {
            searchable_fields,
            filterable_fields: filterable_fields.unwrap_or_default(),
            sortable_fields: sortable_fields.unwrap_or_default(),
        }
    } else {
        let mut config = IndexConfig::default();
        config.filterable_fields = filterable_fields.unwrap_or_default();
        config.sortable_fields = sortable_fields.unwrap_or_default();
        config
    };
    let summary = service.create_index(name, config)?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn update_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(name): AxumPath<String>,
    Json(config): Json<IndexConfig>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    Ok((
        StatusCode::OK,
        Json(service.set_index_config(&name, config)?),
    ))
}

async fn delete_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    service.delete_index(&name)?;
    Ok((StatusCode::OK, Json(DeleteResponse { success: true })))
}

async fn get_index_stats(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    let stats: IndexStats = service.get_index_stats(&name)?;
    Ok((StatusCode::OK, Json(stats)))
}

async fn add_documents_to_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(name): AxumPath<String>,
    Json(payload): Json<DocumentsPayload>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    let added = service.add_documents(&name, payload.into_documents())?;
    Ok((
        StatusCode::CREATED,
        Json(WriteResponse {
            success: true,
            documents: added,
        }),
    ))
}

async fn list_documents_in_index(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    let documents: DocumentListResponse = service.list_documents(&name)?;
    Ok((StatusCode::OK, Json(documents)))
}

async fn get_document_in_index(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<DocumentPath>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    Ok((
        StatusCode::OK,
        Json(service.get_document(&path.name, &path.id)?),
    ))
}

async fn patch_document_in_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(path): AxumPath<DocumentPath>,
    Json(patch): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    Ok((
        StatusCode::OK,
        Json(service.patch_document(&path.name, &path.id, patch)?),
    ))
}

async fn delete_document_in_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(path): AxumPath<DocumentPath>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    service.delete_document(&path.name, &path.id)?;
    Ok((StatusCode::OK, Json(DeleteResponse { success: true })))
}

async fn search_index(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    let request = parse_search_request(params)?;
    Ok((
        StatusCode::OK,
        Json(service.search(
            &name,
            &request.query,
            request.offset,
            request.limit,
            &request.filters,
            request.sort.as_ref(),
        )?),
    ))
}

async fn set_default_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(config): Json<IndexConfig>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    Ok((
        StatusCode::OK,
        Json(service.set_index_config(DEFAULT_INDEX, config)?),
    ))
}

async fn add_default_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DocumentsPayload>,
) -> Result<impl IntoResponse, ApiError> {
    require_api_key(&state, &headers)?;
    let mut service = state.service.write().expect("service lock poisoned");
    let added = service.add_documents(DEFAULT_INDEX, payload.into_documents())?;
    Ok((
        StatusCode::CREATED,
        Json(WriteResponse {
            success: true,
            documents: added,
        }),
    ))
}

async fn search_default_index(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.service.read().expect("service lock poisoned");
    let request = parse_search_request(params)?;
    Ok((
        StatusCode::OK,
        Json(service.search(
            DEFAULT_INDEX,
            &request.query,
            request.offset,
            request.limit,
            &request.filters,
            request.sort.as_ref(),
        )?),
    ))
}

struct SearchRequest {
    query: String,
    offset: usize,
    limit: usize,
    filters: HashMap<String, String>,
    sort: Option<SortSpec>,
}

fn parse_search_request(params: HashMap<String, String>) -> Result<SearchRequest, EngineError> {
    let mut query = String::new();
    let mut offset = 0usize;
    let mut limit = 10usize;
    let mut filters = HashMap::new();
    let mut sort = None;

    for (key, value) in params {
        if key == "q" {
            query = value;
        } else if key == "offset" {
            offset = value.parse::<usize>().unwrap_or(0);
        } else if key == "limit" {
            limit = value.parse::<usize>().unwrap_or(10).min(100);
        } else if key == "sort" {
            sort = Some(parse_sort(&value)?);
        } else if let Some(field) = key.strip_prefix("filter.") {
            if !field.is_empty() {
                filters.insert(field.to_string(), value);
            }
        }
    }

    Ok(SearchRequest {
        query,
        offset,
        limit,
        filters,
        sort,
    })
}

fn parse_sort(value: &str) -> Result<SortSpec, EngineError> {
    let Some((field, direction)) = value.split_once(':') else {
        return Err(EngineError::InvalidSort(value.to_string()));
    };

    let direction = match direction {
        "asc" => SortDirection::Asc,
        "desc" => SortDirection::Desc,
        _ => return Err(EngineError::InvalidSort(value.to_string())),
    };

    if field.trim().is_empty() {
        return Err(EngineError::InvalidSort(value.to_string()));
    }

    Ok(SortSpec {
        field: field.to_string(),
        direction,
    })
}

fn require_api_key(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.api_key.as_deref() else {
        return Ok(());
    };

    let provided = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    if provided == Some(expected) {
        Ok(())
    } else {
        Err(ApiError(EngineError::Unauthorized))
    }
}

#[derive(Debug)]
pub struct ApiError(EngineError);

impl From<EngineError> for ApiError {
    fn from(value: EngineError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.0 {
            EngineError::MissingId | EngineError::EmptyConfig | EngineError::EmptyIndexName => {
                StatusCode::BAD_REQUEST
            }
            EngineError::IndexNotFound(_) | EngineError::DocumentNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            EngineError::IndexExists(_) => StatusCode::CONFLICT,
            EngineError::InvalidDocumentPatch
            | EngineError::InvalidFilterField(_)
            | EngineError::InvalidSortField(_)
            | EngineError::InvalidSort(_) => StatusCode::BAD_REQUEST,
            EngineError::Unauthorized => StatusCode::UNAUTHORIZED,
            EngineError::Persist(_) | EngineError::Serialize(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(ErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
