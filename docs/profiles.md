# Dock profiles

The profile store is separate from app settings and from the browser prototype.
It stores inert action data. Loading, editing or importing a layout never runs an
action, opens a device, reads an account or starts another app.

## Core interface

`core::profiles::ProfileStore::new(base)` takes the UC home directory. The
constructor does no disk I/O. Keep one store in the app for each home directory;
its mutex serializes all readers and writers in that process.

- `load()` and `state()` return `Result<ProfileState, ProfileError>`. The first
  load reads disk, creates an empty default layout only for a new store, and
  establishes the startup backup. Later reads return the last successful state.
- `save(&ProfileState)` validates and persists the entire state and returns the
  saved state. Call it when changes occur; there is no separate Save step.
- `state_with_revision()` returns state and an opaque in-process revision atomically. `save_if_revision()` validates and
  persists only if that revision still matches, under the same store mutex. Every successful write advances the token;
  restarting the app creates a new epoch. The token is separate from the on-disk schema version.
- Native UI `profiles_save` requires `expectedRevision`. Agent clients use typed batches through the running host with
  the same revision checks and an editor lease; they never replace profile files directly. [MCP contract](agent-control.md).
- `recovery_notice()` returns a short message if loading used the recovery
  journal or startup backup. Show this message to the user.
- `import_preview(&serde_json::Value)` converts the original index-based
  prototype draft. It assigns stable IDs and converts page-action indices to
  those IDs. It does not write anything until the caller saves the result.
- `Page::new(name)`, `Profile::new(name)`, `Key::empty()` and
  `ProfileState::default()` create valid empty structures with new IDs.

The state serializes as:

```text
{ version: 1, currentProfile: <profile ID>, currentPage: <page ID>,
  profiles: [{ id, name, pages: [{ id, name, keys: [
    { a: {kind,text}, b: {kind,text}, action, value }, ... exactly 14
  ] }] }] }
```

Indices 0–12 are the square keys; index 13 is the wide display. Each key always
has two faces; use the same face twice for a fixed display. `currentPage` must
belong to `currentProfile`. Page/folder action values are stable page IDs within
that profile. A `back` action has no target; the future runtime will own its
navigation stack. Active A/B states and navigation history are runtime data and
are not stored here.

Face kinds: `text`, `icon`, `clock`, `claude`, `codex`, `reset`, `trend`, `cpu`,
`gpu`, `ram`. Action kinds: `none`, `hotkey`, `media`, `launch`, `url`, `text`,
`page`, `monitor`, `folder`, `back`. There is no script action. An allowed action
name does not establish that its execution adapter has been built.

## Limits and recovery

IDs contain at most 64 ASCII letters, digits, underscores or hyphens and cannot
contain path separators or dots. The store derives its own filenames from these
validated IDs. Names are nonempty, printable and at most 80 characters. Face
text is at most 40 printable characters; action values are at most 4096 UTF-8
bytes, with no null character. A layout has at most 50 profiles, 50 pages per
profile and 200 pages in total, with an 8 MiB total encoded-state limit. Reads
are bounded, including the total bytes read across the indexed profile files.

Files under `<base>/profiles/`:

| File | Contents |
| --- | --- |
| `index.json` | Version, ordered profile IDs and active profile/page IDs |
| `profile-<id>.json` | One profile, including all of its pages |
| `startup-backup.json` | A complete validated snapshot from the first load this session |
| `transaction.json` | A complete pending save, retained only if a save is interrupted |

Each file is written to a unique sibling temporary file, synced, then renamed
atomically using the existing app helper. The recovery journal is committed
first; canonical profiles and index follow. Unreferenced managed profile files
are removed after the new index is written. The journal is removed last. A crash
between files can therefore recover the whole pending snapshot. An interrupted
save can appear after restart even if its caller did not receive success.

The first successful load backs up validated current data. A damaged current
profile/index falls back to the startup backup without replacing that backup.
An invalid journal also falls back to the startup backup because current files
could be from different stages of the interrupted save. Recovery does not
silently reset the layout. If neither source validates, loading and saving fail
with an error and the existing files are kept. A later successful edit repairs
the canonical files through the same journaled write path.

The store does not provide coordination between independent processes sharing
one home directory; UC's single-instance ownership must remain in force. This is
not an import/export archive format, a device reader or an action executor.
