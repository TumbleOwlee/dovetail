# Stack: Go

Slot values, Go ecosystem. Each fenced block = one slot; heading names the placeholder it fills.

**Ask about:** whether `golangci-lint` is wanted on top of `go vet`; whether the race detector runs in CI only (recommended) or also locally.

> CI block's GitHub Actions `${{ … }}` expressions are NOT template placeholders — copy verbatim.

## `{{STACK_NAME}}`

Go 1.23+ (`go test`, `gofmt`, `go vet`, `golangci-lint`)

## `{{FULL_COMMANDS}}`

```sh
test -z "$(gofmt -l .)"
go vet ./...
golangci-lint run
go build ./...
go test -race ./...
go test -coverprofile=cover.out ./... && go tool cover -func=cover.out | tail -1
```

## `{{NARROW_COMMANDS}}`

```sh
go test ./internal/frame -run TestUtChecksum   # one test
go test ./internal/frame                       # one package
go build ./cmd/...                             # one command tree
go tool cover -html=cover.out                  # browsable coverage
```

## `{{UNIT_TEST_CONVENTION}}`

`<file>_test.go` in the package under test, functions named `TestUt*`.

## `{{INTEGRATION_TEST_CONVENTION}}`

`<file>_integration_test.go` behind `//go:build integration`, functions named `TestIt*`.

## `{{ID_CITATION_EXAMPLE}}`

`// FR-R-012 — …` directly above the `func Test…`

## `{{ID_CITATION_BLOCK}}`

```go
// FR-R-012 — The checksum is computed over the full frame excluding the checksum field.
func TestUtChecksumExcludesTrailer(t *testing.T) { /* … */ }
```

## `{{SETUP_STEPS}}`

Install Go 1.23+ from [go.dev/dl](https://go.dev/dl/), then:

```sh
go mod download
go build ./...
```

Lint gate also needs `golangci-lint`:

```sh
go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
```

## `{{STACK_CONVENTIONS}}`

- Errors typed: package-level sentinel (`var ErrTimeout = errors.New(…)`) or a struct implementing `error`, wrapped with `%w`, matched with `errors.Is`/`errors.As`. Never a formatted string compared by text.
- Exported identifiers are the public API; `internal/` is free to change. New exported names are spec (gate 1).
- Domain values are defined types (`type UnitID uint8`), not bare integers, wherever mixing two would be a bug.
- Every exported identifier has a doc comment starting with its name.
- Contexts are first parameters and honoured — no unbounded blocking call without a `ctx` cancellation path.
- Table-driven tests, named subtests; `t.Parallel()` where the test allows it.
- `go test -race` is default in CI; a data race is a failure, not a flake.

## `ci` → `.github/workflows/check.yml`

```yaml
name: check

on:
  push:
  pull_request:

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: stable
      - run: test -z "$(gofmt -l .)" || { gofmt -l .; exit 1; }

  vet:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: stable
      - run: go vet ./...

  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: stable
      - uses: golangci/golangci-lint-action@v6

  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        go-version: ["1.23", "stable"]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: ${{ matrix.go-version }}
      - run: go test -race ./...

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: stable
      - run: go test -coverprofile=cover.out ./...
      - name: Enforce coverage floor
        run: |
          pct="$(go tool cover -func=cover.out | tail -1 | awk '{print $3}' | tr -d '%')"
          echo "total coverage: ${pct}%"
          awk -v p="$pct" -v f={{COVERAGE_FLOOR}} 'BEGIN { exit (p+0 >= f+0) ? 0 : 1 }' \
            || { echo "coverage ${pct}% is below the {{COVERAGE_FLOOR}}% floor"; exit 1; }
```

## `bitbucket-pipelines` → `bitbucket-pipelines.yml`

```yaml
image: golang:1.23

pipelines:
  default:
    - step:
        name: fmt
        script:
          - test -z "$(gofmt -l .)" || { gofmt -l .; exit 1; }
    - step:
        name: vet
        script:
          - go vet ./...
    - step:
        name: lint
        script:
          - go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
          - golangci-lint run
    - step:
        name: test
        script:
          - go test -race ./...
    - step:
        name: coverage
        script:
          - go test -coverprofile=cover.out ./...
          - |
            pct="$(go tool cover -func=cover.out | tail -1 | awk '{print $3}' | tr -d '%')"
            awk -v p="$pct" -v f={{COVERAGE_FLOOR}} 'BEGIN { exit (p+0 >= f+0) ? 0 : 1 }' \
              || { echo "coverage ${pct}% is below the {{COVERAGE_FLOOR}}% floor"; exit 1; }
```

## `lefthook` → `.lefthook.yml`

```yaml
pre-commit:
  piped: true
  commands:

    fmt:
      glob: "*.go"
      run: |
        unformatted="$(gofmt -l {staged_files})"
        [ -z "$unformatted" ] || { echo "$unformatted"; exit 1; }
      fail_text: |
        gofmt found formatting issues in the files listed above.
        Run `gofmt -w .` to fix them, then re-stage your changes.

    vet:
      glob: "*.go"
      run: go vet ./...
      fail_text: |
        go vet found issues. Fix them, then re-stage your changes.

    # Warning only (never blocks the commit): this project is spec-driven, so a
    # change to product code usually comes with a docs/specs/ update in the same commit.
    spec-reminder:
      run: |
        staged="$(git diff --cached --name-only)"
        code="$(printf '%s\n' "$staged" | grep -E '\.go$' | grep -v '_test\.go$' || true)"
        spec="$(printf '%s\n' "$staged" | grep -E '^docs/specs/' || true)"
        if [ -n "$code" ] && [ -z "$spec" ]; then
          echo "note: product code changed but no docs/specs/ file is staged."
          echo "      If this commit changes behavior, update the area's requirements.md too"
          echo "      (see AGENTS.md 'Spec-driven'). This is a reminder, not a failure."
        fi
        exit 0

    # Warning only: every test that pins observable behavior cites its requirement ID
    # in a comment directly above the func (see AGENTS.md).
    test-id-reminder:
      run: |
        staged="$(git diff --cached --name-only | grep -E '_test\.go$' || true)"
        [ -z "$staged" ] && exit 0
        if ! git diff --cached -U1 -- $staged | grep -qE '^\+\s*//\s*({{ID_PREFIX_ALTERNATION}})-R-[0-9]+'; then
          echo "note: tests changed but no requirement ID appears in an added comment."
          echo "      Tests pinning observable behavior cite their requirement (see AGENTS.md)."
          echo "      Reminder, not a failure."
        fi
        exit 0
```

## `config` → `.golangci.yml` (default)

```yaml
version: "2"

linters:
  enable:
    - errcheck
    - errorlint
    - govet
    - ineffassign
    - staticcheck
    - unused
    - revive
    - bodyclose
    - contextcheck

  settings:
    revive:
      rules:
        - name: exported
```

## `{{GAUNTLET_STEPS}}` → `.claude/scripts/gauntlet.sh`

Step name, timeout (s), command — one `run` line each. Coverage step present only with a floor.

```sh
run fmt   600  'test -z "$(gofmt -l .)"'
run vet   900  go vet ./...
run lint  900  golangci-lint run
run build 900  go build ./...
run test  1800 go test -race ./...
run cov   1800 'go test -coverprofile=cover.out ./... && go tool cover -func=cover.out | tail -1'
```

## `{{COVERAGE_EXTRACT}}`

```sh
cov=$(grep -E '^total:' "$log" | tail -1 | grep -oE '[0-9.]+%')
```
