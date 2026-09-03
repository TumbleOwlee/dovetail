# Stack: Node / TypeScript

Slot values, Node ecosystem. Each fenced block = one slot; heading names the placeholder it fills.

**Ask about:** package manager (`pnpm` — assumed below — vs `npm` vs `bun`); test runner (`vitest` — assumed — vs `jest` vs `node:test`); whether the package ships ESM only.

> CI block's GitHub Actions `${{ … }}` expressions are NOT template placeholders — copy verbatim.

## `{{STACK_NAME}}`

TypeScript on Node 22+ (`pnpm`, `tsc`, `eslint`, `prettier`, `vitest`)

## `{{FULL_COMMANDS}}`

```sh
pnpm prettier --check .
pnpm eslint .
pnpm tsc --noEmit
pnpm vitest run
pnpm vitest run --coverage.enabled --coverage.thresholds.lines={{COVERAGE_FLOOR}}
```

## `{{NARROW_COMMANDS}}`

```sh
pnpm vitest run src/frame.test.ts        # one file
pnpm vitest run -t "checksum"            # one test by name
pnpm vitest                              # watch mode
pnpm vitest run --coverage.enabled --coverage.reporter=html   # browsable coverage
```

## `{{UNIT_TEST_CONVENTION}}`

`<module>.test.ts` beside the module under test, test names prefixed `ut:`.

## `{{INTEGRATION_TEST_CONVENTION}}`

`tests/`, files named `*.it.test.ts`, test names prefixed `it:`.

## `{{ID_CITATION_EXAMPLE}}`

`// FR-R-012 — …` directly above the `it(...)` call

## `{{ID_CITATION_BLOCK}}`

```ts
// FR-R-012 — The checksum is computed over the full frame excluding the checksum field.
it("ut: checksum excludes trailer", () => {
  /* … */
});
```

## `{{SETUP_STEPS}}`

Install Node 22+ and [pnpm](https://pnpm.io/), then:

```sh
pnpm install --frozen-lockfile
```

## `{{STACK_CONVENTIONS}}`

- `strict: true` in `tsconfig.json`, plus `noUncheckedIndexedAccess`. `any` is a defect; use `unknown` and narrow.
- No default exports — named exports only, so renames are mechanical and the public surface is greppable.
- Errors are classes extending one package-level base `Error` with a discriminant `code`; never a thrown string, never a bare `Error("...")`.
- Domain values are branded types wherever mixing two would be a bug, not bare `string`/`number`.
- Validate external input at the boundary with a schema (zod or equivalent); the parsed type crosses into the core.
- Public API = what `src/index.ts` exports; everything else is internal and free to change.

## `ci` → `.github/workflows/check.yml`

```yaml
name: check

on:
  push:
  pull_request:

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm prettier --check .
      - run: pnpm eslint .

  types:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm tsc --noEmit

  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node-version: [22, 24]
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node-version }}
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm vitest run

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm vitest run --coverage.enabled --coverage.thresholds.lines={{COVERAGE_FLOOR}}
```

## `bitbucket-pipelines` → `bitbucket-pipelines.yml`

```yaml
image: node:22

pipelines:
  default:
    - step:
        name: lint
        script:
          - corepack enable
          - pnpm install --frozen-lockfile
          - pnpm prettier --check .
          - pnpm eslint .
    - step:
        name: types
        script:
          - corepack enable
          - pnpm install --frozen-lockfile
          - pnpm tsc --noEmit
    - step:
        name: test
        script:
          - corepack enable
          - pnpm install --frozen-lockfile
          - pnpm vitest run
    - step:
        name: coverage
        script:
          - corepack enable
          - pnpm install --frozen-lockfile
          - pnpm vitest run --coverage.enabled --coverage.thresholds.lines={{COVERAGE_FLOOR}}
```

## `lefthook` → `.lefthook.yml`

```yaml
pre-commit:
  piped: true
  commands:

    format:
      glob: "*.{ts,tsx,js,jsx,json,md}"
      run: pnpm prettier --check {staged_files}
      fail_text: |
        prettier found formatting issues.
        Run `pnpm prettier --write .` to fix them, then re-stage your changes.

    lint:
      glob: "*.{ts,tsx,js,jsx}"
      run: pnpm eslint {staged_files}
      fail_text: |
        eslint found issues.
        Fix the findings above, then re-stage your changes.

    types:
      glob: "*.{ts,tsx}"
      run: pnpm tsc --noEmit
      fail_text: |
        tsc found type errors. Fix them, then re-stage your changes.

    # Warning only (never blocks the commit): this project is spec-driven, so a
    # change to src/ usually comes with a docs/specs/ update in the same commit.
    spec-reminder:
      run: |
        staged="$(git diff --cached --name-only)"
        code="$(printf '%s\n' "$staged" | grep -E '^src/' || true)"
        spec="$(printf '%s\n' "$staged" | grep -E '^docs/specs/' || true)"
        if [ -n "$code" ] && [ -z "$spec" ]; then
          echo "note: product code changed but no docs/specs/ file is staged."
          echo "      If this commit changes behavior, update the area's requirements.md too"
          echo "      (see AGENTS.md 'Spec-driven'). This is a reminder, not a failure."
        fi
        exit 0

    # Warning only: every test that pins observable behavior cites its requirement ID
    # in a comment directly above the it(...) call (see AGENTS.md).
    test-id-reminder:
      run: |
        staged="$(git diff --cached --name-only | grep -E '\.test\.tsx?$' || true)"
        [ -z "$staged" ] && exit 0
        if ! git diff --cached -U1 -- $staged | grep -qE '^\+\s*//\s*({{ID_PREFIX_ALTERNATION}})-R-[0-9]+'; then
          echo "note: tests changed but no requirement ID appears in an added comment."
          echo "      Tests pinning observable behavior cite their requirement (see AGENTS.md)."
          echo "      Reminder, not a failure."
        fi
        exit 0
```

## `{{GAUNTLET_STEPS}}` → `.claude/scripts/gauntlet.sh`

Step name, timeout (s), command — one `run` line each. Coverage step present only with a floor.

```sh
run fmt   600  pnpm prettier --check .
run lint  900  pnpm eslint .
run types 900  pnpm tsc --noEmit
run test  1800 pnpm vitest run
run cov   1800 pnpm vitest run --coverage.enabled --coverage.thresholds.lines={{COVERAGE_FLOOR}}
```

## `{{COVERAGE_EXTRACT}}`

```sh
cov=$(grep -E '^All files' "$log" | tail -1 | awk -F'|' '{gsub(/ /,"",$5); print $5 "%"}')
```
