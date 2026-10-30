---
title: "ADR-0031 — Where annotations live: Sophia annotations in store-truth folders, the folder's jsonl elsewhere"
tags: [adr, annotations, sophia, storage]
status: proposed
date: 2026-10-30
---
**Status:** proposed 2026-10-30 (Claude, with the Sophia session's review of the storage question). Not yet accepted; Daniel decides. Related: [[Annotations]], [[North Star]] §5 and §7, [[ADR-0030 Where the truth lives]], [[ADR-0029 Sophia stores the code graph]], [[Shared Work]] (comments, SW-30 to SW-34), Sophia SPEC-001 §8, SPEC-004, SPEC-009, SPEC-010 and SPEC-006 §5.

## Context

WP-19 (an annotations layer, including proofs) and WP-50 (comments) need a place to store what people, agents and tools **claim** about the graph:
- comments and review threads;
- documentation;
- type-system results (North Star §5);
- proofs, meaning equivalence, implication, memory safety, safe concurrency and so on (North Star §7), each with its checker, the versions checked and a status.

[[Annotations]] (2026-09-20) predates Sophia. It put everything in `.moonkale/annotations.jsonl`, anchored to a node plus a text range and re-mapped through the entity log. Since then:
- [[ADR-0030 Where the truth lives]] put the truth in Sophia for everything but legacy folders.
- Legacy folders keep their files as the truth, with a derived Sophia cache (ADR-0029, SPEC-009).
- Store-truth folders (SPEC-010) make the store the truth.

Sophia already has two record types (SPEC-001 §8, `sophia-store` `types.rs`):
- **A fact** is what a tool *computed*, reproducibly. It has one subject, `by`/`by_version` and inputs. It is meant to be dropped and recomputed, and it follows its file in a derived store.
- **An annotation** is what a person, agent or tool *claims*:
  - `kind` (doc, comment, equivalence, `proves:*`, …);
  - `subjects: Vec<Digest>`, all of them hashed into the id;
  - `payload`, `checker`/`checker_version`, `status` (asserted, checked, failed, stale) and `inputs`.

  It has `Store::annotate`/`annotate_many` and `annotations_of`, and SPEC-004 draws it as `NodeKind::Annotation` with `Annotates` edges.

A derived store is a deletable cache. Anything that lives only in it is lost on a rebuild, and folder collection removes the old record versions an annotation points at.

## Decision

1. **Comments, documentation, type results and proofs are Sophia annotations, never facts.** They are claims. Tool output that is recomputed (the index's call edges, imports) stays as facts.
2. **Where the truth of an annotation lives depends on the folder's truth:**
   - **Store-truth folders (SPEC-010):** the annotation lives in Sophia. Nothing is collected there, so the versions an annotation was checked against stay resolvable.
   - **Legacy (file-truth) folders and derived caches:** `.moonkale/annotations.jsonl` stays the truth (append-only, merged by id, as in [[Annotations]]). When the cache is built or synced, the jsonl is loaded into it as Sophia annotations, so queries, the graph view and agents see one model in both cases. A cache rebuild loses nothing, because it reloads from the jsonl.
   - **Non-folder sources** (a database): annotations live in the project, keyed by the source id, as [[Annotations]] says.
3. **Subjects are versions.** An annotation's subjects are the immutable digests it is about, so "the version checked" is the subject itself, with no separate version field. A proof about several terms (an equivalence) is one annotation over all of them. A proof about a generic function also records the relation version, as facts do with `Input::Relation`.
4. **Staleness:**
   - A proof (or a type result) is **stale** when the name or entity it is about now resolves to a different digest than its subject. Sophia's names and `supersedes` lineage answer that for store-truth folders, and the entity log does for legacy folders.
   - **Comments follow the definition forward.** They are re-pointed to the new version with the original quote kept, and marked *drifted* if the text they quoted is gone.
   - **Proofs don't follow; they go stale** and stay attached to the version they proved, visible as a goal. This is SPEC-001 §4.5's rule for migrations.
5. **Ranges:** until Sophia has term-path subjects (SPEC-002 §8, planned), a range inside a definition is kept in the payload and re-mapped by Moonkale. Where a term path can be had it is preferred over byte offsets, because in store-truth folders the text is printed by Sophia and byte offsets move when the printer changes.
6. **Moving the truth (SPEC-010 `give_truth`):** when a store-truth folder becomes file truth again, its annotations are first written to `.moonkale/annotations.jsonl`, in the same confirmation the person already gives. `take_truth` loads the jsonl in. This needs a hook in Sophia (below).

## Consequences

- **Moonkale builds:**
  - the jsonl ↔ Sophia loading for derived and legacy folders;
  - the anchor model (subject digests plus an optional payload range or term path);
  - staleness through names and lineage (or the entity log);
  - the UI from [[Annotations]]: gutters, the panel, graph badges and a proof-status view (North Star §7).

  Proof payloads follow SPEC-006 §5 (checker, kernel, assumptions, certificate).
- **Sophia gaps to close before or with WP-19** (none scheduled yet; the Sophia session raises them with Daniel):
  - removing an annotation (deleting a comment);
  - updating an annotation's status (status is not in the id, but there is no API);
  - an annotation staleness check (`Store::is_stale` covers facts only);
  - term-path subjects;
  - a `give_truth`/`take_truth` hook for annotations.
- **[[Annotations]]** keeps its UI plan. Its storage section is superseded by this ADR once accepted.
- **Not decided here:**
  - whether resolved threads are compacted out of the jsonl;
  - GitHub/GitLab issue import (still [[Annotations]] step 4).
