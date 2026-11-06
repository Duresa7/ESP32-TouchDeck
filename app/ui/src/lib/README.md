# UI packages

Each immediate child folder of `src/lib` is a package. Loose files such as
`css-properties.d.ts` are not packages. Import a package only through its root
files. These files are entry points; every subfolder is private. Use `lib/` for
implementation and `tests/` for tests (Companion keeps its fake implementation
in `fake/`). No barrel files that re-export a whole subtree: prefer several
small entry points with explicit interfaces.

Copy this shape for a new package:

```text
src/lib/example/
  client.ts          # public entry point
  model.ts           # another small entry point
  lib/
    transport.ts     # private implementation
  tests/
    client.test.ts   # imports ../client.ts
    fixtures.ts      # shared only by this package's tests
```

## Rules

**Entry-point boundary.** App code and other packages may import a package's
root files, never its subfolder internals.

**Intra-package freedom.** A package's implementation and entry points may
import each other freely. Tests and their fixtures remain private to tests.

**Tests through entry points.** Files under a package's `tests/` may import
any package's entry points and their own test fixtures, but no package's
implementation internals, including their own.

**No dependency cycles.** Dependency cycles are errors throughout the scanned UI.

Run `pnpm --filter touchdeck-ui lint:boundaries` from `app/`, or
`pnpm lint:boundaries` from `app/ui/`. The app's `pnpm lint` also runs this
check. `.dependency-cruiser.cjs` enforces these rules at error severity;
package names are captured dynamically, so new packages need no rule changes.
