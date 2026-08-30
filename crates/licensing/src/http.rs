use std::net::SocketAddr;
use std::sync::Arc;

use httpd::{serve, Request, Response, ServerConfig};
use serde_json::{json, Value};

use crate::crypto::constant_time_eq;
use crate::model::{License, LicenseError};
use crate::service::{now_secs, IssueRequest, LicenseService};

pub struct ApiConfig {
    pub addr: String,
    pub admin_token: Option<String>,
    pub product: String,
}

pub fn router(
    service: Arc<LicenseService>,
    config: Arc<ApiConfig>,
) -> impl Fn(&Request) -> Option<Response> + Send + Sync + 'static {
    let started = now_secs();
    move |request| {
        request
            .path
            .starts_with("/v1")
            .then(|| route(request, &service, &config, started))
    }
}

pub fn spawn(service: Arc<LicenseService>, config: ApiConfig) -> std::io::Result<SocketAddr> {
    let addr = config.addr.clone();
    let config = Arc::new(config);
    let index = index_of(&config);
    let handler = router(service, config);

    serve(
        ServerConfig::new(addr, "licence-api").rate_limit(120, 60),
        move |request| {
            if request.is("GET", "/") {
                return Response::json(200, &index);
            }
            handler(request).unwrap_or_else(Response::not_found)
        },
    )
}

fn index_of(config: &ApiConfig) -> Value {
    json!({
        "service": "badomen-licensing",
        "product": config.product,
        "version": 1,
        "endpoints": [
            "GET /v1/health",
            "GET /v1/public-key",
            "POST /v1/activate",
            "POST /v1/validate",
            "POST /v1/refresh"
        ]
    })
}

fn route(
    request: &Request,
    service: &LicenseService,
    config: &ApiConfig,
    started: u64,
) -> Response {
    if request.is("GET", "/v1") || request.is("GET", "/v1/") {
        return Response::json(200, &index_of(config));
    }

    if request.is("GET", "/v1/health") {
        return Response::json(
            200,
            &json!({"status": "ok", "uptime": now_secs().saturating_sub(started)}),
        );
    }

    if request.is("GET", "/v1/public-key") {
        return Response::json(
            200,
            &json!({
                "algorithm": "ed25519",
                "public_key_hex": service.public_key_hex(),
                "public_key_base64url": service.public_key_base64(),
                "offline_grace_seconds": service.offline_grace()
            }),
        );
    }

    if request.is("POST", "/v1/activate") {
        return activate(request, service);
    }
    if request.is("POST", "/v1/validate") {
        return validate(request, service);
    }
    if request.is("POST", "/v1/refresh") {
        return refresh(request, service);
    }
    if request.path.starts_with("/v1/admin") {
        return admin(request, service, config);
    }

    Response::not_found()
}

fn activate(request: &Request, service: &LicenseService) -> Response {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };
    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.activate(&key, &hwid) {
        Ok((license, token)) => Response::json(
            200,
            &json!({
                "status": license.status(now_secs()).as_str(),
                "licence": public_view(&license),
                "token": token.token,
                "expires_at": token.expires_at,
                "offline_until": token.offline_until
            }),
        ),
        Err(e) => failure(e),
    }
}

fn validate(request: &Request, service: &LicenseService) -> Response {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };

    if let Some(token) = string_of(&payload, "token") {
        return match service.verify_token(&token) {
            Ok(claims) => {
                let matching = string_of(&payload, "hwid")
                    .map(|hwid| hwid == claims.hwid)
                    .unwrap_or(true);
                if matching {
                    Response::json(
                        200,
                        &json!({
                            "status": "valid",
                            "key_prefix": claims.key_prefix,
                            "product": claims.product,
                            "plan": claims.plan,
                            "expires_at": claims.expires_at,
                            "offline_until": claims.offline_until
                        }),
                    )
                } else {
                    failure(LicenseError::InvalidHardware)
                }
            }
            Err(e) => failure(e),
        };
    }

    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.validate(&key, &hwid) {
        Ok(license) => Response::json(
            200,
            &json!({"status": "valid", "licence": public_view(&license)}),
        ),
        Err(e) => failure(e),
    }
}

fn refresh(request: &Request, service: &LicenseService) -> Response {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };
    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.refresh(&key, &hwid) {
        Ok((license, token)) => Response::json(
            200,
            &json!({
                "status": license.status(now_secs()).as_str(),
                "token": token.token,
                "expires_at": token.expires_at,
                "offline_until": token.offline_until
            }),
        ),
        Err(e) => failure(e),
    }
}

fn admin(request: &Request, service: &LicenseService, config: &ApiConfig) -> Response {
    let Some(expected) = config.admin_token.as_deref() else {
        return Response::json(503, &json!({"error": "admin_api_disabled"}));
    };

    if !constant_time_eq(request.bearer().unwrap_or(""), expected) {
        return Response::json(401, &json!({"error": "unauthorized"}));
    }

    if request.is("GET", "/v1/admin/stats") {
        return Response::json(
            200,
            &serde_json::to_value(service.stats()).unwrap_or_default(),
        );
    }

    if request.is("POST", "/v1/admin/licenses") {
        let Some(payload) = request.json() else {
            return failure(LicenseError::InvalidRequest);
        };

        let days = payload.get("days").and_then(Value::as_u64).unwrap_or(0);
        let issue = IssueRequest::new(
            string_of(&payload, "product").unwrap_or_else(|| config.product.clone()),
            string_of(&payload, "plan").unwrap_or_else(|| "standard".to_string()),
        )
        .duration((days > 0).then(|| days * 86_400))
        .machines(
            payload
                .get("machines")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .min(50) as u32,
        )
        .owner(string_of(&payload, "owner_id"))
        .note(string_of(&payload, "note"))
        .issued_by(Some("api".to_string()));

        let issued = service.issue(issue);
        let mut view = admin_view(&issued.license);
        view["key"] = Value::String(issued.key);
        return Response::json(201, &view);
    }

    let Some(rest) = request.path.strip_prefix("/v1/admin/licenses/") else {
        return Response::not_found();
    };
    let (key, action) = match rest.split_once('/') {
        Some((key, action)) => (key, action),
        None => (rest, ""),
    };

    let outcome = match (request.method.as_str(), action) {
        ("GET", "") => service.resolve(key),
        ("POST", "revoke") => service.revoke(
            key,
            request.json().as_ref().and_then(|p| string_of(p, "reason")),
        ),
        ("POST", "restore") => service.restore(key),
        ("POST", "reset-hwid") => service.reset_hardware(key),
        ("POST", "assign") => service.assign(
            key,
            request
                .json()
                .as_ref()
                .and_then(|p| string_of(p, "owner_id")),
        ),
        _ => return Response::not_found(),
    };

    match outcome {
        Ok(license) => Response::json(200, &admin_view(&license)),
        Err(e) => failure(e),
    }
}

fn failure(error: LicenseError) -> Response {
    Response::json(
        error.http_status(),
        &json!({"error": error.code(), "message": error.message()}),
    )
}

fn public_view(license: &License) -> Value {
    json!({
        "key_prefix": license.key_prefix,
        "product": license.product,
        "plan": license.plan,
        "lifetime": license.is_lifetime(),
        "expires_at": license.expires_at,
        "machines_used": license.activations.len(),
        "machines_allowed": license.max_activations
    })
}

fn admin_view(license: &License) -> Value {
    let now = now_secs();
    json!({
        "key_prefix": license.key_prefix,
        "product": license.product,
        "plan": license.plan,
        "status": license.status(now).as_str(),
        "owner_id": license.owner_id,
        "note": license.note,
        "created_at": license.created_at,
        "duration_secs": license.duration_secs,
        "expires_at": license.expires_at,
        "machines_allowed": license.max_activations,
        "activations": license
            .activations
            .iter()
            .map(|activation| {
                json!({
                    "hwid": activation.hwid,
                    "first_seen": activation.first_seen,
                    "last_seen": activation.last_seen,
                    "checks": activation.checks
                })
            })
            .collect::<Vec<_>>(),
        "revoked": license.revoked,
        "revoked_reason": license.revoked_reason,
        "issued_by": license.issued_by,
        "last_check": license.last_check
    })
}

fn string_of(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}
