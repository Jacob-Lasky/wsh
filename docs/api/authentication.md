# Authentication

wsh uses bearer token authentication to protect terminal access when exposed
on a network.

## When Authentication Is Required

Authentication depends on the bind address:

| Bind address | Auth required | Rationale |
|-------------|---------------|-----------|
| `127.0.0.1` (default) | No | Localhost -- you already have local access |
| `::1` | No | IPv6 loopback |
| Any other address | Yes | Network-accessible -- must authenticate |

When no authentication is required, all endpoints are open. When authentication
is required, every endpoint except `/health`, `/docs`, and `/openapi.yaml`
requires a valid token.

## Token Configuration

### Automatic Token Generation

When binding to a non-loopback address without specifying `--token`, wsh
generates a random 32-character alphanumeric token and prints it to stderr:

```
$ wsh --bind 0.0.0.0:8080
wsh: authentication token: aB3kM9xR2pL7nQ4wT8yF1vJ6hD5gC0eS
```

### Providing Your Own Token

Use the `--token` flag or `WSH_TOKEN` environment variable:

```bash
# Via flag
wsh --bind 0.0.0.0:8080 --token my-secret-token

# Via environment variable
export WSH_TOKEN=my-secret-token
wsh --bind 0.0.0.0:8080
```

The flag takes precedence over the environment variable.

## Sending Credentials

wsh accepts tokens via two mechanisms, checked in this order:

### 1. Authorization Header (Preferred)

```
Authorization: Bearer <token>
```

```bash
curl -H "Authorization: Bearer my-secret-token" http://host:8080/screen
```

### 2. Query Parameter (Convenience)

```
?token=<token>
```

```bash
curl 'http://host:8080/screen?token=my-secret-token'
```

The query parameter is provided as a convenience for contexts where setting
headers is awkward (browser bookmarks, simple scripts, WebSocket URLs). Prefer
the Authorization header when possible -- query parameters may appear in server
logs and browser history.

If both are present, the Authorization header takes precedence.

### WebSocket Authentication

WebSocket connections use the same mechanisms. The token is checked during the
HTTP upgrade request:

```bash
# Via query parameter (most common for WebSockets)
websocat 'ws://host:8080/ws/json?token=my-secret-token'

# Via header (if your client supports it)
websocat -H 'Authorization: Bearer my-secret-token' ws://host:8080/ws/json
```

## Error Responses

| Status | Code | Meaning |
|--------|------|---------|
| `401` | `auth_required` | No token provided |
| `403` | `auth_invalid` | Token provided but incorrect |

**401 example:**

```json
{
  "error": {
    "code": "auth_required",
    "message": "Authentication required. Provide a token via Authorization header or ?token= query parameter."
  }
}
```

**403 example:**

```json
{
  "error": {
    "code": "auth_invalid",
    "message": "Invalid authentication token."
  }
}
```

## Security Notes

- wsh provides **authentication**, not **encryption**. For remote access over
  untrusted networks, use SSH tunneling, Tailscale/WireGuard, or a reverse
  proxy with TLS.
- Tokens are compared in constant time to prevent timing attacks.
- `/health`, `/docs`, and `/openapi.yaml` are always unauthenticated so
  monitoring tools and documentation browsers work without credentials.
