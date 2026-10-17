//! `thumb://` protocol: `<id>.grid` or `<id>.preview` → cached JPEG.
//! Only ids from the index are accepted; no file path ever comes from the UI.

use kadr_core::services::thumbnail::{ThumbError, ThumbSize};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::app_state::AppState;

fn reply(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        // The URL carries the file's mtime, so a cached image is never stale.
        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn parse(path: &str) -> Option<(i64, ThumbSize)> {
    let (id, size) = path.trim_start_matches('/').split_once('.')?;
    Some((id.parse().ok()?, ThumbSize::parse(size)?))
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let response = (|| {
            let (id, size) = parse(&path)
                .ok_or((StatusCode::BAD_REQUEST, "bad thumbnail address".to_owned()))?;
            let source = state
                .db()
                .ok()
                .and_then(|db| db.thumb_source(id).ok().flatten())
                .ok_or((StatusCode::NOT_FOUND, "not in the index".to_owned()))?;
            let thumbnailer = state
                .thumbnailer()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.code.to_owned()))?;
            let file = thumbnailer.get(&source, size).map_err(|e| match e {
                ThumbError::Unsupported => (StatusCode::UNSUPPORTED_MEDIA_TYPE, e.to_string()),
                ThumbError::Failed(m) => (StatusCode::UNPROCESSABLE_ENTITY, m),
            })?;
            std::fs::read(file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        })();
        responder.respond(match response {
            Ok(bytes) => reply(StatusCode::OK, "image/jpeg", bytes),
            Err((status, message)) => {
                reply(status, "text/plain; charset=utf-8", message.into_bytes())
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_known_addresses() {
        assert_eq!(parse("/42.grid"), Some((42, ThumbSize::Grid)));
        assert_eq!(parse("7.preview"), Some((7, ThumbSize::Preview)));
        assert_eq!(parse("/../etc/passwd"), None);
        assert_eq!(parse("/42.huge"), None);
        assert_eq!(parse("/x.grid"), None);
    }
}
