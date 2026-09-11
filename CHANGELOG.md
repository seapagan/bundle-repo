# Changelog

This is an auto-generated log of all the changes that have been made to the
project since the first release, with the latest changes at the top.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0](https://github.com/seapagan/bundle-repo/releases/tag/0.9.0) (September 11, 2026)

### New Features

- Feat: add custom repository metadata ([#124](https://github.com/seapagan/bundle-repo/pull/124)) by [seapagan](https://github.com/seapagan)
- Feat: overhaul repository file selection ([#122](https://github.com/seapagan/bundle-repo/pull/122)) by [seapagan](https://github.com/seapagan)
- Fix: make XML serialization deterministic ([#121](https://github.com/seapagan/bundle-repo/pull/121)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Fix(deps): update rust crate toml to v1.1.6 ([#125](https://github.com/seapagan/bundle-repo/pull/125)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update taiki-e/install-action action to v2.87.10 ([#123](https://github.com/seapagan/bundle-repo/pull/123)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update rust crate encoding_rs to v0.8.41 ([#120](https://github.com/seapagan/bundle-repo/pull/120)) by [renovate[bot]](https://github.com/apps/renovate)
- Fix(deps): update rust crate tabled to 0.22.0 ([#119](https://github.com/seapagan/bundle-repo/pull/119)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update rust crate tokenizers to v0.23.2 ([#118](https://github.com/seapagan/bundle-repo/pull/118)) by [renovate[bot]](https://github.com/apps/renovate)
- Fix(deps): update rust crate toml to v1.1.5 ([#117](https://github.com/seapagan/bundle-repo/pull/117)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update ghcr.io/zizmorcore/zizmor docker tag to v1.30.1 ([#116](https://github.com/seapagan/bundle-repo/pull/116)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update zizmorcore/zizmor-action action to v0.6.4 ([#115](https://github.com/seapagan/bundle-repo/pull/115)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update rust crate flate2 to v1.1.10 ([#114](https://github.com/seapagan/bundle-repo/pull/114)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update taiki-e/install-action action to v2.87.9 ([#113](https://github.com/seapagan/bundle-repo/pull/113)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.8.0...0.9.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.8.0...0.9.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.8.0...0.9.0.patch)

## [0.8.0](https://github.com/seapagan/bundle-repo/releases/tag/0.8.0) (August 16, 2026)

### New Features

- Feat: add secret-safe repository bundles ([#111](https://github.com/seapagan/bundle-repo/pull/111)) by [seapagan](https://github.com/seapagan)
- Feat: format summary metrics for the numeric locale ([#109](https://github.com/seapagan/bundle-repo/pull/109)) by [seapagan](https://github.com/seapagan)
- Feat: add semantic terminal colour ([#108](https://github.com/seapagan/bundle-repo/pull/108)) by [seapagan](https://github.com/seapagan)

### CI / CD Pipeline

- Ci: use released setup-rust action ([#107](https://github.com/seapagan/bundle-repo/pull/107)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Chore(deps): update taiki-e/install-action action to v2.86.1 ([#110](https://github.com/seapagan/bundle-repo/pull/110)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update taiki-e/install-action action to v2.85.13 ([#106](https://github.com/seapagan/bundle-repo/pull/106)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.7.0...0.8.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.7.0...0.8.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.7.0...0.8.0.patch)

## [0.7.0](https://github.com/seapagan/bundle-repo/releases/tag/0.7.0) (August 09, 2026)

### Closed Issues

- 'output_file' path not expanded in the config file ([#26](https://github.com/seapagan/bundle-repo/issues/26)) by [seapagan](https://github.com/seapagan)

### New Features

- Chore: add advisory Clippy maintainability checks ([#101](https://github.com/seapagan/bundle-repo/pull/101)) by [seapagan](https://github.com/seapagan)
- Fix: report effective output path on creation failure ([#95](https://github.com/seapagan/bundle-repo/pull/95)) by [seapagan](https://github.com/seapagan)
- Fix: generate well-formed XML with CDATA ([#94](https://github.com/seapagan/bundle-repo/pull/94)) by [seapagan](https://github.com/seapagan)
- Perf: optimize dependencies in dev builds ([#93](https://github.com/seapagan/bundle-repo/pull/93)) by [seapagan](https://github.com/seapagan)
- Chore: adopt Rust 2024 and enforce MSRV ([#92](https://github.com/seapagan/bundle-repo/pull/92)) by [seapagan](https://github.com/seapagan)
- Feat: add optional legacy text transcoding ([#86](https://github.com/seapagan/bundle-repo/pull/86)) by [seapagan](https://github.com/seapagan)
- Feat: modernize tokenizer support ([#82](https://github.com/seapagan/bundle-repo/pull/82)) by [seapagan](https://github.com/seapagan)
- Feat: add configurable gzip output ([#67](https://github.com/seapagan/bundle-repo/pull/67)) by [seapagan](https://github.com/seapagan)

### Testing

- Test: improve production coverage ([#98](https://github.com/seapagan/bundle-repo/pull/98)) by [seapagan](https://github.com/seapagan)

### CI / CD Pipeline

- Ci: add native macOS testing ([#105](https://github.com/seapagan/bundle-repo/pull/105)) by [seapagan](https://github.com/seapagan)
- Ci: guard package publishability ([#104](https://github.com/seapagan/bundle-repo/pull/104)) by [seapagan](https://github.com/seapagan)
- Publish combined coverage to Codacy ([#96](https://github.com/seapagan/bundle-repo/pull/96)) by [seapagan](https://github.com/seapagan)
- Add/Update cross platform quality gates ([#73](https://github.com/seapagan/bundle-repo/pull/73)) by [seapagan](https://github.com/seapagan)

### Bug Fixes

- Fix: rename the BINARY_NAME  in release.yml ([#85](https://github.com/seapagan/bundle-repo/pull/85)) by [seapagan](https://github.com/seapagan)
- Fix: expand home-relative output paths ([#80](https://github.com/seapagan/bundle-repo/pull/80)) by [seapagan](https://github.com/seapagan)

### Refactoring

- Refactor: clean up Codacy findings ([#102](https://github.com/seapagan/bundle-repo/pull/102)) by [seapagan](https://github.com/seapagan)
- Refactor: improve production coverage architecture ([#99](https://github.com/seapagan/bundle-repo/pull/99)) by [seapagan](https://github.com/seapagan)
- Test: reorganize crate unit tests ([#97](https://github.com/seapagan/bundle-repo/pull/97)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Chore(deps): update taiki-e/install-action action to v2.85.11 ([#103](https://github.com/seapagan/bundle-repo/pull/103)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update taiki-e/install-action action to v2.85.10 ([#91](https://github.com/seapagan/bundle-repo/pull/91)) by [renovate[bot]](https://github.com/apps/renovate)
- Fix: support sha2 0.11 fixture hashes ([#90](https://github.com/seapagan/bundle-repo/pull/90)) by [seapagan](https://github.com/seapagan)
- Chore(deps): update rust crate xml to v1.4.0 ([#89](https://github.com/seapagan/bundle-repo/pull/89)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update rust crate clap to v4.6.6 ([#88](https://github.com/seapagan/bundle-repo/pull/88)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update swatinem/rust-cache action to v2.9.2 ([#87](https://github.com/seapagan/bundle-repo/pull/87)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update taiki-e/install-action action to v2.85.9 ([#83](https://github.com/seapagan/bundle-repo/pull/83)) by [renovate[bot]](https://github.com/apps/renovate)
- Chore(deps): update rust crate ignore to v0.4.33 ([#81](https://github.com/seapagan/bundle-repo/pull/81)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate xml to v1 ([#79](https://github.com/seapagan/bundle-repo/pull/79)) by [renovate[bot]](https://github.com/apps/renovate)
- Fix: support config 0.15.25 ([#78](https://github.com/seapagan/bundle-repo/pull/78)) by [seapagan](https://github.com/seapagan)
- *and 18 more dependency updates*

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.6.0...0.7.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.6.0...0.7.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.6.0...0.7.0.patch)

## [0.6.0](https://github.com/seapagan/bundle-repo/releases/tag/0.6.0) (February 28, 2025)

### Closed Issues

- .gitignore file is still parsed and files ignored, even when using `exclude` option. ([#29](https://github.com/seapagan/bundle-repo/issues/29)) by [seapagan](https://github.com/seapagan)

### New Features

- Create a GitHub action to create binaries ([#50](https://github.com/seapagan/bundle-repo/pull/50)) by [seapagan](https://github.com/seapagan)
- Implement converting files to utf-8 ([#38](https://github.com/seapagan/bundle-repo/pull/38)) by [seapagan](https://github.com/seapagan)

### Documentation

- Update docs to mention .gitignore files always ignored ([#39](https://github.com/seapagan/bundle-repo/pull/39)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Update Rust crate tabled to 0.18.0 ([#49](https://github.com/seapagan/bundle-repo/pull/49)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate rust-embed to v8.6.0 ([#48](https://github.com/seapagan/bundle-repo/pull/48)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate serde to v1.0.218 ([#47](https://github.com/seapagan/bundle-repo/pull/47)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate config to v0.15.8 ([#45](https://github.com/seapagan/bundle-repo/pull/45)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate tabled to 0.18.0 ([#44](https://github.com/seapagan/bundle-repo/pull/44)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate clap to v4.5.31 ([#43](https://github.com/seapagan/bundle-repo/pull/43)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate infer to 0.19.0 ([#42](https://github.com/seapagan/bundle-repo/pull/42)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate config to v0.15.7 ([#41](https://github.com/seapagan/bundle-repo/pull/41)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate tempfile to v3.16.0 ([#40](https://github.com/seapagan/bundle-repo/pull/40)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.5.0...0.6.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.5.0...0.6.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.5.0...0.6.0.patch)

## [0.5.0](https://github.com/seapagan/bundle-repo/releases/tag/0.5.0) (January 25, 2025)

### New Features

- Add the 'deepseek' model as an option for counting tokens ([#36](https://github.com/seapagan/bundle-repo/pull/36)) by [seapagan](https://github.com/seapagan)

### Testing

- Add some more testing ([#35](https://github.com/seapagan/bundle-repo/pull/35)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Update Rust crate tokenizers to 0.21 ([#37](https://github.com/seapagan/bundle-repo/pull/37)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate git2 to 0.20 ([#34](https://github.com/seapagan/bundle-repo/pull/34)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate tempfile to v3.15.0 ([#33](https://github.com/seapagan/bundle-repo/pull/33)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate config to 0.15.0 ([#32](https://github.com/seapagan/bundle-repo/pull/32)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate serde to v1.0.217 ([#31](https://github.com/seapagan/bundle-repo/pull/31)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate clap to v4.5.27 ([#30](https://github.com/seapagan/bundle-repo/pull/30)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.4.0...0.5.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.4.0...0.5.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.4.0...0.5.0.patch)

## [0.4.0](https://github.com/seapagan/bundle-repo/releases/tag/0.4.0) (November 27, 2024)

### New Features

- Implement the 'exclude' option ([#28](https://github.com/seapagan/bundle-repo/pull/28)) by [seapagan](https://github.com/seapagan)
- Implement the extend-exclude option ([#27](https://github.com/seapagan/bundle-repo/pull/27)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Update Rust crate tabled to 0.17.0 ([#25](https://github.com/seapagan/bundle-repo/pull/25)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate url to v2.5.4 ([#24](https://github.com/seapagan/bundle-repo/pull/24)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate clap to v4.5.21 ([#23](https://github.com/seapagan/bundle-repo/pull/23)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate serde to v1.0.215 ([#22](https://github.com/seapagan/bundle-repo/pull/22)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate tempfile to v3.14.0 ([#21](https://github.com/seapagan/bundle-repo/pull/21)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate url to v2.5.3 ([#20](https://github.com/seapagan/bundle-repo/pull/20)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.3.0...0.4.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.3.0...0.4.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.3.0...0.4.0.patch)

## [0.3.0](https://github.com/seapagan/bundle-repo/releases/tag/0.3.0) (November 02, 2024)

### New Features

- Implement a local config file to override the global one ([#19](https://github.com/seapagan/bundle-repo/pull/19)) by [seapagan](https://github.com/seapagan)
- Add a TOML configuration file to the project ([#18](https://github.com/seapagan/bundle-repo/pull/18)) by [seapagan](https://github.com/seapagan)
- Add exit codes where missing ([#16](https://github.com/seapagan/bundle-repo/pull/16)) by [seapagan](https://github.com/seapagan)

### Documentation

- Fix README rendered by crates.io ([#17](https://github.com/seapagan/bundle-repo/pull/17)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Update Rust crate regex to v1.11.1 ([#15](https://github.com/seapagan/bundle-repo/pull/15)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate tiktoken-rs to 0.6.0 ([#14](https://github.com/seapagan/bundle-repo/pull/14)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.2.0...0.3.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.2.0...0.3.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.2.0...0.3.0.patch)

## [0.2.0](https://github.com/seapagan/bundle-repo/releases/tag/0.2.0) (October 11, 2024)

### New Features

- Allow checking out a specific branch for a remote git repo ([#13](https://github.com/seapagan/bundle-repo/pull/13)) by [seapagan](https://github.com/seapagan)
- Display the branch name for detected local git repo ([#12](https://github.com/seapagan/bundle-repo/pull/12)) by [seapagan](https://github.com/seapagan)
- Add optional line numbers to all files ([#10](https://github.com/seapagan/bundle-repo/pull/10)) by [seapagan](https://github.com/seapagan)

### Dependency Updates

- Update Rust crate clap to v4.5.20 ([#11](https://github.com/seapagan/bundle-repo/pull/11)) by [renovate[bot]](https://github.com/apps/renovate)
- Update Rust crate clap to v4.5.19 ([#7](https://github.com/seapagan/bundle-repo/pull/7)) by [renovate[bot]](https://github.com/apps/renovate)
- Configure Renovate ([#6](https://github.com/seapagan/bundle-repo/pull/6)) by [renovate[bot]](https://github.com/apps/renovate)

[`Full Changelog`](https://github.com/seapagan/bundle-repo/compare/0.1.0...0.2.0) | [`Diff`](https://github.com/seapagan/bundle-repo/compare/0.1.0...0.2.0.diff) | [`Patch`](https://github.com/seapagan/bundle-repo/compare/0.1.0...0.2.0.patch)

## [0.1.0](https://github.com/seapagan/bundle-repo/releases/tag/0.1.0) (October 05, 2024)

Initial public release of the project.

### New Features

- Add option to send output to clipboard and stdout ([#5](https://github.com/seapagan/bundle-repo/pull/5)) by [seapagan](https://github.com/seapagan)
- Properly ensure binary files are not included. ([#3](https://github.com/seapagan/bundle-repo/pull/3)) by [seapagan](https://github.com/seapagan)
- Display the token count for the generated xml file after running. Add option to choose the model. ([#2](https://github.com/seapagan/bundle-repo/pull/2)) by [seapagan](https://github.com/seapagan)
- Add '--file' option for custom output filename ([#1](https://github.com/seapagan/bundle-repo/pull/1)) by [seapagan](https://github.com/seapagan)

### Bug Fixes

- Fix bug where the repo was being cloned twice ([#4](https://github.com/seapagan/bundle-repo/pull/4)) by [seapagan](https://github.com/seapagan)

---
*This changelog was generated using [github-changelog-md](http://changelog.seapagan.net/) by [Seapagan](https://github.com/seapagan)*
