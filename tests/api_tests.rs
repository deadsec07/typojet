use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tempfile::tempdir;
use tower::util::ServiceExt;
use typojet::api::{build_router, build_router_with_auth};

#[tokio::test]
async fn api_supports_multiple_indexes() {
    let dir = tempdir().unwrap();
    let app = build_router(dir.path());

    let create_index_response = app
        .clone()
        .oneshot(
            Request::post("/indexes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "books",
                        "searchable_fields": [
                            { "name": "title", "boost": 4.0 },
                            { "name": "tags", "boost": 2.0 },
                            { "name": "description", "boost": 1.0 }
                        ],
                        "filterable_fields": ["tags", "category", "published"],
                        "sortable_fields": ["title", "category"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_index_response.status(), StatusCode::CREATED);

    let documents_response = app
        .clone()
        .oneshot(
            Request::post("/indexes/books/documents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!([
                        {
                            "id": "book-1",
                            "title": "Rust Search Engine",
                            "tags": ["rust", "search"],
                            "description": "Tiny search for docs"
                        },
                        {
                            "id": "book-2",
                            "title": "Gardening Notes",
                            "tags": ["plants"],
                            "category": "lifestyle",
                            "published": true,
                            "description": "A guide to backyard herbs"
                        },
                        {
                            "id": "book-3",
                            "title": "Rust Web Handbook",
                            "tags": ["rust", "web"],
                            "category": "engineering",
                            "published": true,
                            "description": "A guide to backyard herbs"
                        }
                    ])
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(documents_response.status(), StatusCode::CREATED);

    let list_documents_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/documents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_documents_response.status(), StatusCode::OK);

    let get_document_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/documents/book-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_document_response.status(), StatusCode::OK);

    let patch_document_response = app
        .clone()
        .oneshot(
            Request::patch("/indexes/books/documents/book-1")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "description": "Updated instant search handbook"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_document_response.status(), StatusCode::OK);

    let search_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/search?q=instnt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(search_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["hits"][0]["id"], "book-1");
    assert_eq!(
        payload["hits"][0]["snippets"]["description"],
        "Updated instant search handbook"
    );

    let filtered_search_response = app
        .clone()
        .oneshot(
            Request::get(
                "/indexes/books/search?q=rust&filter.tags=web&filter.category=engineering",
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(filtered_search_response.status(), StatusCode::OK);

    let filtered_body = axum::body::to_bytes(filtered_search_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let filtered_payload: Value = serde_json::from_slice(&filtered_body).unwrap();
    assert_eq!(filtered_payload["total"], 1);
    assert_eq!(filtered_payload["hits"][0]["id"], "book-3");

    let sorted_search_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/search?q=rust&sort=title:asc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sorted_search_response.status(), StatusCode::OK);

    let sorted_body = axum::body::to_bytes(sorted_search_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sorted_payload: Value = serde_json::from_slice(&sorted_body).unwrap();
    assert_eq!(sorted_payload["hits"][0]["id"], "book-1");

    let paginated_search_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/search?q=guide&limit=1&offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paginated_search_response.status(), StatusCode::OK);

    let paginated_body = axum::body::to_bytes(paginated_search_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let paginated_payload: Value = serde_json::from_slice(&paginated_body).unwrap();
    assert_eq!(paginated_payload["total"], 2);
    assert_eq!(paginated_payload["offset"], 1);
    assert_eq!(paginated_payload["limit"], 1);
    assert_eq!(paginated_payload["hits"].as_array().unwrap().len(), 1);

    let stats_response = app
        .clone()
        .oneshot(
            Request::get("/indexes/books/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stats_response.status(), StatusCode::OK);

    let stats_body = axum::body::to_bytes(stats_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let stats_payload: Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(stats_payload["documents"], 3);
    assert_eq!(stats_payload["filterable_fields"][0], "tags");

    let list_response = app
        .clone()
        .oneshot(Request::get("/indexes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let update_response = app
        .clone()
        .oneshot(
            Request::put("/indexes/books")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "searchable_fields": [
                            { "name": "title", "boost": 5.0 }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let delete_document_response = app
        .clone()
        .oneshot(
            Request::delete("/indexes/books/documents/book-2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_document_response.status(), StatusCode::OK);

    let delete_response = app
        .oneshot(
            Request::delete("/indexes/books")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn legacy_default_routes_still_work() {
    let dir = tempdir().unwrap();
    let app = build_router(dir.path());

    let documents_response = app
        .clone()
        .oneshot(
            Request::post("/documents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "documents": [
                            {
                                "id": "doc-1",
                                "title": "Schema Aware Search",
                                "tags": ["ranking"],
                                "description": "Field boosts improve results"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(documents_response.status(), StatusCode::CREATED);

    let search_response = app
        .oneshot(
            Request::get("/search?q=schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn write_routes_require_bearer_token_when_configured() {
    let dir = tempdir().unwrap();
    let app = build_router_with_auth(dir.path(), Some("secret-token".to_string()));

    let unauthorized = app
        .clone()
        .oneshot(
            Request::post("/indexes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "secure-books",
                        "searchable_fields": [
                            { "name": "title", "boost": 3.0 }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .clone()
        .oneshot(
            Request::post("/indexes")
                .header("content-type", "application/json")
                .header("authorization", "Bearer secret-token")
                .body(Body::from(
                    json!({
                        "name": "secure-books",
                        "searchable_fields": [
                            { "name": "title", "boost": 3.0 }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::CREATED);

    let health = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}
