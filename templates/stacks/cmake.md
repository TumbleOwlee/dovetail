# Stack: C / C++ (CMake)

Slot values, CMake project driven by Make or Ninja. Each fenced block = one slot; heading names the placeholder it fills.

**Ask about:**

- **Generator** — Ninja (assumed below, `-G Ninja`) or Unix Makefiles (`-G "Unix Makefiles"`). Only the `-G` flag differs; `cmake --build` drives either.
- **Test framework** — GoogleTest (assumed), Catch2, or doctest.
- **Coverage tool** — `gcovr` (assumed) or `lcov`/`llvm-cov`.
- **Preset file** — whether to generate `CMakePresets.json` (recommended; commands below assume it).

> CI block's GitHub Actions `${{ … }}` expressions are NOT template placeholders — copy verbatim.

## `{{STACK_NAME}}`

C++20 with CMake (Ninja generator, GoogleTest, `clang-format`, `clang-tidy`, `gcovr`)

## `{{FULL_COMMANDS}}`

```sh
cmake --preset dev
cmake --build --preset dev
ctest --preset dev --output-on-failure
clang-format --dry-run --Werror $(git ls-files '*.cpp' '*.hpp' '*.c' '*.h')
clang-tidy -p build/dev $(git ls-files '*.cpp')
gcovr --root . --fail-under-line {{COVERAGE_FLOOR}}
```

## `{{NARROW_COMMANDS}}`

```sh
cmake --build --preset dev --target frame_test   # one target
ctest --preset dev -R UtChecksum                 # one test by name
ctest --preset dev -R '^It'                      # integration tests only
gcovr --root . --html-details build/coverage.html   # browsable coverage
```

## `{{UNIT_TEST_CONVENTION}}`

`tests/unit/<module>_test.cpp`, one `TEST` per behavior, suite named `Ut<Module>`.

## `{{INTEGRATION_TEST_CONVENTION}}`

`tests/integration/<capability>_test.cpp`, suite named `It<Capability>`, registered with a `ctest` label so the set can be excluded.

## `{{ID_CITATION_EXAMPLE}}`

`// FR-R-012 — …` directly above the `TEST(...)`

## `{{ID_CITATION_BLOCK}}`

```cpp
// FR-R-012 — The checksum is computed over the full frame excluding the checksum field.
TEST(UtFrame, ChecksumExcludesTrailer) { /* … */ }
```

## `{{SETUP_STEPS}}`

Install a C++20 compiler, CMake 3.25+, Ninja, and the test framework (fetched by CMake via `FetchContent`, no manual install needed), then:

```sh
cmake --preset dev
cmake --build --preset dev
```

Coverage gate also needs `gcovr`:

```sh
pipx install gcovr
```

## `{{STACK_CONVENTIONS}}`

- C++20, warnings-as-errors (`-Wall -Wextra -Wpedantic -Werror`) on the project's own targets; third-party targets excluded, not silenced globally.
- No raw owning pointers. Ownership is `std::unique_ptr`/`std::shared_ptr` or a value; raw pointers/references are non-owning views only.
- No exceptions across the public API boundary if the project targets embedded builds — decide once, record as a non-functional requirement, hold it. Otherwise errors are `std::expected`/`tl::expected` or a typed exception, never an `int` return code paired with an out-parameter.
- Headers self-contained: each compiles alone, includes what it uses, carries `#pragma once`.
- Public headers under `include/<project>/`; anything under `src/` is internal and free to change.
- Sanitizers (`-fsanitize=address,undefined`) on in the `dev` preset and CI test job. A sanitizer finding is a failure, not a warning.
- Every target declared with `target_link_libraries(... PRIVATE|PUBLIC ...)` and explicit `target_include_directories` — never a directory-scope `include_directories`.

## `ci` → `.github/workflows/check.yml`

```yaml
name: check

on:
  push:
  pull_request:

jobs:
  format:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y clang-format
      - run: clang-format --dry-run --Werror $(git ls-files '*.cpp' '*.hpp' '*.c' '*.h')

  build-test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        preset: [dev, release]
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y ninja-build
      - run: cmake --preset ${{ matrix.preset }}
      - run: cmake --build --preset ${{ matrix.preset }}
      - run: ctest --preset ${{ matrix.preset }} --output-on-failure

  tidy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y ninja-build clang-tidy
      - run: cmake --preset dev
      - run: clang-tidy -p build/dev $(git ls-files '*.cpp')

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y ninja-build gcovr
      - run: cmake --preset coverage
      - run: cmake --build --preset coverage
      - run: ctest --preset coverage --output-on-failure
      - run: gcovr --root . --fail-under-line {{COVERAGE_FLOOR}}
```

## `bitbucket-pipelines` → `bitbucket-pipelines.yml`

Generator is `Ninja` here; swap to `Unix Makefiles` if the user chose Make.

```yaml
image: ubuntu:24.04

pipelines:
  default:
    - step:
        name: format
        script:
          - apt-get update && apt-get install -y clang-format
          - clang-format --dry-run --Werror $(git ls-files '*.cpp' '*.hpp' '*.c' '*.h')
    - step:
        name: build-test
        script:
          - apt-get update && apt-get install -y ninja-build
          - cmake --preset dev
          - cmake --build --preset dev
          - ctest --preset dev --output-on-failure
    - step:
        name: tidy
        script:
          - apt-get update && apt-get install -y ninja-build clang-tidy
          - cmake --preset dev
          - clang-tidy -p build/dev $(git ls-files '*.cpp')
    - step:
        name: coverage
        script:
          - apt-get update && apt-get install -y ninja-build gcovr
          - cmake --preset coverage
          - cmake --build --preset coverage
          - ctest --preset coverage --output-on-failure
          - gcovr --root . --fail-under-line {{COVERAGE_FLOOR}}
```

## `lefthook` → `.lefthook.yml`

```yaml
pre-commit:
  piped: true
  commands:

    format:
      glob: "*.{c,h,cpp,hpp,cc,hh}"
      run: clang-format --dry-run --Werror {staged_files}
      fail_text: |
        clang-format found formatting issues.
        Run `clang-format -i` on the files above, then re-stage your changes.

    build:
      glob: "*.{c,h,cpp,hpp,cc,hh,txt,cmake}"
      run: cmake --build --preset dev
      fail_text: |
        The build failed. Fix the errors above, then re-stage your changes.

    # Warning only (never blocks the commit): this project is spec-driven, so a
    # change to src/ usually comes with a docs/specs/ update in the same commit.
    spec-reminder:
      run: |
        staged="$(git diff --cached --name-only)"
        code="$(printf '%s\n' "$staged" | grep -E '^(src|include)/' || true)"
        spec="$(printf '%s\n' "$staged" | grep -E '^docs/specs/' || true)"
        if [ -n "$code" ] && [ -z "$spec" ]; then
          echo "note: product code changed but no docs/specs/ file is staged."
          echo "      If this commit changes behavior, update the area's requirements.md too"
          echo "      (see AGENTS.md 'Spec-driven'). This is a reminder, not a failure."
        fi
        exit 0

    # Warning only: every test that pins observable behavior cites its requirement ID
    # in a comment directly above the TEST(...) macro (see AGENTS.md).
    test-id-reminder:
      run: |
        staged="$(git diff --cached --name-only | grep -E '^tests/.*\.(cpp|cc)$' || true)"
        [ -z "$staged" ] && exit 0
        if ! git diff --cached -U1 -- $staged | grep -qE '^\+\s*//\s*({{ID_PREFIX_ALTERNATION}})-R-[0-9]+'; then
          echo "note: tests changed but no requirement ID appears in an added comment."
          echo "      Tests pinning observable behavior cite their requirement (see AGENTS.md)."
          echo "      Reminder, not a failure."
        fi
        exit 0
```

## `config` → `CMakePresets.json` (default)

Generator is `Ninja` here; swap to `Unix Makefiles` if the user chose Make.

```json
{
  "version": 6,
  "configurePresets": [
    {
      "name": "dev",
      "generator": "Ninja",
      "binaryDir": "build/dev",
      "cacheVariables": {
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_EXPORT_COMPILE_COMMANDS": "ON",
        "{{PROJECT_UPPER}}_SANITIZE": "ON",
        "{{PROJECT_UPPER}}_BUILD_TESTS": "ON"
      }
    },
    {
      "name": "release",
      "generator": "Ninja",
      "binaryDir": "build/release",
      "cacheVariables": {
        "CMAKE_BUILD_TYPE": "RelWithDebInfo",
        "{{PROJECT_UPPER}}_BUILD_TESTS": "ON"
      }
    },
    {
      "name": "coverage",
      "generator": "Ninja",
      "binaryDir": "build/coverage",
      "cacheVariables": {
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_CXX_FLAGS": "--coverage -O0 -g",
        "{{PROJECT_UPPER}}_BUILD_TESTS": "ON"
      }
    }
  ],
  "buildPresets": [
    { "name": "dev", "configurePreset": "dev" },
    { "name": "release", "configurePreset": "release" },
    { "name": "coverage", "configurePreset": "coverage" }
  ],
  "testPresets": [
    {
      "name": "dev",
      "configurePreset": "dev",
      "output": { "outputOnFailure": true }
    },
    {
      "name": "release",
      "configurePreset": "release",
      "output": { "outputOnFailure": true }
    },
    {
      "name": "coverage",
      "configurePreset": "coverage",
      "output": { "outputOnFailure": true }
    }
  ]
}
```

## `config` → `.clang-format` (default)

```yaml
BasedOnStyle: LLVM
Standard: c++20
ColumnLimit: 100
IndentWidth: 4
PointerAlignment: Left
```

## `config` → `.clang-tidy` (default)

```yaml
Checks: >
  bugprone-*,
  cppcoreguidelines-*,
  modernize-*,
  performance-*,
  readability-*,
  -modernize-use-trailing-return-type,
  -readability-magic-numbers,
  -cppcoreguidelines-avoid-magic-numbers
WarningsAsErrors: "bugprone-*,performance-*"
HeaderFilterRegex: "^(src|include)/"
```

## `{{GAUNTLET_STEPS}}` → `.claude/scripts/gauntlet.sh`

Step name, timeout (s), command — one `run` line each. Coverage step present only with a floor.

```sh
run configure 900  cmake --preset dev
run build     1800 cmake --build --preset dev
run test      1800 ctest --preset dev --output-on-failure
run fmt       600  "clang-format --dry-run --Werror \$(git ls-files '*.cpp' '*.hpp' '*.c' '*.h')"
run tidy      1800 "clang-tidy -p build/dev \$(git ls-files '*.cpp')"
run cov       1800 gcovr --root . --fail-under-line {{COVERAGE_FLOOR}}
```

## `{{COVERAGE_EXTRACT}}`

```sh
cov=$(grep -E '^TOTAL' "$log" | tail -1 | grep -oE '[0-9]+%' | head -1)
```
