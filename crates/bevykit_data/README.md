# bevykit_data

Data infrastructure for Bevy games:

- **Storage**: atomic file replacement with backups, browser `localStorage`, or a custom
  backend. Writes are ordered so older requests never overwrite newer data.
- **Settings**: typed, sanitized, persisted settings with preview, commit, and cancel.
- **Saves**: versioned save slots with migrations, backup recovery, and `SaveId` references.
