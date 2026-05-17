# Security Policy

## Supported Versions
ImageViz is currently in pre-release development (v0.x). Security updates are applied to the latest commit on the `main` branch.

## Security Posture
ImageViz is designed as a **local-only desktop tool**. It binds to `127.0.0.1` (localhost) by default and is not intended for public network exposure. There is no authentication, authorization, or user management.

## Security Headers
All API responses include the following security headers (applied via `tower-http` middleware as the outermost layer):

| Header | Value | Purpose |
|--------|-------|---------|
| `X-Content-Type-Options` | `nosniff` | Prevents MIME type sniffing |
| `X-Frame-Options` | `SAMEORIGIN` | Prevents clickjacking |
| `X-XSS-Protection` | `0` | Disables legacy XSS filter (modern browsers use CSP instead) |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Controls referrer header leakage |
| `Permissions-Policy` | *(restrictive)* | Disables camera, microphone, geolocation, etc. |
| `Content-Security-Policy` | *(restrictive)* | Limits script/style sources to same-origin |

## Input Validation
All API endpoints validate input at the boundary:

| Endpoint | Validates |
|----------|-----------|
| `GET /api/v1/media` | `limit` [1-500], `cursor` (ISO 8601/NaiveDateTime), `cursor_id` (UUID v4) |
| `GET /api/v1/media/{id}` | `id` (max 128 chars) |
| `GET /api/v1/media/{id}/thumbnail` | `width` [100-500] |
| `GET /api/v1/search` | `q` (max 1000 chars), `limit` [1-500] |
| `PUT /api/v1/config` | `watched_folders` (non-empty, no `..` traversal, max 4096 chars per path) |

## Timeouts
- Default: 60 seconds (configurable via `REQUEST_TIMEOUT_SECS`)
- Media routes: 120 seconds (thumbnail generation is CPU-bound)
- SSE connections: 3600 seconds (long-lived streams)
- Timeouts return HTTP 408

## Known Limitations
- No authentication (local-only tool)
- No transport encryption (HTTP, not HTTPS — localhost only)
- No rate limiting
- Thumbnails are served without access control
- File paths in API responses may leak local filesystem structure

## Reporting a Vulnerability
If you discover a security issue, please open a GitHub issue at:
https://github.com/paulomarciano/imageviz/issues

Do not email or DM — public disclosure is fine for this local-only tool. For significant issues, please mention `[security]` in the issue title.
