# Stack: Python

Slot values, Python ecosystem. Each fenced block = one slot; heading names the placeholder it fills.

**Ask about:** package manager (`uv` — assumed below — vs `poetry` vs plain `pip`/`venv`); type checker `mypy` vs `pyright`; layout `src/` (assumed) vs flat — hook blocks below hard-code `^src/` as the product-code path and must be adjusted for a flat layout.

> CI block's GitHub Actions `${{ … }}` expressions are NOT template placeholders — copy verbatim.

## `{{STACK_NAME}}`

Python 3.12+ (`uv` for environments/locking, `ruff` for lint/format, `pytest` for tests, `mypy` for types)

## `{{FULL_COMMANDS}}`

```sh
uv run ruff format --check .
uv run ruff check .
uv run mypy src
uv run pytest
uv run pytest --cov=src --cov-fail-under={{COVERAGE_FLOOR}}
```

## `{{NARROW_COMMANDS}}`

```sh
uv run pytest tests/test_frame.py::test_ut_checksum   # one test
uv run pytest -k checksum                             # one pattern
uv run pytest tests/integration                       # one directory
uv run pytest --cov=src --cov-report=html             # browsable coverage
```

## `{{UNIT_TEST_CONVENTION}}`

`tests/unit/test_<module>.py`, functions named `test_ut_*`.

## `{{INTEGRATION_TEST_CONVENTION}}`

`tests/integration/test_<capability>.py`, functions named `test_it_*`.

## `{{ID_CITATION_EXAMPLE}}`

`"""FR-R-012 — …"""` as the test's first docstring line

## `{{ID_CITATION_BLOCK}}`

```python
def test_ut_checksum_excludes_trailer() -> None:
    """FR-R-012 — The checksum is computed over the full frame excluding the checksum field."""
```

## `{{SETUP_STEPS}}`

Install [uv](https://docs.astral.sh/uv/), then:

```sh
uv sync --all-extras --dev
```

## `{{STACK_CONVENTIONS}}`

- Full type annotations on every public function; `mypy --strict` on `src`. An untyped public signature is a defect, not a style choice.
- Errors are exception classes deriving from one package-level base, never bare `Exception` and never a returned error string.
- No mutable default arguments; no module-level side effects on import.
- Domain values are `NewType` or frozen dataclasses wherever mixing two would be a bug — not bare `int`/`str`.
- Public API = what `__all__` exports; everything else is `_`-prefixed and free to change.
- Prefer the standard library; a dependency is a scope boundary (see AGENTS.md).

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
      - uses: astral-sh/setup-uv@v5
      - run: uv sync --all-extras --dev
      - run: uv run ruff format --check .
      - run: uv run ruff check .

  types:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v5
      - run: uv sync --all-extras --dev
      - run: uv run mypy src

  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python-version: ["3.12", "3.13"]
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v5
        with:
          python-version: ${{ matrix.python-version }}
      - run: uv sync --all-extras --dev
      - run: uv run pytest

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v5
      - run: uv sync --all-extras --dev
      - run: uv run pytest --cov=src --cov-fail-under={{COVERAGE_FLOOR}}
```

## `bitbucket-pipelines` → `bitbucket-pipelines.yml`

```yaml
image: python:3.12

pipelines:
  default:
    - step:
        name: lint
        script:
          - pip install uv
          - uv sync --all-extras --dev
          - uv run ruff format --check .
          - uv run ruff check .
    - step:
        name: types
        script:
          - pip install uv
          - uv sync --all-extras --dev
          - uv run mypy src
    - step:
        name: test
        script:
          - pip install uv
          - uv sync --all-extras --dev
          - uv run pytest
    - step:
        name: coverage
        script:
          - pip install uv
          - uv sync --all-extras --dev
          - uv run pytest --cov=src --cov-fail-under={{COVERAGE_FLOOR}}
```

## `lefthook` → `.lefthook.yml`

```yaml
pre-commit:
  piped: true
  commands:

    format:
      glob: "*.py"
      run: uv run ruff format --check {staged_files}
      fail_text: |
        ruff format found formatting issues.
        Run `uv run ruff format .` to fix them, then re-stage your changes.

    lint:
      glob: "*.py"
      run: uv run ruff check {staged_files}
      fail_text: |
        ruff check found issues.
        Fix the findings above, then re-stage your changes.

    types:
      glob: "*.py"
      run: uv run mypy src
      fail_text: |
        mypy found type errors. Fix them, then re-stage your changes.

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
    # in its docstring (see AGENTS.md).
    test-id-reminder:
      run: |
        staged="$(git diff --cached --name-only | grep -E '^tests/.*\.py$' || true)"
        [ -z "$staged" ] && exit 0
        if ! git diff --cached -U1 -- $staged | grep -qE '^\+.*({{ID_PREFIX_ALTERNATION}})-R-[0-9]+'; then
          echo "note: tests changed but no requirement ID appears in an added docstring."
          echo "      Tests pinning observable behavior cite their requirement (see AGENTS.md)."
          echo "      Reminder, not a failure."
        fi
        exit 0
```

## `config` → `ruff.toml` (default, omit if pyproject.toml carries a `[tool.ruff]` table)

```toml
line-length = 100
target-version = "py312"

[lint]
select = ["E", "F", "W", "I", "N", "UP", "B", "A", "C4", "RET", "SIM", "ARG", "PTH", "RUF"]

[lint.per-file-ignores]
"tests/**" = ["ARG"]
```

## `{{GAUNTLET_STEPS}}` → `.claude/scripts/gauntlet.sh`

Step name, timeout (s), command — one `run` line each. Coverage step present only with a floor.

```sh
run fmt   600  uv run ruff format --check .
run lint  600  uv run ruff check .
run types 900  uv run mypy src
run test  1800 uv run pytest
run cov   1800 uv run pytest --cov=src --cov-fail-under={{COVERAGE_FLOOR}}
```

## `{{COVERAGE_EXTRACT}}`

```sh
cov=$(grep -E '^TOTAL' "$log" | tail -1 | grep -oE '[0-9]+%' | tail -1)
```
