# Validate the app revision before running checked-out code

The app checks resolve their checkout once in a separate `source` job. That job
runs only pinned checkout and inline Git commands, checks every explicit ref is
an ancestor of freshly fetched `origin/main`, and emits the full commit ID only
after success. Both the app matrix and the UI e2e shards depend on that job and
check out its immutable `revision` output. Empty refs still select the triggering
commit, including pull request merge commits and manually checked branches.

CodeQL repeatedly raised `actions/cache-poisoning/poisonable-step` when app steps
moved. The app matrix already rejected explicit refs outside main before running
project code, so those findings were false positives; the query does not model
that inline ancestry check. Its source is an input-controlled checkout, its sink
is later execution of checked-out code in the same job, and reusable jobs inherit
the caller's event. A cache action is not required: executed code can access the
runner's cache token even with `contents: read`.

## Considered options

- **Disable caches on releases.** Does not remove the query's execution sink or
  the runner cache token, and slows release checks.
- **Filter the query.** Hides future real findings and would require changing the
  current default CodeQL setup. Keep the analysis enabled.
- **Resolve and validate in a separate job.** Establishes a real execution
  boundary: no project code runs while the caller's ref is unvalidated. Consumers
  receive only a Git-derived commit ID, not the original ref expression.

## Consequences

One short Linux job precedes the existing parallel checks. The test commands,
runner matrix, e2e selection and caches stay the same. Rust caches still save only
on pushes to main, and workflow permissions remain `contents: read`. The e2e
shards now share the matrix's validation and cannot resolve a moving tag to a
different commit from the matrix.

`release-windows.yml` still calls `app.yml` with its selected ref, both operating
systems and all three e2e shards. An explicit release ref must pass the main
ancestry check before either job starts. The release build's separate ancestry
check and the publish job's dependency on app, Firmware and installer checks
remain in place. No release approval or repository setting changes.

Query sources: [execution query](https://github.com/github/codeql/blob/main/actions/ql/src/Security/CWE-349/CachePoisoningViaPoisonableStep.ql)
and [checkout model](https://github.com/github/codeql/blob/main/actions/ql/lib/codeql/actions/security/UntrustedCheckoutQuery.qll).
