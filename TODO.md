# Planned Improvements

- add more output formats - Text, Markdown, maybe others.
- improve the test suite to ensure the tool works as expected in a variety of
  scenarios.
- include the resolved output path in file-creation errors. Attach the context
  at the `File::create` boundary so stdout, clipboard, and earlier XML-generation
  failures remain accurately described.
- make generated bundles well-formed XML without entity-escaping textual file
  contents. Implement this as a focused follow-up after the `xml` 1.x update:
  - write all structural markup through the XML writer rather than assembling
    tags and comments with raw `write_all` calls. This must include escaping
    metadata attributes such as file and folder paths, and safely emitting
    diagnostic comments whose text may contain `--` or other XML syntax.
  - wrap each included text file's contents in a CDATA section inside its
    `<file>` element. Do not convert source characters such as `<`, `>`, and
    `&` to entities, and do not base64-encode ordinary text files. Use the
    `xml` 1.x writer's CDATA event so embedded `]]>` sequences are split into
    adjacent safe CDATA sections instead of terminating the content early.
  - define and document a deliberate policy for characters XML 1.0 cannot
    represent even inside CDATA, especially NUL and forbidden control
    characters. Never silently emit malformed XML; either classify such input
    as binary/unsupported and omit it with an explicit reason, or apply a
    clearly documented replacement policy.
  - preserve the current binary-file behavior and existing metadata semantics
    unless a separately approved design requires changing them. Account for
    XML line-ending normalization when defining what content round-tripping
    means, particularly for CRLF and lone-CR input and when line numbering is
    enabled.
  - add regression tests which parse the complete generated document and
    verify its structure and recovered logical content. Cover `<`, `>`, `&`,
    quotes in path attributes, literal `</file>` text, embedded `]]>`, Unicode,
    CRLF/lone-CR input, forbidden control characters, nested folders, binary
    placeholders, read-error diagnostics, and line-numbered content.
  - verify that plain-file, stdout, clipboard, and gzip output all use the same
    serialization, and update exact size/token-count expectations and the XML
    layout documentation for the added CDATA delimiters. Keep this behavior
    change separate from the dependency-only pull request so its rendered
    output and compatibility impact can be reviewed independently.
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
- ensure that the tool works on Windows, Linux, and macOS. It does work great on
  all 3 at this current code state, but we need to develop a test suite and get
  the CI pipeline working to ensure that it continues to work on all 3.
- allow to work with non-git repositories (local only obviously).
- modernise token counting with model-aware tokenizer backends for current
  OpenAI, Claude, DeepSeek, GLM, Gemini, and other commonly used models.
  Prefer official local tokenizers, optionally support provider token-counting
  APIs, retain a clearly labelled conservative fallback estimate, and record
  the tokenizer source, model profile, and exact-versus-estimated status in the
  generated output.
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
