# bevykit_data

Data infrastructure for Bevy games:

- **Assets**: typed collections and loading groups with progress, failures, and retry.
- **Storage**: atomic file replacement with backups, browser `localStorage`, or a custom
  backend. Writes are ordered so older requests never overwrite newer data.
- **Settings**: typed, sanitized, persisted settings with preview, commit, and cancel.
- **Saves**: versioned save slots with migrations, backup recovery, and `SaveId` references.
- **Localization**: TOML translations with plurals, interpolation, fallbacks, and validation.
