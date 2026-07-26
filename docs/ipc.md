# Tauri IPC Command Layer

ShenDesk uses Tauri commands as a thin transport boundary between React and Rust.

## Registered commands

| Command | Input | Output |
| --- | --- | --- |
| `health_check` | none | `HealthStatus` |
| `get_config` | none | `AppConfig` |
| `save_config` | `{ config: AppConfig }` | normalized `AppConfig` |
| `reset_config` | none | default `AppConfig` |

## Layering rule

```text
React service
  -> Tauri Command
  -> Application Service
  -> Domain
  -> Infrastructure
```

Commands may deserialize transport input, access Tauri managed state, and map errors. They must not contain business rules or issue SQL directly.

## Error contract

Fallible commands return a serializable payload:

```json
{
  "code": "internal_error",
  "message": "Human-readable diagnostic message"
}
```

The React client converts this payload to `ShenDeskIpcError`.

## Frontend usage

```ts
const config = await getConfig();
const saved = await saveConfig({ ...config, theme: "light" });
```

All raw calls to `invoke` are centralized in `src/services/tauri.ts`, keeping page components independent from Tauri transport details.
