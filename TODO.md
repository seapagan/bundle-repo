# Planned Improvements

- add more output formats - Text, Markdown, maybe others.
- improve the test suite to ensure the tool works as expected in a variety of
  scenarios.
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
- allow user to add custom metadata to the XML file, this could be used to
  store information about the repository, such as the name, description, extra
  instructions, etc. Would use the TOML config file.
- ignore `dotfiles` by default, but allow the user to include them if they want.
- Add secret-checking to the tool, to ensure that no secrets are included in the
  output XML file. Hopefully this can be done with a library, but may need to
  write our own checks.
