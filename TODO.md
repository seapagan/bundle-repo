# Planned Improvements

- add more output formats - Text, Markdown, maybe others.
- include the resolved output path in file-creation errors. Attach the context
  at the `File::create` boundary so stdout, clipboard, and earlier XML-generation
  failures remain accurately described.
- allow individual default-excluded files or categories to be included without
  replacing the default exclusion set; initially support licence files and
  `.gitignore`, then consider lockfiles, `.github/`, and tool configuration.
- add the ability to check for updates and update the tool (or at least notify
  the user that an update is available and where to get).
- actually remove comments from the generated XML file. Perhaps add a flag to
  allow the user to choose whether to include comments or not. Looking at using
  'tree-sitter' to parse the code and remove comments. Need to think about
  needed comments - like for linters and such, we may not want them removed?
  perhaps start by removing block comments and all docstrings? Then again, it is
  always good to have comments in the returned code from the LLM. Needs more
  thought. **If we add this, remember to re-add the note on comment removal to
  the output file `<notes>` node.**
- add native macOS CI coverage so continued macOS compatibility is verified.
- allow to work with non-git repositories (local only obviously).
- extend the model-aware token counting backends beyond the current GPT,
  DeepSeek, and GLM support. Future work includes official local tokenizers for
  Gemini, Claude, Qwen, and other provider families where suitable tokenizer
  assets exist. Provider token-counting APIs, fallback estimates, and generated
  output metadata remain separate design work.
- choose a tokenizer asset distribution and package-capacity strategy. The
  generated `.crate` is approximately 8.76 MB against crates.io's normal
  10 MiB compressed package limit, leaving approximately 1.7 MB of headroom.
  All embedded tokenizer source assets are included in the published crate,
  and the release binary contains the compressed form of every tokenizer
  family even though each invocation uses only one. GLM expands to roughly
  20 MB and is JSON-parsed when selected.

  **Do not add another embedded tokenizer family until a package-size and
  tokenizer-distribution strategy has been selected.** Future investigation
  should consider, without choosing an approach here:
  - requesting a crates.io package-size limit increase;
  - storing assets in a more efficiently precompressed representation and
    decompressing them in memory;
  - moving non-default tokenizer families into optional or downloaded
    tokenizer packs;
  - splitting tokenizer assets into a separate crate or family-specific
    crates;
  - providing Cargo features for smaller custom binaries, while recognising
    that features alone do not reduce the published `.crate` unless unused
    assets are excluded or separated;
  - adding PR CI that builds and verifies the `.crate`, checks required
    contents, and enforces an agreed package-size ceiling; and
  - reconsidering third-party-notice and archive presentation during the
    future release and packaging workflow cleanup.
- revisit the `cargo deny` duplicate-version warnings alongside the pending
  dependency upgrades. Prefer compatible lockfile refreshes that collapse
  transitive versions, and avoid forced or convoluted dependency unification.
- allow user to add custom metadata to the XML file, this could be used to
  store information about the repository, such as the name, description, extra
  instructions, etc. Would use the TOML config file.
- ignore `dotfiles` by default, but allow the user to include them if they want.
- Add secret-checking to the tool, to ensure that no secrets are included in the
  output XML file. Hopefully this can be done with a library, but may need to
  write our own checks.
