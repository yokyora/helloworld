# Hello World

A Windows desktop companion by YOKYORA. The current implementation is a basic interaction slice, not a complete AI companion.

Includes a transparent, borderless, always-on-top window; an interim CSS character; poke/rest interactions; persisted quiet mode; and DeepSeek streaming chat with cancellation, bounded conversation context, and local SQLite history. Windows DPAPI protects the API key. Long-term memory, screen awareness, external actions, tray support, and position persistence are not implemented.

On 2026-10-26 the user selected remote DeepSeek API integration, with credentials configured by the user inside the desktop app. Saving settings makes no API request. Explicit consent and a user-initiated send are required. Chat text is stored locally without encryption; selected conversation content is sent to DeepSeek and may incur API charges. See [Architecture](docs/ARCHITECTURE.md).

See [中文说明](README-zh.md) for the scope, setup, build commands, and verification limitations.

The idle window now shows only the character (180 × 180 logical pixels). Click or right-click the character for shortcuts, then open chat or settings as needed. Drag the character to move it; click the character again or press Escape to collapse. Native shadows remain disabled. Chat drafts survive panel changes; collapsing does not cancel a reply. The 2026-10-27 UI checks did not call or test the model.

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

Windows development requires the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Historical dependency errors from the original template are not a current environment diagnosis.
