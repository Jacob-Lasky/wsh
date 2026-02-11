# Overlays

Overlays are positioned text elements rendered on top of terminal content.
They are useful for status bars, notifications, debug info, and agent-driven
UI elements that shouldn't interfere with the terminal's own output.

## Concepts

An overlay has:

- **Position** (`x`, `y`): Column and row on the terminal grid (0-based)
- **Z-order** (`z`): Stacking order when overlays overlap (higher = on top)
- **Spans**: One or more styled text segments
- **ID**: A unique identifier assigned on creation

Overlays exist independently of terminal content. They persist across screen
updates and are not affected by scrolling or screen clearing.

## Create an Overlay

```
POST /overlay
Content-Type: application/json
```

**Request body:**

```json
{
  "x": 10,
  "y": 0,
  "z": 100,
  "spans": [
    {"text": "Status: ", "bold": true},
    {"text": "OK", "fg": "green"}
  ]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `x` | integer | yes | Column position (0-based) |
| `y` | integer | yes | Row position (0-based) |
| `z` | integer | no | Z-order (default: 0) |
| `spans` | array | yes | Styled text spans |

**Response:** `201 Created`

```json
{"id": "f47ac10b-58cc-4372-a567-0e02b2c3d479"}
```

**Example:**

```bash
curl -X POST http://localhost:8080/overlay \
  -H 'Content-Type: application/json' \
  -d '{"x": 10, "y": 0, "z": 100, "spans": [{"text": "Status: ", "bold": true}, {"text": "OK", "fg": "green"}]}'
```

## List Overlays

```
GET /overlay
```

**Response:** `200 OK`

```json
[
  {
    "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    "x": 10,
    "y": 0,
    "z": 100,
    "spans": [
      {"text": "Status: ", "bold": true},
      {"text": "OK", "fg": "green"}
    ]
  }
]
```

**Example:**

```bash
curl http://localhost:8080/overlay
```

## Get a Single Overlay

```
GET /overlay/:id
```

**Response:** `200 OK` with the overlay object.

**Error:** `404` with code `overlay_not_found` if the ID doesn't exist.

**Example:**

```bash
curl http://localhost:8080/overlay/f47ac10b-58cc-4372-a567-0e02b2c3d479
```

## Update Overlay Spans

```
PUT /overlay/:id
Content-Type: application/json
```

Replaces the overlay's spans while keeping its position and z-order.

**Request body:**

```json
{
  "spans": [
    {"text": "Status: ", "bold": true},
    {"text": "Error", "fg": "red"}
  ]
}
```

**Response:** `204 No Content`

**Error:** `404` with code `overlay_not_found` if the ID doesn't exist.

**Example:**

```bash
curl -X PUT http://localhost:8080/overlay/f47ac10b-58cc-4372-a567-0e02b2c3d479 \
  -H 'Content-Type: application/json' \
  -d '{"spans": [{"text": "Status: ", "bold": true}, {"text": "Error", "fg": "red"}]}'
```

## Move or Reorder an Overlay

```
PATCH /overlay/:id
Content-Type: application/json
```

Updates position and/or z-order without changing spans. All fields are
optional -- only provided fields are updated.

**Request body:**

```json
{
  "x": 20,
  "y": 5,
  "z": 200
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `x` | integer | no | New column position |
| `y` | integer | no | New row position |
| `z` | integer | no | New z-order |

**Response:** `204 No Content`

**Error:** `404` with code `overlay_not_found` if the ID doesn't exist.

**Example:**

```bash
curl -X PATCH http://localhost:8080/overlay/f47ac10b-58cc-4372-a567-0e02b2c3d479 \
  -H 'Content-Type: application/json' \
  -d '{"x": 20, "y": 5, "z": 200}'
```

## Delete an Overlay

```
DELETE /overlay/:id
```

**Response:** `204 No Content`

**Error:** `404` with code `overlay_not_found` if the ID doesn't exist.

**Example:**

```bash
curl -X DELETE http://localhost:8080/overlay/f47ac10b-58cc-4372-a567-0e02b2c3d479
```

## Clear All Overlays

```
DELETE /overlay
```

Removes every overlay.

**Response:** `204 No Content`

**Example:**

```bash
curl -X DELETE http://localhost:8080/overlay
```

## Overlay Spans

Each span in an overlay's `spans` array is a styled text segment:

```json
{
  "text": "Hello",
  "fg": "red",
  "bg": {"r": 0, "g": 0, "b": 0},
  "bold": true,
  "italic": false,
  "underline": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `text` | string | yes | The text content |
| `fg` | OverlayColor | no | Foreground color |
| `bg` | OverlayColor | no | Background color |
| `bold` | boolean | no | Bold (default: false) |
| `italic` | boolean | no | Italic (default: false) |
| `underline` | boolean | no | Underline (default: false) |

Boolean style fields default to `false` and are omitted from responses when
`false`.

### Overlay Colors

Overlay colors are either a named color string or an RGB object:

**Named colors:**

```json
"red"
"green"
"blue"
"yellow"
"cyan"
"magenta"
"black"
"white"
```

**RGB:**

```json
{"r": 255, "g": 128, "b": 0}
```

Note: Overlay colors differ from terminal span colors. Terminal spans use
`{"indexed": N}` or `{"rgb": {"r": N, "g": N, "b": N}}`. Overlay colors
use named strings or flat `{"r": N, "g": N, "b": N}` objects.

## Example: Agent Status Bar

```bash
# Create a status overlay at the top-right
curl -X POST http://localhost:8080/overlay \
  -H 'Content-Type: application/json' \
  -d '{
    "x": 60, "y": 0, "z": 100,
    "spans": [
      {"text": " Agent: ", "bg": "blue", "bold": true},
      {"text": "watching ", "bg": "blue"},
      {"text": "\u2713", "fg": "green", "bg": "blue"}
    ]
  }'
# {"id":"abc123"}

# Update the status
curl -X PUT http://localhost:8080/overlay/abc123 \
  -H 'Content-Type: application/json' \
  -d '{
    "spans": [
      {"text": " Agent: ", "bg": "red", "bold": true},
      {"text": "action needed ", "bg": "red"}
    ]
  }'

# Clean up when done
curl -X DELETE http://localhost:8080/overlay/abc123
```
