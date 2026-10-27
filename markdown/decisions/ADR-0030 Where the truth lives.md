---
title: "ADR-0030 — Where the truth lives: legacy folders keep files, everything else is Sophia"
tags: [adr, architecture, code-graph, sophia]
status: accepted
date: 2026-10-26
---
**Status:** accepted 2026-10-26 by Daniel (the decision below, in his words). The points under "How it applies" are Claude's reading of it; Daniel may correct them. Refines [[ADR-0028 Files or the graph]] (the `truth: files | store` field) and [[ADR-0029 Sophia stores the code graph]] (the derived folder store). Related: [[North Star]].

## Context

ADR-0029 made Sophia the code graph's store with files as the truth, and set four gates before the derived folder store could become the default. All four were met on 2026-10-26 (see ADR-0029). Asked whether the folder store should now become the default, Daniel answered with a broader rule about which side is the truth.

## Decision (Daniel, 2026-10-26)

> If there are legacy data where working on a folder is expected, then the folder should be the source of truth, except if the user explicitly moves the source of truth into Sophia.
> In all other cases assume Sophia store to be the source of truth, even if there are files generated from the store.

## How it applies

1. **Legacy folders: files are the truth.** A folder whose code already exists as files that other tools read: a git repository, a Cargo, Julia, Python or other project, or any folder opened with Open Folder that isn't a Sophia project. It is `truth: files`. The Sophia store in `.moonkale/code.sophia` stays a derived cache (ADR-0029, SPEC-009), and deleting it loses nothing.
2. **Moving the truth is explicit.** Only a deliberate user action makes a legacy folder store-backed: a command such as "Make Sophia the source of truth", with a confirmation that says what changes. It ingests the files, switches the folder to `truth: store`, and records that in the folder. It is never automatic, never triggered by an agent on its own, and can be reversed. Reversing exports the files, makes them the truth again, and turns the store back into a derived cache.
3. **Everything else: the Sophia store is the truth.** A new project or new code created in Moonkale, code created by an agent in Moonkale, a `.sophia` file, a folder already moved: all are `truth: store`.
4. **Generated files don't change that.** A store-backed project may print files for compilers, language servers, CI or git (Sophia prints in the chosen profile). Those files are outputs. Their header or a manifest marks them as generated, and they are rewritten from the store. If one is edited outside Moonkale, the edit is detected and offered as a change to the store, which the user accepts or rejects. It is never silently overwritten, and never silently taken as the truth.
5. **One folder, one truth.** A folder is either files-truth or store-truth as a whole. Mixing within a folder (some definitions in files, some in the store) is out of scope until it's needed.

## Consequences

- **Detection.** The workspace decides a folder's mode when it opens it: a store-truth marker in `.moonkale/` means store, anything else that has files means legacy files-truth. A new project starts store-backed. The source descriptor's `truth` field (ADR-0028) carries the result.
- **ADR-0029's default question splits in two.** For new work, the store is the truth, so the store isn't optional there. For legacy folders, Daniel decided (2026-10-26) to keep both, with the Sophia cache as the default:
  - the code graph reads the derived folder store; search stays on the in-memory index (Sophia has no search dialect);
  - if the store can't open (a browser without a server, a broken file), Moonkale falls back to the in-memory index on its own and says so in the status bar; a setting forces the in-memory index;
  - a "Check the cache" command builds a fresh store from the files and compares it with the cache (the replay's full-digest check); a mismatch rebuilds the cache and is reported as a problem;
  - the cache can be exported in the same text form as a store-truth folder (SPEC-010 Q5), optional and not committed by default, since the files stay the truth. Uses: handing over a built cache of a large repo, and the starting point for moving the truth into Sophia.
- **Legacy default implementation (2026-10-26):** retain the `sophia` feature, default-on for desktop/mobile and implied by the server feature; browser-only builds stay free of the native writer and read through their server.
- **Sharing a store-truth folder** (Daniel, 2026-10-26): a committed text export of the store, rebuilt into SQLite on open. Later perhaps a pointer file type that only says which information to request from the store.
- **New work:**
  - TODO: export the derived cache after SPEC-010 defines its text form;
  - the "move the truth into Sophia" command and its reverse;
  - a store-truth project template ("New project");
  - generated-file output: a marker, regeneration, and detecting outside edits as proposed store changes;
  - the mode in the status bar and the explorer, so the user always sees which side is the truth (ADR-0027: simple by default).
- **Sophia's side.** `truth: store` folders use Sophia's normal, non-derived mode, where the graph is the version history (no garbage collection, SR-146). Generated files need Sophia to print a whole project to files and to take an outside edit back in as a patch. Sophia's session is told.
- **Kept:** ADR-0028's patch model for file-backed code, ADR-0029's derived store and its gates, and the flow editor's diagram-as-document (ADR-0020 to ADR-0022), which is already store-like.

**Sophia's answer (2026-10-26).**
- **Moving the truth:** switching a store between derived and normal mode can happen in place, in one transaction each way. It keeps each file's text and the order of its definitions, which export needs. Sophia doesn't yet have a call that clears derived mode, nor a rule for who owns names after the switch. Claude's proposal for that rule: the name belongs to the store, and its file and span stay as layout for export.
- **Generated files:** these need a new Sophia spec, which goes to Daniel. Moonkale's position, sent to Sophia: Moonkale normalises Julia, Rust and Python code on the write path and patches the edited definition back into the file text with its lossless syntax layer. It hands Sophia the finished records, as `replace_files` does. Sophia then needs three things: a counterpart of `replace_files` for stores that are the truth, guarded by its edit tokens; export markers; and a "propose, don't apply" mode for outside edits.

**SPEC-010 accepted (2026-10-26).** Daniel accepted Sophia's SPEC-010 (Store-Truth Folders and Generated Files): Sophia refuses interface edits of names laid out in files (`LaidOut { path, token }`); a confirmation is enough for `give_truth`, after Moonkale copies the store to `.moonkale/code.sophia.before-give-truth-<date>`; sophialang is printed one file per namespace; generated-file markers live in `.moonkale/generated.json`; a store-truth folder is shared as a committed text export (Sophia SR-149). Not implemented yet. Before store-truth folders land, the cache repair (`FolderStore::rebuild`) must refuse to delete an unreadable store in a `truth: store` folder: today it treats any unreadable database at the cache path as disposable.
