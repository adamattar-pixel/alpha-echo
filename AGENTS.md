# Repository instructions

## Feature documentation

Keep the root `audit.md` and `plan.md` current whenever an app feature is added,
removed, or its user-visible behavior changes. Update both in the same change as
the implementation, before handing off or committing the work.

- `plan.md` is the living feature inventory and development plan. Add or update
  the feature's scope, status, acceptance criteria, and relevant dependencies.
  Mark removed features as removed rather than silently deleting their history.
- `audit.md` is the chronological change record. Append a dated entry describing
  the addition, removal, or behavior change, why it was made, the affected feature
  IDs and files, and the actual validation results or known limitations.
- Use the same feature IDs in both documents and use Asia/Hong_Kong dates.
- Do not mark planned work as implemented, invent validation results, or rewrite
  previous audit entries to conceal later changes. Corrections get a new entry.
- Pure refactoring, formatting, or infrastructure maintenance without a feature
  behavior change does not require a feature entry. Document fixes that change
  user-visible feature behavior.

Read both documents before feature work. These files record development history;
they are not substitutes for the application's immutable report and trade audit
records. Keep credentials, wallet signing keys, and private user data out of them.
