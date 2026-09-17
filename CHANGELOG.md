# Changelog

All notable changes to Wickra are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **The READMEs say what CI does.** `fuzz/README.md` names the family's pinned
  `nightly-2026-07-01` instead of a rolling `nightly` the CI job stopped using,
  with the reason beside it, and `examples/c/README.md` heads its CMake block
  "Build and run the examples" -- it builds every example, not only the smoke
  one. Both are the reference every sibling's README is now shaped after.

### Changed

- **The repository spells shared things the way the family does.** A cross-repo
  scan lined the 24 wickra-lib repositories up and this one differed in: the R
  floor (`R (>= 2.10)` in DESCRIPTION and README while every sibling requires
  4.1, which the package's `configure` needs anyway), the Go and Java binding
  jobs testing only the newest toolchain (a `go 1.23` / `release 22` floor row on
  ubuntu now runs alongside stable / 25, as the siblings' example jobs do), the
  Maven compiler and surefire plugins one line behind (3.16.0 / 3.6.0),
  `@napi-rs/cli` at ^3.7.2 against the family's ^3.9.0, `proptest = "1.5"` and
  `rayon = "1.10"` where the family writes the major, the Go binding module
  absent from Dependabot, and `packaging` 26.2 in the Python locks where every
  other lock has 26.3. The 3.9 requirement row now says `pytest<9` -- what it
  resolved to all along, since pytest 9 requires 3.10 -- with the reason next
  to it.

### Fixed

- **The publish gate refused a tag whose CI had not finished yet.** It asked the
  API for `conclusion` on the tagged commit's `ci.yml` run and required
  `success`. A run still in progress reports `null` there, which the check read
  the same way it read a failure -- so a tag pushed shortly after its merge, with
  CI still queued behind the release run's own build jobs, was refused for having
  "no successful ci.yml run". Nothing was published, which is the gate working as
  designed, but the verdict was wrong: v1.0.4 hit this, the commit went green 32
  minutes later, and a rerun published it without a single change.

  The step now waits for that run to settle, bounded to 45 minutes, and reports
  what it is waiting on. A conclusion other than `success` still fails
  immediately, so a genuinely broken commit is rejected as fast as before -- only
  the undecided case waits. `timeout-minutes` on the job moves from 10 to 60 to
  outlive that wait.

### Changed

- **uv 0.12.15 for the lockfile script.** `scripts/update-lockfiles.sh`
  bootstraps 0.12.15 (was 0.12.7); the pin and all four release
  checksums move together, taken from the release's `.sha256` files.

## [1.0.4] - 2026-09-01

### Added

- **Streaming and batch are compared through the C ABI.** Every indicator
  exposes `update` and `batch`, and they must agree -- a consumer switching
  between a live feed and a backfill gets the same numbers, or one of the two
  paths is wrong. Node and Python assert this; C, the ABI every other binding
  sits on, never did. `examples/c/streaming_vs_batch_test.c` checks a scalar
  and a candle indicator, deliberately reusing the Node suite's series,
  tolerance and NaN handling so a disagreement between languages is about the
  library rather than about the test.

- **`scripts/check_readme_links.py`** holds each `bindings/*/README.md` to
  linking only what travels with the package. These are published long
  descriptions -- PyPI renders the Python one, NuGet the C# one -- where a link
  out of the package is dead while resolving fine on GitHub, so nothing says
  so. Links that stay inside the package are fine: `man/figures/logo.png` in
  the R binding is the R convention and ships with the package.

- **`scripts/check_license_copies.py`** verifies that every published package
  carries both licence texts byte-for-byte, deriving the list from the
  workspace rather than a hand-kept one.

- **The published crates carry their licence texts.** `cargo package` only
  includes files inside the crate directory, so the root `LICENSE-MIT` and
  `LICENSE-APACHE` never travelled: `wickra-core-1.0.3.crate` shipped 532 files
  and not one of them was a licence. Each of the three crates and the Python
  binding now has its own copy, and `cargo package --list` shows both.

- **`.gitattributes` states the line-ending rule for the whole tree** — one
  `* text=auto eol=lf` plus explicit binary exceptions, replacing 41 lines that
  named six languages and were silent about the rest. `.rs`, `.py`, `.js`,
  `.ts`, `.md`, `.toml` and `.yml` were unregulated; the index stayed clean only
  because the people committing happened to be configured consistently, which is
  a property of their machines and not of this repository. It matters most for
  `bindings/node/index.js` and `index.d.ts`, which come back from napi with CRLF
  on Windows and are checked for drift by regenerating and diffing. The
  renormalisation changed no file: all 2621 tracked text files were already LF.

- **`fuzz/Cargo.lock` is watched by Dependabot.** The fuzz targets are their own
  cargo workspace, which the root entry does not reach, so `libfuzzer-sys` and
  `arbitrary` received no updates while nothing was looking.

- **A `## Security` section in the README** points at private vulnerability
  reporting, and the pull request template names the longer template that GitHub
  offers no picker for.

- **Every version declaration is checked against the others on each pull
  request.** The version sits in 22 declarations across six package managers,
  and the ones that go stale are never the obvious ones: `index.js` shipped
  0.9.7 inside the v0.9.9 tag because napi only rewrites it when somebody
  rebuilds, the Java benchmarks pom was found pinned to 0.9.6 while 1.0.0 was
  being cut, and the C# project file sat at 0.7.9. Each of those ships a package
  that pins a binary nobody published, and it surfaces at install time on
  someone else's machine, after a tag that cannot be taken back.

  `wickra-backtest` has had this check since it shipped; this repository, with
  more bindings and therefore more places to miss, did not. The file list is
  explicit rather than a grep, because `Cargo.lock` carries third-party crates
  that occasionally sit at our version, and counts are exact: a pattern that
  should find six platform dependencies and finds five has found the bug.

- **CodSpeed measures the benchmarks on every pull request.** Performance
  regressions were the one class of defect nothing here reported: `bench.yml`
  runs on a schedule and prints numbers a person has to read, and the Python
  batch regression in August was found by re-measuring by hand, weeks after it
  landed.

  It counts instructions under instrumentation rather than timing wall clock,
  which is what makes a shared runner usable -- the figure does not move because
  a neighbour VM got busy. These numbers are therefore not the ones in
  `BENCHMARKS.md`: those are throughput, this is relative change.

  `criterion` in `crates/wickra` now resolves to `codspeed-criterion-compat`
  under the same name, so `indicators.rs` and `data_layer.rs` are untouched and
  `cargo bench` behaves as before off a CodSpeed runner. `crates/wickra-bench`
  keeps the plain crate: its `cross_lib` bench measures kand, ta and yata
  alongside us, and instrumenting other people's crates on every pull request
  costs the most and answers nothing about this one.

- **CodeQL analyses the C# and Java bindings.** The matrix covered Rust, Python
  and JavaScript/TypeScript. C# and Java were the two largest surfaces it had no
  view of, and the two where a memory mistake is possible at all: both reach the
  C ABI directly, C# through `WickraHandle.cs` and `WickraNative.cs` with manual
  handle lifetimes and native library resolution, Java through
  `java.lang.foreign` with an `Arena` per handle and a `Cleaner` to release it.
  None of that had ever been read by a static analyser.

  Both are compiled for the analysis rather than read as source. The first C#
  pass ran with `build-mode: none` and GitHub reported it as low quality: 64% of
  calls resolved to a target against a threshold of 85%. Two causes, and the
  second was self-inflicted -- without a build no dependency resolves, and
  `paths-ignore` keeps the excluded files out of the database entirely, so the
  hand-written code calling into them pointed at nothing. Building fixes both:
  the generated code compiles so the calls resolve, while `paths-ignore` still
  filters its findings out of the results.

  The generated files are excluded before the languages are enabled, not triaged
  afterwards. `Indicators.g.cs` and `NativeMethods.g.cs` are 79,000 lines of
  P/Invoke declarations emitted from the C header, `GoldenAllTests.g.cs` is one
  test per catalogue entry, and 624 of the 636 Java files are generated the same
  way and carry `Do not edit by hand`. Turning the languages on without
  excluding them would have repeated what the napi glue did once already: 518
  findings at once, one per exported class, none of them about anything anyone
  wrote. What stays in the analysis is the hand-written surface --
  `WickraHandle.cs`, `WickraNative.cs`, `internal/WickraNative.java`, and the
  tests -- which is the whole area where a leak or a use-after-free could
  originate.

- **`cargo-semver-checks` guards the published API.** `wickra-core`,
  `wickra-data` and `wickra` are on crates.io, and since 1.0.0 the version
  number is a promise: a patch release must not move the public surface. Nothing
  checked that. The surface is 514 exported types with ten language bindings on
  top of it -- exactly the shape where a rename slips past review and out to six
  registries in one run that cannot be undone.

  The baseline is the newest version already on crates.io, fetched by the tool.
  The three crates are named explicitly rather than `--workspace`: the bench and
  binding crates are unpublished, so they have no baseline and make no promise.

- **actionlint checks the workflows for correctness**, which is the half zizmor
  does not cover. zizmor reads them for security -- template injection,
  over-broad tokens, unpinned actions. actionlint reads them for whether they
  work at all: unknown `${{ }}` contexts, wrong event properties, invalid
  `needs` references, and, through its bundled shellcheck, errors inside every
  `run:` block. There are around 2,400 lines of workflow here with several
  hundred lines of shell inside them, and none of it was being checked.

  The binary is fetched from the upstream release and verified against the
  published checksum. `taiki-e/install-action` carries no actionlint manifest
  and the project ships no action of its own, so the alternatives were a
  third-party wrapper or a docker pull; a pinned download with a verified hash
  adds no supply-chain dependency at all.

- The Java jar is attached to the GitHub Release. `java-publish` stages the
  native libraries for all six platforms into the binding's resources, deploys
  to Maven Central, and now also uploads the packaged jar, so the release page
  carries the same file Maven Central received. The sources and javadoc jars
  stay on Maven Central, where a build tool resolves them on demand.

- **CodeQL analyses Go and C/C++.** The matrix left out the binding that casts
  slice base addresses through `unsafe.Pointer` and manages handle lifetimes
  with `runtime.SetFinalizer`, and the hand-written C++ RAII wrapper that the C
  examples compile and run in CI. The stated reason — that C# and Java were the
  only bindings where a memory mistake is possible — was wrong.

- **Every platform package is proved to have its binary before publishing.** If
  a build artefact never arrives, `napi artifacts` leaves that directory without
  a `.node` and the stub publishes as an empty package that installs cleanly and
  fails at `require()`. Hard to reach, cheap to prove.

- **The committed napi loader is regenerated for `@napi-rs/cli` 3.8.6.** The
  bump (#426) changed only the lockfile, and `index.js` is generated rather than
  written: 3.8.6 emits WASI-flavour selection and a shared
  `createLoadErrorChain` that 3.7.4 did not. The drift check added the day
  before caught it on `main` -- the pull request itself was green because
  Dependabot tested it against a base commit from before that check existed.
- **The npm lockfile recorded the package's own version twice, and one of them
  was stale.** `bindings/node/package-lock.json` carries the version at the top
  level and again in the `packages[""]` entry; only the first is what `npm
  version` rewrites, so the second sat at 1.0.1 across two releases. It was
  repaired by an unrelated Dependabot regeneration rather than by anything
  watching, and `check_version_sync.py` had missed it because it treats the
  lockfile as generated and only asserted that the version appears somewhere in
  it. Both records are now checked exactly; the rest of the file stays a
  presence check, because those versions belong to other people.

- **`Vpin::update` could never return for a large trade size.** It distributes a
  trade's volume across buckets with `while remaining > 0.0 { ... remaining -=
  take }`, and once the size is large enough that the bucket volume falls below
  one ULP of it, the subtraction changes nothing: at 1.4e277 against a bucket of
  8, `remaining` stays exactly where it was and the loop never ends. Not slow --
  non-terminating. A single trade hung the caller for good, and 1.4e277 passes
  `Trade::new` unchanged, so this was reachable through the ordinary validating
  API rather than only through `new_unchecked`.

  The loop is now bounded by what can still be observed. A trade is one-sided,
  so every complete bucket it fills carries an imbalance of exactly the bucket
  volume, and the window keeps only the last `num_buckets`; anything beyond that
  pushes values identical to the ones it evicts. The bound keeps the remainder
  that decides where the next bucket boundary falls, so it is exact rather than
  an approximation — a test asserts that one 1000.5-sized trade leaves the same
  state as 2001 trades of 0.5.

  Found by `fuzz/fuzz_targets/indicator_update_trade.rs` on its first ever run.
  It was one of seven targets that were built but never executed, which is what
  the change to fuzzing every declared target was for.

- **Every release since the .NET binding shipped attached the package to
  nuget.org but not to the release page.** `csharp-publish` packs the package,
  pushes it, and uploads it as a build artifact -- and the staging step in
  `github-release` listed six `find` patterns, none of which matched `*.nupkg`,
  so the file was downloaded with the rest and then left behind. With
  `fail_on_unmatched_files` false nothing reported it. `v1.0.3` carries 36
  assets -- wheels, sdist, `.node` binaries, npm tarballs, `.crate` files and
  SBOMs -- and NuGet was the one registry artefact with no counterpart there.
  The registry was never affected: all 28 published versions are on nuget.org.

- **The release could be published before NuGet, Maven Central and the Go module
  were.** `github-release` waited on five of the eight publishing jobs, so the
  three that ship to those three destinations could still be running -- or
  failing -- while the release notes already told readers they were live. It now
  waits on all eight: `csharp-publish` and `java-publish` because it stages the
  files they upload, `go-mirror` because the notes claim the Go module is
  available.


### Changed

- **`release.yml` runs in this shape for the first time.** The publish gate, the
  split between building and publishing the wasm package, the provenance
  extended to the `.nupkg`, the jar and the C ABI archives, and the npm licence
  proof were all written after 1.0.3 had shipped. The workflow executes only on
  a pushed `v*` tag, so none of them had ever run: they were written, linted and
  merged with nothing able to exercise them. The one bug that survived that gap
  is in *Fixed* below.

- **A release is all-or-nothing: nothing is published unless everything is
  green.** `cargo-publish` and `wasm-publish` declared no dependencies at all,
  so crates.io could receive a version while the Python wheels were still
  building. A wheel failing afterwards left the crate published, unwithdrawable,
  and the release half-shipped -- with the notes claiming otherwise.

  Every publish job now waits on a `gate` job, and the gate waits on all five
  build jobs *and* on evidence that the tagged commit was green. `ci.yml` does
  not run on tags, so the release cannot infer the tag's health from its own
  run: the gate asks the API what the commit was graded, requires a successful
  `ci.yml` run on that exact SHA, and refuses if any other workflow that ran on
  it failed. Listing required workflows by name would go stale -- `codspeed.yml`
  is path-filtered and legitimately does not run for a version bump -- so the
  rule is about outcomes rather than about a roster.

  The WASM package is built in its own `wasm-build` job for the same reason: it
  used to be compiled inside the publish job, which put its build behind the
  gate rather than in front of it. What is published is now the very tarball
  that is attached to the release, so the two cannot diverge.

  Six registries cannot be made atomic -- a registry can still fail mid-upload
  after another has succeeded. What this removes is the common case, which is
  not a registry outage but something that does not build.

- **Every fuzz target is fuzzed, not six of thirteen.** The workflow listed the
  targets to run by hand and seven had never been run: `fuzz build` proves a
  target compiles, which is not the same as proving it survives input. The list
  now comes from `cargo fuzz list`, so a new target is fuzzed the moment it
  exists rather than when somebody remembers.

- **The Go module is built and run before it is published.** `go-mirror`
  assembles the tree by copying files and rewriting an import path with `sed`,
  then pushed it to pkg.go.dev on that basis alone -- a wrong header, a missing
  library or a botched `sed` would have surfaced on a user's machine. It now
  compiles the assembled module and exercises one indicator end to end first.
  The `*_test.go` files stay behind: they read `../../testdata/golden`, which
  does not exist in a module fetched with `go get`, so shipping them meant
  `go test` on the published module failed at `os.Open`.

- **The committed napi loader and the generated C# golden tests are checked for
  drift.** `index.js` shipped 0.9.7 inside the v0.9.9 tag because napi rewrites
  it only when somebody rebuilds. Both are now regenerated in CI and compared.
  `Indicators.g.cs` and `NativeMethods.g.cs` cannot be checked this way: their
  generator is not in this repository.

- **CodeQL analyses the C# and Java examples and benchmarks.** Under
  `build-mode: manual` it sees exactly what the compiler walks, so building only
  the test project left 11 of 23 C# files unanalysed -- including every example,
  which is the code people copy into their own projects.

- **Seven action pins moved to their newest release**, each resolved to the
  commit rather than to the annotated tag object that a ref lookup returns.

- **`scripts/update-lockfiles.sh` no longer installs uv by itself.** It piped
  `https://astral.sh/uv/install.sh` into a shell, which runs whatever is behind
  that URL at that moment, with the privileges of everyone who regenerates a
  lockfile. It now stops and says how to install uv; `WICKRA_BOOTSTRAP_UV=1`
  opts into a bootstrap that fetches one pinned release archive and verifies its
  SHA-256 before using it. Convenience is still there for whoever asks for it,
  and nothing unattested runs for whoever does not.

- **`ci.yml` cancels superseded runs** on every branch but `main`. The matrix
  expands to sixty-odd jobs across three operating systems, and a superseded run
  held runners for a result nobody would read. `main` is exempt: its runs feed
  the badge and the coverage baseline.

- **The lychee-action pin says `# v2.9.0`** instead of `# v2`. Dependabot reads
  that comment to decide what to bump; a major-only comment gives it nothing to
  compare against.


- **The indicator count is no longer pushed into four other repositories.**
  `sync-about.yml` carried the number outward on every push to main and every
  tag -- into `wickra-docs`, `webpage`, the org profile and the wiki -- and also
  wrote the repo and org descriptions. All of it went through a fine-grained
  personal access token with write access to four repositories, `Administration:
  write` here and `admin:org`; the only repository-level secret in the
  organisation.

  The consumers pull instead. `wickra-docs`, `webpage` and `.github` each read
  the count and the released version from this repository hourly and commit into
  themselves with their own `GITHUB_TOKEN`. A schedule is the only trigger that
  can work for them: the number changes here, so a push there is not the event
  that makes them wrong. The delay is bounded by the hour, and a documentation
  page does not need a figure that moved in the last one.

  What could not be inverted stays manual, because it is settings rather than
  files. The About description and homepage need `Administration: write`, which
  `GITHUB_TOKEN` has no scope for even in its own repository, and the org
  description needs `admin:org`; the wiki has no Actions at all. `bump_version.py`
  now prints those three -- with the live values read back and compared -- only
  when the count has actually moved.

  What remains here is the check, renamed to `check-indicator-count.yml` because
  it no longer syncs anything. It also runs on main now, not only on pull
  requests: with no required status checks, a pull request can merge without it
  having run, and nothing would have noticed.

- Every job in `bench.yml`, `codeql.yml`, `links.yml`, `scorecard.yml`,
  `sync-metadata.yml` and `zizmor.yml` now declares `timeout-minutes`. Without
  it a hung job runs to GitHub's six-hour default; the values are backstops well
  above the observed runtimes, not budgets.


### Fixed

- **`npm publish` was handed a path npm read as a repository.** The wasm publish
  step passed the packed tarball as a bare relative path
  (`wasm-pkg/wickra-wasm-<version>.tgz`). npm parses an argument of that shape as
  the `owner/repo` GitHub shorthand rather than as a file, so it ran
  `git ls-remote ssh://git@github.com/wasm-pkg/...` and exited with code 128
  before reaching the registry. A leading `./` marks it as a path; the step now
  publishes `./$tarball`.

  The step never ran under a tag: the job was split into build and publish in
  `6293ffce`, after 1.0.3 had shipped, and `release.yml` runs only on `v*`. The
  publish jobs are independent of one another, so this would have left
  crates.io, PyPI, npm, NuGet and Maven Central published while the GitHub
  release -- which needs all of them -- was skipped, taking the C ABI archives
  with it.

- **A manual run of `release.yml` could publish from a branch.** The workflow
  carries `workflow_dispatch` so a failed publish can be retried without moving
  the tag, and the `release` environment has no deployment-branch policy, so
  nothing stopped a dispatch from `main`: the gate would pass (main *has* a
  green CI run), cargo, npm and PyPI would publish whatever the manifests said,
  and `go-mirror` would replace the contents of the public `wickra-go` and tag
  it `vmain`. The gate now refuses any ref that is not `refs/tags/v*`. The
  blueprint's claim that "release.yml runs on `push: tags` and nothing else" —
  which is why the environment was left unprotected — was simply wrong.

- **The seven npm packages shipped without either licence text.** Every manifest
  declares `MIT OR Apache-2.0`, which is a reference to two documents, and
  neither travelled. `check_license_copies.py` asserted npm was "handled at
  publish time (see release.yml)" — true in the sibling repository it was ported
  from, never true here. release.yml now stages the copies and proves with `npm
  pack --dry-run` that they are actually packed, because `files` decides what npm
  ships and a name missing from it is dropped without a word. All seven `files`
  allowlists name them, and the script verifies both halves rather than
  delegating to a step that might not exist.

- **`CITATION.cff` was three hundred indicators out of date** — "214 indicators
  across 16 families", naming three languages of ten, while the catalogue holds
  514 across twenty-four. It is what GitHub's citation box and Zenodo read, and
  no check had ever looked at it. `check-indicator-count.yml` now holds it to the
  count alongside the two READMEs.

- **`.nupkg`, `.jar` and the C ABI archives had no build provenance.** The
  attestation job covered crates, wheels and the sdist; npm packages carry their
  own inline provenance. That left the two packages C# and Java consumers
  install, and the archive people download and link against, attested nowhere —
  while Scorecard reported Signed-Releases 10/10, because it looks for a
  provenance file on the release rather than for coverage of its contents.

- **`osv-scanner.toml` was executed by nothing.** The file records two advisories
  assessed as not affecting this project, and that assessment was load-bearing
  for no one: no workflow consulted it. `cargo-deny` covers the Rust graph only,
  so npm, PyPI, Maven, NuGet, Go modules and R had no vulnerability scanning in
  CI at all. osv-scanner now runs in the supply-chain job.

- **Three Dependabot `ignore` entries suppressed security updates.**
  `open-pull-requests-limit: 0` already blocks every version-update pull request
  for the CI Python tooling, so the ignores on numpy, hypothesis and pytest had
  exactly one remaining effect: security updates, which are exempt from the
  limit, were suppressed too. A future advisory in any of the three would have
  produced silence rather than a red pull request somebody decides about.

- **Two jobs failed on every version bump, on the artefact the bump exists to
  create.** `examples/java` and the Java benchmarks depend on
  `org.wickra:wickra` at the version in the tree, which reaches Maven Central
  only when the tag publishes it. CodeQL's java-kotlin build resolved that
  dependency from Central, and osv-scanner's default enricher resolved it there
  too — so the bump that moves those poms to the next version broke both, and
  osv-scanner printed "0 packages affected by 0 known vulnerabilities" directly
  above its own error. CodeQL now installs the binding into the local
  repository first, which also means the examples are analysed against the
  binding in this tree rather than the last published one. osv-scanner runs with
  `--no-resolve`, which costs nothing here: every non-test dependency in all
  three poms is `org.wickra:wickra` itself, whose own dependencies are
  test-scoped and outside the resolver's graph either way, and lockfiles need no
  resolution to be scanned transitively.

- **Two version declarations survived a bump.** A `package-lock.json` states the
  root package's version twice — at the top of the file and inside
  `packages[""]` — and only the first is what a bump rewrites, so
  `scripts/check_version_sync.py` failed this release's own pull request.
  `CITATION.cff` was worse: its `date-released` moved to the day of the bump
  while its `version` stayed on the previous release, so the two lines
  contradicted each other in the file GitHub's citation box and Zenodo both
  read — and no check here looked at it. The bump tooling now writes both
  declarations, and `check_version_sync.py` holds the citation version too.

### Security

- **Command injection in the C live-Binance example.** `examples/c/live_binance.c`
  took a trading symbol from `argv[1]`, interpolated it into a URL and handed
  that to `popen` as `curl "<url>"`. The quotes look like protection and are
  not: a symbol containing a double quote closes them, and the rest runs as a
  command. `./live_binance 'X" ; echo PWNED ; echo "'` executed `echo PWNED`.

  Found by CodeQL on the first run after C/C++ joined the matrix — the language
  the blueprint had described as one where "a memory mistake is not possible",
  which was wrong twice over. Fixed by validating at the boundary: a Binance
  symbol is uppercase letters and digits, anything else is refused before it
  reaches the URL — and the URL is built from a *copy* the check filled one
  character at a time, not from `argv[1]`. Checking in place and then using the
  original pointer anyway leaves the value that reaches the command the one that
  came from outside, and loses the guarantee to any later edit between the check
  and the use. Verified that the injection no longer executes, that an
  over-long or lowercase symbol is refused, and that a valid symbol still
  fetches.

- **Go standard-library advisories are recorded as not applying, with the
  reasoning.** Turning osv-scanner on surfaced 90 of them at once, all against
  `stdlib 1.23.99` — the scanner reads the `go` directive in a `go.mod` as the
  standard library in use. That is the right reading for an application and the
  wrong one for a library: the directive is a *minimum* language version, and
  whoever imports the module builds it with their own toolchain. Nothing here
  ships a Go binary; CI and the release workflow both build the mirror with
  `stable`.

  Raising the directive would fix the report and force every consumer of the Go
  binding onto that toolchain — a decision about who can use the module, not a
  fix for anything this repository builds. It is left as a decision rather than
  taken quietly.

- **GHSA-6w46-j5rx-g56g (pytest tmpdir handling) is recorded as not affecting
  this project**, with the reasoning in `osv-scanner.toml` rather than left to
  be rediscovered. pytest is a CI-only test dependency and is never shipped, and
  the flaw needs a second local user on the machine, which an ephemeral
  single-user runner does not have. It also cannot be upgraded away: pytest 9
  requires Python 3.10, and `ci-dev-py39.txt` exists to test the 3.9 row. It
  resolves itself when Python 3.9 support ends.

## [1.0.3] - 2026-08-28

### Fixed
- **The npm package shipped every platform binary twice.** `wickra@1.0.2`
  weighed 98.64 MB unpacked across 22 files -- exactly twice the 49.18 MB that
  the six `wickra-<platform>` packages occupy on npm together. The `files` list
  named both `npm` and `*.node`: `napi artifacts` fills `npm/<rid>/` with each
  target's freshly built binary before publish, and a copy of each also sits in
  the package root, so both entries pulled in the full matrix. None of it was
  ever loaded -- `index.js` resolves `./wickra.<rid>.node` first and only falls
  back to the `wickra-<rid>` optional dependency, so the bundled copy always won
  and the platform package npm downloaded alongside it was dead weight.
  Comparable napi-rs projects ship 0.04-0.49 MB. Dropping only `npm` would have
  halved the package rather than fixed it, so both entries are gone; what
  remains is `index.js`, `index.d.ts`, `package.json` and the README. The
  platform packages are unaffected -- they carry their own
  `files: ["wickra.<rid>.node"]` and are published from `npm/<rid>/` on disk.


## [1.0.2] - 2026-08-28

### Fixed
- **`DemarkPivots` had the two conditional branches the wrong way round.** Tom
  DeMark's formulation weights the low on a down bar and the high on an up bar;
  the implementation did the opposite (`C < O` produced `2H + L + C`, `C > O`
  produced `H + 2L + C`). Every mainstream reference agrees on the convention --
  DeMark's *The New Science of Technical Analysis*, TradingView's "Pivot Points
  Standard", Interactive Brokers TWS, WealthCharts -- and `PP`, `R1` and `S1`
  were already derived correctly from `X`, so only the two branches moved. For
  `O=100, H=110, L=90, C=105` the pivot goes from `98.75` to `103.75`. The unit
  tests encoded the reversed branches, so they passed while diverging from the
  published definition; they now assert the convention. Reported in #406.

- **`Cypher` validated the C point against the wrong leg.** The pattern
  (Darren Oglesbee) constrains the X-to-C projection `XC / XA` to roughly
  1.272-1.414. The detector instead constrained `BC / XA` to 1.13-1.414, which
  is a leg no harmonic pattern measures -- every sibling detector in the family
  (`Gartley`, `Bat`, `Butterfly`, `Crab`, `Shark`, `Abcd`) uses `BC / AB`. The
  error ran both ways: a canonical Cypher with `XC/XA = 1.3` was rejected, while
  a shape at `XC/XA = 1.7` was accepted. That false positive was the module's
  own bullish fixture, so the test suite confirmed the bug. Both fixtures have
  been recomputed and a regression test pins the previously accepted shape at
  `0.0`. Reported in #407.

- **`ThreeDrives` detected two drives, not three.** The detector read five
  pivots, which span only two alternating drive legs, and never measured a third
  drive -- so a two-push extension could complete a pattern named for three. It
  now reads seven pivots as six alternating legs (`R1 D1 R2 D2 R3 D3`), requires
  each drive to extend the retracement before it, and checks symmetry across all
  three drives and all three retracements. `warmup_period()` moves from `6` to
  `8` and a five-pivot shape now returns `None` rather than a verdict. Reported
  in #408.

  Golden fixtures for `DemarkPivots` and `ThreeDrives` were regenerated; the
  `Cypher` fixture is unchanged because the shared golden series never produced
  a match under either rule.

## [1.0.1] - 2026-08-27

### Fixed
- **The Python backtest example imported NumPy.** The README promises a data
  layer that needs "no foreign package -- no pandas, no `csv-parse`, ... not even
  NumPy", and `examples/python/live_binance.py` and `multi_timeframe.py` already
  keep that promise; `backtest.py` still used NumPy for two things the standard
  library does as well -- pulling one field out of a multi-output `batch`, and
  taking the mean/min/max of a series. The `Matrix` rows now feed a list
  comprehension, the columns are `array('d')` (which is what a scalar `batch`
  already hands back), and the summary uses `math.isnan` with `sum`/`min`/`max`.
  Output is byte-identical to the NumPy version across all nine series, and
  `numpy` no longer appears in `sys.modules` after the example runs.

- **The per-binding reference benchmark discarded its own results.** Every other
  harness in the repository guards its measurement loop, but
  `examples/rust/src/bin/throughput.rs` -- the FFI-free Rust baseline that all
  nine bindings in BENCHMARKS.md §3 are measured against -- called
  `ind.update(price);` and dropped the value, leaving the optimiser free to
  delete work nothing reads. It has been that way since the benchmarks landed in
  #246. Measured cost of the omission: `SMA(20)` streaming reports 1836 Mupd/s
  unguarded and 1341 guarded, a 27% inflation on the one row every other row is
  compared to. Batch was never affected (505 vs 514) because the result array is
  returned.
- **`bindings/java/benchmarks` did not compile.** Its pom pinned `wickra` 0.9.6,
  three releases behind, so it built against an API where `Atr.batch` took a
  `double[]` of timestamps; the current one takes `long[]`. Against a current jar
  it failed at runtime with `NoSuchMethodError`, and against its own source it
  failed to compile. Nothing caught it because CI does not build that module.
  `Throughput.java` now declares `long[] timestamp`, which is what the binding
  has taken for some time, and the redundant `(long)` cast at the call site is
  gone.

- **`sync-about` reported success while syncing nothing.** Every clone and push
  in the workflow was written as `if ! git <cmd> 2>/dev/null; then echo
  "::warning::…"`, so a failure printed a warning, discarded the reason and let
  the job go green. The v1.0.0 tag run did exactly that: it built the docs
  commit, could not push it, and reported success — `wickra-docs` and the
  webpage stayed on 0.9.9 while the release itself was live on five registries.
  `ABOUT_SYNC_TOKEN` was rotated on 2026-07-01, two days after the last release,
  so the first run that needed write was this one, seven weeks later. All twelve
  guards now let git print why and fail the step: a sync that did not happen is
  not a success. Nothing about the token is asserted in the message any more —
  the clone guards claimed it "likely lacks write", which cannot be the cause of
  a failed clone of a public repository.
- **The 1.0.0 release left `SECURITY.md` contradicting itself.** The version
  strings were bumped, the prose around them was not: the supported-versions
  section read "Wickra is pre-1.0. Security fixes are applied to the latest
  released `1.0.0`", and the support timeline promised a policy revision "after
  the `1.0.0` release" while claiming only `0.y.z` versions are supported.
  `ROADMAP.md` still listed "API stabilization toward 1.0" as a future theme.
  Both now describe 1.0 as shipped and the API as semver-stable.

## [1.0.0] - 2026-08-26

### Security
- **The `rust-cache` action pin claimed a version it no longer points at.** All
  19 uses across `ci.yml`, `release.yml` and `bench.yml` pinned commit
  `e18b497` with the comment `# v2`, but `v2` has since moved to `6323deb`
  and no tag points at the pinned commit any more. A stale comment is worse
  than none: it invites a reviewer to trust a version claim that no longer
  holds. The pin is now `6323deb` with the exact release it corresponds to,
  `# v2.9.2`. Every other pinned action was checked the same way — all 25
  resolve to the commit their comment names.

### Changed
- **Eight hand-written midpoints now call `f64::midpoint`.** Averaging two
  values as `0.5 * (a + b)` is the same arithmetic for any pair under
  `f64::MAX / 2` — both scale by a power of two exactly — so no value moves,
  and regenerating every golden fixture confirms it: not one changed.
  `midpoint` says what the line means and satisfies the lint a newer
  toolchain raises.
- **`wickra_core::Error` and `wickra_data::Error` are now `#[non_exhaustive]`.**
  The core enum has already grown from 4 to 11 variants, and the data enum's
  variant set is feature-dependent (`live-binance`), so a downstream exhaustive
  `match` was already fragile. Adding a variant is now a minor-version change
  rather than a breaking one. Downstream code that matched every variant needs a
  wildcard arm. The Python binding's `map_err` mapped all 11 variants to the same
  `ValueError`, so the match was dropped entirely rather than given a dead
  catch-all; the raised exception and message are unchanged.

### Fixed
- **Nothing checked the R binding against the C ABI it actually links.** Every
  other binding ships its native half in the same artifact as its wrapper, so the
  two cannot disagree. R is the exception: `bindings/r/configure` downloads a
  prebuilt `wickra-c-<triple>.tar.gz` from the release named by
  `DESCRIPTION: Version` and compiles the generated `src/wickra.c` against it,
  while the R CI job sets `WICKRA_INCLUDE_DIR`/`WICKRA_LIB_DIR` and builds
  against the header in the tree, which match by construction. r-universe
  compiles the pairing CI never sees, and went red for it: 177 exports the
  wrapper calls were added to the C ABI after v0.9.9 and `wickra_resampler_new`
  gained a second parameter, so the source build failed with 354 compile errors
  that all had one cause. `scripts/check_r_abi_skew.py` now checks the generated
  wrapper against both headers. A symbol absent from the header in the tree means
  the wrapper is stale, which is a defect and fails; a symbol absent from the
  released ABI means main is ahead of the last release and r-universe stays red
  until the next one, which is a release-readiness signal and warns. A version
  with no release at all is a release in flight, where the tag publishes wrapper
  and ABI together, and passes.
- **Nothing compared the bindings to each other.** Each is generated or written
  separately and tested separately, so a method that went missing in one of them
  failed nowhere — which is how the WASM binding shipped 73 classes without
  `isReady`/`warmupPeriod` and the Node loader shipped 518 exports resolving to
  `undefined`. `scripts/check_binding_surface.py` now reads the method symbols
  out of the C ABI header, the one artefact every binding consumes, derives what
  each of the 514 indicators must expose, and holds C, Go, C#, Java, Node,
  Python and R to it in each language's own spelling. The check is two-sided: a
  bar builder that grew an `isReady` fails as loudly as an indicator that lost
  one. WASM's surface is a build artefact rather than a file in the tree, so its
  `completeness.test.js` asserts the same contract from the same manifest at
  runtime, replacing a hand-maintained class count. It runs in CI as a new
  `binding-surface` job.
- **`batch` is exercised across the catalogue in C#, Java and R.** Each had a
  `BatchShapes`-style test covering one indicator per awkward input shape, which
  is what it was for, and nothing that drove the whole catalogue through the
  batch path — so a batch that disagreed with streaming only had to avoid those
  few to pass. All three now replay the whole catalogue through it against the
  same fixtures the streaming pass uses. Verified by feeding a scalar batch the
  open instead of the close and watching each suite fail.
- **The R vignette promised an O(1) update for every indicator.** The same
  unqualified claim corrected elsewhere, in the getting-started vignette that
  ships to CRAN and r-universe. It now says what the code promises: a step
  whose cost does not grow with the history behind it.
- **One indicator was never held to the property contract.** `vwap.rs`
  defines two indicators and only one of them was listed in the invariants
  suite, so `RollingVwap` alone went unchecked for warmup, readiness, reset
  and non-finite handling — 513 of the 514 were covered and nothing said so.
  It is covered now, and a guard reads the catalogue back against the suite
  so the next indicator cannot be added without one.
- **Four indicators are compared at 1e-9 rather than 1e-12 in the R golden
  suite.** Every other binding parses the golden input with a correctly
  rounded decimal parser and reproduces the fixture bit for bit; R parses it
  with its own, which differs by a last bit on aarch64. `Adl`,
  `ChaikinOscillator`, `EffectiveSpread` and `IntradayIntensity` amplify that
  bit because each subtracts nearly equal quantities, reaching 1.0e-11 at
  worst. The library is unaffected — its streaming and batch paths report
  byte-identical values for these four, so `batch == streaming` remains exact;
  only the comparison against a Rust-parsed fixture moves.
- **The R `multi_timeframe` example drove the resampler with `update()`.**
  The seven other languages were repaired when the resampler moved to
  `push`/`drain`; R's was missed, and the only thing that runs it is a CI step
  that had never been reached because the job failed earlier. It now prints the
  same numbers as the other seven.
- **An R example still drove the resampler with `update()`.** The resampler
  moved to `push`/`drain`, which removed the `wk_resampler_update` symbol, but
  the example under `flush()` was never updated — so anyone copying it hit
  "not available for .Call()", and `R CMD check` failed on it. All 532 manual
  pages now run their examples clean.
- **Two R manual pages documented a signature that had changed.**
  `Resampler.Rd` still showed `Resampler(timeframe)` after the resampler gained
  `gap_fill`, and `push.Rd` was equally behind. The R job runs `R CMD check`
  precisely to catch that class of drift — a stale page shipped to r-universe
  once before — so this was failing there. Regenerated, along with the R
  package description, which called every indicator an "O(1) streaming state
  machine": the same unqualified claim corrected elsewhere, and the one visible
  on CRAN and r-universe.
- **R had no `batch` for 39 of the 514.** The cross-section, order-book,
  profile, bar-builder and footprint families take a per-bar snapshot or emit a
  variable number of rows, and no shim was ever generated for them, while the
  other seven bindings had one for every indicator. All four shapes are
  generated now. A snapshot arrives as one flat column per field — bar `i` at
  `[i*width, (i+1)*width)` — with the width passed by name (`members`,
  `n_bids`/`n_asks`), the same shape the Java and C# batches take. A profile
  comes back as an `n x width` matrix, the width read from the handle rather
  than asked of the caller. A bar builder's batch reports what the whole series
  completed and drains it, so the result is as long as the data makes it rather
  than one row per input. `batch()` gained the one rule that makes this
  expressible: a named argument is a per-bar width, an unnamed one is a column,
  and with no width given the guard against a short column being read past its
  end is unchanged.
- **The catalogue-wide binding suites now check the contract, not just the
  values.** Go, WASM, C#, Java and R replay all 514 indicators through `update`
  and compare against the Rust fixtures, and stop there — a `reset` that forgot
  a field, or an `isReady` keyed off a value that happens to move at the right
  moment, replayed perfectly clean. Each of the five now also asserts that a
  fresh indicator is not ready with a warmup of at least one, that a fully
  driven one is ready whenever its fixture holds a value, and that a second pass
  after `reset` reproduces the first exactly. Node was not in the audit's list
  and had the same gap, so it gets the same pass. In Go and C# the archetype
  dispatch was lifted into a shared per-indicator driver so the two passes
  cannot drift; the WASM and Node suites had each carried a duplicated harness
  and now drive from a single one.
- **Every shipped example that resamples was broken, in eight languages.** When
  `Resampler::update` became `push` returning the candles a bar closed, and grew
  a `gap_fill` argument, no example followed. The C, C# and Go examples plus
  `examples/c/data_layer_test.c` no longer compiled; the Java one did not
  compile either; and the Node, WASM and Python `multi_timeframe` examples
  treated the returned list as a single candle, which crashed at runtime or
  silently aggregated nothing. All eight now agree to the digit with
  `cargo run -p wickra-examples --bin multi_timeframe`.
- **Four WASM demos tested a warmup value with `!== null`.** A WASM `update`
  returns `undefined` before warmup completes, so the guard passed immediately
  and the code went on to read fields off nothing. They compare with `!= null`
  now, which covers both spellings.
- **`backtest.py`, `multi_timeframe.py` and `parallel_assets.py` raised
  `TypeError` on their first summary line.** They index the result of `batch`
  as a NumPy array, which it stopped being when the NumPy dependency was
  dropped: a scalar batch returns `array.array('d')` and a multi-output batch
  returns a `Matrix`. They convert explicitly now.
- **`Matrix`'s docstring promised NumPy interop it does not have.** It claimed
  `numpy.asarray(result)` rebuilds an `(nrows, ncols)` array; the result is
  actually a 0-d object array, which is why the test suite detours through
  `.tolist()`. Exposing the block through the buffer protocol would make the
  claim true, but `Py_buffer` is outside the limited API the `abi3` wheels are
  built against. The docstring now gives the incantation that works and says
  why.
- **A zero `max_reconnect_attempts` could panic the Binance stream task.**
  `BinanceConfig::max_reconnect_attempts` is a public field documented as
  "must be >= 1", but nothing enforced it: `connect_with_config` validated only
  the symbol list. With zero attempts the reconnect loop body never runs, so
  there is no last error to surface and the final `expect` fired on the first
  dropped connection. The value is now rejected at the only entry point that
  accepts a configuration, which makes that `expect` unreachable by
  construction.
- **`Trix::is_ready()` reported ready one input early.** Readiness was keyed off
  `prev_tr`, but the bar that first fills it is the rate-of-change baseline and
  still returns `None`, so `is_ready()` flipped to `true` one input before the
  first value. It now tracks the emission itself, which also propagates to every
  binding's `isReady`/`is_ready` and to `Chain::is_ready`. `warmup_period()`
  (`3 * period - 1`) was already correct and is unchanged.
- **Rolling variance lost accuracy at realistic price levels.** The dispersion
  family computed its variance as `E[x²] - E[x]²` over raw price levels and
  clamped the result at zero. That form cancels catastrophically once the values
  are large relative to their spread, which is exactly the shape of a price
  series, and the clamp hid the failure instead of exposing it. Measured against
  a two-pass reference over a 20-bar window: at a level of 1e5 the relative error
  was 4.3e-06, at 1e5 with a 0.01 range it reached 9.4e-02, and at 1e8 the result
  collapsed to exactly zero — a permanent squeeze reading, and a z-score dividing
  by zero dispersion. The moments are now accumulated relative to a reference
  point taken from inside the window, re-anchored once per window, which brings
  the error to the 1e-16 floor at every level while keeping the same O(1)
  per-tick cost. Migrated in this release: `StdDev`, `Variance`, `ZScore`,
  `CoefficientOfVariation`, `BollingerBands` (both the streaming path and the
  vectorized `batch_bands` fast path, which stay bit-identical to each other),
  `Skewness`, `Kurtosis`, `RviVolatility`, `SpreadBollingerBands`,
  `FundingRateZScore`, and the Bessel-corrected family — `HistoricalVolatility`,
  `InformationRatio`, `JumpIndicator`, `KaseDevStop`, `M2Measure`, `RegimeLabel`,
  `SharpeRatio`, `VolatilityCone`, `VolatilityOfVolatility` and `YangZhang`.
  A follow-up sweep found the claim that no indicator still used that form to
  be wrong: it had been checked with a pattern that only matched a bare
  `mean * mean`, and missed every suffixed identifier. See the covariance
  entry below.
  The golden fixtures in `testdata/golden/` that these indicators feed were
  regenerated: 21 files move in their trailing digits, and every changed value
  is closer to a two-pass reference than the one it replaces. The shape statistics were affected worse still:
  they reconstruct the third and fourth central moments from raw power sums,
  whose terms are of order `level⁴` while the result is of order `spread⁴`.
- **Covariance and correlation had the same cancellation as the variance.**
  `E[xy] − E[x]E[y]` on raw levels fails exactly the way `E[x²] − E[x]²` does,
  and correlation inherits it through the covariance and both variances.
  Measured on `PearsonCorrelation` over a 20-bar window against a two-pass
  reference: 1.1e-05 relative error at a price level of 1e5, 9.6e-02 at 1e5 with
  a 0.01 spread, and a complete collapse at 1e8 — the same magnitudes the
  variance showed. A paired accumulator now centres both channels on their own
  reference point; the error drops to the 1e-16 floor. `PearsonCorrelation` and
  `RollingCorrelation` are migrated, and with them the pairwise regression
  family: `Alpha`, `Beta`, `BetaNeutralSpread`, `Cointegration`,
  `PairwiseBeta` and `TreynorRatio`. Exposure varied and was checked per
  indicator: `Cointegration` takes price levels and `BetaNeutralSpread` a
  price pair, so both were fully exposed, while `Alpha`, `TreynorRatio` and
  `PairwiseBeta` accumulate returns and were barely affected. `Beta` takes a
  generic pair of series, so a caller feeding prices was exposed; it now
  measures exactly zero deviation from a two-pass reference at a price level
  of 1e5. Golden fixtures move in the last digits only (worst 1.3e-09).
- **`PairSpreadZScore` was the worst case of the same defect, and its own
  purpose was what exposed it.** It regresses `ln(a)` on `ln(b)` and z-scores
  the residual, so the quantity it measures — the spread between two
  cointegrated legs — is by construction tiny next to the log-levels it is
  derived from, and both of its accumulators were raw power sums. Two legs
  priced at 1e5 with a 0.01% spread, the regime a statistical-arbitrage signal
  is built for, measured a relative error of 66 against a two-pass reference:
  the magnitude was meaningless and the sign was not reliable either. At 1e8 it
  reached 125. Centring the regression on a shifted pair accumulator and the
  spread window on a shifted scalar one takes the same two cases to 3.7e-11 and
  6.0e-11; the residual is the z-score's own division by a small dispersion,
  not the accumulator. The golden fixture moves in 42 of its 80 cells, all
  toward the reference: its worst deviation from an independent two-pass
  computation falls from 4.7e-09 to 1.5e-13.
- **Recomputing a statistic from scratch does not fix the cancellation.**
  Three indicators rebuilt their regression from the live window on every
  update rather than carrying running sums, and were still doing it in the
  one-pass form. Recomputing bounds the *drift*; it does nothing at all about
  `E[xy] - E[x]E[y]`. `SpreadAr1Coefficient` and `OuHalfLife` were the exposed
  pair: a cointegrated spread sits at a large constant offset with a small
  wobble on top, which is the worst possible ratio for that expression. At a
  spread of 5000 wobbling by 0.1 the half-life deviated 2.8e-06 from a two-pass
  reference, and 2.6e-04 once the wobble tightened to 0.01.
  `LeadLagCrossCorrelation` computed a correlation at every candidate lag the
  same way and reached 2.2e-05 on a quantity bounded to [-1, 1]. All three now
  make a second pass about the window means and match the reference exactly.
  The two spread regressions also stop allocating a `Vec` per update, since the
  pairs are now produced lazily and traversed twice instead of collected.
- **Two more pairwise accumulators are centred.** `RollingCovariance` and
  `KylesLambda` carried running sums that were never rebuilt, so they had both
  the one-pass cancellation and unbounded drift. `KylesLambda` is the less
  obvious of the two: signed volume is only centred on zero while order flow is
  balanced, and a persistent one-sided imbalance moves its mean far from zero.
  At a trade size around 1e8 with a 99% buy imbalance it measured 2.9e-09
  against a two-pass reference, now 2.1e-12; `RollingCovariance` goes from
  1.5e-11 to 3.3e-12.
  `SpearmanCorrelation` was checked and deliberately left alone: it correlates
  ranks, which are bounded by the window length, so the ratio that drives the
  cancellation is O(1) by construction. Measured at 1.4e-14 and flat across
  price levels from 1e2 to 1e8.
- **The whole linear-regression family was fitting on raw power sums.** Twelve
  indicators built a least-squares fit of a window against its own index from
  `Σy`, `Σxy` and `Σy²` of the raw prices. The slope is mathematically invariant
  when a constant is subtracted from `y`, so this was avoidable throughout.
  Scored against an *exact rational* computation of the same fit over 301
  windows at a price level of 1e8 with a one-unit wobble:

  | | before | after |
  |---|---|---|
  | `LinRegSlope`, `LinRegAngle` | 5.1e-04 | 1.0e-16 |
  | `RSquared` | 5.5e+04 | 1.1e-14 |
  | `StandardError` | 1.0 | 8.7e-15 |
  | `DetrendedStdDev` | 1.0 | 8.8e-15 |
  | `LinearRegression`, `LinRegIntercept`, `Tsf` | 2.2e-14 | 1.4e-16 |

  `RSquared` was off by four orders of magnitude on a value defined to lie in
  `[0, 1]`; only the clamp kept it in range. `StandardError` and
  `DetrendedStdDev` are worse than they look at a relative error of exactly 1:
  they reconstruct the residual sum of squares by subtracting the explained
  variation from a collapsed total, the subtraction clamped at zero, and both
  reported a *perfect* fit for a series they had not fitted at all.

  The seven incremental ones now share a `ShiftedTrend` accumulator that holds
  its sums relative to a reference point drawn from inside the window and
  rebuilds once per window; the reference point is added back only where an
  absolute price level is returned. `ProjectionBands` recomputes per bar and
  centres its slope directly. `Cfo`, `Inertia`, `ProjectionOscillator` and
  `TsfOscillator` inherit the fix through the indicators they wrap.
- **`StandardErrorBands` and `LinRegChannel` formed their residuals at the price
  level.** Both computed `y − (intercept + slope·i)` with each side the size of
  the price, which throws away eight digits of a residual at a level of 1e8
  before it is ever squared. Both now fit and take residuals on deviations from
  the window mean, restoring it only for the band levels. The middle line
  improves from 1.2e-15 to 1.4e-16 against the exact reference; the band levels
  themselves cannot improve further, because they are returned as absolute
  prices and a half-width of 0.7 on a price of 1e8 is quantised at 1.5e-08 by
  the output representation alone. `TtmSqueeze` was checked and needs no change:
  it already regresses a detrended series.
- **A catalogue-wide sweep found three more, two of them silently reporting
  nothing at all.** The earlier sweep for this pattern matched only a bare
  `mean * mean` and missed every other spelling, so it was redone against the
  shapes that actually occur -- suffixed identifiers, `n * mean_y * mean_y`,
  cross-sum products, and `mul_add`, which is what had been hiding
  `TrendStrengthIndex`. At a price level of 1e8 with a one-unit wobble, against
  a centred reference:

  | | before | after |
  |---|---|---|
  | `TrendStrengthIndex` | 1.0 | 2.4e-14 |
  | `CorrelationTrendIndicator` | 1.0 | 1.2e-14 |
  | `VwapStdDevBands` (deviation) | 32.2 | 7.6e-14 |

  A relative error of exactly 1 is not a rounding problem: both correlations
  collapsed past their own zero guard and returned 0 for a clean sine wave,
  reporting no trend and no correlation whatever the data did. `VwapStdDevBands`
  reported a band width 32 times too wide, and it accumulates over a whole
  session rather than a window, so nothing bounded it. All three now hold their
  moments on the scale of the deviation -- the two correlations about the window
  mean, the session VWAP about a reference price seeded from its first bar.

  The sweep also confirmed what does *not* need changing, so it does not get
  revisited: `SpearmanCorrelation` correlates ranks,
  `AutocorrelationPeriodogram` a roofing-filtered series that oscillates about
  zero, `HurstExponent` a log-log fit, and `DepthSlope` distances from the mid.
  The index-based OLS denominators (`n·Σx² − (Σx)²` over `0..period`) are exact
  arithmetic on small integers.
- **`StandardError` and `DetrendedStdDev` no longer rebuild the residual sum of
  squares by subtraction, and are now O(period) rather than O(1).** Both wrote
  it as `Σ(y − ȳ)² − slope²·S_xx`, which slides in constant time but converges
  to a difference of two nearly equal numbers as the fit improves — so it
  cancels precisely when the answer is smallest, which is the case these
  indicators exist to report. On a straight line carrying a small wobble,
  scored against exact rational arithmetic:

  | | before | after |
  |---|---|---|
  | wobble 1e-4 on a price of 100 | 6.2e-08 | 5.5e-14 |
  | wobble 1e-8 on a price of 100 | 2.148 | 7.4e-10 |
  | wobble 1e-4 on a price of 1e8 | 3.4e-02 | 7.2e-11 |
  | one-unit wobble on 1e8, r² 0.88 | 8.7e-15 | 3.1e-16 |

  A relative error of 2.148 means the reported spread had no relationship to the
  data at all. The residuals are now summed directly over the window the
  indicator already holds, which is what the siblings `StandardErrorBands` and
  `LinRegChannel` have always done — `LinRegChannel` even documents why.

  The residuals are anchored on a value taken from inside the window rather than
  on the window mean. Centring on the mean is the obvious choice and is measured
  as the worse one: the mean is a computed quantity carrying rounding at the
  scale of the price, about 1e-08 at a level of 1e8, and every residual inherits
  it. That alone put the last row of the table at 2.3e-14, worse than the
  constant-time form it replaced; anchoring on a stored input, which is an exact
  subtraction whenever the two share an exponent, brings it to 3.1e-16.

  The cost is real and is stated rather than implied. At period 20 throughput
  falls from an O(1) 126 Mupd/s to 27 Mupd/s, and at period 200 to 2.7 Mupd/s;
  both now sit alongside `StandardErrorBands` (25 and 2.9), which was always
  O(period). Correctness in the indicator's own best case is worth that.
- **The "bit-for-bit parity" claim was not true, and is corrected rather than
  quietly dropped.** All seven golden runners (C#, Go, Java, Node, WASM, Python,
  R) compare with a relative tolerance of `1e-6`; none has ever compared by bit
  equality, while `README.md` asserted bit-for-bit parity in four places and the
  npm README in four more.

  The diagnosis came from a fixture that stopped reproducing: regenerating
  `g_LinRegAngle.csv` on an untouched checkout rewrote one value by exactly one
  ulp. The cause is `f64::atan`, which no mainstream libm rounds correctly --
  scored against 200-bit arithmetic over the golden input, Rust's
  `atan(x).to_degrees()` differs from the correctly rounded result on **24 of 67
  bars**. That last bit belongs to the machine's math library, not to Wickra, so
  bit equality is not achievable for the indicators that reach one.

  Classifying the catalogue by whether it can reach a transcendental (`ln`,
  `sin`, `cos`, `atan`, `exp` and the rest), directly or through an indicator it
  composes: **461 of the 514 use only IEEE-754 arithmetic** and are bit-identical
  on any conforming platform; **53 are libm-dependent**. Both READMEs now say
  this instead of overclaiming, and each runner explains in place why its
  comparison is a tolerance -- so the next reader does not "fix" it by
  tightening.
- **The Go golden generator did not reproduce its own output.** Regenerating
  `golden_all_test.go` rewrote four blocks, because the generator emitted a
  one-line `if ok { ... } else { ... }` while the committed file carried the
  expanded form `gofmt` produces. Anyone regenerating it would have committed a
  file that fails a format check. The generator now emits the expanded form and
  regeneration is idempotent.
- **The golden tolerance is now per-indicator, and six orders tighter for the
  461 that do not need slack.** Every runner compared at `1e-6 * max(1, |want|)`,
  ten orders looser than the 1-ulp libm difference it exists for and loose enough
  to hide a real defect -- the `StandardError` cancellation fixed two commits ago
  measured `6.2e-08` at a price of 100 and would have passed untouched.

  `scripts/gen_libm_set.py` derives which indicators can reach a transcendental
  (directly, or through one they compose) and writes
  `testdata/golden/libm_dependent.txt`: 54 of 514. All seven runners read that
  one file rather than each carrying a copy, so it cannot drift from the
  catalogue. Those 54 keep `1e-6`; the other 460 are held to `1e-12`.

  `1e-12` rather than exact equality is deliberate. Exact *does* pass here — the
  C# suite was run with the bound set to `0.0` and all 556 tests passed, which
  confirms the IEEE argument on one platform — but the suites also run on Linux
  and macOS across two architectures, and `1e-12` still leaves twelve orders of
  headroom over the double floor while removing any chance of a red build over a
  last bit somewhere unforeseen.

  The tighter bound immediately earned itself: it failed eight Node tests, all
  of them indicators whose fixtures moved on this branch, against a prebuilt
  `.node` artifact from two months earlier. A stale binding build is exactly the
  kind of thing a `1e-6` comparison cannot see.
- **Seven more indicators stopped allocating on every update.** A10 covered the
  sort-based family; these collect their window into a fresh `Vec` purely to get
  a contiguous slice, which costs a malloc and a free per tick. They now reuse a
  scratch buffer on the struct, the same shape A10 established. Measured over
  300k updates:

  | | before | after |
  |---|---|---|
  | `RollMeasure` | 23.8 | 46.8 Mupd/s |
  | `VarianceRatio` | 4.8 | 7.6 Mupd/s |
  | `Rwi` | 8.7 | 13.6 Mupd/s |
  | `HurstExponent` | 3.2 | 3.7 Mupd/s |
  | `SpreadHurst` | 2.1 | 2.4 Mupd/s |
  | `KendallTau` | 3.8 | 4.1 Mupd/s |
  | `SampleEntropy` | 0.96 | 0.99 Mupd/s |

  The spread is the point: where the per-update work is small the allocation was
  half the cost, and where it is a quadratic template scan (`SampleEntropy`) it
  was noise. Three of them allocated more than once — `SpreadHurst` three times,
  `VarianceRatio` three — and `VarianceRatio`'s third buffer is gone rather than
  pooled, since its overlapping q-step changes are only ever summed. `Rwi` needed
  no buffer for its candles at all: it indexes them at single positions, which a
  `VecDeque` does directly.

  Every golden fixture is byte-identical afterwards, which is the point of a
  change that is meant to be free.
- **`GrangerCausality` was allocating about a hundred times per update; now it
  allocates none.** It is the slowest indicator in the catalogue, and the reason
  was structural rather than arithmetic: two channel copies, three outer
  vectors, a freshly allocated design-matrix *row* for each of the `period − lag`
  observations in each of two models, a `Vec<Vec<f64>>` normal-equation matrix
  built inside `ols_rss` twice, and a pivot row cloned per column inside
  `solve`. At `period = 40, lag = 2` that is roughly 103 allocations for one
  bar.

  Both design matrices are now flat and row-major behind a stride, `ols_rss` and
  `solve` take slices and a caller-supplied workspace, and `solve` reaches its
  pivot row through `split_at_mut` instead of cloning it. The channels are read
  at single positions, which a `VecDeque` indexes directly, so the two copies
  are gone entirely. Throughput goes from 0.24 to 1.09 Mupd/s at that
  configuration — about four and a half times — and 0.54 Mupd/s at
  `period = 60, lag = 3`.

  The golden fixture is byte-identical, which is the only acceptable outcome:
  every floating-point operation happens in the same order as before, including
  the ascending summation in the back-substitution and the residual pass.

- **Nine more rolling sums now shed their accumulated rounding.** `sum += new;
  sum -= old` is O(1) but never forgets a rounding error, so the deviation from
  a from-scratch sum grows without bound. Measured over three million updates by
  comparing a long run against a fresh instance fed only the trailing window:

  | | before | after |
  |---|---|---|
  | `EaseOfMovement` | 1.3e-13 | 1.9e-16 |
  | `BipowerVariation` | 9.4e-14 | 1.4e-16 |
  | `Mfi` | 2.8e-14 | exact |
  | `Vwma` | 2.1e-14 | exact |
  | `RollingVwap` | 1.3e-14 | exact |
  | `UlcerIndex` | 6.6e-15 | 1.2e-16 |
  | `VolumeWeightedSr` | 4.1e-15 | exact |
  | `HiLoActivator` | 4.1e-15 | 2.7e-16 |
  | `RealizedVolatility` | 4.4e-16 | exact |

  The audit expected fifty files here; there are thirty-five with a sliding
  accumulator, and measuring them first showed only these nine actually drift.
  Sixteen are already exact, and the rest carry an exponential term, which has
  unbounded memory by design — `AdaptiveRsi` differs from a window-only replay by
  0.57, and that is the indicator working, not drifting. Measuring first is what
  separated the two.

  The pairing was the risk: `RealizedVolatility` accumulates `r²` for each `r`
  in its window, `BipowerVariation` the product of each adjacent pair, `Vwma`
  three different projections of one `(close, volume)` window, and
  `VolumeWeightedSr` five across three parallel windows. Each reseed source was
  written from the eviction it mirrors rather than inferred from the field name.
- **BREAKING: the pattern indicators now withhold during warmup instead of
  reporting "no pattern".** 73 indicators declared a warmup of 3 to 6 bars and
  then answered `Some(0.0)` from the very first bar, which conflates *I looked
  and found nothing* with *I cannot look yet*. The trait defines `None` as the
  warming-up answer, so the gate that fires for want of history now returns it.

  Callers that treated a leading `0.0` as "no pattern" will now see `None` there
  instead. The value stream after warmup is unchanged, and `is_ready()` and
  `warmup_period()` finally agree with what `update` does.

  A new `check_warmup` invariant asserts the bound across all 514 indicators. It
  only forbids emitting *earlier* than declared: several indicators declare a
  bound that is right for their intended bar size and merely look late on a
  probe series of another, so the other direction is not an error.

  Six declarations were wrong rather than the emission, and are corrected:

  | | was | is |
  |---|---|---|
  | `Chain` | `first + second` | `max(first,1) + max(second,1) − 1` |
  | `MacdExt` | `slow + signal` | `slow + signal − 1` |
  | `EhlersStochastic` | `period + roofing` | `period + roofing − 1` |
  | `RoofingFilter` | 2 | 1 |
  | `Ichimoku` | 77 | 1 |
  | `Vpin` | `num_buckets` | 1 |
  | `DoubleTopBottom` | 5 | 4 |
  | `LongLine` / `ShortLine` | `period` | `period + 1` |

  The three compositions each double-counted the bar on which the first stage
  emits, which is also the second stage's first input. `Chain` called its sum a
  "conservative upper bound", but this method promises the input count before
  the first value, so over-declaring is as wrong as under-declaring.
  `Ichimoku` really does emit from bar 1 — its components are `Option` fields
  that fill in progressively — and 77 is when the last one arrives, a different
  question. `Vpin` declared a bucket count where an input count belongs: one
  large trade can fill every bucket, so no number of trades guarantees
  readiness.

  68 golden fixtures move, every one of them by withholding a leading run of
  rows and nothing else — checked cell by cell, in both directions, that no
  value outside that prefix changed and that nothing non-zero was withheld.
- **BREAKING: one policy for a non-finite input, and it is `None`.** Half the
  catalogue rejected a NaN or an infinity by returning `None` and half by
  repeating the last computed value; the property test only ever injected
  *before* the first real input, so the divergence was never exercised.

  `None` wins. Repeating the previous value hands a caller a stale number that
  is indistinguishable from a fresh one, which is the failure that costs money,
  and it needs a cached field to do it — so the safer answer is also the one
  that carries less state. The trait now says exactly this: `None` means the
  indicator is warming up *or* the input was rejected, and a rejected input is
  skipped rather than absorbed.

  88 indicators changed. Two of them were doing something else again, and only
  the extended test found them:

  * `WavePm` had no non-finite guard at all, so a single NaN entered its window
    and poisoned every value after it.
  * `PairwiseBeta` cleared its previous price on a bad tick to "restart the
    return chain", which silently dropped one return and changed the values
    that followed. It now leaves the state alone, so the next good pair
    measures from the last price actually observed.

  `check_scalar_nonfinite` and `check_pairwise_nonfinite` now inject after every
  input as well as before the first, and compare the whole interleaved series
  against a clean run — so "a rejected input changed the values around it" is a
  test failure across all 514 indicators rather than an assumption.

  157 unit assertions pinned the old behaviour and were updated. Fifteen of them
  captured a cached value purely to compare against it; rather than delete those
  bindings they now assert that the accessor is *unchanged* after the rejection,
  which is the half of each test that still means something. No golden fixture
  moves: `gen_golden` feeds only finite data.
- **The three ratio indicators document their unbounded output instead of
  clamping it.** `GainLossRatio`, `ProfitFactor` and `OmegaRatio` return
  `f64::INFINITY` when a window has gains but no losses. That value is correct
  — the ratio genuinely has no denominator — the tests already pinned it, and
  the whole binding stack carries infinity end to end, so clamping would have
  substituted an invented number for a right one.

  What was missing is the practical part. Nothing in the wording told a reader
  that `+inf` is an ordinary occurrence rather than an exotic edge: any
  `period`-bar window without a single down bar produces it, which on a trending
  instrument happens routinely. Each now says so, and says that it propagates —
  `inf - inf` is `NaN` — with `f64::is_finite` named as the guard.

  The audit recorded this as "+inf on a flat market", which measurement did not
  support: a flat window returns `0.0`, not `+inf`. `OmegaRatio` is the
  exception, and only with a negative threshold, where every flat return clears
  the threshold and the same window becomes unbounded — the opposite answer to
  the obvious one. That was undocumented and untested, and is now both.
- **BREAKING: a window with neither gains nor losses now reports `1.0`, not
  `0.0`.** `GainLossRatio`, `ProfitFactor` and `OmegaRatio` answered `0.0` for a
  0/0 window — and `0.0` is also what they return for a window that lost on
  every single bar. Measured, both cases came back identical from all three, so
  a caller could not tell a market that did not move from one that only fell.
  Those are opposite states.

  `1.0` is the value these indicators already give for break-even, and a window
  that neither gained nor lost is break-even. The collision that remains is
  with a state of the same meaning rather than with its opposite. `NaN` was the
  other candidate and was rejected: it is honest about being undefined but
  poisons every arithmetic operation downstream, which the non-finite policy
  settled one commit earlier.

  The old tests asserted the flat-window value on its own, which is why they
  could never see the collision. Each indicator now also has a test asserting
  that the flat and all-losing windows *differ*, which is the property that
  actually matters.

  No golden fixture moves — the golden candles never produce a flat window,
  which is precisely why this survived as long as it did.
- **The session family says that its UTC offset does not follow daylight
  saving.** `utc_offset_minutes` is a constant shift, which is correct
  arithmetic and a real limitation that appeared nowhere in the crate: for a
  venue that observes a transition, one offset is right for part of the year and
  an hour out for the rest, and an hour moves every session boundary. For
  indicators whose whole job is bucketing bars by session, an hour of bars lands
  in the wrong bucket for roughly eight months of a U.S. calendar year.

  The `calendar` module now states the model and the two ways to be correct
  about it — analyse spans that do not cross a transition with the offset in
  force, or convert timestamps to the venue's wall clock upstream and pass `0` —
  and says plainly why Wickra does not ship a timezone database. The same
  guidance is on all twelve affected constructors, inline rather than as a link,
  because `calendar` is private and a reader cannot follow a link into it.
- **Every rustdoc warning in `wickra-core` is gone: 33 to 0.** 21 predate this
  work and are dead links in published documentation, which docs.rs has been
  rendering as plain text. Fourteen pointed at `pub(crate)` constants
  (`SWING_THRESHOLD`, `LEVEL_TOLERANCE`, `ShiftedMoments`) and now name them
  without linking; the rest referenced types under names they no longer have —
  `Cmf` is `ChaikinMoneyFlow`, `OIDelta` is `OpenInterestDelta` — or needed a
  path, as `Hma` and the two `Error` variants in `MacdHistogram` did.
- **BREAKING: `Resampler` gained gap filling, and `push` now returns every
  candle that closed.** `TickAggregator` could emit a flat placeholder for each
  skipped bucket; its sibling could not, so the same hole in the same feed
  produced an evenly spaced series one way and a series with time holes the
  other. `Resampler::with_gap_fill` and `fills_gaps` now mirror it exactly —
  same placeholder price, same zero volume, same cap on a runaway timestamp
  jump — because the filling itself was lifted out of `TickAggregator` into one
  shared function rather than copied.

  With filling on, one input candle can close a bar *and* emit several
  placeholders, so `push` returns `Vec<Candle>` where it returned
  `Option<Candle>`. That is also what `TickAggregator::push` already returned.
  Every binding follows: the C ABI grows `wickra_resampler_push` /
  `wickra_resampler_drain` in place of `wickra_resampler_update`, and
  `wickra_resampler_new` takes the `gap_fill` flag, exactly as the aggregator's
  entry points do.

  A test pins that the two agree on the same gap rather than merely looking
  alike.
- **The R generator emitted a miscalled push.** `gen_r.py` hardcoded
  `(price, size, timestamp)` for any push/drain pair, which was right while the
  tick aggregator was the only one. Adding a second exposed it: the generated
  glue called `wickra_resampler_push` with four arguments instead of seven and
  registered the wrong arity with R. The parameters now come from the parsed
  header, as the C#, Go and Java generators already did.
- **The validated types are `#[non_exhaustive]`, and now say what their
  guarantee is worth.** `Candle`, `Tick`, `Trade`, `TradeQuote`, `Level`,
  `OrderBook` and `DerivativesTick` could be built from a field literal, which
  skips the validation in `new` entirely — and 259 of the 261 candle-consuming
  indicators rely on that validation instead of re-checking each bar. Code
  outside the crate must now go through `new`, or through `new_unchecked` as an
  explicit opt-out.

  The fields stay public. Private fields with accessors would close the
  remaining hole — a validated value can still be *written* into an invalid
  state — but at the cost of turning roughly 2200 field reads in this repository
  alone into method calls, across every binding, to prevent a failure the caller
  has to construct deliberately. The documentation says plainly what is
  guaranteed and where the guarantee stops, rather than implying an invariant
  the type does not enforce.
- **The R binding's documentation survives a regeneration.**
  `bindings/r/R/indicators.R` carries a DO-NOT-EDIT header and had been edited
  anyway: seven blocks of roxygen — the constructors for `CandleReader`,
  `Resampler` and `TickAggregator`, and the parameter and return documentation
  for the four Binance entry points — existed only in the generated file, where
  the generator emitted a one-line stub. That documentation ships to CRAN and
  r-universe, so the next regeneration would have deleted it silently.

  The blocks now live in the generator, keyed by function name, together with
  the one signature default that had been hand-applied. Regenerating reproduces
  the committed file byte for byte, which is the only condition under which the
  header is true. The header also names where documentation belongs now, since
  saying "do not edit" without saying where to edit instead is how this
  happened.
- **Node: a number reaching a period parameter is checked instead of
  reinterpreted.** Every numeric parameter was declared `u32`, and napi converts
  a JS number to `u32` the way a bitwise operator would: `new SMA(-1)` produced
  a period of 4294967295, `new SMA(1.5)` silently became 1, and `new SMA(1e10)`
  wrapped to its low 32 bits. All three reached the Rust constructor as a
  plausible period, so the validation there had nothing to reject and the caller
  got an indicator configured for something they never asked for.

  A `Count` wrapper now takes the value as `f64` — which is what a JS number is
  — and refuses anything that is not a whole, non-negative, exactly
  representable integer, naming what was passed. 350 parameters across 115 names
  go through it. The domain rules stay where they were, in the Rust
  constructors.

  Three things deliberately kept their old type, because the blanket change was
  wrong for them: the moving-average `matype` arguments are *codes* that
  `MaType::from_code` range-checks itself, `RunBarValue.length` is an output
  field leaving for JavaScript, and the `-> u32` returns such as
  `warmupPeriod()` are values on the way out, where nothing wraps. Where a core
  constructor genuinely takes a `u32` — `Jma`'s power, `TurnOfMonth`'s bounds —
  the narrowing is checked rather than cast.
- **Go: a negative count is refused by name.** A Go `int` reaching a
  `uintptr_t` parameter wraps, so `NewSma(-1)` passed 2^64-1 to the C ABI. That
  value is rejected downstream — but only because it exceeds `MAX_PERIOD`
  (2^24), two magnitudes that happen to sit that way round with nothing stating
  the relationship, and the caller got "invalid indicator parameters" with no
  hint that their number was negative. Constructors now check each unsigned
  parameter and say which one it was.

  The audit recorded this as "aborts the process", which measurement no longer
  supports: the `capacity overflow` panic it referred to became a clean `Err`
  when `MAX_PERIOD` was added earlier in this cycle. What was left is the
  wrapping itself and the unhelpful error.
- **The Go generator now emits gofmt-clean output.** `indicators_gen.go` had
  been formatted after generation, so a regeneration produced a file CI's gofmt
  check would reject — which is exactly what happened when the resampler change
  regenerated it. Struct fields, the wrapper's `handle` field, the import order
  and the trailing newline are all emitted the way gofmt wants them, so
  regenerating is idempotent.
- **The Go binding's copy of the C header was stale.** `bindings/go/include/`
  carries a committed copy that CI diffs against `bindings/c/include/`, and the
  resampler change updated only the latter. Both are in sync again.
- **Half of what the Node package exported was `undefined`.** NAPI-RS emits a
  TypeScript alias for the Rust name of any class that sets a `js_name`
  (`export type SmaNode = SMA`), and then re-exports that alias as a runtime
  value as well. The native module only registers the `js_name`, so 518 of the
  package's 1038 exports resolved to `undefined`. They are pruned after the
  build, driven off the type aliases the generator itself declares; the names
  remain importable as types. The package now exports 520 names, all defined.
- **The generated loader was two releases stale.** `index.js` is a committed,
  published artifact, and it pinned `0.9.7` in all 26 of its per-platform
  version checks — so `NAPI_RS_ENFORCE_VERSION_CHECK` would have rejected a
  correctly matched platform package. Regenerated, and a test now compares the
  pinned version against `package.json`.
- **The Node type definitions declare `Count`.** Validating the integer
  parameters gave them a Rust newtype, which the generator writes into
  `index.d.ts` verbatim — 435 references to a type nothing declared, which
  would have broken every TypeScript consumer as soon as the loader was
  regenerated. It is declared as `number`, with the boundary check documented.
- **The Node completeness test asserts the catalogue exactly.** It accepted
  `>= 200` classes while claiming 214 in a comment and the real figure was 504,
  so a partial native build that dropped half the catalogue still passed. It
  now asserts 504, that no export is `undefined`, and that the classes in
  `index.d.ts` and in the native module are the same 518.
- **The Python type stubs are generated, and cover the whole package.**
  `__init__.pyi` was hand-written and had reached 72 of the 520 exported names,
  so a type checker rejected almost everything the package exports. It is
  produced from the binding source now, and a test re-runs the generator to
  keep it that way.
- **The stubs no longer describe NumPy.** They annotated batch inputs and
  results as `numpy.typing.NDArray`, which the binding stopped returning when
  the NumPy dependency was dropped: a batch takes any sequence and returns a
  stdlib `array.array("d")`, or a `Matrix` when the indicator has several
  outputs. 133 doc comments still saying "Batch over numpy columns" — the text
  `help()` prints — were corrected with them.
- **`Matrix` is exported.** The result type of every multi-output `batch` was
  described in the package docstring but never registered on the module, so it
  could not be imported, annotated, or used in an `isinstance` check.
- **`KST` was listed twice** in `__all__` and in the import block.
- **713 unreachable error branches removed from the Python binding.** Every
  batch method opened with
  `.as_slice().map_err(|_| PyValueError::new_err(NON_CONTIGUOUS))?`, raising
  "array must be C-contiguous; pass np.ascontiguousarray(arr)". The input is
  copied through the sequence protocol on the way in, so layout never reaches
  Rust and `as_slice` cannot fail — the suite already had a test feeding it a
  strided view and getting the right answer. The advice was stale too, naming a
  dependency the package no longer has. `as_slice` returns a plain slice now,
  which removes 1431 lines.
- **The WASM array returns are typed too.** The bar builders, the data-layer
  candle streams and the Footprint and bucket-profile batches all returned
  `Array<any>`, so nothing described their elements. `RenkoBars.update` now
  reads `{ open: number; close: number; direction: number }[]` and
  `TickAggregator.push` reads the candle shape. `any` is gone from the type
  definitions entirely.
- **WASM multi-output `update` is typed.** It handed back a `JsValue`, which
  the generated `.d.ts` writes as `any`, so a TypeScript caller lost every field
  name and type for 94 of the 517 indicators. `ADX.update` now reads
  `{ plusDi: number; minusDi: number; adx: number } | undefined`, and `any` is
  gone from every method signature the type definitions declare.
- **A warming-up WASM `update` returns `undefined`, not `null`.** The
  multi-output methods returned `null` while every scalar one has always
  returned `undefined`; typing the multi-output returns settles the
  disagreement in favour of the majority. Code testing for `null` specifically
  needs to test for absence instead — `== null`, `?.` and `??` already cover
  both.
- **The Node build config uses the current NAPI-RS field names.** `napi.triples`
  and `napi.name` were both deprecated; `name` in particular falls back to the
  CLI default of `index` once the shim goes, which would rename every published
  `.node` artifact. They are `napi.targets` and `napi.binaryName` now, and the
  generated loader and type definitions are byte-identical across the change.
- **The C ABI gained `batch` for 91 multi-output indicators.** Every scalar
  indicator had a vectorized path and no multi-output one did, so C, C++, Go, C#
  and Java crossed the FFI boundary once per bar for a fifth of the catalogue.
  `out` is an array of the output struct the ABI already defines, one entry per
  input; a row the indicator did not produce — warmup, or an input it rejected —
  has every floating-point field set to `NaN`. Go, C# and Java expose it.
- **28 more C ABI indicators gained `batch`.** The derivatives, trade and
  trade-with-quote handles emit one value per input, so they take the same
  vectorized path the candle indicators always had; `MACDEXT` joins them, having
  been missed only because its constructor takes moving-average type codes and
  it therefore sits outside the generated section. 475 of the 517 handles have a
  batch now.
- **Go reaches every batch.** 39 indicators — the cross-section, order-book,
  bar-builder and profile families — had a vectorized path in the C ABI that the
  Go wrapper could not call, because their per-bar input is itself an array or
  their output length depends on the data. All 514 Go types have a `Batch` now.
- **C# and Java reach every batch too.** The same 39 indicators — the
  cross-section, order-book, bar-builder and profile families — were unreachable
  from those two for the same reason. All three generated bindings now expose a
  batch for every one of the 514 indicators.
- **The O(1) claim is qualified to what is actually true.** The documentation
  said every indicator updates in O(1) per tick, which reads as constant work
  regardless of period — and is false for any indicator that needs a full-window
  pass, as CCI does for its mean absolute deviation and RollingQuantile does for
  its sort. The contract `traits.rs` states is O(1) *in the input length*: a
  tick never triggers a pass over the history behind it, and the cost is bounded
  by the window you configure. Corrected in the README, BENCHMARKS,
  ARCHITECTURE, ROADMAP and all eight binding READMEs.
- **A batch row the indicator did not produce is `NaN` in every field.** It was
  `NaN` only in the floating-point ones, which left two gaps: the single
  multi-output struct carrying an integer (`LeadLagCrossCorrelation`'s lag) read
  back as `0` during warmup, and the profile batches left their scalar fields
  untouched, so those read back as zeros too. Integers surface as `f64` in the C
  output structs now, so the whole row can say "no value".
- **The Go golden suite drives `batch` as well as `update`.** All 514 indicators
  are replayed through both against the same fixtures; the suite had only ever
  exercised the streaming path, which is how the warmup gaps above survived.
- **R gained the multi-output batch.** `batch()` was wired only for the scalar
  indicators, so 158 of the 514 had a native batch R could not call. The
  multi-output ones come back as an `n x k` matrix with the field names as
  columns; 475 of the 514 have a batch now.
- **The R bar builders read past their buffer.** The `update` shim indexed a
  64-element stack array with the number of bars the candle completed rather
  than the number that fit — undefined behaviour in C, where the other bindings
  raised. It drains the surplus now, like they do.
- **Profile indicators report their width.** The C-family bindings sized their
  output buffer by guessing from the constructor parameter names, falling back
  to 4096 elements when none looked like a count — which is what
  `DayOfWeekProfile` got, for a profile that is seven wide. `width()` is part of
  the six profile indicators now and the bindings ask for it.
- **Every indicator in the C ABI has a `batch`.** The bar builders feed the
  whole series and buffer the bars for the drain; the profiles write a flat
  `n * width` block with `NaN` rows during warmup. Of the 517 handles, the only
  three without a vectorized path are the data-layer types, which are not
  indicators.
- **Footprint truncated its price levels.** It reports one level per distinct
  price seen, so any session spanning more than 64 ticks of range overflowed the
  fixed buffer the bindings pass — returning a list sized to the real count with
  zero-valued levels in the tail. It buffers and drains the surplus now, like
  the bar builders.
- **The bar builders dropped bars, and crashed three bindings doing it.** One
  candle can complete any number of bars — a Renko builder with a box size of 1
  turns a 500-point move into 500 bricks — but `update` writes into a
  caller-sized buffer, and every generated binding passed a hard-coded 64 and
  then indexed it with the *returned* count. Go panicked with `index out of
  range [64] with length 64`; C# and Java read past their buffers the same way.
  The surplus is buffered on the handle now and comes out through a `drain`,
  which the Go, C# and Java wrappers call, so a large move returns every bar
  instead of crashing.
- **The cross-section and order-book indicators gained `batch` too.** They take
  an array per bar rather than a scalar, so the batch takes the flat form: the
  member and level arrays cover `n * members` elements with the stride passed
  explicitly. 497 of the 517 C ABI handles now have a vectorized path; the 20
  left are the bar builders, profiles and data-layer types, whose output length
  depends on the data rather than on the input length.
- **`dotnet publish` failed for every Windows app using the NuGet package.**
  The managed assembly is `Wickra.dll` and the native library shipped as
  `wickra.dll`; Windows file names are case-insensitive and a RID-specific
  publish flattens `runtimes/` into the output folder, so the SDK saw two
  outputs claiming one path and stopped with `NETSDK1152`. The Windows native
  asset is packaged as `wickra_native.dll` now, which the resolver looks for
  first.
- **The .NET resolver no longer falls back to unvalidated probing.** When it
  found nothing it returned zero, handing the load back to the runtime's own
  search — which does none of the ABI validation the resolver exists for, so a
  same-named library that exports nothing would be accepted and surface later
  as an `EntryPointNotFoundException` from inside an unrelated call. It throws
  a `DllNotFoundException` naming every location it probed.
- **The Java loader tries every candidate.** It took the first file named like
  the native library and gave up if that one did not export the Wickra ABI, so
  a stale build earlier in the search order hid a good one further up. It now
  skips a candidate that fails the check and reports all of them if none works.
- **68 WASM classes gained `batch`.** 63 indicators had no batch method at all,
  so a browser caller had to loop in JavaScript and pay a boundary crossing per
  bar — the opposite of what the batch API is for — and five of the ten bar
  builders were missing it while the other five had it. Every one of the 514
  indicators now produces the same values through `batch` as through `update`,
  checked against the Rust reference fixtures.
- **The WASM suite covers `batch` and the interface contract.** The golden
  suite only ever called `update`, so a missing or wrong `batch` was invisible;
  there are now a batch-parity replay of all 514 and a completeness test
  mirroring the Node one.
- **CI runs the whole WASM test directory.** It named a single file, so the
  data-layer tests had never run there — which is how a resampler test that had
  been failing since the streaming change went unnoticed.
- **73 WASM classes can be asked whether they are ready.** `isReady` and
  `warmupPeriod` are part of the indicator contract everywhere else, and the
  macro-generated WASM wrappers had them from the start — but the classes
  written out by hand did not, so a caller had no way to tell warmup output
  from a real value for 73 of the 517. Both are delegations to the same core
  object; for every class Node and WASM can both construct without arguments,
  the two now report identical values.
- **The resampler fills gaps in every binding, not just the C ABI.** Gap
  filling arrived with the streaming resampler but was only wired into the C
  entry point, which Go, Java and C# inherit; Node, Python and WASM could not
  reach it. All three take the flag now and report it back through
  `fillsGaps` / `fills_gaps`, the same shape `TickAggregator` already had.
- **Three resampler doc comments described the old return type.** They promised
  "the completed candle on a bucket boundary, otherwise null" while the method
  had become one that returns every candle the push completed — which is more
  than one when gap filling covers skipped buckets.
- **Gap filling in the resampler is covered by a golden fixture.** Nothing
  exercised it from any binding. The shared input candles are contiguous, so
  the generator drops a run of rows to open a five-bucket hole, and the new
  `data_resampled_gap` fixture pins the flat placeholder candles that fill it.














- **`is_ready()` is now checked against its own definition, catalogue-wide.**
  The trait defines it as whether a value has been emitted since the last reset,
  and nothing verified that. Four indicators keyed it off something else and
  reported ready at the wrong moment: `Ichimoku` required every output component
  to be present, long after it began emitting; `ZigZag` keyed off a trend state
  seeded on the first bar, which emits nothing; `LongLine` and `ShortLine`
  required a full window while emitting from bar one. All four now track
  emission. The property suite asserts the contract for all 514 indicators, in
  every input family — including that a fresh instance is not ready, and that
  `reset()` returns it to not ready. Verified to bite by reintroducing the
  `Trix` defect fixed earlier in this release: the check fails on it immediately.
- **Fuzz coverage closed the last two gaps, and gained a chain target.**
  `BollingerBandwidth` and `PercentB` were the only two of the 514 catalogue
  entries no fuzz target reached. A new `indicator_chain` target covers
  `Chain`, which exercises a path no single-indicator target can: the second
  stage consumes a stream produced by the first, which starts later, can be
  constant for long stretches, and for a rate of change can be infinite or NaN
  from finite input. CI now also runs `cargo fuzz build` over every target —
  `fuzz run` builds only the target it names, so eight of the thirteen were
  neither run nor compiled and could drift out of sync with the core API
  unnoticed.
- **Indicator methods are now `#[inline]`, so a downstream crate can inline
  them.** None of the 514 indicator files carried a single `#[inline]`. A
  trait-impl body is not generic, so without link-time optimisation it is not
  available across a crate boundary — and a downstream Rust crate on Cargo's
  default release profile has LTO off. Every tick therefore paid a function
  call into a body that in many cases is a handful of arithmetic operations.
  Measured from a crate built with `lto = false`, over two million updates:
  `Sma(20)` goes from 369 to 1322 Mupd/s, `Atr(14)` from 312 to 342, `Rsi(14)`
  from 292 to 314; `Ema(20)` is unchanged. The no-LTO build now matches what the
  same crate gets with `lto = "fat"`. Applied to `update` where the body is 40
  lines or fewer (438 of 513, the rest being large enough that the hint would
  only export metadata LLVM declines to use anyway) and unconditionally to
  `is_ready`, `warmup_period` and `name`, which are one-liners. The C ABI cdylib
  grows 1.8%.
- **Seven sort-based indicators allocated on every tick.** Each copied its
  window into a freshly allocated `Vec` inside `update` before sorting, while
  seven siblings doing the same work already reused a scratch buffer. Measured
  over a million updates: `MedianMa` 16.2 to 21.3 Mupd/s, `ValueAtRisk` 15.3 to
  19.6, `TailRatio` 14.3 to 19.7 — all now level with `RollingQuantile`, the
  sibling that already had the buffer and which is unchanged as a control.
  `CommonSenseRatio`, `ConditionalValueAtRisk`, `VolatilityCone` and
  `AdaptiveLaguerreFilter` get the same treatment. `MedianMa::value()` used to
  sort on every read; the median is now computed once per `update` and the
  accessor just returns it. No output changes.
- **Comparators unified on `f64::total_cmp`.** Three of the sort-based
  indicators ordered with `partial_cmp` and either swallowed the `None`
  (`unwrap_or(Equal)`) or unwrapped it. No `partial_cmp` remains in indicator
  production code.
- **Rolling sums drifted without bound on long streams.** `sum += new;
  sum -= old` is O(1) but never forgets a rounding error, so an accumulator's
  deviation from a from-scratch sum grows with the length of the stream — the
  case this library is built for. Measured over three million updates against a
  fresh instance fed only the final window: `Cci` was off by 5e-09 relative and
  `Dpo` by 2e-07, while `Sma`, which already rebuilt its sum periodically, was
  exact. A shared `RollingSum` now rebuilds once per window, amortised O(1), and
  twelve indicators use it: `AmihudIlliquidity`, `CalmarRatio`, `Cci`, `Dpo`,
  `FundingRateMean`, `GarmanKlassVolatility`, `GeometricMa`,
  `OrderFlowImbalance`, `ParkinsonVolatility`, `RogersSatchellVolatility`,
  `SortinoRatio` and `Vpin`. `Cci` and `Dpo` now measure exactly zero deviation.
  Affected golden fixtures move in their last digits only (worst 4e-13).
- **`TdLines` and `TdRiskLevel` reported a value when they had none.** Both
  emitted `Some` with two `NAN` fields from the end of warmup onwards, before
  either of their two levels existed — on a series that never completes a TD
  setup, on every bar forever. They were the only two indicators in the
  catalogue encoding "no value" as anything other than `None`, and in the
  bindings it surfaced as NaNs in a flat output buffer that silently poison
  downstream arithmetic. They now return `None` until at least one level exists.
  A single `NAN` field is still how "this one level is not established yet" is
  expressed, because the two levels are set independently and the C ABI mirrors
  each output as two plain `double`s; that is now documented on the fields.
  `warmup_period()` is a lower bound for both, since a completed setup is
  data-dependent.
- **A panic inside a binding killed the host process.** The workspace release
  profile set `panic = "abort"`, and the Python, Node, WASM and C ABI cdylibs are
  workspace members, so they inherited it — Cargo refuses `panic` in a
  per-package profile override, so there was no way to set it for them alone.
  Both pyo3 and napi turn a panic into a language-level exception through
  `catch_unwind`, machinery that can never run under `abort`, so any panic
  reaching an FFI boundary aborted the interpreter instead of raising. The
  profile now unwinds. The cost, measured over 500000 bars: streaming throughput
  is unchanged and the C ABI cdylib grows 34.5%. `Sma::batch_nan` initially lost
  roughly 40%, traced to the `Vec::push` in its hot loop — under unwinding that
  carries drop glue for the output vector. Writing into a pre-sized output
  instead brings it to 513-540 Mupd/s, above the 500 the `abort` build managed.
  `BollingerBands::batch_bands` already wrote into a pre-sized buffer and was
  never affected.
- **An absurd period aborted the process instead of returning an error.**
  Constructors size their buffers from the period, and only `period == 0` was
  rejected: `Ema::new(usize::MAX)` hit a capacity overflow inside `Vec` and
  aborted, while `Ema::new(1_000_000_000)` quietly reserved eight gigabytes.
  Bindings make that easy to reach by accident — a mistyped literal, a period
  read from a config file. Every constructor that takes a window length now
  also rejects anything above the new public `MAX_PERIOD` (`1 << 24`, i.e.
  16777216) with `Error::InvalidPeriod`. A single `f64` buffer of that length is
  128 MiB, far past any real window, and it leaves period arithmetic such as
  `6 * period - 5` nowhere near overflowing. 222 bounds were added across 219
  indicator files. The file-local `MAX_PERIOD` in `DynamicMomentumIndex`, which
  meant something else entirely (its slowest RSI lookback), is renamed
  `MAX_RSI_LOOKBACK`.
- **Java binding: calling any method after `close()` crashed the JVM.** Every
  generated method read the `handle` field directly, so a call made after
  `close()` had already run the cleaner dereferenced freed memory and the JVM
  died with an access violation — exactly what the Panama FFM API is meant to
  make impossible from safe Java. There was no `closed` flag anywhere in the
  binding. Calls now go through a guarded accessor that raises
  `IllegalStateException`, `close()` is idempotent, and every downcall is wrapped
  in `Reference.reachabilityFence(this)` so the cleaner cannot run while a native
  call is still in flight.
- **Java binding: candle timestamps were marshalled as doubles.** The C ABI
  declares them `const int64_t *`, but the generated `batch` methods typed every
  input array as `double[]` and allocated it with `JAVA_DOUBLE`, so the native
  side reinterpreted the IEEE-754 bit pattern as an integer — a millisecond
  epoch of 1700000000000 arrived as roughly 4.8e18. This was inert for
  indicators that ignore the timestamp and silently wrong for every session- or
  calendar-aware one (`SessionVwap`, `TurnOfMonth`, `SeasonalZScore`,
  `OvernightGap`, and 182 others). Array parameter types and layouts are now
  derived from the C declaration, so those 186 methods take `long[]`. In the
  same pass, cross-section flag arrays declared `const bool *` changed from
  `double[]` to `boolean[]`, matching what the C# binding already exposed.
- **C# binding: calling any method on a disposed indicator read freed memory.**
  Every generated method reached the native library through
  `_handle.DangerousGetHandle()`, which keeps returning the pointer after
  `Dispose()` has already freed it; the accompanying `GC.KeepAlive` guarded only
  against the collector, not against an explicit dispose. Using an indicator
  after disposing it — a captured lambda, a cached dictionary, an async
  continuation outliving its `using` block — therefore read freed memory,
  yielding silently wrong numbers or an access violation depending on the heap
  layout. The 2914 affected call sites now pass the `SafeHandle` itself, so the
  `LibraryImport` marshaller ref-counts it for the duration of the call and
  raises `ObjectDisposedException` on a released handle. `DangerousGetHandle` and
  the hand-written `GC.KeepAlive` are gone from the generated binding entirely.
- **R binding: `batch()` with mismatched column lengths crashed R.** The native
  routine takes its row count from the first column and indexes every other
  column with it, so a shorter column was read past its end — a segfault
  reachable from ordinary R code, e.g. passing a three-element timestamp
  alongside full OHLCV vectors. `batch()` now coerces every column with
  `as.double()` (an integer vector such as `1:100` was previously reinterpreted
  as doubles) and rejects a length or type mismatch with a clear message. The
  generated C carries the same guards plus a null-handle check, so a direct
  `.Call` that bypasses the R layer is refused too.
- **`warmup_period()` was one too small for the directional-movement family.**
  `PlusDm`, `MinusDm`, `PlusDi`, `MinusDi` and `Dx` reported `period`, but the
  first candle only seeds the previous bar, so seeding starts on bar 2 and the
  first value lands on bar `period + 1` — which is what each module's own
  documentation already said. A caller slicing `output[warmup_period()..]` kept
  a leading `None` in what it believed was the dense region. They now report
  `period + 1`, and each carries a test pinning the declared warmup against the
  index of the first emitted value so the two cannot drift apart again. `Adx`
  (`2 * period`) and `Adxr` (`3 * period - 1`) were already correct and are
  unchanged.
- **R binding: correct C ABI architecture on Windows on ARM.** `configure.win`
  detected the target with `uname -m`, which reports `x86_64` on the Windows/arm64
  builder (the shell runs under emulation), so an x64 C ABI was linked into an
  aarch64 R and failed to load (`%1 is not a valid Win32 application`). It now
  queries `R.version$arch`, which reflects the R build that loads the library.
  The import-library step no longer dumps the DLL with `objdump` at all: the
  export list is taken from the staged cbindgen header, which declares exactly
  the exported symbols. The `objdump` that resolves on the Windows/arm64 builder
  is the runner image's x86_64 mingw copy, which carries no aarch64 PE backend
  and rejects an arm64 DLL outright. `dlltool` is still needed for the import
  library, so the target machine is now passed explicitly (`-m arm64`) and the
  candidates R can point at (`BINPREF`, `llvm-dlltool`) are tried ahead of the
  bare name — a binutils build without the requested backend fails loudly
  instead of quietly emitting a wrong-architecture archive.

## [0.9.9] - 2026-06-28

Maintenance release. No library or binding code changes from `0.9.8`; this
re-release simply runs through the corrected release workflow (the `0.9.8` npm
publish required a hotfix to the napi-rs 3 `napi artifacts` invocation, now in
place), so the published `0.9.9` artifacts are byte-identical to `0.9.8`.

## [0.9.8] - 2026-06-28

Maintenance release. The library API and every indicator are unchanged from
`0.9.7`; this release migrates the Node.js binding's build to napi-rs 3 and
carries routine dependency and CI tooling updates.

### Changed
- **Node binding built with napi-rs 3.** Migrate `napi` 2.16 → 3.9, `napi-derive`
  2.16 → 3.5 (Rust) and `@napi-rs/cli` 2.18 → 3.7 (npm). The published API and
  every computed value are unchanged — the generated TypeScript surface is
  identical (626 symbols), only the code-generation format differs. napi 3's
  derive macros emit `#[allow(unsafe_code)]`, so the Node crate's `unsafe_code`
  lint is relaxed from the workspace `forbid` to `deny` for that crate only;
  `forbid` stays in force for every other crate. The `engines.node` floor is
  raised to `>= 22` (matching the CI test matrix). `ureq` is intentionally held
  at 2.x — ureq 3 unconditionally pulls `webpki-root-certs` (CDLA-Permissive-2.0),
  which the native-tls / OS-trust-store setup deliberately avoids.
- **Dependency and CI housekeeping.** Bump the Maven
  `central-publishing-maven-plugin` and the GitHub Actions used by CI
  (`actions/checkout`, `taiki-e/install-action`, `actions/setup-java`,
  `softprops/action-gh-release`). No runtime code changes.

## [0.9.7] - 2026-06-21

Maintenance release. The library API and every indicator are unchanged from
`0.9.6`; this release carries an R package metadata fix and routine dependency
and CI tooling updates.

### Fixed
- **R package: credit `kingchenc` as the package author and maintainer.** The R
  `DESCRIPTION` was the only binding still listing "Wickra contributors" as the
  sole `aut`/`cre` — an outlier introduced when the binding was added. It now
  matches the Python, Node, and Rust core metadata, the package `.Rd` is
  regenerated, and the binding `LICENSE` copyright holder is aligned with the
  root `LICENSE-MIT`.

### Changed
- **Dependency and CI housekeeping.** Bump the C# test dependencies
  (`Microsoft.NET.Test.Sdk`, `xunit`, `xunit.runner.visualstudio`), the GitHub
  Actions used by CI (`setup-python`, `upload-artifact`, `codecov-action`,
  `taiki-e/install-action`), and the Java benchmark's `org.wickra:wickra`
  dependency. No runtime code changes.

## [0.9.6] - 2026-06-18

Documentation release for the R binding. The library API and every indicator
are unchanged from `0.9.5`; only the R package's help pages change.

### Fixed
- **R package: document the data-layer exports and refresh the man pages.**
  r-universe's `R CMD check` reported two warnings against the `0.9.3` data layer
  it built for the first time in `0.9.5`: twelve undocumented exported objects
  (`BinanceFeed`, `CandleReader`, `Resampler`, `TickAggregator`,
  `fetch_binance_klines` and the `name` / `is_ready` / `warmup_period` / `push` /
  `read` generics) and a codoc mismatch on `AwesomeOscillatorHistogram` (its help
  page still listed `sma_period` after the argument was renamed to `lookback`).
  The roxygen sources existed but the `man/*.Rd` had never been regenerated; they
  are now complete, and a `push()` example that constructed a `TickAggregator`
  without its required `gap_fill` argument is fixed. CI now runs `R CMD check` so
  documentation drift fails the pull request instead of surfacing on r-universe.


## [0.9.5] - 2026-06-17

Maintenance release. The library API and every indicator are unchanged from
`0.9.4`; the only change that ships to users is to the R package's build script.
The rest of the release is CI / release-pipeline hardening (dependency caching,
job timeouts, and network-install retries) that does not affect the artifacts.

### Fixed
- **R package: retry the C ABI download.** `configure` / `configure.win` fetch the
  prebuilt `wickra-c-<triple>.tar.gz` from the matching GitHub release. A freshly
  cut release can briefly return 404 while its assets propagate across the CDN
  (and a transient network blip would also fail it), so the single-shot download
  is now retried with a backoff (~2 min) before giving up. Fixes
  `cannot open URL … 404 Not Found` on r-universe / source installs taken right
  after a release.


## [0.9.4] - 2026-06-17

Packaging fix for the `0.9.3` data layer. The library is identical to `0.9.3` on
every platform that already published; the only additions are an opt-in
`vendored-tls` build feature and the Linux Python wheels, which `0.9.3` could not
build.

### Fixed
- **Linux Python wheels (`manylinux` / `musllinux`) now build.** The `live-binance`
  data layer links `native-tls` -> `openssl-sys`, which needs OpenSSL at build
  time. The `manylinux` wheel containers ship no OpenSSL headers and the
  `musllinux` build cross-compiles against a musl sysroot that has no OpenSSL at
  all, so the wheels failed to compile. The Linux wheels are now built with a new
  opt-in `vendored-tls` feature that compiles OpenSSL from source and links it
  statically (no system OpenSSL required, on either libc). The native macOS and
  Windows wheels were unaffected (Security.framework / SChannel). As a result
  `0.9.3` shipped to crates.io, Maven Central, NuGet, and npm but not to PyPI;
  PyPI publishes starting with `0.9.4`.

### Added
- **`vendored-tls` feature** on `wickra-data` (and the Python binding): builds the
  `live-binance` TLS stack against a statically compiled OpenSSL. Off by default;
  used by the release wheels and exercised on every PR by a `manylinux` /
  `musllinux` container build-smoke CI job.


## [0.9.3] - 2026-06-17

### Changed
- **Python: zero third-party dependencies — NumPy is no longer required**
  (breaking). `pip install wickra` now pulls nothing else. Batch inputs accept any
  sequence or buffer of numbers (`array.array`, `memoryview`, a NumPy array, or a
  plain `list`); single-output `batch(...)` now returns a stdlib `array.array('d')`
  and multi-output indicators return a buffer-protocol `Matrix` (with `.shape`,
  integer-row and `[i, j]` element access, `.tolist()`) instead of 1-D / 2-D NumPy
  arrays. Both expose the buffer protocol, so `numpy.asarray(result)` still wraps a
  1-D result zero-copy when NumPy is installed — it is now an optional extra
  (`pip install wickra[numpy]`). Streaming `update(...)` is unchanged, and results
  are numerically identical. Single-output `batch(...)` is slower than the previous
  NumPy path — a stdlib `array.array` cannot take ownership of the Rust result, so
  it is copied rather than moved — though absolute batch latency stays in the
  low-millisecond range. The other nine languages were already dependency-free.

### Removed
- **Python: `numpy` runtime dependency** (see *Changed*). NumPy moves to the
  optional `numpy`/`test`/`bench` extras.

### Fixed
- **Binance kline feed: add the missing `3d` and `1M` intervals.** The
  `live-binance` `Interval` enum was missing three-day (`3d`) and one-month (`1M`)
  candles, two of Binance's 16 supported kline intervals. Both are now selectable
  and map to the correct wire-format strings.

### Added
- **Native historical Binance REST kline fetcher in 9 languages (data layer).**
  `fetchBinanceKlines` (Node.js / Python `fetch_binance_klines` / Go
  `FetchBinanceKlines` / C# `BinanceFeed.FetchKlines` / Java `BinanceFeed.fetchKlines`
  / R `fetch_binance_klines`; C / C++ call `wickra_binance_fetch_klines`) downloads
  historical OHLCV candles straight from Binance's public REST endpoint — no
  third-party HTTP/JSON client (`jackson`, `jsonlite`, `urllib`, …) needed. Pass a
  symbol, interval, and limit (`1..=1000`) plus optional millisecond start/end
  bounds; it blocks until the response arrives and returns the parsed candles. It
  is built on `ureq` with native-tls, sharing the live feed's TLS stack, and is
  covered by mock-HTTP-server tests. The historical counterpart to the live
  `BinanceFeed`; WASM is excluded (browsers use the host `fetch`). Ships with the
  C ABI's default `live-binance` feature.
- **Native live Binance kline feed in 9 languages (data layer).** `BinanceFeed`
  streams live OHLCV candles straight from Binance's public WebSocket — no
  third-party WebSocket client (`ws`, `websockets`, `gorilla/websocket`, …) in any
  binding. Construct it with comma-separated symbols + an interval, then poll
  `next(timeout)` for the next event (or `null`/`None` on timeout); the connection
  reconnects transparently. Exposed natively (Node.js / Python — a blocking poll
  that drives the tested async stream on a tokio runtime) and over the C ABI as Go
  `Next()`, C# `Next()`, Java `next()`, and the R `binance_next()`; C / C++ call
  `wickra_binance_connect` / `_next` / `_close` / `_free` directly. The connect →
  read → reconnect pipeline is covered by the existing mock-WS-server tests. WASM
  is excluded (a browser has no raw sockets; use the host `WebSocket`). The C ABI
  ships the feed by default (`live-binance` feature); the wasm build drops it.
- **CSV candle reading in all 10 languages (data layer).** The `CandleReader`
  parses a `timestamp,open,high,low,close,volume` CSV buffer (a leading UTF-8 BOM
  and field whitespace are tolerated) into candles: construct it from a CSV string
  and call `read()` for every candle in file order. Exposed natively (Node.js /
  WASM `read(): Candle[]`, Python `read() -> list[tuple]`) and over the C ABI as Go
  `Read() []Candle`, C# `Candle[] Read()`, Java `Candle[] read()`, and the R
  `read()` S3 generic (an `n×6` matrix); C / C++ call `wickra_candle_reader_new` /
  `_count` / `_read` directly. A cross-language golden
  (`testdata/golden/data_csv*.csv`) pins the parsed candles identically across
  every binding. This makes CSV backtest loading dependency-free in every binding.
- **Candle resampling in all 10 languages (data layer).** The `Resampler`
  aggregates candles into a higher timeframe (e.g. 1m → 5m): `update(open, high,
  low, close, volume, timestamp)` returns the completed higher-timeframe candle on
  a bucket boundary (else `null`/`None`/`NA`), and `flush()` emits the final,
  still-open candle. Exposed natively (Node.js / WASM / Python) and over the C ABI
  (Go / C# / Java return `(Candle, bool)` / `Candle?` / `Candle`; R via `update()`
  and a `flush()` S3 method; C / C++ directly). A cross-language golden
  (`testdata/golden/data_resampled.csv`) pins the resampled stream identically.
- **Tick-to-candle aggregation in all 10 languages (data layer).** The
  `TickAggregator` — roll trade ticks up into fixed-timeframe OHLCV candles, with
  optional gap filling — is now exposed natively (Node.js / WASM `push(price,
  size, ts): Candle[]`, Python `push(...) -> list[tuple]`) and over the C ABI as
  Go `Push() []Candle`, C# `Candle[] Push()`, Java `Candle[] push()`, and the R
  `push()` generic (an `n×6` matrix); C / C++ call the C ABI directly. The C ABI
  uses a lossless two-step `wickra_tick_aggregator_push` / `_drain` so a single
  push that gap-fills across many empty buckets never overflows a fixed buffer. A
  new cross-language golden (`testdata/golden/data_*.csv`) pins the candle stream
  identically across every binding. This is the first feature of a data layer that
  makes the non-Rust bindings dependency-free for tick aggregation.
- **`name()` on every indicator in all 10 languages.** The canonical
  `Indicator::name()` / `BarBuilder::name()` accessor is now exposed through every
  binding — Node.js `name()`, WASM `name()`, Python `name()`, and the C ABI
  `wickra_<ind>_name()` surfaced as Go `Name()`, C# `Name()`, Java `name()`, and
  the R `name()` S3 generic (C/C++ call the C ABI directly). The returned string
  is the core canonical name, which may differ from the registered class name
  (e.g. `ChaikinMoneyFlow` reports `"CMF"`, `Donchian` reports
  `"DonchianChannels"`). A new cross-language golden (`testdata/golden/names.json`)
  pins this name for all 514 indicators identically across every binding.

## [0.9.2] - 2026-06-15

### Added
- **Cross-language golden parity for all 514 indicators across all 10 languages.**
  A new `gen_golden` reference emits a deterministic OHLCV input series plus the
  Rust output of every one of the 514 indicators to `testdata/golden/`. Each
  binding now replays that shared input and is checked **bit-for-bit against the
  Rust reference**, covering every archetype (scalar, multi-output, pairwise,
  derivatives-tick, cross-section, order-book, trade, profile, alt-chart bars,
  footprint):
  - Python, Node.js, Java and R via reflection-driven runners.
  - Go, C# and C/C++ via generated dispatch (`golden_all_test.go`,
    `GoldenAllTests.g.cs`, `examples/c/golden_test.c` compiled as both C and C++).
  - WASM via a `node --test` runner over the nodejs-target build.
- CI now runs the WASM golden suite; the C/C++ golden tests run as `ctest`
  targets in the existing C-ABI job, and the Python/Node/Go/C#/Java/R suites pick
  up their golden runners automatically.
- **README:** a "verified across 10 languages" badge (linking to the FAQ that
  explains the cross-language golden parity) and a per-binding throughput table so
  readers can pick a binding by its streaming FFI cost.

### Fixed
- **Java binding marshalled C ABI `bool` parameters incorrectly.** The
  cross-section state flags (`newHigh`, `newLow`, `aboveMa`, `onBuySignal`) were
  allocated as `JAVA_DOUBLE` arrays and passed to `const bool*` parameters, so the
  native side read the low byte of each 8-byte double and saw every flag as
  `false` (affecting e.g. `NewHighsNewLows`, `HighLowIndex`, `BullishPercentIndex`,
  `PercentAboveMa`). They are now packed into a real `bool` buffer. `MacdExt`'s
  `MaType` arguments are now passed as `byte` to match the `uint8_t` downcall.
- **R binding marshalled C ABI `bool` flags incorrectly.** `(bool *)REAL(x)`
  reinterpreted the 8-byte doubles as 1-byte bools across the 15 cross-section
  update wrappers, reading every flag as `false`; the flags are now converted into
  a real C `bool` buffer.
- C# binding: added the `#nullable enable` directive the generated
  `Indicators.g.cs` requires, clearing four `CS8669` warnings.

### Changed
- Renamed the `live_trading` examples to `live_binance` across the Python, Node.js,
  WASM and C examples — they poll Binance market data, they do not place trades.
- **Breaking — de-duplicated four indicators that computed identically to another
  one.** Each is now its own distinct, correctly-defined indicator (the catalogue
  stays at the same count):
  - `AverageDrawdown` now reports the mean of the maximum depths of the distinct
    drawdown episodes in the window (previously the per-bar mean under-water
    fraction, which equalled `PainIndex`).
  - `IntradayIntensity` now reports the raw per-bar Bostian intensity
    `volume * (2*close − high − low) / (high − low)` (previously a cumulative line
    that equalled the A/D Line `Adl`; its normalized form is `Cmf`).
  - `AwesomeOscillatorHistogram` now reports the AO momentum
    `AO[t] − AO[t−lookback]`; its third parameter is the momentum `lookback`
    (default 1) instead of an SMA period (the old `AO − SMA(AO, n)` equalled
    `AcceleratorOscillator`).
  - `AdOscillator` is now the Williams **A/D Oscillator** (`WAD − SMA(WAD, 13)`),
    distinct from the cumulative Williams A/D line `Wad`. Its native (Python /
    Node.js / WASM) alias is renamed **`WilliamsAD` → `ADOSC`**.

## [0.9.1] - 2026-06-14

### Added
- C ABI hub: every indicator now exposes `wickra_<ind>_warmup_period` and
  `wickra_<ind>_is_ready`, closing the gap with the native bindings (which
  already had them). The C-ABI languages surface them idiomatically: C# `int
  WarmupPeriod()` / `bool IsReady()`, Go `WarmupPeriod()` / `IsReady()`, Java
  `int warmupPeriod()` / `boolean isReady()`, and R `warmup_period()` /
  `is_ready()` generics. The alt-chart bar builders are excluded by design (a
  candle can complete 0..n bars, so they have no warmup).
- Runnable rustdoc examples for 23 indicators that previously lacked one.
- A Requirements reference documenting the minimum supported version per
  language — a new page in the documentation site plus README, marketing-site
  and organization-profile sections.

### Changed
- Raised the minimum Node.js version to 20 — Node 18 reached end-of-life. The
  prebuilt N-API addon is now tested on the active LTS lines (22 and 24).
- The Java binding now builds on the JDK 25 LTS in CI (JDK 22 reached
  end-of-life); the published bytecode still targets Java 22, so the runtime
  requirement is unchanged.
- Standardised programming-language naming and ordering across all docs, READMEs,
  the documentation site, marketing site, organization profile and GitHub
  repository descriptions. Canonical list:
  `Rust, Python, Node.js, WASM, C, C++, C#, Go, Java, R`. Uses C# (not .NET) as
  the language label, lists C and C++ separately, prefers `Node.js` and `WASM` in
  prose, and frames the C ABI as a hub (`C ABI hub → …`) rather than a
  language-list entry. Documentation only — no code or public API changes.
- Python binding: upgraded `pyo3` and `rust-numpy` from 0.28 to 0.29. No public
  API changes; the full test suite passes unchanged.

### Fixed
- Corrected the internal casing of the `RelativeStrengthAB` binding wrappers,
  which used `...Ab` (`WasmRelativeStrengthAb` in the WASM crate,
  `RelativeStrengthAbNode` in the Node crate) while every other surface uses the
  acronym `AB`. The published JS/WASM class name was already `RelativeStrengthAB`
  (set via `js_name`/`js_class`), so the runtime API is unchanged; the only
  visible change is the auto-generated TypeScript type alias, renamed
  `RelativeStrengthAbNode` → `RelativeStrengthABNode` in `index.d.ts`.

### Security
- Resolved the pyo3 advisories RUSTSEC-2026-0176 (out-of-bounds read in
  `PyList`/`PyTuple` `nth`/`nth_back`) and RUSTSEC-2026-0177 (missing `Sync`
  bound on `PyCFunction::new_closure`) by upgrading to pyo3 0.29, which fixes
  both. The upgrade was previously blocked upstream by rust-numpy 0.28 pinning
  pyo3 `^0.28`; rust-numpy 0.29 lifts that pin. The not-affected exceptions are
  removed from `deny.toml` and `osv-scanner.toml`.

## [0.9.0] - 2026-06-13

Maintenance release: Java build-dependency updates and CI/Dependabot
housekeeping only. No library code or public API changes.

### Changed
- Java binding: upgraded the test framework to JUnit Jupiter 6.1.0 (from
  5.10.2) and bumped the Maven build plugins — `maven-compiler-plugin`
  3.13.0 → 3.15.0, `maven-surefire-plugin` 3.2.5 → 3.5.6, `maven-jar-plugin`
  3.4.1 → 3.5.0, `maven-source-plugin` 3.3.1 → 3.4.0, `maven-javadoc-plugin`
  3.7.0 → 3.12.0, and `maven-gpg-plugin` 3.2.4 → 3.2.8.
- Java benchmarks and examples: bumped `maven-compiler-plugin` to 3.15.0 and
  `exec-maven-plugin` to 3.6.3; examples bumped `jackson-databind` 2.17.1 →
  2.22.0.
- Grouped Dependabot updates per ecosystem into a single pull request and
  extended tracking to the NuGet (C#) binding and the Node/Go examples.


## [0.8.9] - 2026-06-12

Maintenance release: supply-chain and CI housekeeping only. No library code or
public API changes.

### Security
- Triaged the pyo3 advisories RUSTSEC-2026-0176 (out-of-bounds read in
  `PyList`/`PyTuple` `nth`/`nth_back`) and RUSTSEC-2026-0177 (missing `Sync`
  bound on `PyCFunction::new_closure`) as **not affecting Wickra**: neither
  vulnerable API is reachable from the Python binding. Both are fixed in pyo3
  0.29, but rust-numpy 0.28 pins pyo3 `^0.28`, so the upgrade is blocked
  upstream; the advisories are recorded with their not-affected rationale in
  `deny.toml` and `osv-scanner.toml` and will be cleared once rust-numpy 0.29
  ships.

### Changed
- Java binding: bumped `central-publishing-maven-plugin` 0.5.0 → 0.10.0 (the
  Maven Central publishing plugin used at release time).
- Bumped the SHA-pinned GitHub Actions used in CI (`actions/checkout`,
  `actions/setup-go`, `actions/setup-java`, `github/codeql-action`,
  `taiki-e/install-action`) to their latest releases.
- Added a Maven ecosystem to Dependabot so the Java binding's build plugins and
  dependencies are tracked going forward.


## [0.8.8] - 2026-06-11
### Fixed
- R binding: declare `Depends: R (>= 2.10)`, clearing the `R CMD check` warning
  ("package needs dependence on R (>= 2.10)") that the bundled, lazy-loaded
  `sample_ohlcv` dataset triggers on r-universe / CRAN.

## [0.8.7] - 2026-06-11
### Added
- R binding: a *Getting started* vignette and a synthetic `sample_ohlcv` example
  dataset, giving new users a runnable, self-contained walkthrough and populating
  the R-universe Articles and Datasets tabs. The vignette's code is exercised in
  CI so a broken example is caught before the published build.

## [0.8.6] - 2026-06-11
### Changed
- Package registry metadata for better discoverability:
  - R (R-universe): added the R-universe URL and `X-schema.org-keywords` to the
    R `DESCRIPTION`, plus a package logo at `bindings/r/man/figures/logo.png`
    (pkgdown convention).
  - Python (PyPI): added a `Documentation` project URL.
  - C# (NuGet): added a package icon via `PackageIcon`.

## [0.8.5] - 2026-06-11
### Fixed
- The R binding's golden-fixture parity test now skips gracefully when the shared
  `testdata/golden` fixtures are not bundled with the package — standalone
  r-universe / CRAN builds package only `bindings/r`, so the repo-root fixtures
  are unreachable there. The parity stays enforced by the repository CI, where
  the fixtures are present.

## [0.8.4] - 2026-06-11
### Fixed
- A single non-finite (NaN/inf) tick no longer poisons indicator state.
  The 16 pairwise running-sum/buffer indicators fixed first (`Beta`,
  `BetaNeutralSpread`, `Cointegration`, `HasbrouckInformationShare`,
  `PearsonCorrelation`, `RollingCorrelation`, `RollingCovariance`,
  `DistanceSsd`, `GrangerCausality`, `KendallTau`, `LeadLagCrossCorrelation`,
  `OuHalfLife`, `SpearmanCorrelation`, `SpreadAr1Coefficient`, `SpreadHurst`,
  `VarianceRatio`) were joined by 38 more scalar/pairwise indicators the new
  property harness surfaced (the linear-regression family, rolling quantiles
  and IQR, `Variance`/`StdDev`-derived stats, `Kurtosis`/`Skewness`, the
  trailing stops, `KalmanHedgeRatio`, `SpreadBollingerBands`, and more). Every
  `f64` / `(f64, f64)` indicator now rejects non-finite input and returns
  `None`, matching the streaming-robustness guarantee — and the harness enforces
  it going forward.

### Added
- Catalogue-wide property-based invariant harness
  (`crates/wickra-core/tests/invariants.rs`) asserting `batch == streaming`,
  `reset == fresh`, and non-finite-input rejection for every indicator and
  bar-builder.

### Changed
- CI: every job now has a runtime cap and the historically flaky Node test step
  auto-retries, so a wedged runner fails fast instead of hanging for hours.
- Documentation accuracy fixes in `SECURITY.md`, `ARCHITECTURE.md`, and
  `THREAT_MODEL.md` (supported version, indicator count, WASM test coverage,
  numerical-stability notes, and the C-ABI panic strategy).

## [0.8.3] - 2026-06-10
### Added
- **Per-binding throughput benchmarks** — every target now ships a `throughput`
  benchmark mirroring the Node `throughput.js`: streaming and batch
  updates-per-second for `SMA(20)`, `ATR(14)` and `MACD(12,26,9)` over a
  synthetic OHLCV series. New for Python (`bindings/python/benchmarks/`), C
  (`bindings/c/benchmarks/`), C# (`bindings/csharp/benchmarks/`), Go
  (`bindings/go/benchmarks/`), Java (`bindings/java/benchmarks/`), R
  (`bindings/r/benchmarks/`), WebAssembly (`bindings/wasm/benchmarks/`) and the
  Rust core baseline (`examples/rust/.../throughput.rs`, no FFI). They measure
  each binding's FFI overhead — the same Rust core runs underneath all of them —
  and are documented in [BENCHMARKS.md](BENCHMARKS.md) §3, not a cross-library
  speed claim.
- **C ABI archetype test** — `examples/c/archetypes.c` exercises one indicator
  per FFI archetype (scalar, multi-output, bars, profile, array input) through
  the C boundary, matching the Go/R/Java suites.

## [0.8.2] - 2026-06-10
### Fixed
- **R binding builds for WebAssembly** — `bindings/r/configure` now builds the
  C ABI from source for the `wasm32-unknown-emscripten` target (r-universe /
  webR) using the build image's cargo + emscripten, instead of failing with
  "unsupported OS Emscripten". rayon is dropped on wasm via
  `--no-default-features`; the indicators are pure computation, so the serial
  path is functionally identical.

## [0.8.1] - 2026-06-10
### Fixed
- **`wickra-go` license** — the release-time Go module mirror now ships the dual
  `LICENSE-MIT` and `LICENSE-APACHE` files, so pkg.go.dev detects a
  redistributable license for `github.com/wickra-lib/wickra-go`. The previous
  mirror shipped no license file.

## [0.8.0] - 2026-06-09
### Added
- **Standalone `wickra-go` module** — the Go binding is now mirrored to a
  dedicated `github.com/wickra-lib/wickra-go` repository on every release, with
  the prebuilt C ABI libraries committed per platform under
  `lib/<goos>_<goarch>/` and the C ABI header vendored alongside the source, so
  `go get github.com/wickra-lib/wickra-go` builds with no extra steps. The
  in-repo `bindings/go` module is unchanged for repo-clone workflows.

### Changed
- **Go binding (`bindings/go`) is self-contained** — the C ABI header is now
  vendored inside the module (`bindings/go/include/wickra.h`) instead of being
  referenced from the parent `bindings/c` directory, and the cgo link flags
  resolve the prebuilt library per `GOOS`/`GOARCH` under `lib/<goos>_<goarch>/`.
  This removes the dependency on a full repository checkout for building the
  module.

## [0.7.9] - 2026-06-09
### Added
- **Java binding (`bindings/java`)** — a Java binding reaching the C ABI hub
  through the Java Foreign Function & Memory API (Panama, `java.lang.foreign`,
  final in Java 22) rather than JNI or jextract, exposing all 514 indicators as
  idiomatic `AutoCloseable` classes. The downcall handles, per-indicator
  wrappers and output records are generated from `wickra.h`; the opaque handle is
  a `MemorySegment` freed by a `java.lang.ref.Cleaner` action. Ships a full
  example suite mirroring the C, C#, Go and R examples; published to Maven
  Central as `org.wickra:wickra`.

## [0.7.8] - 2026-06-09
### Added
- **R binding (`bindings/r`)** — an R package reaching the C ABI hub through R's
  native `.Call` interface, exposing all 514 indicators as constructors that
  return a `wickra_indicator` object with `update`/`batch`/`reset` methods. The
  C glue and R wrappers are generated from `wickra.h`; the native handle is freed
  by a registered finalizer. Ships a full example suite mirroring the C, C# and
  Go examples; distributed for r-universe / source install.

## [0.7.7] - 2026-06-09
### Added
- **Go binding (`bindings/go`)** — a cgo binding over the C ABI hub exposing all
  514 indicators as idiomatic types with `New<Indicator>` constructors and
  `Update`/`Batch`/`Reset`/`Close` methods, generated from `wickra.h`. Handles are
  freed by `Close()` with a `runtime.SetFinalizer` backstop. Ships a full example
  suite mirroring the C and C# examples; distributed as a subdirectory module
  (`go get github.com/wickra-lib/wickra/bindings/go`).

## [0.7.6] - 2026-06-09
### Added
- **C# / .NET binding (`bindings/csharp`)** — the first language stecker on the
  C ABI hub. Exposes all 514 indicators as idiomatic `IDisposable` classes via
  `[LibraryImport]` source-generated P/Invoke, generated from `wickra.h`. Ships
  on NuGet as `Wickra` with prebuilt native libraries for six target triples
  (win/linux/osx × x64/arm64), plus a full example suite mirroring the C examples.

## [0.7.5] - 2026-06-09
### Added
- **C ABI (`bindings/c`)** — a `cdylib` + `staticlib` plus a generated
  `include/wickra.h` exposing all 514 indicators and 10 bar builders over an
  opaque-handle C ABI: the hub any C-capable language (C, C++, Go, C#, Java, R)
  links against, complementing the native Python/Node/WASM bindings. Ships a
  full example suite (streaming, backtest, multi-timeframe, OpenMP parallel
  fan-out, three educational strategies, and Binance fetch/live over `curl`)
  mirroring the other bindings, plus an optional `wickra.hpp` C++ RAII wrapper.

## [0.7.4] - 2026-06-08
- **Three-Line Break** — Three-line-break bars (reversal needs N-line break) (`THREE_LINE_BREAK_BARS`).
- **Run** — Run bars (consecutive same-direction tick runs) (`RUN_BARS`).
- **Imbalance** — Imbalance bars (tick-rule signed imbalance threshold) (`IMBALANCE_BARS`).
- **Dollar** — Dollar bars (fixed traded value per bar, Lopez de Prado) (`DOLLAR_BARS`).
- **Volume** — Volume bars (fixed traded volume per bar) (`VOLUME_BARS`).
- **Tick** — Tick bars (fixed candle count per bar) (`TICK_BARS`).
- **Range** — Range bars (fixed price-range bricks) (`RANGE_BARS`).

## [0.7.3] - 2026-06-08
- **M2Measure** — M2 measure (Modigliani; Sharpe expressed in benchmark return units) (`M2Measure`).
- **UpsidePotentialRatio** — Upside Potential Ratio (upside mean over downside deviation) (`UpsidePotentialRatio`).
- **GainToPainRatio** — Gain-to-Pain Ratio (sum of returns over sum of losses) (`GainToPainRatio`).
- **CommonSenseRatio** — Common Sense Ratio (tail ratio times gain-to-pain) (`CommonSenseRatio`).
- **KRatio** — K-Ratio (Kestner; equity-curve slope over its standard error) (`KRatio`).
- **TailRatio** — Tail Ratio (95th over absolute 5th return percentile) (`TailRatio`).
- **MartinRatio** — Martin Ratio (Ulcer Performance Index; return over RMS drawdown) (`MartinRatio`).
- **BurkeRatio** — Burke Ratio (return over root-sum-squared drawdowns) (`BurkeRatio`).
- **SterlingRatio** — Sterling Ratio (mean return over average drawdown) (`SterlingRatio`).

## [0.7.2] - 2026-06-08
- **Composite Profile** — multi-session composite volume profile exposing POC, VAH and VAL (`CompositeProfile`).
- **High/Low Volume Nodes** — highest- and lowest-volume price nodes in the profile (`HighLowVolumeNodes`).
- **Profile Shape** — profile shape classification (b/P/D normal) as a numeric code (`ProfileShape`).
- **Single Prints** — count of single-print (low-activity) price levels in the profile (`SinglePrints`).
- **Naked POC** — most recent untouched (naked) point of control level (`NakedPoc`).

## [0.7.1] - 2026-06-08
- **Open-Interest Momentum** — rate-of-change of open interest over a rolling window (`OpenInterestMomentum`).
- **Funding-Implied APR** — annualised funding rate (per-interval funding times intervals per year) (`FundingImpliedApr`).
- **Perpetual Premium Index** — relative premium of the mark price over the index price (`PerpetualPremiumIndex`).
- **OI-to-Volume Ratio** — open interest divided by taker volume (position turnover proxy) (`OiToVolumeRatio`).
- **Estimated Leverage Ratio** — open interest divided by aggregate long+short position size (leverage proxy) (`EstimatedLeverageRatio`).

## [0.7.0] - 2026-06-08
- **Hasbrouck Information Share** — variance-ratio proxy for each venue's share of price discovery (Hasbrouck information share) (`HasbrouckInformationShare`).
- **PIN** — probability of informed trading from rolling buy/sell imbalance (EKOP single-window estimator) (`Pin`).
- **Trade-Sign Autocorrelation** — lag-1 autocorrelation of the signed trade aggressor (order-flow persistence) (`TradeSignAutocorrelation`).

## [0.6.9] - 2026-06-08
- **Tristar** — a three-doji star reversal: three consecutive dojis with the middle gapped above (bearish) or below (bullish) its neighbours (`Tristar`).
- **Harami Cross** — a Harami whose second candle is a contained doji, a stronger reversal than a plain Harami (`HaramiCross`).
- **Tower Top/Bottom** — a tall bar, a small pause bar, then a tall opposite bar marking a reversal (`TowerTopBottom`).
- **Frying Pan Bottom** — a rounded (U-shaped) accumulation base over the lookback window, confirmed when price recovers above the rim (`FryPanBottom`).
- **Dumpling Top** — a rounded (dome-shaped) distribution top over the lookback window, confirmed when price breaks below the start (`DumplingTop`).
- **New Price Lines** — flags a run of N consecutive new closing highs (+1) or lows (-1), the eight/ten-new-price-lines exhaustion gauge (`NewPriceLines`).

## [0.6.8] - 2026-06-08
- **Smoothed Heikin-Ashi** — a Heikin-Ashi candle computed from EMA-smoothed OHLC, damping noise into a cleaner trend candle (`SmoothedHeikinAshi`).
- **Heikin-Ashi Oscillator** — the Heikin-Ashi candle body (`ha_close − ha_open`), optionally EMA-smoothed, as a zero-line oscillator (`HeikinAshiOscillator`).
- **Three Line Break** — the trend direction of a line-break chart, reversing only when the close breaks the extreme of the last N lines (`ThreeLineBreak`).
- **Equivolume** — a chart box whose height is the bar range and whose width is volume-relative, fusing price range with activity (`Equivolume`).
- **CandleVolume** — a candle whose body is close-minus-open and whose width is volume-relative, a volume-weighted candle chart (`CandleVolume`).

## [0.6.7] - 2026-06-08
- **TD Camouflage** — a DeMark qualifier flagging hidden intrabar strength or weakness against the prior close (`TDCamouflage`).
- **TD Clop** — a DeMark two-bar open/close engulfing reversal where the bar opens beyond and closes back across the prior body (`TDClop`).
- **TD Clopwin** — the inside-body cousin of TD Clop, marking a compression bar whose direction hints at the next move (`TDClopwin`).
- **TD Propulsion** — a DeMark continuation thrust that opens on the trend side and closes beyond the prior bar's extreme (`TDPropulsion`).
- **TD Trap** — an inside ("trap") bar followed by a close beyond its range, triggering a directional breakout signal (`TDTrap`).
- **TD D-Wave** — a streaming Elliott-style swing-wave counter labelling the market's 1–5 impulse / A–C correction sequence (`TDDWave`).
- **TD Moving Averages** — the DeMark ST1 (fast) and ST2 (slow) median-price trend ribbon whose crossover frames the trend (`TDMovingAverage`).

## [0.6.6] - 2026-06-08
- **Pivot Reversal** — a breakout signal when price closes through the most recently confirmed swing pivot (`PIVOT_REVERSAL`).
- **Volume-Weighted Support/Resistance** — a band whose edges are the volume-weighted average of recent highs and lows (`VOLUME_WEIGHTED_SR`).
- **Andrews Pitchfork** — median line and two parallels projected from the last three swing pivots (`ANDREWS_PITCHFORK`).
- **Murrey Math Lines** — T. H. Murrey's eighths grid over the recent trading range, each level acting as support/resistance (`MURREY_MATH_LINES`).
- **Central Pivot Range** — the classic pivot flanked by two central levels gauging the day's expected character (`CENTRAL_PIVOT_RANGE`).
- **Faster scalar batch paths** — `Ema`, `Rsi`, `BollingerBands`, `MacdIndicator` and `Atr` gained dedicated batch fast paths (used by the Python bindings) that strip per-element `Option`/validation overhead and the intermediate `Vec<Option<_>>` allocation, while staying *bit-for-bit* equal to replaying `update` (including the SMA/Bollinger drift-reseed). Python batch is ~2× faster on EMA/RSI/MACD/ATR; streaming is unchanged.
- **Cross-library benchmark refresh** — `benchmarks/compare_libraries.py` now measures the median across timing rounds (`--rounds` / `--streaming-rounds`), adds `--skip-batch` / `--skip-streaming`, and drives every peer through the streaming arena (recompute for batch-only libraries). `wickra-bench` compares the batch fast paths against `kand`.

## [0.6.5] - 2026-06-07
- **Autocorrelation Periodogram** — Ehlers autocorrelation periodogram: dominant cycle period estimate (`AUTOCORRPGRAM`).
- **Even Better Sinewave** — Ehlers Even Better Sinewave: normalized cycle-phase oscillator (`EVENBETTERSINE`).
- **Bandpass Filter** — Ehlers bandpass filter: isolates a frequency band around the dominant cycle (`BANDPASS`).
- **Adaptive CCI** — Adaptive CCI: efficiency-ratio-adaptive CCI on typical price (`ADAPTIVECCI`).
- **Universal Oscillator** — Ehlers Universal Oscillator: SuperSmoother-based normalized cycle oscillator (`UNIVERSALOSC`).
- **Adaptive RSI** — Adaptive RSI: dominant-cycle-tuned RSI length (Ehlers) (`ADAPTIVERSI`).
- **Correlation Trend Indicator** — Ehlers Correlation Trend Indicator: Pearson correlation of price vs time (`CTI`).
- **Trendflex** — Ehlers Trendflex: trend-following companion to Reflex (`TRENDFLEX`).
- **Reflex** — Ehlers Reflex: trend-cycle oscillator measuring slope-adjusted displacement (`REFLEX`).
- **Highpass Filter** — Ehlers highpass filter: removes low-frequency trend, leaving cyclic component (`HIGHPASS`).

## [0.6.4] - 2026-06-07
- **Kendall Tau** — Kendall rank correlation (tau-b) over a rolling window of paired observations (`KENDALLTAU`).
- **Sample Entropy** — Sample entropy: regularity/complexity of a rolling series (Richman-Moorman) (`SAMPLEENT`).
- **Shannon Entropy** — Shannon entropy of a rolling value distribution over fixed bins (`SHANNONENT`).
- **Rolling Min-Max Scaler** — Rolling min-max scaler mapping the latest value to 0..1 over a rolling window (`ROLLINGMINMAX`).
- **Jarque-Bera** — Jarque-Bera normality test statistic over a rolling window (`JARQUEBERA`).

## [0.6.3] - 2026-06-07
- **Volume-Weighted MACD** — Volume-Weighted MACD: MACD computed on VWMA instead of EMA, with signal line and histogram (`VWMACD`).
- **Better Volume** — Better Volume (VSA): classifies volume against bar spread to surface effort/result imbalance (`BETTERVOL`).
- **Intraday Intensity Index** — Intraday Intensity Index: volume weighted by close position within the bar range (`INTRADAYINT`).
- **Trade Volume Index** — Trade Volume Index: accumulates volume by tick direction past a min-tick threshold (distinct from TSV) (`TRADEVOLIDX`).
- **Twiggs Money Flow** — Twiggs Money Flow: volume-weighted accumulation using true range and Wilder smoothing (distinct from CMF) (`TWIGGSMF`).
- **Williams Accumulation/Distribution** — Williams Accumulation/Distribution: cumulative price-direction accumulator (distinct from Chaikin A/D) (`WILLIAMSAD`).
- **Volume RSI** — Volume RSI: Wilder-style RSI computed on signed volume flow (`VOLUMERSI`).

## [0.6.2] - 2026-06-07
- **Modified MA Stop** — Modified MA Stop — SMMA-ratcheted trailing stop with directional flip (`MODIFIED_MA_STOP`).
- **Time-Based Stop** — Time-Based Stop — bar-count timer that fires after a fixed holding period (`TIME_BASED_STOP`).
- **NRTR** — NRTR (Nick Rypock Trailing Reverse) — percentage trailing-reverse stop (`NRTR`).
- **ATR Ratchet** — ATR Ratchet — Kaufman per-bar tightening volatility trailing stop (`ATR_RATCHET`).
- **Elder SafeZone** — Elder SafeZone Stop — average noise-penetration trailing stop with directional flip (`ELDER_SAFE_ZONE`).
- **Kase DevStop** — Kase DevStop volatility trailing stop using standard-deviation of two-bar true range (`KASE_DEV_STOP`).

## [0.6.1] - 2026-06-07
- **Projection Oscillator** — Widner projection oscillator: close position inside the projection bands, scaled 0..100 (`ProjectionOscillator`).
- **Projection Bands** — Widner projection bands: forward-projected high/low regression envelope (`ProjectionBands`).
- **Median Channel** — robust median +/- multiplier*MAD envelope (`MedianChannel`).
- **Bomar Bands** — adaptive percentage bands containing a target coverage fraction of recent closes (`BomarBands`).
- **Quartile Bands** — rolling 25th/50th/75th-percentile (Q1/median/Q3) envelope (`QuartileBands`).

## [0.6.0] - 2026-06-06
- **Volatility Cone** — volatility cone: current realized volatility within its historical min/median/max envelope (`VolatilityCone`).
- **VolatilityRatio** — Schwager's volatility ratio: true range over the EMA of prior true ranges (`VolatilityRatio`).
- **BipowerVariation** — jump-robust realized bipower variation (pi/2 sum of adjacent absolute log-return products) (`BipowerVariation`).
- **VolatilityOfVolatility** — vol-of-vol: sample stddev of a rolling realized-volatility series (`VolatilityOfVolatility`).
- **Garch11** — GARCH(1,1) conditional volatility with a long-run-variance anchor (`Garch11`).
- **EwmaVolatility** — RiskMetrics exponentially-weighted volatility of log returns (lambda decay) (`EwmaVolatility`).

## [0.5.9] - 2026-06-06

### Added

- Internal Rust cross-library benchmark harness (`crates/wickra-bench`, not
  published) comparing Wickra against `kand`, `ta-rs` and `yata` on an identical
  candle series in both streaming and batch modes; wired into the nightly
  `cross-library-bench` workflow.
- `tulipy` runners and expanded per-tick streaming coverage (SMA, EMA, RSI,
  MACD, Bollinger) in the Python `compare_libraries` benchmark.

### Changed

- Faster streaming and batch updates for SMA, Bollinger Bands, RSI, EMA and ATR
  (flat ring buffers replacing `VecDeque`, hoisted reciprocals in the Wilder
  smoothing, leaner hot state) — indicator outputs are unchanged.
- Rewrote the README benchmark section into honest, tiered tables (Rust core vs
  the other Rust crates, and Python vs the Python ecosystem) that show where
  Wickra wins and where it loses, not only the favourable comparisons.

## [0.5.8] - 2026-06-04
- **TSF Oscillator** — the percentage gap of the close to the one-bar-ahead time-series forecast, a close-relative companion to CFO (`TsfOscillator`).
- **MACD Histogram** — the standalone macd-minus-signal bar of MACD as a scalar series (`MacdHistogram`).
- **PPO Histogram** — the Percentage Price Oscillator with its signal EMA and the resulting zero-centered histogram (`PpoHistogram`).

## [0.5.7] - 2026-06-04
- **Qstick** — Qstick (Chande), the SMA of the candle body (close − open) as a net buying/selling pressure gauge (`QSTICK`).
- **TTM Trend** — TTM Trend (John Carter), +1/−1 by whether the close sits above the SMA of recent median prices (`TTM_TREND`).
- **Trend Strength Index** — trend strength index, the signed r² of a linear regression of price against time (`TREND_STRENGTH_INDEX`).
- **Polarized Fractal Efficiency** — polarized fractal efficiency (Hannula), directional trend efficiency over a fractal lookback (`POLARIZED_FRACTAL_EFFICIENCY`).
- **Wave PM** — Wave PM (Kase), a variance-normalised peak-momentum statistic (`WAVE_PM`).
- **Gator Oscillator** — Gator Oscillator (Bill Williams), the Alligator convergence/divergence histogram (`GATOR_OSCILLATOR`).
- **Kase Permission Stochastic** — Kase Permission Stochastic, a double-smoothed stochastic used as a trade-permission filter (`KASE_PERMISSION_STOCHASTIC`).

## [0.5.6] - 2026-06-04
- **QQE** — quantitative qualitative estimation, a smoothed RSI with an ATR-of-RSI trailing line (`QQE`).
- **Intraday Momentum Index** — intraday momentum index (Chande), RSI on the open-to-close body (`IMI`).
- **Elder Ray** — Elder Ray bull power and bear power around an EMA of close (`ElderRay`).
- **Derivative Oscillator** — derivative oscillator (Constance Brown), a double-smoothed RSI histogram (`DerivativeOscillator`).
- **RMI** — relative momentum index (RMI), RSI over a multi-bar momentum lookback (`RMI`).
- **Stochastic CCI** — stochastic CCI, a stochastic oscillator over the CCI (`StochasticCCI`).
- **Dynamic Momentum Index** — dynamic momentum index (Chande), a volatility-adaptive RSI (`DynamicMomentumIndex`).
- **RSX** — RSX, a Jurik-style three-stage smoothed RSI (`RSX`).
- **Fisher RSI** — Fisher RSI, the Fisher transform of a normalised RSI (`FisherRSI`).
- **Disparity Index** — disparity index, the percent gap between price and its moving average (`DisparityIndex`).

## [0.5.5] - 2026-06-04
- **GD** — generalized DEMA (GD), Tillson's volume-factor double EMA and the building block of T3 (`GD`).
- **GMA** — geometric moving average (GMA), the rolling geometric mean of prices (`GMA`).
- **Holt-Winters** — Holt's linear (double exponential) smoothing with level and trend components (`HoltWinters`).
- **Adaptive Laguerre** — Ehlers adaptive Laguerre filter with median-error-adaptive gamma (`AdaptiveLaguerre`).
- **Median MA** — median moving average, the rolling median of prices (`MedianMA`).
- **EHMA** — exponential Hull moving average (EHMA), the Hull construction built from EMAs (`EHMA`).
- **SWMA** — sine-weighted moving average (SWMA), a symmetric half-cycle sine window (`SWMA`).

## [0.5.4] - 2026-06-04
- **Roll Measure** — effective spread implied by the negative serial covariance of trade-price changes (Roll 1984) (`RollMeasure`).
- **Amihud Illiquidity** — average absolute log return per unit of traded value (price-impact liquidity proxy, Amihud 2002) (`AmihudIlliquidity`).
- **VPIN** — volume-synchronised probability of informed trading (volume-bucketed order-flow toxicity) (`Vpin`).
- **Order Flow Imbalance** — rolling sum of best-level order-flow events (Cont-Kukanov-Stoikov OFI) (`OrderFlowImbalance`).
- **Expectancy** — expected return per unit of average loss (R-multiple) over a rolling window of returns (`Expectancy`).
- **Win Rate** — fraction of strictly-positive returns over a rolling window (`WinRate`).
- **Regime Label** — volatility-quantile regime classification: −1 calm / 0 normal / +1 stressed, by where the rolling volatility sits in its own recent distribution (`RegimeLabel`).
- **Jump Indicator** — flags return outliers beyond `threshold ×` trailing return volatility (−1 down / 0 / +1 up) (`JumpIndicator`).
- **Trend Label** — discrete trend state from the sign of the rolling least-squares slope (−1 / 0 / +1) (`TrendLabel`).
- **High-Low Range** — bar high-low range as a fraction of close (scale-free per-bar volatility) (`HighLowRange`).
- **Wick Ratio** — signed upper-vs-lower shadow imbalance as a fraction of the range (`WickRatio`).
- **Body Size Percent** — absolute candle body as a fraction of the bar range (`BodySizePct`).
- **Close vs Open** — signed body as a fraction of the open price, `(close − open) / open` (`CloseVsOpen`).
- **Spread AR(1) Coefficient** — first-order autoregression coefficient of the spread `a − b` (direct cointegration / mean-reversion strength) (`SpreadAr1Coefficient`).
- **Rolling Quantile** — interpolated q-th quantile over a trailing window (type-7 / NumPy default) (`RollingQuantile`).
- **Rolling Percentile Rank** — percentile rank of the latest value within its trailing window (`RollingPercentileRank`).
- **Rolling IQR** — interquartile range (Q3 − Q1) over a trailing window (robust dispersion) (`RollingIqr`).
- **Realized Volatility** — square root of the summed squared log returns (raw, un-annualised quadratic variation) (`RealizedVolatility`).
- **Log Return** — logarithmic return over a fixed lag, `ln(price_t / price_{t−period})` (`LogReturn`).

## [0.5.3] - 2026-06-04
- **Fibonacci Time Zones** — vertical markers at Fibonacci bar-distances (1/2/3/5/8/...) from the latest swing pivot (`FIB_TIME_ZONES`).
- **Fibonacci Channel** — a sloped base trendline plus parallel lines at Fibonacci multiples of the channel width (`FIB_CHANNEL`).
- **Fibonacci Arcs** — semicircular retracement levels centred on the swing end, normalised by leg bar-width (`FIB_ARCS`).
- **Fibonacci Fan** — three trendlines fanning from a swing start through its 38.2/50/61.8% retracement levels (`FIB_FAN`).
- **Fibonacci Confluence** — densest cluster of retracement levels across recent swing legs (price + strength) (`FIB_CONFLUENCE`).
- **Golden Pocket** — the 0.618-0.65 optimal-trade-entry band of the most recent swing leg (`GOLDEN_POCKET`).
- **Auto-Fibonacci** — retracement anchored on the dominant (largest-magnitude) leg among recent swings (`AUTO_FIB`).
- **Fibonacci Projection** — measured-move target zone from the last three pivots (A-B-C), projecting A->B from C (`FIB_PROJECTION`).
- **Fibonacci Extension** — projects the latest swing leg to the canonical extension ratios (127.2/141.4/161.8/200/261.8%) (`FIB_EXTENSION`).
- **Fibonacci Retracement** — seven retracement levels (0/23.6/38.2/50/61.8/78.6/100%) of the most recent confirmed swing leg (`FIB_RETRACEMENT`).

## [0.5.2] - 2026-06-03

### Added
- **Three Drives** — three symmetric drives with extension legs; bullish +1, bearish -1 (`THREE_DRIVES`).
- **Cypher** — five-point harmonic whose D retraces XC by 0.786; bullish +1, bearish -1 (`CYPHER`).
- **Shark** — five-point harmonic with an expansion leg and 0.886-1.13 D; bullish +1, bearish -1 (`SHARK`).
- **Crab** — five-point harmonic with the deepest (1.618 XA) D completion; bullish +1, bearish -1 (`CRAB`).
- **Bat** — five-point harmonic with a shallow B and 0.886 D completion; bullish +1, bearish -1 (`BAT`).
- **Butterfly** — five-point harmonic with an extended (1.27-1.618 XA) D; bullish +1, bearish -1 (`BUTTERFLY`).
- **Gartley** — five-point harmonic with a 0.786 D completion; bullish +1, bearish -1 (`GARTLEY`).
- **AB=CD** — four-point AB=CD harmonic: BC retraces AB, CD mirrors AB; bullish +1, bearish -1 (`ABCD`).
- **Cup and Handle** — rounded base with a shallow handle near the rim; bullish +1, inverse -1 (`CUP_AND_HANDLE`).
- **Rectangle / Range** — flat support and resistance; mean-reversion signal off the just-touched boundary; support +1, resistance -1 (`RECTANGLE_RANGE`).
- **Flag / Pennant** — shallow consolidation against a sharp pole; continuation in the pole direction; bull +1, bear -1 (`FLAG_PENNANT`).
- **Wedge (rising/falling)** — both trendlines slope the same way but converge; rising wedge -1, falling wedge +1 (`WEDGE`).
- **Triangle (asc/desc/sym)** — converging trendlines; ascending +1, descending -1, symmetrical follows the last swing (`TRIANGLE`).
- **Head and Shoulders** — central head flanked by two matching shoulders over a flat neckline; top -1, inverse +1 (`HEAD_AND_SHOULDERS`).
- **Triple Top / Bottom** — three matching peaks / troughs; a stronger reversal than the double; bearish -1, bullish +1 (`TRIPLE_TOP_BOTTOM`).
- **Double Top / Bottom** — twin-peak / twin-trough reversal confirmed on the second matching swing extreme; bearish -1, bullish +1 (`DOUBLE_TOP_BOTTOM`).

## [0.5.1] - 2026-06-03

### Added — Seasonality & Session family (12 indicators)

- **Volume-by-Time Profile** — mean traded volume bucketed by intraday time (`VOLUME_BY_TIME_PROFILE`).
- **Intraday Volatility Profile** — return standard deviation bucketed by intraday time (`INTRADAY_VOLATILITY_PROFILE`).
- **Day-of-Week Profile** — mean bar return bucketed by weekday (`DAY_OF_WEEK_PROFILE`).
- **Time-of-Day Return Profile** — mean bar return bucketed by intraday time (`TIME_OF_DAY_RETURN_PROFILE`).
- **Seasonal Z-Score** — z-score of the current return versus the same hour-of-day history (`SEASONAL_Z_SCORE`).
- **Turn-of-Month** — mean daily return inside the turn-of-month window (`TURN_OF_MONTH`).
- **Overnight/Intraday Return** — decomposition of session return into overnight and intraday legs (`OVERNIGHT_INTRADAY_RETURN`).
- **Overnight Gap** — close-to-open return across the session boundary (`OVERNIGHT_GAP`).
- **Average Daily Range** — mean high-low range of the last N completed sessions (`AVERAGE_DAILY_RANGE`).
- **Session Range** — per-session (Asia/EU/US) high-low range (`SESSION_RANGE`).
- **Session High/Low** — running high and low of the current session (`SESSION_HIGH_LOW`).
- **Session VWAP** — session-anchored volume-weighted average price (`SESSION_VWAP`).

## [0.5.0] - 2026-06-03

### Added
- **TICK Index** — instantaneous net advancing-minus-declining issues (`TICK_INDEX`).
- **Absolute Breadth Index** — absolute value of net advancing-minus-declining issues (`ABSOLUTE_BREADTH_INDEX`).
- **Cumulative Volume Index** — running total of volume-normalised net advancing volume (`CUMULATIVE_VOLUME_INDEX`).
- **Bullish Percent Index** — percentage of the universe on a point-and-figure buy signal (`BULLISH_PERCENT_INDEX`).
- **Up/Down Volume Ratio** — advancing volume divided by declining volume (`UP_DOWN_VOLUME_RATIO`).
- **Percent Above Moving Average** — percentage of the universe trading above its reference moving average (`PERCENT_ABOVE_MA`).
- **High-Low Index** — moving average of the record-high percentage (`HIGH_LOW_INDEX`).
- **New Highs - New Lows** — net count of new period highs minus new period lows (`NEW_HIGHS_NEW_LOWS`).
- **Breadth Thrust** — moving average of the advancing-issues share (Zweig) (`BREADTH_THRUST`).
- **TRIN / Arms Index** — advance-decline ratio divided by the up-down volume ratio (`TRIN`).
- **McClellan Summation Index** — running cumulative total of the McClellan Oscillator (`MCCLELLAN_SUMMATION_INDEX`).
- **McClellan Oscillator** — spread between a 19- and 39-period EMA of ratio-adjusted net advances (`MCCLELLAN_OSCILLATOR`).
- **Advance/Decline Volume Line** — cumulative net advancing-minus-declining volume across the universe (`AD_VOLUME_LINE`).
- **Advance/Decline Ratio** — advancing issues divided by declining issues across the universe (`ADVANCE_DECLINE_RATIO`).

### Changed
- **Relicensed** from PolyForm Noncommercial 1.0.0 to dual **MIT OR Apache-2.0**. Wickra is now OSI-approved, permissive open source; commercial use is permitted under either license. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).

## [0.4.7] - 2026-06-03

### Added
- **Spread Bollinger Bands** — Bollinger bands on the spread of two series for pairs mean-reversion (`SPREAD_BOLLINGER_BANDS`).
- **Kalman Hedge Ratio** — Kalman-filter dynamic hedge ratio and spread between two series (`KALMAN_HEDGE_RATIO`).
- **Granger Causality** — Granger causality F-statistic measuring whether one series predicts another (`GRANGER_CAUSALITY`).
- **Variance Ratio** — Lo-MacKinlay variance-ratio test on the spread of two series (`VARIANCE_RATIO`).
- **Beta-Neutral Spread** — beta-neutral spread: the rolling OLS regression residual of two series (`BETA_NEUTRAL_SPREAD`).
- **Distance SSD** — Gatev sum-of-squared-deviations distance between two normalised series (`DISTANCE_SSD`).
- **Spread Hurst** — Hurst exponent of the spread of two series for regime detection (`SPREAD_HURST`).
- **OU Half-Life** — Ornstein-Uhlenbeck half-life of mean reversion for the spread of two series (`OU_HALF_LIFE`).
- **Rolling Covariance** — rolling covariance of the period-over-period returns of two series (`ROLLING_COVARIANCE`).
- **Rolling Correlation** — rolling Pearson correlation of the period-over-period returns of two series (`ROLLING_CORRELATION`).

- **Market Breadth family** — a new indicator family built on a new
  `CrossSection` input type that carries the per-symbol state of an entire
  universe in one tick (each `Member` holds a signed `change`, a `volume`, and
  `new_high` / `new_low` flags). `CrossSection::new` validates the universe
  (non-empty, finite changes, finite non-negative volumes); `new_unchecked`
  skips validation for hot paths.
  - `AdvanceDecline` (`ADVANCE_DECLINE`) — the Advance/Decline Line, the running
    cumulative sum of net advancing-minus-declining issues across the universe.

## [0.4.6] - 2026-06-03

### Added

- **TA-Lib parity — Directional Movement components** — the ADX building blocks,
  previously available only bundled inside `Adx`, as standalone single-output
  indicators:
  - `PlusDm` (`PLUS_DM`) — Wilder-smoothed plus directional movement.
  - `MinusDm` (`MINUS_DM`) — Wilder-smoothed minus directional movement.
  - `PlusDi` (`PLUS_DI`) — plus directional indicator, `100 · smoothed(+DM) / ATR`.
  - `MinusDi` (`MINUS_DI`) — minus directional indicator, `100 · smoothed(-DM) / ATR`.
  - `Dx` (`DX`) — directional movement index, `100 · |+DI − −DI| / (+DI + −DI)`.
- **TA-Lib parity — price transforms** — window and per-bar price aggregates:
  - `MidPrice` (`MIDPRICE`) — `(highest high + lowest low) / 2` over a window.
  - `MidPoint` (`MIDPOINT`) — `(max + min) / 2` of a scalar series over a window.
  - `AvgPrice` (`AVGPRICE`) — per-bar `(open + high + low + close) / 4`.
- **TA-Lib parity — rate-of-change variants** — the ratio forms of `Roc`:
  - `Rocp` (`ROCP`) — `(close − close[period]) / close[period]` (fraction).
  - `Rocr` (`ROCR`) — `close / close[period]` (ratio).
  - `Rocr100` (`ROCR100`) — `close / close[period] · 100`.
- **TA-Lib parity — linear-regression outputs** — the remaining OLS endpoints:
  - `LinRegIntercept` (`LINEARREG_INTERCEPT`) — the OLS intercept `a`.
  - `Tsf` (`TSF`) — time series forecast, `a + b·period` (one bar ahead).
- **TA-Lib parity — `MacdFix` (`MACDFIX`)** — MACD with fast/slow fixed at 12/26
  and only the signal period configurable; output is the usual `{macd, signal,
  histogram}` triple.
- **TA-Lib parity — `SarExt` (`SAREXT`)** — Parabolic SAR with a start value,
  reversal offset, independent long/short acceleration, and a signed output
  (positive in long phases, negative in short phases).
- **TA-Lib parity — `MacdExt` (`MACDEXT`)** — MACD with an independently
  selectable moving-average type (new `MaType` enum: SMA/EMA/WMA/DEMA/TEMA/TRIMA)
  for each of the fast, slow and signal lines.
- **TA-Lib parity — `HtPhasor` (`HT_PHASOR`)** — the in-phase and quadrature
  components of the Hilbert-transform analytic signal, as a `{inphase,
  quadrature}` pair.
- **TA-Lib parity — `HtDcPhase` (`HT_DCPHASE`)** — the phase angle (in degrees)
  of the Hilbert-transform dominant cycle.
- **TA-Lib parity — `HtTrendMode` (`HT_TRENDMODE`)** — Ehlers' trend (`1`) vs
  cycle (`0`) classification from the Hilbert-transform dominant cycle.

## [0.4.5] - 2026-06-02

### Added

- **Anchored RSI** — a cumulative Relative Strength Index whose averaging begins at a runtime-chosen anchor bar (`set_anchor`), the momentum counterpart to Anchored VWAP. Every up- and down-move since the anchor is weighted equally, so it reports the RSI of the entire move since the anchor point. Scalar input, Momentum Oscillators family; available in Rust, Python, Node and WASM.
- **Volume Profile** — the full per-bin volume distribution over a rolling window, exposing the raw histogram (price bounds plus per-bin volume) that Value Area reduces to POC/VAH/VAL. Market Profile family; candle input, available in Rust, Python, Node and WASM.
- **TPO Profile** — the Time-Price-Opportunity (market-profile letter) distribution: a volume-agnostic count of how many periods traded at each price level over a rolling window. Market Profile family; candle input, available in Rust, Python, Node and WASM.
- **Alt-Chart Bars** — a new `BarBuilder` trait and family of price-driven chart constructors that emit a variable number of completed bars per candle (so they are deliberately not `Indicator`s): **Renko** (fixed box-size bricks with the 2-box reversal rule), **Kagi** (reversal-amount line segments), and **Point & Figure** (box-size X/O columns with an N-box reversal). Available in Rust, Python, Node and WASM.

## [0.4.4] - 2026-06-02

### Added
- **TA-Lib candlestick patterns (part 1).** New candlestick pattern detectors
  matching TA-Lib `CDL*`, emitting the family's signed `+1 / 0 / −1` convention
  over OHLCV candles in Rust, Python, Node and WASM:
  - **Two Crows** — a three-bar bearish reversal (`CDL2CROWS`): a long white
    candle, a black candle whose body gaps up, then a black candle that opens
    inside the second's body and closes inside the first's.
  - **Upside Gap Two Crows** — a three-bar bearish reversal
    (`CDLUPSIDEGAP2CROWS`): two black candles gap up over a long white candle,
    the second engulfing the first crow yet still closing above the white body,
    leaving the upside gap open.
  - **Identical Three Crows** — a three-bar bearish reversal
    (`CDLIDENTICAL3CROWS`): three red candles with steadily lower closes, each
    opening at the prior candle's close so the bodies stack in an identical
    staircase.
  - **Three Line Strike** — a four-bar pattern (`CDL3LINESTRIKE`): a
    three-candle advance or decline struck by a fourth opposite-colour candle
    that engulfs the entire run; bullish `+1`, bearish `−1`.
  - **Three Stars in the South** — a rare three-bar bullish reversal
    (`CDL3STARSINSOUTH`): three shrinking red candles each carving a higher low
    and contracting toward a tiny black marubozu as selling exhausts.
  - **Abandoned Baby** — a strong three-bar reversal (`CDLABANDONEDBABY`): a doji
    isolated by price gaps on both sides; bullish `+1` after a decline, bearish
    `−1` after an advance.
  - **Advance Block** — a three-bar bearish warning (`CDLADVANCEBLOCK`): three
    green candles to higher closes whose bodies shrink as their upper shadows
    lengthen, signalling the advance is stalling.
  - **Belt-hold** — a single-bar reversal that opens at one extreme of its range and runs the other way; bullish +1, bearish -1 (`CDLBELTHOLD`).
  - **Breakaway** — a 5-bar reversal that gaps with the trend, drifts two more bars, then snaps back into the bar1/bar2 body gap; bullish +1, bearish -1 (`CDLBREAKAWAY`).
  - **Counterattack** — a 2-bar reversal where an opposite-coloured second bar closes level with the first (the counterattack line); bullish +1, bearish -1 (`CDLCOUNTERATTACK`).
  - **Doji Star** — a long body followed by a doji gapping away in the trend direction; bullish +1, bearish -1 (`CDLDOJISTAR`).
  - **Dragonfly Doji** — a doji opening and closing at the high with a long lower shadow, a bullish reversal; +1 (`CDLDRAGONFLYDOJI`).
  - **Gravestone Doji** — a doji opening and closing at the low with a long upper shadow, a bearish reversal; -1 (`CDLGRAVESTONEDOJI`).
  - **Long-Legged Doji** — a doji with long shadows on both sides, an indecision signal; +1 detection (`CDLLONGLEGGEDDOJI`).
  - **Rickshaw Man** — a long-legged doji with the body centred in the range, an indecision signal; +1 detection (`CDLRICKSHAWMAN`).
  - **Evening Doji Star** — a bearish top reversal: long white bar, a doji gapping up, then a black bar closing deep into the first body; -1 (`CDLEVENINGDOJISTAR`).
  - **Morning Doji Star** — a bullish bottom reversal: long black bar, a doji gapping down, then a white bar closing deep into the first body; +1 (`CDLMORNINGDOJISTAR`).
  - **Gap Side-by-Side White** — two similar white candles opening side by side after a gap, a continuation; gap up +1, gap down -1 (`CDLGAPSIDESIDEWHITE`).
  - **High-Wave** — a small body with very long shadows on both sides, an extreme indecision signal; +1 detection (`CDLHIGHWAVE`).
  - **Hikkake** — an inside bar followed by a failed breakout, a trap; bullish +1, bearish -1 (`CDLHIKKAKE`).
  - **Modified Hikkake** — a close-confirmed Hikkake: an inside bar then a failed breakout closing back inside; bullish +1, bearish -1 (`CDLHIKKAKEMOD`).
  - **Homing Pigeon** — two black candles, the second a small body inside the first, a bullish reversal; +1 (`CDLHOMINGPIGEON`).
  - **On-Neck** — a long black candle then a white candle closing at its low (the neckline), a bearish continuation; -1 (`CDLONNECK`).
  - **In-Neck** — a long black candle then a white candle closing just into its body, a bearish continuation; -1 (`CDLINNECK`).
  - **Thrusting** — a long black candle then a white candle closing well into but below the midpoint of its body, a bearish continuation; -1 (`CDLTHRUSTING`).
  - **Separating Lines** — opposite-coloured candles sharing the same open, the second an opening marubozu resuming the trend; bullish +1, bearish -1 (`CDLSEPARATINGLINES`).
  - **Kicking** — two opposite-coloured marubozu separated by a gap; bullish +1, bearish -1 (`CDLKICKING`).
  - **Kicking by Length** — a kicking pattern signalled by the colour of the longer marubozu; +1 / -1 (`CDLKICKINGBYLENGTH`).
  - **Ladder Bottom** — three descending black candles, a fourth with an upper shadow, then a white candle gapping up, a bullish reversal; +1 (`CDLLADDERBOTTOM`).
  - **Mat Hold** — a long white candle, a holding three-bar pullback, then a new-high white candle, a bullish continuation; +1 (`CDLMATHOLD`).
  - **Matching Low** — a 2-bar bullish reversal where two black candles in a decline share the same close, signalling selling pressure is exhausting; bullish +1 (`CDLMATCHINGLOW`).
  - **Long Line** — a single long-bodied candle with short shadows; bullish +1 (white) or bearish -1 (black) by colour (`CDLLONGLINE`).
  - **Short Line** — a single short-bodied candle with short shadows; bullish +1 (white) or bearish -1 (black) by colour (`CDLSHORTLINE`).
  - **Rising Three Methods** — a 5-bar bullish continuation: a long white candle, three small pullback bars holding within its range, then a white breakout to new highs; bullish +1 (`CDLRISEFALL3METHODS`).
  - **Falling Three Methods** — the bearish mirror of rising three methods: a long black candle, three small bars holding within its range, then a black breakdown to new lows; bearish -1 (`CDLRISEFALL3METHODS`).
  - **Upside Gap Three Methods** — a 3-bar bullish continuation: two white candles gap up, then a black candle opens within the second body and closes within the first; bullish +1 (`CDLXSIDEGAP3METHODS`).
  - **Downside Gap Three Methods** — the bearish mirror of upside gap three methods: two black candles gap down, then a white candle opens within the second body and closes within the first; bearish -1 (`CDLXSIDEGAP3METHODS`).
  - **Stalled Pattern** — a 3-bar bearish reversal warning: two long white candles then a small white candle riding the shoulder, signalling the rally is stalling; bearish -1 (`CDLSTALLEDPATTERN`).
  - **Stick Sandwich** — a 3-bar bullish reversal: two black candles closing at the same level sandwich a white candle, marking a support floor; bullish +1 (`CDLSTICKSANDWICH`).
  - **Takuri** — a single-bar bullish reversal, a strict Dragonfly Doji with a negligible upper shadow and very long lower shadow; bullish +1 (`CDLTAKURI`).
  - **Closing Marubozu** — a single long-bodied candle with no shadow on the close end; bullish +1 (white, closes at the high) or bearish -1 (black, closes at the low) (`CDLCLOSINGMARUBOZU`).
  - **Opening Marubozu** — a single long-bodied candle with no shadow on the open end; bullish +1 (white, opens at the low) or bearish -1 (black, opens at the high). No direct TA-Lib equivalent — completes the pair with the closing marubozu.
  - **Tasuki Gap** — a 3-bar continuation: two same-coloured candles gap in the trend direction, then an opposite candle opens within the second body and closes back into the gap without filling it; upside +1, downside -1 (`CDLTASUKIGAP`).
  - **Unique Three River** — a 3-bar bullish reversal: a long black candle, a black candle probing a new low with its body inside the first, then a small white candle held below it; bullish +1 (`CDLUNIQUE3RIVER`).
  - **Concealing Baby Swallow** — a rare 4-bar bullish capitulation: two black marubozu, a black candle gapping down with an upper shadow into the second, then a large black candle engulfing it entirely; bullish +1 (`CDLCONCEALBABYSWALL`).
- **Derivatives family — funding & open interest (part 1).** A new family of
  indicators that consume a perpetual / futures tick (`DerivativesTick`,
  bundling funding rate, mark / index / futures price, open interest,
  positioning, taker flow and liquidations) rather than OHLCV, exposed in Rust,
  Python, Node and WASM:
  - **Funding Rate** — the current perpetual funding rate.
  - **Funding Rate Mean** — the rolling mean funding rate over a window.
  - **Funding Rate Z-Score** — the latest funding rate in standard deviations
    from its rolling mean.
  - **Funding Basis** — the perpetual's relative premium to spot,
    `(markPrice − indexPrice) / indexPrice`.
  - **Open-Interest Delta** — the tick-over-tick change in open interest.
- **Derivatives family — open interest, flow & liquidations (part 2).** More
  indicators over the same `DerivativesTick` feed:
  - **OI / Price Divergence** — relative open-interest change minus relative
    price change over a window, the positioning-vs-price gap.
  - **OI-Weighted Price** — the cumulative mark price weighted by open interest.
  - **Long/Short Ratio** — aggregate long size over short size.
  - **Taker Buy/Sell Ratio** — taker buy volume over taker sell volume.
  - **Liquidation Features** — a multi-output breakdown of long/short
    liquidation notional into net, total and a bounded imbalance.
- **Derivatives family — basis & term structure (part 3).** The final
  perpetual-vs-futures basis indicators over the `DerivativesTick` feed:
  - **Term-Structure Basis** — the dated future's relative premium to spot,
    `(futuresPrice − indexPrice) / indexPrice`.
  - **Calendar Spread** — the dated future's relative premium to the perpetual,
    `(futuresPrice − markPrice) / markPrice`.

## [0.4.3] - 2026-06-01

### Added
- **Microstructure family — price impact & depth (part 3).** Indicators over a
  trade paired with the prevailing mid (`TradeQuote`) and over the order-book
  depth profile, exposed in Rust, Python, Node and WASM:
  - **Effective Spread** — `2 · D · (tradePrice − mid) / mid · 10_000` bps, the
    realised round-trip cost of a single trade against the mid.
  - **Realized Spread** — `2 · D · (tradePrice − mid_{t+horizon}) / mid_t ·
    10_000` bps, the share of the effective spread a liquidity provider keeps
    once the mid has moved over a configurable horizon.
  - **Kyle's Lambda** — the rolling OLS slope of mid changes on signed volume
    (`cov(Δmid, q) / var(q)`), the canonical price-impact / market-depth proxy.
  - **Depth Slope** — the mean per-side OLS slope of cumulative resting size
    against distance from the mid, measuring how fast the book thickens away
    from the touch.
- **Microstructure family — footprint (part 4).** **Footprint** decomposes the
  volume traded in a bar across price buckets (`round(price / tick_size)`),
  splitting each bucket into buy-initiated (ask) and sell-initiated (bid)
  volume. A multi-output, variable-length indicator: every `update` returns the
  full footprint accumulated since the last `reset`, exposed in Rust, Python
  (`(k, 3)` arrays), Node (`{ price, bidVol, askVol }` rows) and WASM.

## [0.4.2] - 2026-06-01

### Added
- **Microstructure family — order book (part 1).** A new family of indicators
  that consume an order-book depth snapshot (`OrderBook` of sorted, uncrossed
  bid/ask `Level`s) rather than OHLCV, exposed in Rust, Python, Node and WASM:
  - **Order-Book Imbalance** — `OrderBookImbalanceTop1`, `OrderBookImbalanceTopN`
    (configurable depth) and `OrderBookImbalanceFull` measure signed depth
    pressure `(bidDepth − askDepth) / (bidDepth + askDepth)` over the top level,
    the top-N levels, or the full book.
  - **Microprice** — the size-weighted fair value
    `(bidPx·askSz + askPx·bidSz) / (bidSz + askSz)`, tilting the mid toward the
    side more likely to be hit.
  - **Quoted Spread** — the top-of-book spread in basis points of the mid.
- **Microstructure family — trade flow (part 2).** Indicators over a trade tape
  (`Trade` with an aggressor `Side`), exposed in Rust, Python, Node and WASM:
  - **Signed Volume** — per-trade size signed by aggressor side (`+size` buy,
    `−size` sell).
  - **Cumulative Volume Delta** — the running total of signed volume; reset to
    re-anchor per session.
  - **Trade Imbalance** — the rolling `(buyVol − sellVol)/(buyVol + sellVol)`
    over a configurable window of trades.

  New public value types `Level`, `OrderBook`, `Side`, `Trade` and `TradeQuote`
  back this and the upcoming trade-flow and price-impact indicators. Python and
  Node accept a batch over a list of snapshots; WASM exposes per-snapshot
  `update`.
- **Signed Doji encoding.** `Doji` gains an opt-in `.signed()` mode
  (`Doji(signed=True)` in Python, `new Doji(true)` in Node and WASM) that
  classifies a detected Doji by the position of its body within the bar range —
  a dragonfly (long lower shadow) emits `+1.0` (bullish), a gravestone (long
  upper shadow) emits `−1.0` (bearish), and a long-legged / standard Doji emits
  `0.0` (neutral). The default construction is unchanged — a direction-less
  `+1.0` / `0.0` detection flag — so existing callers are unaffected. This
  completes the uniform `+1` bull / `−1` bear / `0` none sign convention across
  every candlestick pattern, making the family a drop-in machine-learning
  feature where bullish and bearish instances share a single dimension.

### Fixed
- **README banner now self-updates.** The top README banner points at the org
  profile image that `.github/banner.yml` regenerates from the indicator count,
  and `sync-about.yml` bumps a `?v=<count>` cache-buster so GitHub's Camo proxy
  refetches it immediately. Also fixes the webpage indicator-count sync, which
  silently crashed on a removed `public/hero.svg` and left the marketing site's
  count (and its OG banner) stale.

### Security
- **CI dependency installs are pinned by hash.** The Node binding now installs
  with `npm ci` (strict `package-lock.json`), and the Python CI/bench tooling is
  installed from hash-locked `--require-hashes` requirements under
  `.github/requirements/` (OpenSSF Scorecard PinnedDependencies). The `ci-dev`
  tooling is locked twice — for Python 3.9 and for 3.10+ — because numpy ships no
  single release with wheels for both cp39 and cp313. A new
  `scripts/update-lockfiles.sh` regenerates every workspace lockfile (Rust, Node
  and the hash-pinned Python requirements) via `uv`, and Dependabot keeps the
  pinned requirements current.

## [0.4.1] - 2026-06-01

### Added
- **Cross-asset pairwise indicators.** A new two-series family of
  `Indicator<Input = (f64, f64)>` implementations that relate two distinct
  assets rather than a single OHLCV stream. Each is exposed in Rust, Python,
  Node, and WASM:
  - **Pairwise Beta** (`PairwiseBeta`) — rolling OLS slope of one asset's
    **log-returns** on another's. Unlike `Beta`, which regresses the raw inputs
    it is fed, `PairwiseBeta` differences consecutive prices into log-returns
    internally — the conventional way to measure cross-asset beta, where a beta
    on price levels would be dominated by the shared trend.
  - **Pair Spread Z-Score** (`PairSpreadZScore`) — the standardised log-spread
    `ln(a) − β·ln(b)` of a pair, where `β` is a rolling-OLS hedge ratio and the
    spread is z-scored over its own look-back. The canonical mean-reversion /
    statistical-arbitrage entry signal, with independent `beta_period` and
    `z_period` windows.
  - **Lead–Lag Cross-Correlation** (`LeadLagCrossCorrelation`) — the integer
    offset `k ∈ [−max_lag, max_lag]` that maximises `|corr(a[t], b[t+k])|`,
    answering which of two assets leads the other and by how many bars. Emits
    `{ lag, correlation }`; a positive lag means `a` leads `b`.
  - **Cointegration** (`Cointegration`) — the Engle–Granger two-step screen for
    pairs trading: a rolling OLS hedge ratio `β`, the spread (residual)
    `a − (α + β·b)`, and an augmented Dickey–Fuller `t`-statistic on the spread
    (configurable `adf_lags`). A strongly negative statistic flags a
    mean-reverting, tradeable spread. Emits `{ hedge_ratio, spread, adf_stat }`.
  - **Relative Strength A-vs-B** (`RelativeStrengthAB`) — the comparative
    relative strength of two assets: the ratio line `a / b` together with its
    moving average and its RSI, the classic asset-vs-asset / asset-vs-index
    rotation screen. Emits `{ ratio, ratio_ma, ratio_rsi }`.

## [0.4.0] - 2026-06-01

### Added
- **Build-provenance attestations for release artifacts.** The release workflow
  now emits signed SLSA build-provenance attestations for the published crates
  and Python wheels/sdist (`actions/attest-build-provenance`); npm packages
  carry inline Sigstore provenance from `npm publish --provenance`. Every
  published artifact is cryptographically traceable to this repository's release
  workflow run.

### Security
- **CodeQL static analysis and OpenSSF Scorecard run in CI.** CodeQL (Rust,
  Python, JavaScript) and the OpenSSF Scorecard workflow now run on every push;
  results appear under Security → Code scanning and a public Scorecard badge is
  shown in the README.
- **CI workflows hardened against script injection.** Untrusted event contexts
  (PR branch names, `workflow_dispatch` inputs) are passed through the step
  environment instead of being interpolated directly into shell commands.

### Changed
- **Node binding: invalid indicator periods now throw instead of being silently
  clamped.** The scalar-indicator constructors previously clamped `period = 0`
  to `1`; every Node constructor now propagates the core's validation error
  (e.g. `period must be greater than zero`), matching the Python and WASM
  bindings and the Rust core. Constructing with a valid period is unaffected.
- **Binding package READMEs are now per-ecosystem.** The Python, Node.js, and
  WebAssembly READMEs were byte-identical 314-line copies of the workspace
  README and had drifted out of sync (stale indicator count, Python snippets
  shown on the Node and WASM package pages). Each is now a focused landing page
  with the correct install command, a language-correct quick-start snippet, and
  links to the canonical documentation — removing the manual three-way sync
  burden. No code or API changes.
- **CONTRIBUTING now states the correct MSRV (1.86 workspace / 1.88
  `bindings/node`)** and documents that these are the dependency-forced floors,
  kept minimal on purpose. The previous text claimed 1.75 / 1.77, which the
  `msrv` CI job has enforced against since the criterion and napi-build bumps.

## [0.3.1] - 2026-05-30

### Fixed
- **Release pipeline — CycloneDX SBOM generation.** `cargo-cyclonedx` has no
  `-p`/`--package` selector; it walks the whole workspace in a single pass.
  The `release.yml` SBOM step invoked it as `cargo cyclonedx … -p <crate>` and
  aborted with `error: unexpected argument '-p' found`, which failed the
  crates.io publish job *after* the crates were already published and skipped
  the GitHub Release attach-assets job (no release page, no SBOM artefacts).
  The step now runs a single workspace pass and collects the three crates.io
  crate SBOMs. No library changes relative to 0.3.0 — this patch republishes
  the same code with a working release pipeline.

## [0.3.0] - 2026-05-30

### Added
- **Family 15 — Risk / Performance metrics (17 new indicators).** Implemented
  pragmatically as standard `Indicator`s rather than a separate
  `wickra-metrics` crate; the input is a scalar `f64` per bar (period return,
  equity sample, or trade P&L depending on the metric).
  - **Scalar `Indicator<f64>` — 14 metrics:** Sharpe Ratio, Sortino Ratio,
    Calmar Ratio, Omega Ratio, Max Drawdown (rolling), Average Drawdown,
    Drawdown Duration (time-under-water), Pain Index, Value at Risk
    (historical, linear-interpolated percentile), Conditional Value at Risk
    (Expected Shortfall), Profit Factor, Gain/Loss Ratio, Recovery Factor,
    Kelly Criterion.
  - **Two-series `Indicator<(f64, f64)>` — 3 metrics on `(asset_return,
    benchmark_return)` pairs:** Treynor Ratio, Information Ratio,
    Jensen's Alpha (CAPM).
- **Candlestick patterns family (15 indicators).** A new "Candlestick
  Patterns" family covers the standard 1- to 3-bar reversal and
  continuation shapes: `Doji`, `Hammer`, `InvertedHammer`, `HangingMan`,
  `ShootingStar`, `Engulfing`, `Harami`, `MorningEveningStar`,
  `ThreeSoldiersOrCrows`, `PiercingDarkCloud`, `Marubozu`, `Tweezer`,
  `SpinningTop`, `ThreeInside` and `ThreeOutside`. Every detector takes a
  `Candle` and emits a signed `f64` (`+1.0` bullish, `-1.0` bearish, `0.0`
  no pattern; `Doji` is direction-less and emits `+1.0`/`0.0`). The MVP is
  a pattern-shape check only — no trend filter is applied. Available
  across Rust, Python, Node and WASM bindings. Harmonic and chart
  patterns remain out of scope and will follow once the pattern-detection
  framework (pivot detector + multi-bar state machines) lands.
- **Market Profile family** (3 new indicators, opens family #9 across the
  catalogue):
  - `ValueArea(period, bin_count, value_area_pct)` — rolling
    bin-approximation volume profile over the last `period` candles.
    Outputs `{poc, vah, val}`: Point of Control is the bin with the highest
    cumulative volume; the Value Area expands symmetrically from POC and
    always absorbs the higher-volume neighbour next, until the configured
    percentage of total volume (default 70%) is enclosed. Each candle's
    volume is spread uniformly across its `[low, high]` range; single-print
    bars (`low == high`) drop their entire volume into one bin.
  - `InitialBalance(period)` — first-N-bar session high / low, frozen
    once `period` bars have been ingested. Outputs `{high, low}`. Default
    `period = 12` (one-hour IB on 5-minute bars for US equities). Callers
    MUST invoke `reset()` at every session boundary, otherwise the IB
    locks and stays fixed for the lifetime of the instance.
  - `OpeningRange(period)` — same lock-after-N-bars semantics as IB but
    with a smaller default window (`period = 6`, 30 min on 5-minute
    bars) and a third output `breakout_distance` = `close - or_mid`,
    signed (positive above the range, negative below).
- Histogram-output Market Profile variants (Volume Profile / VPVR /
  Composite Profile) and tick-data-only variants (TPO / Single Print /
  Cumulative Delta / Order Flow Delta / Volume-Weighted Open) are
  deliberately out of scope of this PR: the former need a new
  histogram-output API layer, the latter need tick / L2 data which
  `wickra-data` does not yet expose.
- **Family 12 — Statistik / Regression (13 indicators).** A complete
  statistical toolkit for analysing rolling price distributions and
  cross-series relationships. Every indicator ships in the Rust core
  plus all three bindings (Python, Node, WASM), with full streaming +
  batch parity, fuzz coverage, and benches against the BTCUSDT
  dataset:
  - **Variance** — rolling population variance (`StdDev` squared).
  - **CoefficientOfVariation** — `StdDev / Mean`, dimensionless dispersion.
  - **Skewness** — rolling third standardised moment (Pearson skewness).
  - **Kurtosis** — rolling excess kurtosis (fourth moment minus `3`).
  - **StandardError** — standard error of estimate for the rolling OLS
    fit, with `n − 2` residual degrees of freedom.
  - **DetrendedStdDev** — population standard deviation of OLS
    residuals (the StdDev that remains after subtracting the linear
    trend).
  - **RSquared** — coefficient of determination of the rolling OLS
    fit; the trend-quality filter.
  - **MedianAbsoluteDeviation** — robust dispersion measure that
    survives outliers (median of absolute deviations from the median).
  - **Autocorrelation** — rolling lag-`k` Pearson autocorrelation;
    detects periodicity and tests for white-noise behaviour.
  - **HurstExponent** — R/S-analysis estimator of trend-persistence
    vs. mean-reversion regime (`0.5` is random walk).
  - **PearsonCorrelation** — rolling correlation between two
    synchronised series; takes `(x, y)` pairs.
  - **Beta** — rolling OLS slope of an asset on a benchmark; the CAPM
    sensitivity coefficient.
  - **SpearmanCorrelation** — rolling rank correlation (monotone,
    outlier-robust analogue of Pearson).

  Indicator count: 71 → 84.
- **Family 13 — Ichimoku & alternative charts.** Two new indicators:
  - `Ichimoku` (Ichimoku Kinko Hyo) — the full five-line cloud system
    (Tenkan-sen, Kijun-sen, Senkou Span A/B, Chikou Span) with the
    classic `(9, 26, 52, 26)` defaults and configurable periods. Forward
    displacement is handled in a streaming ring buffer so the
    currently-visible Senkou A/B at bar *n* are the values computed
    from bar *n − displacement*.
  - `HeikinAshi` — the candle smoothing transform that recursively
    averages OHLC into a four-component output (`ha_open`, `ha_high`,
    `ha_low`, `ha_close`). Seeds `ha_open` from the first bar's
    `(open + close) / 2`.

  Exposed in all four bindings (Rust, Python, Node, WASM). Renko,
  Kagi, and Point & Figure from the family ideas list are deferred:
  they are custom bar generators rather than indicators and belong in
  `wickra-data`.
- **Family 10 — Ehlers / Cycle (DSP) indicators.** 16 new
  streaming-first indicators implementing John Ehlers'
  digital-signal-processing school of cycle analytics — a strong
  differentiation feature versus TA-Lib and pandas-ta, which only
  ship fragments of this catalogue:
  - **MAMA / FAMA** (MESA Adaptive Moving Average + Following
    Adaptive Moving Average) — phase-rate-adaptive smoothing pair
    from the 2001 MESA paper, exposed both jointly via `Mama` (multi-
    output) and as a scalar `Fama` wrapper.
  - **Fisher Transform** and **Inverse Fisher Transform** — Gaussian
    normalisation of price (Ehlers 2002) and its tanh-based bounded
    counterpart for oscillators.
  - **SuperSmoother**, **Roofing Filter**, **Decycler** and **Decycler
    Oscillator** — 2-pole Butterworth lowpass, bandpass and
    high-pass complement building blocks from *Cycle Analytics for
    Traders* (2013).
  - **Hilbert Dominant Cycle**, **Sine Wave** and **Adaptive Cycle**
    — Hilbert-transform-based period estimation from *Rocket Science
    for Traders* (2001).
  - **Center of Gravity**, **Cybernetic Cycle Component**,
    **Instantaneous Trendline**, **Ehlers Stochastic** and
    **Empirical Mode Decomposition** — EasyLanguage classics from
    Ehlers' published catalogue.
  - All sixteen are exposed across Rust, Python, Node.js and WASM
    bindings, fuzz-tested, benchmarked against real BTCUSDT
    1-minute data, and pass `batch == streaming` equivalence.
  - Indicator count rises from 71 to **87** across **nine** families.
- **DeMark family (family 11) — 12 new indicators.** TD Setup (9-bar
  buy/sell setup counter with parameterised lookback and target), TD
  Sequential (Setup + Countdown phase machine emitting setup count,
  countdown count and active countdown direction), TD DeMarker
  (bounded [0, 1] range oscillator built from high/low expansions),
  TD REI (Range Expansion Index — bounded ±100 oscillator with the
  classic 5-bar default), TD Pressure (volume-weighted buying /
  selling pressure normalised to ±100), TD Combo (aggressive
  countdown variant with extra monotone-low / monotone-close
  strictness conditions on top of the classic countdown rule), TD
  Countdown (standalone 13-bar countdown phase machine emitting
  only the signed countdown count and direction — smaller streaming
  payload than the full TD Sequential), TD Lines (TDST horizontal
  support / resistance levels derived from the highs and lows of
  the most-recently-completed setup), TD Range Projection (next-bar
  high / low projection from the current bar's OHLC via DeMark's
  open-vs-close-weighted pivot), TD Differential (2-bar
  buying-pressure-vs-selling-pressure reversal pattern emitting
  +1 / -1 / 0), TD Open (gap-and-fade reversal pattern emitting
  +1 / -1 / 0 when the open prints outside the prior bar's range
  but the subsequent action recovers back into it), and TD Risk
  Level (protective stop levels derived from the lowest-low / highest-
  high setup bar's true range). All twelve are exposed through the
  Rust, Python, Node, and WASM bindings with `batch == streaming`
  equivalence tests, candle-stream fuzz coverage, and benchmark
  entries on the BTCUSDT 1-minute dataset.
- **Family 08 — Pivots & Support/Resistance.** Seven new indicators land
  the previously empty pivot family: Classic (Floor-Trader) Pivot Points
  with three resistance and support tiers, Fibonacci Pivots spaced by
  0.382 / 0.618 / 1.000 of the prior range, Camarilla Pivots
  (Nick Stott's four-tier `(H − L) · 1.1 / {12, 6, 4, 2}` levels),
  Woodie Pivots with the close-weighted `PP = (H + L + 2·C) / 4`,
  DeMark Pivots whose conditional `X` depends on whether the bar closed
  up, down or flat, Williams Fractals as a five-bar swing detector and
  ZigZag as a percent-threshold swing tracker. Every level/swing is
  exposed across Rust, Python, Node and WASM with the standard
  `update` / `batch` / `reset` / `is_ready` / `warmup_period` surface
  and matching streaming-vs-batch and reference-value tests. The fuzz
  candle target now covers all seven.
- **Family 09 — Trailing Stops, seven new indicators.** Rounds out the
  trailing-stop family from 5 to 12: `HiLoActivator` (Crabel's
  SMA-of-high / SMA-of-low trail), `VoltyStop` (Cynthia Kase's
  extreme-anchor ATR stop), `YoyoExit` (long-only ATR trail with a
  re-entry trigger), `DonchianStop` (the original Turtle exit, lowest
  low / highest high), `PercentageTrailingStop` (fixed-percent trail),
  `StepTrailingStop` (round-number grid trail) and `RenkoTrailingStop`
  (block-anchored Renko-style trail). All wired into the four bindings
  (Rust, Python, Node, WASM), the streaming + batch fuzz targets, and
  the bench harness.
- **Klinger Volume Oscillator (KVO).** Stephen J. Klinger's trend-aware
  volume-force oscillator: `EMA(vf, fast) − EMA(vf, slow)` over a daily
  volume force scaled by cumulative-measurement ratio. Classic
  `(fast, slow) = (34, 55)` exposed via `Kvo::classic()`.
- **Volume Oscillator (VO).** Percent difference between a fast and a
  slow SMA of bar volume: `100 · (SMA(vol, fast) − SMA(vol, slow)) /
  SMA(vol, slow)`. Default `(14, 28)`.
- **Negative Volume Index (NVI).** Paul Dysart's cumulative index that
  only updates on volume-contraction bars (`volume_t < volume_{t−1}`),
  absorbing the percent close change on those quiet days. Fosback
  baseline `1000.0`, configurable via `Nvi::with_baseline`.
- **Positive Volume Index (PVI).** The complementary index that
  updates on volume-expansion bars (`volume_t > volume_{t−1}`).
- **Williams Accumulation/Distribution.** Larry Williams' volume-less
  cumulative flow that anchors to the previous close (true high/low) and
  classifies each bar as accumulation, distribution, or neutral by the
  sign of the close-to-close change.
- **Anchored VWAP.** A cumulative VWAP whose accumulation begins at a
  user-chosen anchor bar rather than the session open. Re-anchor at
  runtime via `AnchoredVwap::set_anchor` for click-to-anchor trader
  workflows.
- **Demand Index (Sibbet).** James Sibbet's smoothed buying-vs-selling
  pressure ratio in the streaming-friendly textbook form
  `EMA(volume · close-return · (1 + range/close), period)`.
- **Time Segmented Volume (TSV).** Don Worden's rolling sum of signed
  volume weighted by the close-to-close move: a window-sum measure of
  net accumulation/distribution.
- **Volume Zone Oscillator (VZO).** Walid Khalil's normalised
  volume-flow oscillator bounded in `[−100, 100]`, defined as
  `100 · EMA(signed_volume) / EMA(volume)`.
- **Market Facilitation Index (Bill Williams).** Per-bar
  `(high − low) / volume` — how much price movement the market produces
  per unit of volume.
- **ADXR (Average Directional Movement Index Rating)** in the Trend &
  Directional family. Wilder's directional-strength smoother: the
  average of the current `ADX` and the `ADX` from `period - 1` bars
  ago. Warmup is `3 * period - 1` (e.g. 41 for the default `period =
  14`). Shipped across all four bindings (Rust core, Python, Node,
  WASM) plus fuzz/test/bench coverage.
- **Random Walk Index (RWI)** in the Trend & Directional family. Mike
  Poulos' trend-vs.-random-walk gauge: for each lookback `i ∈ [2,
  period]` the ratio of actual displacement to the random-walk
  expectation `ATR_i * sqrt(i)` is taken; the per-bar output is the
  maximum across lookbacks for both the high (`RWI_High`) and low
  (`RWI_Low`) directions. Multi-output `(high, low)` across all four
  bindings; warmup `= period`.
- **Trend Intensity Index (TII)** in the Trend & Directional family.
  M.H. Pee's `[0, 100]` oscillator: the share of the most recent
  `dev_period` SMA-deviations that are positive, scaled to
  `[0, 100]`. Saturates at 100 on a pure uptrend, at 0 on a pure
  downtrend, and returns the neutral 50 on a perfectly flat market.
  Canonical Python defaults `(sma_period=60, dev_period=30)`; warmup
  `= sma_period + dev_period − 1`.
- **Wave Trend Oscillator (LazyBear)** in the Trend & Directional
  family. Two-line mean-reverting momentum gauge built from the
  typical price and three cascaded EMAs:
  `esa = EMA(ap, channel)`, `d = EMA(|ap − esa|, channel)`,
  `ci = (ap − esa) / (0.015 · d)`, `wt1 = EMA(ci, average)`,
  `wt2 = SMA(wt1, signal)`. `WaveTrend::classic()` exposes the
  LazyBear defaults `(channel = 10, average = 21, signal = 4)`;
  warmup `= 2 · channel + average + signal − 3` (42 for the classic
  defaults). Includes a sub-ULP flat-tolerance guard on `ci` so a
  perfectly flat market reports `(0, 0)` instead of the
  mathematically indeterminate `−1 / 0.015 = −66.67`. Multi-output
  `(wt1, wt2)` across all four bindings.
- **Family 05 — Bands & Channels (11 new indicators).** Eleven additional
  price-envelope overlays organised into the new "Bands & Channels"
  family, exposed across all four bindings (Rust, Python, Node, WASM):
  - `MaEnvelope` — SMA centerline with fixed-percent envelope (the oldest
    band overlay still in use).
  - `AccelerationBands` — Price Headley's momentum-biased bands that widen
    with the bar's relative range `(H − L) / (H + L)`.
  - `StarcBands` — Stoller Average Range Channel: SMA(close) ± k·ATR
    (Keltner's SMA-centerline sibling).
  - `AtrBands` — Close-anchored envelope of width `k · ATR`, the standard
    volatility-targeting stop/target band.
  - `HurstChannel` — SMA centerline wrapped by the rolling high-low range
    (Brian Millard / Hurst-cycle channel).
  - `LinRegChannel` — Linear-regression endpoint ± k·σ of the residuals,
    measuring dispersion about the *trend* rather than the mean.
  - `StandardErrorBands` — Linear regression with the OLS standard error
    (denominator `n − 2`) for prediction-interval bands.
  - `DoubleBollinger` — Kathy Lien's `±1σ` plus `±2σ` zone-partition setup.
  - `TtmSqueeze` — John Carter's BB-inside-KC squeeze flag paired with a
    detrended-close momentum reading.
  - `FractalChaosBands` — Bill Williams 5-bar fractal high/low envelope.
  - `VwapStdDevBands` — Cumulative VWAP with volume-weighted standard
    deviation bands.
  Indicator count rises from 71 to 82 across nine families; the README
  family table and the wiki overview/sidebar/warmup pages were updated to
  match.
- **Yang-Zhang Volatility.** Yang & Zhang (2000) gold-standard OHLC
  estimator: a convex blend of overnight (close-to-open), open-to-close
  and Rogers-Satchell variances. The blending factor
  `k = 0.34 / (1.34 + (n+1)/(n-1))` is the one that minimises
  estimator variance under driftless GBM with overnight gaps. The
  overnight and open-to-close pieces use sample variance (Bessel's
  correction, divisor `n−1`), so the indicator needs `period + 1` bars
  to emit. Output annualised to a percent. Defaults: `period = 20`,
  `trading_periods = 252`. The recommended OHLC estimator for equities,
  futures, and any asset with material close-to-open gaps.
- **Rogers-Satchell Volatility.** Drift-free OHLC realised-volatility
  estimator from Rogers, Satchell & Yoon (1994). Per-bar sample is
  `ln(H/C)·ln(H/O) + ln(L/C)·ln(L/O)`; every term is non-negative by
  construction (high >= open, close; low <= open, close), so the
  rolling mean is exact, not biased, under arbitrary drift. The
  algebraic drift-cancellation is what differentiates it from
  Garman-Klass. Output annualised to a percent. Defaults:
  `period = 20`, `trading_periods = 252`.
- **Garman-Klass Volatility.** Garman & Klass (1980) OHLC realised
  volatility estimator: per-bar sample is
  `0.5·(ln H/L)² − (2·ln2 − 1)·(ln C/O)²`, then take the annualised
  square root of the rolling mean. Roughly 7.4× more statistically
  efficient than close-to-close stddev under driftless GBM. Output
  annualised to a percent. Defaults: `period = 20`,
  `trading_periods = 252`.
- **Parkinson Volatility.** Michael Parkinson's (1980) high-low realised
  volatility estimator: `sigma² = (1 / (4n·ln2)) · Σ (ln(H/L))²`. Output
  annualised to a percent in the same style as `HistoricalVolatility`
  (pass `trading_periods = 1` for the raw per-bar `sigma·100` figure).
  Roughly 5× more statistically efficient than close-to-close stddev
  under a driftless-GBM assumption. Defaults: `period = 20`,
  `trading_periods = 252`.
- **RVIVolatility (Relative Volatility Index).** Donald Dorsey's
  RSI-shaped volatility gauge: partition the rolling standard
  deviation of close into "up" (close rose) and "down" (close fell)
  samples, Wilder-smooth each side, and compute
  `100 · AvgUp / (AvgUp + AvgDown)`. Bounded on `[0, 100]`; saturates
  at `100` in pure uptrends, `0` in pure downtrends, and falls back to
  `50` on a completely flat series (same undefined-RS convention as
  `RSI`). Single `period` parameter (default `10`) drives both the
  stddev window and the Wilder smoothing. Named `RVIVolatility` rather
  than plain `RVI` to disambiguate from Relative Vigor Index, which
  ships in Family 02 under the shorter `RVI` name.
- **Family 03 — MACD & Price Oscillators.** `Stc` (Schaff Trend Cycle,
  Doug Schaff): doubly-`Stochastic`-smoothed MACD producing a bounded
  `[0, 100]` reading that reacts faster than `MACD` itself. Four
  parameters `(fast = 23, slow = 50, schaff_period = 10, factor = 0.5)`.
  Output is clamped to `[0, 100]` to absorb floating-point rounding.
  Exposed in all four bindings.
- **Family 03 — MACD & Price Oscillators.** `ElderImpulse` (Alexander
  Elder's Impulse System): tri-state momentum gauge combining `EMA`
  trend slope with `MACD` histogram slope. Returns `+1` (green/buy)
  when both rise, `−1` (red/sell) when both fall, `0` (blue/neutral)
  on disagreement. Four parameters
  `(ema_period, macd_fast, macd_slow, macd_signal)`; defaults
  `(13, 12, 26, 9)` track *Come Into My Trading Room*. Exposed in all
  four bindings.
- **Family 03 — MACD & Price Oscillators.** `ZeroLagMacd`: classic
  MACD topology with `ZLEMA` substituted for `EMA` everywhere — faster
  reaction to trend changes at the cost of slightly noisier readings.
  Multi-output `ZeroLagMacdOutput { macd, signal, histogram }`. Three
  parameters `(fast = 12, slow = 26, signal = 9)`; `fast` must be
  strictly less than `slow`. Exposed in all four bindings.
- **Family 03 — MACD & Price Oscillators.** `CFO` (Chande Forecast
  Oscillator): `100 · (close − LinReg(close, period)) / close`. Positive
  when the close overshoots the linear forecast, negative when it
  undershoots. Holds the previous value if the close is zero. Default
  period 14. Exposed in all four bindings.
- **Family 03 — MACD & Price Oscillators.** `AwesomeOscillatorHistogram`:
  `AO − SMA(AO, sma_period)`. A configurable variant of the existing
  `AcceleratorOscillator` (which fixes `(fast, slow, sma) = (5, 34, 5)`).
  Three parameters; defaults match Bill Williams' Accelerator. Exposed
  in all four bindings.
- **Family 03 — MACD & Price Oscillators.** `APO` (Absolute Price
  Oscillator): `EMA(close, fast) − EMA(close, slow)`. Like MACD's line
  without the signal EMA. Default `(fast = 12, slow = 26)`. `fast` must
  be strictly less than `slow`. Exposed in all four bindings.
- **Family 02 — Momentum Oscillators.** `Inertia` (Dorsey): a
  `LinearRegression` smoothing of the `RVI` series — preserves trend
  direction while damping the underlying ratio. Candle input, two
  parameters `(rvi_period, linreg_period)` (defaults 14 / 20). Exposed
  in all four bindings.
- **Family 02 — Momentum Oscillators.** `ConnorsRsi`: Larry Connors'
  3-component aggregate — `RSI(close)`, `RSI(streak)`, and the
  percentile rank of the 1-bar return over the recent `period_rank`
  returns. Bounded in `[0, 100]`. Three parameters
  `(period_rsi, period_streak, period_rank)` (defaults 3 / 2 / 100).
  Exposed in all four bindings.
- **Family 02 — Momentum Oscillators.** `LaguerreRsi` (Ehlers):
  four-stage Laguerre polynomial filter wrapped in an RSI-style up/down
  accumulator. Single parameter `gamma` in `[0, 1]` (default 0.5) trades
  lag for smoothness. State is seeded to the first input so a constant
  series stays at the neutral 50. Output clamped to `[0, 100]`. Exposed
  in all four bindings.
- **Family 02 — Momentum Oscillators.** `SMI` (Stochastic Momentum
  Index, Blau): doubly-`EMA`-smoothed bounded oscillator measuring the
  close's displacement from the centre of the recent high-low range,
  scaled by the smoothed range. Candle input, three parameters
  `(period, d_period, d2_period)` (defaults 5 / 3 / 3). Exposed in all
  four bindings.
- **Family 02 — Momentum Oscillators.** `KST` (Know Sure Thing, Pring):
  weighted sum of four `SMA`-smoothed `ROC` series with Pring's fixed
  weights `1, 2, 3, 4`, plus an `SMA` signal line. Nine parameters
  (four ROC periods, four SMA periods, signal period); `Kst::classic()`
  uses Pring's recommended defaults. Multi-output indicator emitting
  `KstOutput { kst, signal }`. Exposed in all four bindings.
- **Family 02 — Momentum Oscillators.** `PGO` (Pretty Good Oscillator,
  Mark Johnson): `(close − SMA(close, period)) / EMA(TR, period)`.
  Candle input, single parameter `period` (default 14). Roughly counts
  how many ATR-equivalents the close is from its mean. Exposed in all
  four bindings.
- **Family 02 — Momentum Oscillators.** `RVI` (Relative Vigor Index,
  Dorsey): per-bar ratio `SMA(close - open, period) / SMA(high - low,
  period)`. Candle input, single parameter `period` (default 10).
  Positive on average-bullish windows, negative on average-bearish.
  Holds previous value if the entire window has zero range. Exposed in
  all four bindings.
- **Family 01 — Moving Averages.** `ALMA` (Arnaud Legoux Moving Average):
  Gaussian-weighted moving average with configurable centre (`offset` in
  `[0, 1]`) and kernel width (`sigma > 0`). Community-standard defaults
  `(period = 9, offset = 0.85, sigma = 6.0)` available via `Alma::classic()`.
  Exposed in all four bindings (Rust, Python, Node, WASM).
- **Family 01 — Moving Averages.** `EVWMA` (Elastic Volume-Weighted
  Moving Average, Fries 2001): an "elastic" recurrence whose smoothing
  weight is the bar's volume relative to the running window-volume.
  Candle input (uses close + volume), single parameter `period`
  (default 20). Holds its previous value if the entire window has zero
  volume. Exposed in all four bindings.
- **Family 01 — Moving Averages.** `Alligator` (Bill Williams): three
  SMMA lines (Jaw / Teeth / Lips) of the median price `(high + low) / 2`
  with default periods 13 / 8 / 5. Multi-output indicator emitting
  `AlligatorOutput { jaw, teeth, lips }`. Visual chart shift is left to
  the consumer. Exposed in all four bindings.
- **Family 01 — Moving Averages.** `JMA` (Jurik Moving Average):
  three-stage filter reconstruction of Mark Jurik's adaptive MA.
  Three parameters: `period` (14), `phase` in `[-100, 100]` (0), `power`
  in `1..=4` (2). State is seeded to the first input so a constant series
  is reproduced exactly. Exposed in all four bindings.
- **Family 01 — Moving Averages.** `VIDYA` (Variable Index Dynamic
  Average, Chande 1992): EMA whose smoothing factor is scaled by the
  absolute Chande Momentum Oscillator. Two parameters `period` and
  `cmo_period` (defaults 14 / 9). Exposed in all four bindings.
- **Family 01 — Moving Averages.** `FRAMA` (Fractal Adaptive Moving
  Average, Ehlers 2005): adapts its smoothing constant to the fractal
  dimension of the recent window — fast in trends, slow in chop. Single
  parameter `period` (must be even, default 16). Exposed in all four
  bindings.
- **Family 01 — Moving Averages.** `McGinleyDynamic`: John McGinley's
  self-adjusting MA. Single parameter `period`; the recurrence
  `MD + (price - MD) / (0.6 * period * (price / MD)^4)` speeds up when price
  falls below the indicator and damps when price runs above. Seeded with the
  simple average of the first `period` inputs. Exposed in all four bindings.

## [0.2.7] - 2026-05-24

### Added
- **Windows ARM64 is back.** npm Support unblocked the
  `wickra-win32-arm64-msvc` sub-package name (same path
  `wickra-win32-x64-msvc` took through 0.1.4) and transferred write
  access to @kingchenc. 0.2.7 ships the binding for
  `aarch64-pc-windows-msvc` alongside the existing five platforms:
  the `napi.triples.additional` entry, the `optionalDependencies`
  pin, the `bindings/node/npm/win32-arm64-msvc/` sub-package and the
  `windows-11-arm` row of the release.yml node-build matrix are all
  restored from 8aa74cb. `npm install wickra` on Windows ARM64 now
  resolves to a native build instead of failing the loader's
  optional-dep lookup. PyPI's `win_arm64` wheel was unaffected and
  carries through as before.

### Changed
- **Benchmark CPU renamed.** The "Reproduced on" line in every
  README listed an AMD Ryzen 9 7950X3D; the canonical machine is
  actually a Ryzen 9 9950X. Speedup ratios in the tables are
  unchanged (they're relative across libraries on the same machine),
  only the labelling is corrected. The performance-regression issue
  template's CPU example was updated for consistency.

## [0.2.6] - 2026-05-24

### Fixed
- **docs.rs build.** Rust 1.92 removed the `doc_auto_cfg` feature gate
  and folded it back into `doc_cfg` (rust-lang/rust#138907). docs.rs
  builds against the latest nightly and sets `--cfg docsrs`, so every
  published 0.2.x failed with E0557 on the
  `#![cfg_attr(docsrs, feature(doc_auto_cfg))]` line at the top of
  `wickra`, `wickra-core`, and `wickra-data`. GitHub CI didn't see
  this — stable rustc never enables the `docsrs` cfg. The three
  library crates now gate on `doc_cfg` (same intent, same rendered
  output on docs.rs, builds again on nightly).

### Changed
- **README — Wickra is now the top row of every comparison table.**
  The "Why Wickra exists" library matrix and the per-indicator
  benchmark tables previously placed Wickra at the bottom; a reader
  landing on the README is here to compare *against* Wickra, so the
  pivot row belongs at the top with a ★ marker. Same column data,
  same winner annotations — only row order changed. Mirrored across
  the umbrella README and every binding README so crates.io / PyPI /
  npm landing pages stay in sync.

## [0.2.5] - 2026-05-24

### Added
- `BinanceConfig` plus `BinanceKlineStream::connect_with_config(symbols, interval, config)`
  in `wickra-data`'s `live::binance` module. `connect()` keeps its previous
  signature and now forwards to the new entry-point with the defaults, so the
  public API is backwards-compatible. The config lets callers point the
  stream at Binance Testnet (`wss://testnet.binance.vision`) or tune the
  read timeout, reconnect attempt count, initial / capped backoff and frame
  size limits without rewriting the connector.
- README **Disclaimer** section clarifying that Wickra is an indicator
  toolkit (not a trading system) and that any production-trading use is at
  the caller's own risk. The legal terms in [LICENSE](LICENSE) are
  unchanged.

### Changed
- `BinanceKlineStream::next_event` now writes the Pong reply to a server
  `Ping` on a best-effort basis. A failed write means the connection is
  already dead, so the existing timeout / read-error reconnect arm one
  loop iteration later picks it up — the previous explicit reconnect on
  Pong-write failure is gone. Observable behaviour is unchanged for every
  healthy connection.

## [0.2.1] - 2026-05-23

### Changed
- **MSRV bumped.** Workspace minimum supported Rust version is now **1.86**
  (was 1.75) and the Node binding (`wickra-node`) is now **1.88** (was 1.77).
  The bumps are driven by transitive-dependency floors that were lifted in
  recent updates: `criterion 0.8.2` (the bench dev-dep) requires Rust 1.86,
  and `napi-build >= 2.3.2` requires Rust 1.88. Pinning those deps to the
  older versions would have frozen us out of future security fixes from
  those upstreams, so lifting the MSRV is the cleaner path for a young 0.x
  library. Downstream consumers on older Rust toolchains can stay on
  Wickra 0.2.0.
- Bumped the bench dev-dep `criterion` from 0.5 to 0.8 and migrated
  `bindings/wickra/benches/indicators.rs` from the deprecated
  `criterion::black_box` re-export to the stable `std::hint::black_box`.
- Bumped `tokio-tungstenite` from 0.24 to 0.29. `WebSocketConfig` became
  `#[non_exhaustive]` upstream, so the struct-literal construction in
  `crates/wickra-data/src/live/binance.rs` is rewritten to the
  builder-style `WebSocketConfig::default().max_message_size(..).max_frame_size(..)`.
  Same caps, same semantics, same default carry-over.
- Bumped every committed CI/release GitHub Action to its latest pinned
  SHA: `actions/checkout` 4 → 6, `actions/setup-node` 4 → 6,
  `actions/setup-python` 5 → 6, `actions/upload-artifact` 4 → 7,
  `actions/download-artifact` 4 → 8, `softprops/action-gh-release` 2 → 3,
  `codecov/codecov-action` 5 → 6, `taiki-e/install-action` patch.

### Fixed
- `tick_aggregator` gap-fill no longer allocates an unbounded number of
  placeholder candles. The new `MAX_GAP_FILL_CANDLES = 1_000_000` cap
  surfaces an adversarial timestamp jump (e.g. a clock-glitch tick years
  in the future) as `Error::Malformed` instead of an OOM panic. Found by
  the new `tick_aggregator` fuzz target.
- `HistoricalVolatility::geometric_series_yields_zero` now uses an `1e-6`
  tolerance instead of `1e-9`. The mathematical result on a perfectly
  geometric price series is exactly zero, but the underlying
  `1.01_f64.powi(i)` + log-return + std-dev cascade accumulates
  platform-sensitive FP drift on the order of 1e-7 on x86_64 Linux and
  macOS. The widened tolerance stays four decimal places below any
  realistic annualised volatility value while absorbing the drift across
  every supported platform.
- Replaced every `(high + low) / 2.0` test-helper and three real call
  sites (`Ohlcv::median_price`, `Donchian.middle`, `EaseOfMovement.mid`,
  `SuperTrend.hl2`) with `f64::midpoint(high, low)`. The change satisfies
  clippy 1.95's new `manual_midpoint` lint without affecting values
  (`f64::midpoint` matches the naive average to better than 1 ULP for the
  inputs used here).
- Replaced `i.is_multiple_of(2)` (unstable on Rust 1.85) with `i % 2 == 0`
  in the SMA / Bollinger long-stream-drift tests so the workspace MSRV
  job builds cleanly on Rust 1.86.
- The `Compile examples` CI step now invokes
  `cargo build -p wickra-examples --bins` instead of the now-deleted
  `cargo build -p wickra --example backtest` / `-p wickra-data --example
  live_binance` (the Z5 reorganisation moved every runnable example into
  the dedicated `wickra-examples` crate, but the CI step had not been
  updated).
- The `Fuzz (smoke)` CI job installs `cargo-fuzz` from a prebuilt binary
  via `taiki-e/install-action` instead of `cargo install cargo-fuzz`.
  The source install resolved against `rustix 0.36.5`, which uses
  internal `#[rustc_*]` attributes the current nightly compiler rejects.
- The fuzz targets now build with an explicit
  `--target x86_64-unknown-linux-gnu`; cargo-fuzz was defaulting to
  `x86_64-unknown-linux-musl`, which is not installed on the standard
  GitHub-hosted Ubuntu runner.

### Removed
- **`wickra-win32-arm64-msvc` is temporarily omitted from this release.**
  The npm spam-detection filter blocks the first publish of this brand-new
  package name (same situation that affected `wickra-win32-x64-msvc`
  through 0.1.4 until npm Support unblocked it). A support ticket is open;
  once the new name is unblocked the
  `aarch64-pc-windows-msvc` triple will be restored in
  `bindings/node/package.json` (`napi.triples.additional` +
  `optionalDependencies`), in the `release.yml` `node-build` matrix, and
  as a fresh `bindings/node/npm/win32-arm64-msvc/` template. Until then,
  `npm install wickra@0.2.1` on Windows ARM64 will surface the loader's
  standard `Cannot find module 'wickra-win32-arm64-msvc'` error; every
  other platform (Linux x64 / Linux ARM64 / macOS x64 / macOS ARM64 /
  Windows x64) ships normally. The PyPI wheel for Windows ARM64 is
  unaffected and still published.

## [0.2.0] - 2026-05-23

### Fixed
- `HistoricalVolatility::update` no longer substitutes a `0.0` log-return on
  non-positive prices (audit finding R13). Negative or zero prices are
  semantically invalid for a log-return calculation; silently treating them as
  "no movement" underreported realised volatility. They are now skipped — the
  previous valid value is returned and the indicator's state (`prev_price`,
  window, sums) is left untouched — matching how every other indicator handles
  invalid inputs.
- `Tick::new` now returns the new `Error::InvalidTick` variant for negative
  volume instead of `Error::InvalidCandle` (audit finding R14). A tick is not
  a candle, and downstream tick-stream pipelines should be able to match on a
  semantically-correct error. The Python binding's `map_err` was extended to
  forward the new variant as a `ValueError`; the Node and WASM bindings format
  via `Error::to_string()` and pick the new variant up automatically.
- `Psar::is_ready` now matches the convention shared by every other indicator:
  `is_ready() == true` iff a real value has been produced (audit finding R6).
  The previous implementation returned `self.initialised`, which flipped to
  `true` after the seed candle even though the seed candle itself returns
  `None`. A streaming consumer that wrote
  `if ind.is_ready() { use(ind.update(c)?) }` would hit an unexpected `None`
  on the first post-seed update. The fix introduces a `has_emitted` gate set
  when the first `Some` value is returned.
- `Psar::reset` now restores the compute fields (`prev_high`, `prev_low`,
  `sar`, `ep`) to `f64::NAN` sentinels instead of `0.0` (audit Opus-Bonus 1).
  The fields are gated by `initialised` today, so the `0.0` sentinel never
  leaked into output — but a future refactor that read them pre-init would
  have silently treated `0.0` as a real price. A `debug_assert!` at the read
  site makes the invariant explicit.

### Changed
- `Sma` and `BollingerBands` now reseed their incremental `sum` (and `sum_sq`
  for Bollinger) from the live window every `16 · period` finite updates,
  capping floating-point drift on long-running streams (audit findings R7 and
  L2-Rust). Previously the incremental single-subtract `sum -= old` could
  accumulate catastrophic-cancellation error on streams with alternating
  large/small magnitudes; the misleading `sma.rs` comment that claimed the
  drift was already bounded "by recomputing the sum after each pop" is
  replaced with an accurate description of the new reseed strategy. Amortised
  cost stays at O(1) (`O(period)` work amortised over `O(period)` updates),
  values are bit-identical on inputs that did not drift to begin with, and
  two new `long_stream_drift_stays_bounded` tests stress the recompute by
  alternating `1e9` / `1.0` (SMA) and `1e6` / `1.0` (Bollinger) for several
  recompute cycles and verify the reported values track a fresh from-scratch
  computation over the live window.
- `LinearRegression`, `LinRegSlope` and `LinRegAngle` (via composition over
  `LinRegSlope`) now run their rolling ordinary-least-squares fit
  **incrementally** in O(1) per update (audit finding R2). Previously every
  tick refit the line from scratch in O(period). The OLS denominators (`Σx`
  and `Σxx`) depend only on `period`, so they were already precomputed; this
  release adds running `Σy` and `Σxy` accumulators and slides them in closed
  form via the identity
  `new_Σxy = old_Σxy − old_Σy + popped_y₀` (then `Σxy += (n − 1) · new_value`
  and `Σy += new_value`). New per-bar equivalence tests compare the O(1)
  output against a fresh O(n) refit on noisy ramps, step functions, and
  constants — values agree to within 1e-9.
- Fuzz suite expanded from 2 indicators to the full catalogue (audit finding
  R9). The existing `indicator_update` target now exercises every scalar-input
  indicator (~33 classes including MACD and Bollinger Bands); a new
  `indicator_update_candle` target exercises every candle-input indicator (~37
  classes, including ATR, ADX, Stochastic, PSAR, Keltner, SuperTrend,
  ChandelierExit, AwesomeOscillator, OBV, MFI, VWAP, RollingVWAP, and the rest
  of the volume / volatility / trailing-stop / price-statistics families). Each
  iteration sweeps every indicator through both the streaming `update` loop
  and a full `batch` call so any state-mutation bug surfaces on either path.
  CI gains a `fuzz-smoke` job that runs each of the five targets for 30 s on
  every push and pull-request.
- `UlcerIndex::update` now tracks the trailing maximum with a monotonically-
  decreasing deque of `(index, price)` pairs instead of scanning the whole
  trailing window on every tick. The indicator now honours the `Indicator`
  trait's O(1)-per-tick contract; values and warmup semantics are unchanged
  (verified by a new adversarial-input test that compares the deque output
  bar-by-bar against a naive O(n) trailing-max scan on strictly increasing,
  strictly decreasing, constant, and sawtooth inputs). The doc comment on
  `warmup_period()` is also corrected: the two windows overlap by one bar, so
  the formula is `2 * period - 1`.

### Added
- `RollingVWAP` is now exposed in Python, Node and WASM under that name
  (previously the rolling-window VWAP existed only in the Rust core, even
  though the README's volume-family table already advertised
  `VWAP (cumulative + rolling)`). All four bindings now ship the same
  cumulative `VWAP` plus the finite-window `RollingVWAP(period)`. The wiki page
  `Indicator-Vwap.md` adds Python, Node and WASM examples and drops the
  "Rust-only" caveat.
- WASM binding now exposes the streaming `update()` method on every candle-input
  indicator: `Adx`, `WilliamsR`, `Cci`, `Mfi`, `Psar`, `Keltner`, `Donchian`,
  `Vwap`, `AwesomeOscillator`, `Aroon`, `Stochastic`, and `Obv`. Multi-output
  indicators (`Adx`, `Keltner`, `Donchian`, `Aroon`, `Stochastic`) return a
  named JS object (`{ plusDi, minusDi, adx }`, `{ upper, middle, lower }`,
  `{ up, down }`, `{ k, d }`) once warm, or `null` during warmup — matching the
  existing `SuperTrend` convention. Each class also gains `reset()`, `isReady()`
  and `warmupPeriod()`, bringing the WASM surface to full parity with Python
  and Node so browser-side streaming code no longer has to replay `batch()`
  on every tick. `WasmKama` gains the previously missing `warmupPeriod()`.
- New `wasm-bindgen` integration test exercises `update == batch` plus the full
  lifecycle (`reset` / `isReady` / `warmupPeriod`) for all twelve newly wired
  classes against a deterministic 40-bar synthetic OHLCV stream.

### Security
- Upgrade `pyo3` (0.22 → 0.28) and `numpy` (0.22 → 0.28) in the Python binding.
  Fixes [RUSTSEC-2025-0020](https://rustsec.org/advisories/RUSTSEC-2025-0020) —
  a buffer overflow in `PyString::from_object` that affected the published
  Python wheels. The `cargo-deny` ignore entry that previously suppressed the
  advisory has been removed; `cargo deny check` is now clean without
  suppression. Migrated `into_pyarray_bound` to `into_pyarray`,
  `downcast::<PyDict>` to `cast::<PyDict>`, and opted every `#[pyclass]` out of
  the deprecated automatic `FromPyObject` derive via `skip_from_py_object`.

### Added
- 46 new technical indicators, taking the library from 25 to 71 and
  reorganising the catalogue into **eight families**, each with at least five
  members. Every indicator is implemented once in the Rust core and wired
  through the Python, Node and WASM bindings, with reference-value tests and a
  dedicated wiki page:
  - Moving Averages: `Smma`, `Trima`, `Zlema`, `T3`, `Vwma`.
  - Momentum Oscillators: `Mom`, `Cmo`, `Tsi`, `Pmo`, `StochRsi`,
    `UltimateOscillator`.
  - Trend & Directional: `AroonOscillator`, `Vortex`, `MassIndex`,
    `ChoppinessIndex`, `VerticalHorizontalFilter`.
  - Price Oscillators: `Ppo`, `Dpo`, `Coppock`, `AcceleratorOscillator`,
    `BalanceOfPower`.
  - Volatility & Bands: `Natr`, `StdDev`, `UlcerIndex`,
    `HistoricalVolatility`, `BollingerBandwidth`, `PercentB`, `TrueRange`,
    `ChaikinVolatility`.
  - Trailing Stops: `SuperTrend`, `ChandelierExit`, `ChandeKrollStop`,
    `AtrTrailingStop`.
  - Volume: `Adl`, `VolumePriceTrend`, `ChaikinMoneyFlow`,
    `ChaikinOscillator`, `ForceIndex`, `EaseOfMovement`.
  - Price Statistics: `TypicalPrice`, `MedianPrice`, `WeightedClose`,
    `LinearRegression`, `LinRegSlope`, `ZScore`, `LinRegAngle`.
- `TickAggregator::with_gap_fill` — opt-in mode that emits a flat placeholder
  candle for every empty bucket between two ticks, keeping the candle series
  evenly spaced for downstream indicators.
- CSV reader: a leading UTF-8 byte-order mark is stripped, fields are trimmed,
  and the header is validated against the required OHLCV columns.
- CI: an `msrv` job that builds and tests the workspace on Rust 1.75 and the
  node binding on Rust 1.77.
- Community health files: `CONTRIBUTING.md`, `SECURITY.md`,
  `CODE_OF_CONDUCT.md`, issue / pull-request templates, `CODEOWNERS`, and a
  Dependabot configuration.
- Seven example OHLCV datasets under `examples/data/`, one per timeframe
  (1m / 5m / 15m / 1h / 12h / 1d / 1month), holding real BTCUSDT spot klines,
  alongside the `fetch_btcusdt` example that regenerates them from the
  Binance REST API.
- `Timeframe::minutes`, `Timeframe::hours` and `Timeframe::days` convenience
  constructors, each building on seconds with a checked-multiplication
  overflow guard.

### Changed
- The indicator wiki is reorganised into eight family folders under
  `docs/wiki/indicators/` (`moving-averages/`, `momentum-oscillators/`,
  `trend-directional/`, `price-oscillators/`, `volatility-bands/`,
  `trailing-stops/`, `volume/`, `price-statistics/`); `Indicators-Overview.md`,
  `Home.md` and the README indicator table follow the same eight families.
- `TickAggregator::push` returns `Result<Vec<Candle>>` (was
  `Result<Option<Candle>>`) so a single tick can yield a closed bar plus gap
  fillers.
- `Resampler::push` returns `Result<Option<Candle>>`: a candle in a bucket
  earlier than the open bar is now rejected as out of order.
- Aggregated candles are finalised through the validating `Candle::new`, so a
  volume that overflows to a non-finite value is surfaced as an error instead
  of producing a poisoned candle.
- All GitHub Actions are pinned to commit SHAs; the four publish jobs run in a
  protected `release` environment.
- The indicator benchmarks (`crates/wickra/benches/indicators.rs`) now run
  against the checked-in real BTCUSDT 1-minute dataset instead of a synthetic
  price series.
- Every language's examples now live under a uniform `examples/<lang>/`
  tree: Rust moved into a new `examples/rust/` workspace member crate
  (`wickra-examples`, run via `cargo run -p wickra-examples --bin <name>`),
  Node into `examples/node/` with its own `package.json` linking `wickra` via
  `file:../../bindings/node`, and the WASM browser demos into
  `examples/wasm/`. The bundled BTCUSDT datasets move alongside them at
  `examples/data/`. Six new examples close the cross-language parity matrix:
  streaming demos for Python and Rust; multi-timeframe and parallel-assets
  demos for both Rust and Node.
- Cross-language data-generator parity: `examples/python/fetch_btcusdt.py`
  (stdlib only: `urllib` + `json` + `csv`) and `examples/node/fetch_btcusdt.js`
  (Node 18+ built-in `fetch`) mirror the Rust `fetch_btcusdt` binary —
  byte-for-byte identical CSV output on the same Binance snapshot.
- Four additional WebAssembly browser demos under `examples/wasm/`
  alongside the original `index.html`: `backtest.html` (fetch + basket of
  indicators), `live_trading.html` (browser-native `WebSocket` to
  Binance), `multi_timeframe.html` (in-page resample) and
  `parallel_assets.html` + `parallel_worker.js` (module-Worker pool with
  serial-vs-parallel speedup). The cross-language matrix is now closed
  for every cell where the pattern makes sense.
- Three new wiki pages: `TA-Lib-Migration.md` (full mapping table from
  `talib.X(...)` calls to Wickra), `Cookbook.md` (seven concrete
  strategy recipes — RSI mean reversion, MACD crossover, Bollinger
  breakout, ADX-gated trend, multi-timeframe confirmation, SuperTrend,
  chained indicators) and `FAQ.md`. All three linked from `Home.md`.

### Fixed
- `Timeframe::floor` no longer overflows for timestamps near `i64::MIN`.
- The aggregator rejects same-bucket ticks that arrive out of order instead of
  silently overwriting the bar's close with a stale price.
- The Binance live stream reconnects with exponential backoff, skips non-kline
  frames, applies a read timeout and message-size limits, and tracks a closed
  flag.
- Example scripts: `live_trading.py` skips non-kline frames and validates the
  symbol/interval; `backtest.py` and `multi_timeframe.py` report clear errors
  for malformed CSV input.

## [0.1.4] - 2026-05-21

### Added
- GitHub Release runs now attach every built artefact (wheels, sdist, native
  Node binaries, npm-pack tarballs, cargo `.crate` files) to the tag's
  release page.

## [0.1.3] - 2026-05-21

### Fixed
- npm package ships the napi-generated loader and is built with `--platform`
  so the per-platform binary is resolved correctly.

## [0.1.2] - 2026-05-21

### Fixed
- Release pipeline: per-platform idempotent npm publishing with a spam-filter
  retry, and committed `npm/<platform>/` package templates.

## [0.1.1] - 2026-05-21

### Fixed
- Node publish step and coordinated version bump across all bindings.

## [0.1.0] - 2026-05-21

### Added
- Initial release: a streaming-first technical-analysis library with 25
  indicators (SMA, EMA, WMA, DEMA, TEMA, HMA, KAMA, RSI, MACD, ROC, Stochastic,
  CCI, Williams %R, ADX, MFI, TRIX, Aroon, Awesome Oscillator, Bollinger Bands,
  ATR, Keltner Channels, Donchian Channels, Parabolic SAR, OBV, VWAP).
- Rust core (`wickra-core`), umbrella crate (`wickra`), and a data layer
  (`wickra-data`) with a CSV reader, tick aggregator, resampler, and an
  optional Binance live feed.
- Bindings for Python, Node.js, and WebAssembly.

[Unreleased]: https://github.com/wickra-lib/wickra/compare/v1.0.4...HEAD
[1.0.4]: https://github.com/wickra-lib/wickra/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/wickra-lib/wickra/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/wickra-lib/wickra/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/wickra-lib/wickra/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/wickra-lib/wickra/compare/v0.9.9...v1.0.0
[0.9.9]: https://github.com/wickra-lib/wickra/compare/v0.9.8...v0.9.9
[0.9.8]: https://github.com/wickra-lib/wickra/compare/v0.9.7...v0.9.8
[0.9.7]: https://github.com/wickra-lib/wickra/compare/v0.9.6...v0.9.7
[0.9.6]: https://github.com/wickra-lib/wickra/compare/v0.9.5...v0.9.6
[0.9.5]: https://github.com/wickra-lib/wickra/compare/v0.9.4...v0.9.5
[0.9.4]: https://github.com/wickra-lib/wickra/compare/v0.9.3...v0.9.4
[0.9.3]: https://github.com/wickra-lib/wickra/compare/v0.9.2...v0.9.3
[0.9.2]: https://github.com/wickra-lib/wickra/compare/v0.9.1...v0.9.2
[0.9.1]: https://github.com/wickra-lib/wickra/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/wickra-lib/wickra/compare/v0.8.9...v0.9.0
[0.8.9]: https://github.com/wickra-lib/wickra/compare/v0.8.8...v0.8.9
[0.8.8]: https://github.com/wickra-lib/wickra/compare/v0.8.7...v0.8.8
[0.8.7]: https://github.com/wickra-lib/wickra/compare/v0.8.6...v0.8.7
[0.8.6]: https://github.com/wickra-lib/wickra/compare/v0.8.5...v0.8.6
[0.8.5]: https://github.com/wickra-lib/wickra/compare/v0.8.4...v0.8.5
[0.8.4]: https://github.com/wickra-lib/wickra/compare/v0.8.3...v0.8.4
[0.8.3]: https://github.com/wickra-lib/wickra/compare/v0.8.2...v0.8.3
[0.8.2]: https://github.com/wickra-lib/wickra/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/wickra-lib/wickra/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/wickra-lib/wickra/compare/v0.7.9...v0.8.0
[0.7.9]: https://github.com/wickra-lib/wickra/compare/v0.7.8...v0.7.9
[0.7.8]: https://github.com/wickra-lib/wickra/compare/v0.7.7...v0.7.8
[0.7.7]: https://github.com/wickra-lib/wickra/compare/v0.7.6...v0.7.7
[0.7.6]: https://github.com/wickra-lib/wickra/compare/v0.7.5...v0.7.6
[0.7.5]: https://github.com/wickra-lib/wickra/compare/v0.7.4...v0.7.5
[0.7.4]: https://github.com/wickra-lib/wickra/compare/v0.7.3...v0.7.4
[0.7.3]: https://github.com/wickra-lib/wickra/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/wickra-lib/wickra/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/wickra-lib/wickra/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/wickra-lib/wickra/compare/v0.6.9...v0.7.0
[0.6.9]: https://github.com/wickra-lib/wickra/compare/v0.6.8...v0.6.9
[0.6.8]: https://github.com/wickra-lib/wickra/compare/v0.6.7...v0.6.8
[0.6.7]: https://github.com/wickra-lib/wickra/compare/v0.6.6...v0.6.7
[0.6.6]: https://github.com/wickra-lib/wickra/compare/v0.6.5...v0.6.6
[0.6.5]: https://github.com/wickra-lib/wickra/compare/v0.6.4...v0.6.5
[0.6.4]: https://github.com/wickra-lib/wickra/compare/v0.6.3...v0.6.4
[0.6.3]: https://github.com/wickra-lib/wickra/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/wickra-lib/wickra/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/wickra-lib/wickra/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/wickra-lib/wickra/compare/v0.5.9...v0.6.0
[0.5.9]: https://github.com/wickra-lib/wickra/compare/v0.5.8...v0.5.9
[0.5.8]: https://github.com/wickra-lib/wickra/compare/v0.5.7...v0.5.8
[0.5.7]: https://github.com/wickra-lib/wickra/compare/v0.5.6...v0.5.7
[0.5.6]: https://github.com/wickra-lib/wickra/compare/v0.5.5...v0.5.6
[0.5.5]: https://github.com/wickra-lib/wickra/compare/v0.5.4...v0.5.5
[0.5.4]: https://github.com/wickra-lib/wickra/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/wickra-lib/wickra/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/wickra-lib/wickra/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/wickra-lib/wickra/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/wickra-lib/wickra/compare/v0.4.7...v0.5.0
[0.4.7]: https://github.com/wickra-lib/wickra/compare/v0.4.6...v0.4.7
[0.4.6]: https://github.com/wickra-lib/wickra/compare/v0.4.5...v0.4.6
[0.4.5]: https://github.com/wickra-lib/wickra/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/wickra-lib/wickra/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/wickra-lib/wickra/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/wickra-lib/wickra/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/wickra-lib/wickra/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/wickra-lib/wickra/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/wickra-lib/wickra/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/wickra-lib/wickra/compare/v0.2.7...v0.3.0
[0.2.7]: https://github.com/wickra-lib/wickra/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/wickra-lib/wickra/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/wickra-lib/wickra/compare/v0.2.1...v0.2.5
[0.2.1]: https://github.com/wickra-lib/wickra/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/wickra-lib/wickra/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/wickra-lib/wickra/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/wickra-lib/wickra/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/wickra-lib/wickra/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/wickra-lib/wickra/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/wickra-lib/wickra/releases/tag/v0.1.0
